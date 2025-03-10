use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::{Path, State};
use hyper::body::{Bytes, Frame, SizeHint};

use crate::server::ServerContext;
use crate::utils::sha1_hex_concat;
use crate::{Error, Result};

pub(crate) async fn speed_test(
    Path((size, time, key, _nonce)): Path<(usize, String, String, String)>,
    State(ctx): State<ServerContext>,
) -> Body {
    let hash = sha1_hex_concat(&[
        "hentai@home-speedtest-",
        &size.to_string(),
        "-",
        &time,
        "-",
        &ctx.client.id.to_string(),
        "-",
        &ctx.client.key.to_string(),
    ]);

    if hash == key {
        return Body::new(SpeedTest::new(size));
    }

    Body::empty()
}

// todo: move to super module
pub(crate) struct SpeedTest {
    total: usize,
    to_fill: usize,
}

impl SpeedTest {
    pub fn new(size: usize) -> SpeedTest {
        SpeedTest {
            total: size,
            to_fill: size,
        }
    }
}

impl hyper::body::Body for SpeedTest {
    type Data = Bytes;

    type Error = Error;

    fn poll_frame(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        let fill = match this.to_fill {
            0 => return Poll::Ready(None),
            1..=65535 => this.to_fill,
            _ => 65535,
        };
        this.to_fill -= fill;
        let buf = Bytes::from_static(&[0; 65535][..fill]);
        let frame = Frame::data(buf);
        Poll::Ready(Some(Ok(frame)))
    }

    fn is_end_stream(&self) -> bool {
        self.to_fill == 0
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.total as u64)
    }
}
