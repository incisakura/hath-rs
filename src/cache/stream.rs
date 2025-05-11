use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{self, Poll, ready};

use http_body_util::BodyExt;
use hyper::body::{Body, Bytes, Frame, SizeHint};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::{AppContext, Error, Result};

use super::CacheFile;

pub enum CacheStream {
    Hit {
        file: File,
        len: u64,
        buf: ReadBuf<'static>,
    },
    Miss {
        rx: UnboundedReceiver<Bytes>,
        size: SizeHint,
    },
}

impl CacheStream {
    pub async fn new(ctx: &AppContext, file_info: &CacheFile, extra: (&str, &str)) -> Result<Option<CacheStream>> {
        let path = file_info.path(&ctx.cache_dir);

        let _ = tokio::fs::create_dir_all(path.parent().unwrap()).await;
        let mut file = OpenOptions::new().create(true).read(true).append(true).open(&path).await?;

        let metadata = file.metadata().await?;
        let len = metadata.len();

        if len == 0 {
            let mut body =
                if let Some(body) = ctx.static_range_fetch(extra.0, extra.1, &file_info.filename(true)).await? {
                    body
                } else {
                    return Ok(None);
                };
            let size = body.size_hint();

            let (tx, rx) = unbounded_channel();
            tokio::spawn(async move {
                while let Some(f) = body.frame().await {
                    match f {
                        Ok(f) => {
                            // if it's data frame, get the data
                            if let Ok(b) = f.into_data() {
                                // ignore error even rx closed
                                let _ = tx.send(b.clone());

                                if let Err(e) = file.write_all(&b).await {
                                    log::error!("write cache file: {}", e);
                                    let _ = file.set_len(0).await;
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            // when error occured, the file is most likely be broken
                            log::error!("read remote cache stream: {}", e);
                            let _ = file.set_len(0).await;
                            break;
                        }
                    }
                }
            });

            ctx.cache_manager.lock().unwrap().add(&ctx.cache_dir, file_info.clone());
            return Ok(Some(CacheStream::Miss { rx, size }));
        }

        ctx.cache_manager.lock().unwrap().update(file_info);
        let buf = vec![MaybeUninit::uninit(); 8192].leak();
        let buf = ReadBuf::uninit(buf);
        Ok(Some(CacheStream::Hit { file, len, buf }))
    }
}

impl Body for CacheStream {
    type Data = Bytes;

    type Error = Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        match this {
            CacheStream::Hit { file, buf, .. } => {
                ready!(Pin::new(file).poll_read(cx, buf)?);

                if !buf.filled().is_empty() {
                    let bytes = Bytes::copy_from_slice(buf.filled());
                    let frame = Frame::data(bytes);
                    buf.clear();
                    Poll::Ready(Some(Ok(frame)))
                } else {
                    Poll::Ready(None)
                }
            }
            CacheStream::Miss { rx, .. } => {
                if let Some(b) = ready!(rx.poll_recv(cx)) {
                    let frame = Frame::data(b);
                    return Poll::Ready(Some(Ok(frame)));
                }

                Poll::Ready(None)
            }
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            CacheStream::Hit { len, .. } => SizeHint::with_exact(*len),
            CacheStream::Miss { size, .. } => size.clone(),
        }
    }
}

impl Drop for CacheStream {
    fn drop(&mut self) {
        if let CacheStream::Hit { buf, .. } = self {
            // SAFETY: buf is static owned by current struct which is dropping
            let buf = unsafe { Box::from_raw(buf.inner_mut() as *mut _) };
            drop(buf);
        }
    }
}
