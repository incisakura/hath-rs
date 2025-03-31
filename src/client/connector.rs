use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http::uri::Scheme;
use hyper::body::Incoming;
use hyper::rt::{Read, ReadBufCursor, Write};
use hyper::{Response, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioExecutor;
use openssl::error::ErrorStack;
use openssl::ssl::{SslConnector, SslMethod};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_openssl::SslStream;
use tower::Service;

use crate::utils::{BoxBody, LimitedStream, Limiter};
use crate::{ALPN, Error, Result};

pub(crate) struct HttpClient {
    client: Client<Conn, BoxBody>,
}

impl HttpClient {
    pub fn new(limiter: Limiter) -> Result<HttpClient, ErrorStack> {
        let mut builder = SslConnector::builder(SslMethod::tls_client())?;
        builder.set_alpn_protos(ALPN)?;
        let tls = Arc::new(builder.build());

        let connecter = Conn { limiter, tls };
        let client = Client::builder(TokioExecutor::new()).build(connecter);
        Ok(HttpClient { client })
    }

    pub async fn get(&self, uri: Uri) -> Result<Response<Incoming>> {
        self.client.get(uri).await.map_err(|e| io::Error::new(io::ErrorKind::Other, e).into()) // todo
    }
}

#[derive(Clone)]
struct Conn {
    limiter: Limiter,
    tls: Arc<SslConnector>,
}

impl Service<Uri> for Conn {
    type Error = Error;
    type Response = AltLimitedStream;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let limiter = self.limiter.clone();
        let tls = self.tls.clone();
        Box::pin(async move {
            let scheme = match uri.scheme() {
                Some(scheme) if *scheme == Scheme::HTTP || *scheme == Scheme::HTTPS => scheme,
                Some(_) => return Err(Error::UnsupportedProtocol),
                None => &Scheme::HTTP,
            };

            let host = match uri.host() {
                Some(host) => host,
                None => return Err(Error::InvalidUri),
            };

            let port = uri.port_u16().unwrap_or_else(|| if *scheme == Scheme::HTTPS { 443 } else { 80 });

            let stream = TcpStream::connect((host, port)).await?;
            let stream = limiter.limit(stream);

            if scheme == &Scheme::HTTPS {
                let tls = tls.configure()?.into_ssl(host)?;
                let mut stream = SslStream::new(tls, stream)?;
                Pin::new(&mut stream).connect().await?;
                Ok(AltLimitedStream::Tls(stream))
            } else {
                Ok(AltLimitedStream::Tcp(stream))
            }
        })
    }
}

/// A stream speed limiter applied on raw TCP connections
pub enum AltLimitedStream {
    Tcp(LimitedStream<TcpStream>),
    Tls(SslStream<LimitedStream<TcpStream>>),
}

impl Read for AltLimitedStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: ReadBufCursor<'_>,
    ) -> Poll<Result<(), io::Error>> {
        let n = {
            let mut buf = ReadBuf::uninit(unsafe { buf.as_mut() });
            let ret = match self.get_mut() {
                AltLimitedStream::Tcp(stream) => Pin::new(stream).poll_read(cx, &mut buf),
                AltLimitedStream::Tls(stream) => Pin::new(stream).poll_read(cx, &mut buf),
            };

            if let Poll::Ready(Ok(())) = ret {
                buf.filled().len()
            } else {
                return ret;
            }
        };
        unsafe { buf.advance(n) };

        Poll::Ready(Ok(()))
    }
}

impl Write for AltLimitedStream {
    fn is_write_vectored(&self) -> bool {
        match self {
            AltLimitedStream::Tcp(stream) => stream.is_write_vectored(),
            AltLimitedStream::Tls(stream) => stream.is_write_vectored(),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.get_mut() {
            AltLimitedStream::Tcp(stream) => Pin::new(stream).poll_flush(cx),
            AltLimitedStream::Tls(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.get_mut() {
            AltLimitedStream::Tcp(stream) => Pin::new(stream).poll_shutdown(cx),
            AltLimitedStream::Tls(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }

    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, io::Error>> {
        match self.get_mut() {
            AltLimitedStream::Tcp(stream) => Pin::new(stream).poll_write(cx, buf),
            AltLimitedStream::Tls(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        match self.get_mut() {
            AltLimitedStream::Tcp(stream) => Pin::new(stream).poll_write_vectored(cx, bufs),
            AltLimitedStream::Tls(stream) => Pin::new(stream).poll_write_vectored(cx, bufs),
        }
    }
}

impl Connection for AltLimitedStream {
    fn connected(&self) -> Connected {
        let mut connected = Connected::new();
        if let AltLimitedStream::Tls(stream) = self {
            if stream.ssl().selected_alpn_protocol() == Some(b"h2") {
                connected = connected.negotiated_h2()
            }
        }
        connected
    }
}
