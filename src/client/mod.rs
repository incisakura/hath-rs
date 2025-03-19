use std::path::PathBuf;

use http::uri::{Scheme, Uri};
use http_body_util::BodyExt;
use hyper::body::{Body, Incoming};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use crate::{CLIENT_VER, AppContext, Result, Error};
use crate::utils::sha1_digest;

mod connector;
pub mod downloader;

pub(crate) use connector::HttpClient;
use downloader::DownloadMeta;

impl AppContext {
    fn get_uri(&self, act: &str, add: &str) -> Uri {
        // parameters
        let id = self.id.to_string();
        let time = crate::unix_time().to_string();

        // key
        let key = sha1_digest(&["hentai@home", act, &add, &id, &time, &self.key]);

        // uri
        let path = format!(
            "/15/rpc?clientbuild={}&act={}&add={}&cid={}&acttime={}&actkey={}",
            CLIENT_VER, act, add, id, time, key
        );

        Uri::builder()
            .scheme(Scheme::HTTP)
            .authority("rpc.hentaiathome.net")
            .path_and_query(path)
            .build()
            .unwrap()
    }

    async fn rpc_request(&self, act: &str, add: &str) -> Result<Vec<String>> {
        let uri = self.get_uri(act, add);
        log::info!("client reqeust: {}", act);

        // request
        let response = self.client.get(uri).await?;

        // max acceptable body size: 10MiB
        match response.body().size_hint().upper() {
            Some(n) if n > 10 * 1024 * 1024 => return Err(Error::BadResponse),
            Some(_) => (),
            None => return Err(Error::BadResponse),
        };

        let body = response.into_body().collect().await?.to_bytes();
        let body = std::str::from_utf8(&body)?;

        let mut iter = body.split('\n');
        let first = iter.next();
        match first {
            Some("OK") => {}
            Some(err) => {
                log::error!("request error: {err}");
                return Err(Error::BadResponse);
            }
            None => return Err(Error::BadResponse),
        }

        let data = iter.map(|x| x.to_string()).collect();
        Ok(data)
    }

    async fn download(&self, uri: Uri, path: PathBuf) -> Result<File> {
        let mut file = OpenOptions::new().create(true).write(true).read(true).open(path).await?;
        let response = self.client.get(uri).await?;

        log::debug!("download: {}", response.status());

        let mut body = response.into_body();
        while let Some(next) = body.frame().await {
            let frame = next?;
            if let Some(chunk) = frame.data_ref() {
                file.write(&chunk).await?;
            }
        }
        file.seek(std::io::SeekFrom::Start(0)).await?;

        Ok(file)
    }

    pub async fn login(&self) -> Result<()> {
        let data = self.rpc_request("client_login", "").await?;
        self.update(data)
    }

    pub async fn notify_start(&self) -> Result<()> {
        self.rpc_request("client_start", "").await?;
        Ok(())
    }

    pub async fn alive(&self) -> Result<()> {
        self.rpc_request("still_alive", "").await?;
        Ok(())
    }

    pub async fn update_settings(&self) -> Result<()> {
        let data = self.rpc_request("client_settings", "").await?;
        self.update(data)
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.rpc_request("client_stop", "").await?;
        Ok(())
    }

    pub async fn download_cert(&self) -> Result<File> {
        let uri = self.get_uri("get_cert", "");

        let mut path = self.data_dir.clone();
        path.push("hathcert.p12");
        self.download(uri, path).await
    }

    pub async fn static_range_fetch(&self, index: &str, xres: &str, file_id: &str) -> Result<Option<Incoming>> {
        let add = format!("{};{};{}", index, xres, file_id);
        let res = self.rpc_request("srfetch", &add).await?;
        let iter = res.into_iter().filter_map(|s| Uri::try_from(s.to_string()).ok());
        for uri in iter {
            if let Ok(res) = self.client.get(uri).await {
                return Ok(Some(res.into_body()));
            }
        }
        Ok(None)
    }

    /// # Argument
    ///
    /// - downloaded: tell server that gallery is completely downloaded
    pub async fn download_gallery(&self, downloaded: Option<&DownloadMeta>) -> Result<DownloadMeta> {
        let act = "fetchqueue";
        let add = match downloaded {
            Some(x) => format!("{};{}", x.gid, x.min_res),
            None => String::from(""),
        };

        // parameters
        let time = crate::unix_time().to_string();
        let id = self.id.to_string();

        // key
        let key = sha1_digest(&["hentai@home", act, &add, &id, &time, &self.key]);

        // uri
        let path = format!(
            "/15/dl?clientbuild={}&act={}&add={}&cid={}&acttime={}&actkey={}",
            CLIENT_VER, act, add, id, time, key
        );

        let uri = Uri::builder()
            .scheme(Scheme::HTTP)
            .authority("rpc.hentaiathome.net")
            .path_and_query(path)
            .build()
            .unwrap();

        let res = self.client.get(uri).await?;
        let body = res.into_body().collect().await?.to_bytes();
        let body = std::str::from_utf8(&body)?;

        Ok(DownloadMeta::parse(body))
    }

    pub async fn downloader_fetch(
        &self,
        gid: u32,
        page: u32,
        file_index: u32,
        xres: u32,
        retry: u32,
    ) -> Result<Option<Incoming>> {
        let add = format!("{gid};{page};{file_index};{xres};{retry}");
        let res = self.rpc_request("dlfetch", &add).await?;
        let iter = res.into_iter().filter_map(|s| Uri::try_from(s.to_string()).ok());
        for uri in iter {
            if let Ok(res) = self.client.get(uri).await {
                return Ok(Some(res.into_body()));
            }
        }
        Ok(None)
    }
}
