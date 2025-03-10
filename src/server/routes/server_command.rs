use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use http::uri::{Authority, Parts, PathAndQuery, Scheme};
use http_body_util::BodyExt;
use hyper::{Response, Uri};

use crate::client::downloader::download_gallery;
use crate::server::ServerContext;
use crate::unix_time;
use crate::utils::sha1_hex_concat;
use crate::{Error, Result};

use super::SpeedTest;

pub(crate) async fn server_command(
    State(ctx): State<ServerContext>,
    Path((command, extra, time, key)): Path<(String, String, u64, String)>,
) -> Result<Response<Body>> {
    let key = key.split_once('/').map_or(key.as_str(), |(x, _)| x);

    let hash = sha1_hex_concat(&[
        "hentai@home-servercmd-",
        &command,
        "-",
        &extra,
        "-",
        &ctx.client.id.to_string(),
        "-",
        &time.to_string(),
        "-",
        &ctx.client.key.to_string(),
    ]);
    let sys_time = unix_time();
    if !(time >= sys_time || sys_time - time <= 300) || hash != key {
        return Err(Error::BadRequest);
    }
    let extra = parse_extra(&extra);
    let service = CommandService { extra };

    match command.as_str() {
        "speed_test" => Ok(service.speed_test()),
        "still_alive" => Ok(Response::new(
            String::from("I feel FANTASTIC and I'm still alive").into(),
        )),
        "threaded_proxy_test" => service.threaded_proxy_test(&ctx).await,
        "refresh_settings" => {
            ctx.client.update_settings().await?;
            Ok(Response::new(Body::empty()))
        }
        "refresh_certs" => {
            let file = ctx.client.download_cert().await?;
            ctx.reload_cert(file).await?;
            Ok(Response::new(Body::empty()))
        }
        "start_downloader" => {
            tokio::spawn(async move {
                let download_meta = match ctx.client.download_gallery(None).await {
                    Ok(x) => x,
                    Err(e) => {
                        log::error!("Error in download gallery metadata: {}", e);
                        return;
                    }
                };

                let client = ctx.client.clone();
                if let Err(e) = download_gallery(client, download_meta).await {
                    log::error!("Error in downloading gallery: {}", e);
                }
            });
            Ok(Response::new(Body::empty()))
        }
        _ => Err(Error::BadRequest),
    }
}

fn parse_extra(extra: &str) -> HashMap<&'_ str, &'_ str> {
    let mut map = HashMap::new();
    let iter = extra.split(';');
    for i in iter {
        if let Some((k, v)) = i.split_once('=') {
            map.insert(k, v);
        }
    }
    map
}

struct CommandService<'a> {
    extra: HashMap<&'a str, &'a str>,
}

impl<'a> CommandService<'a> {
    async fn threaded_proxy_test(&self, ctx: &ServerContext) -> Result<Response<Body>> {
        let hostname = self.extra.get("hostname").ok_or(Error::BadRequest)?;
        let port = self.extra.get("port").ok_or(Error::BadRequest)?;
        let testsize = self.extra.get("testsize").ok_or(Error::BadRequest)?;
        let testtime = self.extra.get("testtime").ok_or(Error::BadRequest)?;
        let testkey = self.extra.get("testkey").ok_or(Error::BadRequest)?;
        let testcount = self.extra.get("testcount").and_then(|s| s.parse().ok()).ok_or(Error::BadRequest)?;

        // build uri
        let mut parts = Parts::default();
        if let Some(Ok(s)) = self.extra.get("protocol").map(|s| Scheme::try_from(*s)) {
            parts.scheme = Some(s);
        }
        let nonce = rand::random::<u32>() / 2;
        parts.authority = Some(Authority::try_from(format!("{}:{}", hostname, port)).unwrap());
        parts.path_and_query =
            Some(PathAndQuery::try_from(format!("/t/{}/{}/{}/{}", testsize, testtime, testkey, nonce)).unwrap());
        let uri = Uri::from_parts(parts).unwrap();

        // create tasks
        let mut vec = Vec::with_capacity(testcount);
        for _ in 0..testcount {
            let ctx = ctx.client.clone();
            let uri = uri.clone();
            vec.push(tokio::spawn(async move {
                let res = ctx.client.get(uri).await?;
                let mut body = res.into_body();

                let start = SystemTime::now();
                while let Some(r) = body.frame().await {
                    let _ = r?; // return error or just drop data
                }
                let end = SystemTime::now();
                let time = end.duration_since(start).unwrap();
                Ok::<_, Error>(time)
            }));
        }

        // collect results
        let mut time = Duration::ZERO;
        let mut success = 0;
        for i in vec {
            match i.await {
                Ok(Ok(d)) => {
                    success += 1;
                    time += d;
                }
                Ok(Err(e)) => log::error!("get error in threaded-proxy-test: {}", e),
                _ => {}
            }
        }

        Ok(format!("OK:{}-{}", success, time.as_millis()).into_response())
    }

    fn speed_test(&self) -> Response<Body> {
        let size = self.extra.get("testsize").and_then(|s| s.parse().ok()).unwrap_or(1000000);
        Body::new(SpeedTest::new(size)).into_response()
    }
}
