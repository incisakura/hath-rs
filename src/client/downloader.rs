use std::path::Path;
use std::sync::Arc;

use http_body_util::BodyExt;
use tokio::io::AsyncWriteExt;

use crate::utils::file_sha1;
use crate::{AppContext, Result};

#[derive(Default, Debug)]
pub struct DownloadMeta {
    // header
    pub gid: u32,
    file_count: u32,
    pub min_res: u32,
    title: String,
    // files
    files: Vec<GalleryFile>,
    // info
    info: String,
}

impl DownloadMeta {
    pub fn parse(data: &str) -> DownloadMeta {
        let mut meta = DownloadMeta::default();

        let mut pharse_titile = true;
        for line in data.split('\n') {
            if pharse_titile {
                if line == "FILELIST" {
                    pharse_titile = false;
                    continue;
                }
                if let Some((key, value)) = line.split_once(' ') {
                    match key {
                        "GID" => meta.gid = value.parse().unwrap_or_default(),
                        "FILECOUNT" => meta.file_count = value.parse().unwrap_or_default(),
                        "MINXRES" => meta.min_res = value.parse().unwrap_or_default(),
                        "TITLE" => meta.title = value.to_owned(),
                        _ => {}
                    }
                }
            } else {
                if line == "INFORMATION" {
                    let str = unsafe {
                        let start_ptr = line.as_ptr().add(line.len()).add(1); // additional add 1 for "\n"
                        let end_ptr = data.as_ptr().add(data.len());
                        let len = end_ptr.byte_offset_from(start_ptr) as usize;
                        let str = std::ptr::slice_from_raw_parts(start_ptr, len);
                        std::str::from_utf8_unchecked(&*str as &[u8])
                    };
                    meta.info = str.to_owned();
                    break;
                }
                if let Some(file) = GalleryFile::parse(line) {
                    meta.files.push(file);
                }
            }
        }
        meta
    }
}

#[derive(Default, Debug)]
struct GalleryFile {
    page: u32,
    index: u32,
    xres: u32,
    sha1_hash: String,
    filetype: String,
    filename: String,
}

impl GalleryFile {
    pub fn parse(line: &str) -> Option<GalleryFile> {
        let mut file = GalleryFile::default();
        let mut filename = None;

        for (i, value) in line.split(" ").enumerate() {
            match i {
                0 => file.page = value.parse().unwrap_or_default(),
                1 => file.index = value.parse().unwrap_or_default(),
                2 => file.xres = value.parse().unwrap_or_default(),
                3 => file.sha1_hash = value.to_owned(),
                4 => file.filetype = value.to_owned(),
                5 => filename = Some(value.to_owned()),
                _ => {}
            }
        }

        if let Some(filename) = filename {
            file.filename = filename;
            Some(file)
        } else {
            None
        }
    }

    pub async fn download(&self, ctx: &AppContext, todir: &Path, gid: u32) -> Result<()> {
        let mut path = todir.to_path_buf();
        path.push(format!("{}.{}", self.filename, self.filetype));

        let mut file = tokio::fs::OpenOptions::new().create(true).write(true).open(path).await?;
        if file.metadata().await?.len() > 0 {
            let hash = file_sha1(&mut file).await?;
            if hash == self.sha1_hash {
                log::debug!("file {}, hash verified", self.filename);
                return Ok(());
            }
        }

        // truncate existed file
        file.set_len(0).await?;

        let mut body = match ctx.downloader_fetch(gid, self.page, self.index, self.xres, 0).await {
            Ok(Some(b)) => b,
            Ok(None) => {
                // TODO: retry
                log::error!("failed to download file: {}, no resource", self.filename);
                return Ok(());
            }
            Err(e) => {
                // TODO: retry
                log::error!("failed to download file: {}, {}", self.filename, e);
                return Ok(());
            }
        };

        while let Some(f) = body.frame().await {
            match f {
                Ok(f) => {
                    // if it's data frame, get the data
                    if let Ok(b) = f.into_data() {
                        if let Err(e) = file.write_all(&b).await {
                            log::error!("write cache file: {}", e);
                            let _ = file.set_len(0).await;
                            break;
                        }
                    }
                }
                Err(_) => {
                    let _ = file.set_len(0).await;
                    break;
                }
            }
        }

        Ok(())
    }
}

fn take_first_100_chars(s: &str) -> &str {
    &s[..s.len().min(125)]
}

pub async fn download_gallery(ctx: Arc<AppContext>, meta: DownloadMeta) -> Result<()> {
    // create download directory
    let mut todir = ctx.data_dir.clone();
    todir.push("downloads");

    let mut meta = Some(meta);
    while let Some(ref m) = meta {
        let dir = format!("[{}-{}] {}", m.gid, m.min_res, m.title);
        todir.push(take_first_100_chars(&dir));
        tokio::fs::create_dir_all(&todir).await?;

        for file in &m.files {
            log::info!("downloading: {}", file.filename);
            file.download(&ctx, &todir, m.gid).await?;
        }

        let m = ctx.download_gallery(Some(&m)).await?;
        if m.gid == 0 {
            meta = None;
        } else  {
            meta = Some(m);
        }
    }
    Ok(())
}
