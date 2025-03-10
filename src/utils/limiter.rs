use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, ready};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::{Instant, Sleep, sleep};

#[derive(Clone)]
pub struct Limiter {
    inner: Arc<LimiterInner>,
}

impl Limiter {
    pub fn new(speed_limit: f64) -> Limiter {
        let bucket = Bucket {
            updated_at: Instant::now(),
            volumn: 0.0,
            speed_limit,
        };
        let inner = LimiterInner {
            bucket: Mutex::new(bucket),
            is_unlimited: AtomicBool::new(speed_limit == f64::INFINITY),
        };

        Limiter { inner: Arc::new(inner) }
    }

    pub fn limit<S>(&self, stream: S) -> LimitedStream<S> {
        let limiter = self.inner.clone();
        let pause = Box::pin(sleep(Duration::ZERO));
        LimitedStream { limiter, pause, stream }
    }

    pub fn set_speed_limit(&self, speed_limit: f64) {
        if speed_limit == f64::INFINITY {
            self.inner.is_unlimited.swap(true, Ordering::Relaxed);
        } else {
            self.inner.is_unlimited.swap(false, Ordering::Relaxed);
            self.inner.bucket.lock().unwrap().speed_limit = speed_limit;
        }
    }
}

struct LimiterInner {
    bucket: Mutex<Bucket>,
    is_unlimited: AtomicBool,
}

impl LimiterInner {
    fn consume(&self, bytes: usize) -> Duration {
        if self.is_unlimited.load(Ordering::Relaxed) {
            return Duration::ZERO;
        }
        let mut bucket = self.bucket.lock().unwrap();
        bucket.refill(Instant::now());
        bucket.consume(bytes)
    }
}

struct Bucket {
    updated_at: Instant,
    volumn: f64,
    speed_limit: f64,
}

impl Bucket {
    fn consume(&mut self, bytes: usize) -> Duration {
        self.volumn -= bytes as f64;
        if self.volumn >= 0.0 {
            Duration::ZERO
        } else {
            let sleep_secs = 0.1 - (self.volumn / self.speed_limit);
            Duration::from_secs_f64(sleep_secs)
        }
    }

    fn refill(&mut self, now: Instant) {
        let elapsed = (now - self.updated_at).as_secs_f64();
        let refilled = self.speed_limit * elapsed;
        self.volumn = (self.speed_limit * 0.1).min(self.volumn + refilled);
        self.updated_at = now;
    }
}

pub struct LimitedStream<S> {
    limiter: Arc<LimiterInner>,
    pause: Pin<Box<Sleep>>,
    stream: S,
}

impl<S> AsyncRead for LimitedStream<S>
where
    S: AsyncRead + Unpin,
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        ready!(self.pause.as_mut().poll(cx));
        ready!(Pin::new(&mut self.stream).poll_read(cx, buf)?);

        if !buf.filled().is_empty() {
            let dur = self.limiter.consume(buf.filled().len());
            self.pause.as_mut().reset(Instant::now() + dur);
        }
        Poll::Ready(Ok(()))
    }
}

impl<S> AsyncWrite for LimitedStream<S>
where
    S: AsyncWrite + Unpin,
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, io::Error>> {
        ready!(self.pause.as_mut().poll(cx));
        let n = ready!(Pin::new(&mut self.stream).poll_write(cx, buf)?);

        let dur = self.limiter.consume(n);
        self.pause.as_mut().reset(Instant::now() + dur);
        Poll::Ready(Ok(n))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        ready!(self.pause.as_mut().poll(cx));
        let n = ready!(Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)?);

        let dur = self.limiter.consume(n);
        self.pause.as_mut().reset(Instant::now() + dur);
        Poll::Ready(Ok(n))
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }
}
