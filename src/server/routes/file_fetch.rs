use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use http::header::CONTENT_TYPE;
use hyper::{Response, StatusCode};

use crate::cache::{CacheFile, CacheStream};
use crate::utils::sha1_digest;
use crate::{Error, Result, ServerContext, unix_time};

pub(crate) async fn file_fetch(
    Path((file_id, extra)): Path<(String, String)>,
    State(ctx): State<ServerContext>,
) -> Result<Response<Body>> {
    let extra = extra.split_once('/').map_or(extra.as_str(), |(x, _)| x);

    let file = CacheFile::try_from(file_id.as_str()).map_err(|_| Error::BadRequest)?;
    let data = FileFetchExtra::from_path_parts(&extra).ok_or(Error::BadRequest)?;

    if !ctx.in_static_range(file.static_range()) {
        return Err(Error::BadRequest);
    }

    let (time_str, hash_part) = data.keystamp.split_once('-').ok_or(Error::BadRequest)?;
    let time: u64 = time_str.parse()?;

    let hash = sha1_digest(&[time_str, file_id.as_str(), &ctx.key, "hotlinkthis"]);
    if unix_time().abs_diff(time) > 900 || !hash[..10].eq(hash_part) {
        return Err(Error::BadRequest);
    }

    let stream = CacheStream::new(&ctx, &file, (data.fileindex, data.xres)).await?;
    if let Some(s) = stream {
        Ok(Response::builder().header(CONTENT_TYPE, file.info.typ.mine_type()).body(Body::new(s)).unwrap())
    } else {
        Ok(StatusCode::NOT_FOUND.into_response())
    }
}

struct FileFetchExtra<'a> {
    keystamp: &'a str,
    fileindex: &'a str,
    xres: &'a str,
}

impl FileFetchExtra<'_> {
    fn from_path_parts(extra: &str) -> Option<FileFetchExtra> {
        let mut keystamp = None;
        let mut fileindex = None;
        let mut xres = None;

        for (k, v) in extra.split(';').filter_map(|x| x.split_once('=')) {
            match k {
                "keystamp" => keystamp = Some(v),
                "fileindex" => fileindex = Some(v),
                "xres" => xres = Some(v),
                _ => continue,
            }
        }

        Some(FileFetchExtra {
            keystamp: keystamp?,
            fileindex: fileindex?,
            xres: xres?,
        })
    }
}

#[cfg(test)]
mod test {
    use std::io::Read;

    //use super::*;

    #[tokio::test]
    async fn file_fetch() {
        let root = env!("CARGO_MANIFEST_DIR");
        //let file_id = "5eb2e462781a2ba02cf435d6baa3573f4551c1a5";

        // get raw file data
        let mut data_buf: Vec<u8> = Vec::with_capacity(37444);
        let file_path = "./tests/image/5eb2e462781a2ba02cf435d6baa3573f4551c1a5-37444-1800-1000.png";
        let mut file = std::fs::File::open(format!("{root}{file_path}")).unwrap();
        file.read_to_end(&mut data_buf).unwrap();

        // request file
        // todo
    }
}
