use std::fmt::{self, Formatter, LowerHex};
use std::path::PathBuf;

use http::HeaderValue;

use crate::utils::hex_to_u8;
use crate::AppContext;

pub struct FetchParseError;

/// Infomation of cached files
pub struct CacheFile {
    pub hash: FileHash,
    pub info: FileInfo,
}

impl CacheFile {
    pub fn path(&self, ctx: &AppContext) -> PathBuf {
        let mut path = ctx.cache_dir.clone();
        let filename = self.filename(false);
        path.push(filename.get(..2).unwrap());
        path.push(filename.get(2..4).unwrap());
        path.push(filename);
        path
    }

    // with extension
    pub fn filename(&self, for_api: bool) -> String {
        let mut name = format!(
            "{:x}-{}-{}-{}",
            self.hash, self.info.size, self.info.res.0, self.info.res.1,
        );

        let ext = self.info.typ.extension();
        name.reserve(ext.len() + 1);
        name.push(if for_api { '-' } else { '.' });
        name.push_str(ext);

        name
    }

    pub fn static_range(&self) -> u16 {
        u16::from_be_bytes(self.hash.0[..2].try_into().unwrap())
    }
}

impl TryFrom<&str> for CacheFile {
    type Error = FetchParseError;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        let mut iter = name.split('-');
        if let (Some(Ok(hash)), Some(Ok(size)), Some(Ok(x_res)), Some(Ok(y_res)), Some(typ)) = (
            iter.next().map(FileHash::try_from),
            iter.next().map(|x| x.parse()),
            iter.next().map(|x| x.parse()),
            iter.next().map(|x| x.parse()),
            iter.next().map(FileType::from),
        ) {
            let info = FileInfo {
                size,
                res: (x_res, y_res),
                typ,
            };
            Ok(CacheFile { hash, info })
        } else {
            Err(FetchParseError)
        }
    }
}

#[derive(Hash, PartialEq, Eq)]
pub struct FileHash([u8; 20]);

impl LowerHex for FileHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for n in self.0 {
            let buf = crate::utils::u8_to_hex(n);
            let str = unsafe { std::str::from_utf8_unchecked(&buf) };
            f.write_str(str)?;
        }
        Ok(())
    }
}

impl TryFrom<&str> for FileHash {
    type Error = FetchParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.len() != 40 {
            return Err(FetchParseError);
        }

        let bytes = value.as_bytes();
        let mut raw = [0; 20];
        for (n, b) in raw.iter_mut().zip(bytes.chunks_exact(2)) {
            match hex_to_u8(b[0], b[1]) {
                Some(x) => *n = x,
                None => return Err(FetchParseError),
            }
        }
        Ok(FileHash(raw))
    }
}

pub struct FileInfo {
    pub size: u64,
    /// Resulution: x, y
    pub res: (u32, u32),
    pub typ: FileType,
}

#[derive(Debug)]
pub enum FileType {
    // Image
    JPEG,
    PNG,
    GIF,
    WebP,
    AVIF,
    JpegXL,

    // Video
    MP4,
    WebM,

    // Unknown
    Unknown(String),
}

impl FileType {
    pub fn mine_type(&self) -> HeaderValue {
        match self {
            FileType::JPEG => HeaderValue::from_static("image/jpeg"),
            FileType::PNG => HeaderValue::from_static("image/png"),
            FileType::GIF => HeaderValue::from_static("image/gif"),
            FileType::WebP => HeaderValue::from_static("image/webp"),
            FileType::AVIF => HeaderValue::from_static("image/avif"),
            FileType::JpegXL => HeaderValue::from_static("image/jxl"),
            FileType::MP4 => HeaderValue::from_static("video/mp4"),
            FileType::WebM => HeaderValue::from_static("video/webm"),
            _ => HeaderValue::from_static("application/octet-stream"),
        }
    }

    pub fn extension(&self) -> &str {
        match self {
            FileType::JPEG => "jpg",
            FileType::PNG => "png",
            FileType::GIF => "gif",
            FileType::WebP => "wbp",
            FileType::AVIF => "avf",
            FileType::JpegXL => "jxl",
            FileType::MP4 => "mp4",
            FileType::WebM => "webm",
            FileType::Unknown(s) => s.as_str(),
        }
    }
}

impl From<&str> for FileType {
    fn from(value: &str) -> Self {
        match value {
            "jpg" => FileType::JPEG,
            "png" => FileType::PNG,
            "gif" => FileType::GIF,
            "wbp" => FileType::WebP,
            "avf" => FileType::AVIF,
            "jxl" => FileType::JpegXL,
            "mp4" => FileType::MP4,
            "webm" => FileType::WebM,
            x => FileType::Unknown(x.to_owned()),
        }
    }
}
