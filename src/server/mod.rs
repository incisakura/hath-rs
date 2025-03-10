use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use http::StatusCode;
use openssl::ssl::{Ssl, SslAcceptor, SslMethod};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_openssl::SslStream;

use crate::{ClientContext, Error, Result, TokioExecutor};

mod service;
use service::ServerService;

mod routes;
use routes::{file_fetch, server_command, speed_test};

mod stream;
use stream::IncomingStream;

pub struct Server {
    listen: TcpListener,
    ctx: ServerContext,

    router: Router,
    conn_handler: Arc<hyper_util::server::conn::auto::Builder<TokioExecutor>>,
}

impl Server {
    pub async fn new(bind: SocketAddr, ctx: ServerContext) -> Result<Server, io::Error> {
        let listen = TcpListener::bind(bind).await?;
        let router = Router::new()
            .route("/h/{file_id}/{*extra}", get(file_fetch))
            .route("/t/{size}/{time}/{key}/{*nonce}", get(speed_test))
            .route("/servercmd/{cmd}/{extra}/{time}/{*key}", get(server_command))
            .route("/favicon.ico", get((
                StatusCode::MOVED_PERMANENTLY,
                [(http::header::LOCATION, "https://e-hentai.org/favicon.ico")]
            )))
            .route("/robots.txt", get("User-agent: *\nDisallow: /"))
            .fallback(StatusCode::NOT_FOUND)
            .with_state(ctx.clone());

        let conn_handler = Arc::new(hyper_util::server::conn::auto::Builder::new(TokioExecutor));
        Ok(Server { listen, ctx, router, conn_handler})
    }

    pub async fn run(self) {
        loop {
            if let Ok((stream, addr)) = self.listen.accept().await {
                log::debug!("incomine from {}", addr);

                let ssl = Ssl::new(self.ctx.tls.read().unwrap().context()).unwrap();
                let mut stream = SslStream::new(ssl, stream).unwrap();
                let service = ServerService::new(self.router.clone(), addr);
                let conn_handler = self.conn_handler.clone();

                tokio::spawn(async move {
                    if timeout(Duration::from_secs(10), Pin::new(&mut stream).accept()).await.is_err() {
                        log::debug!("timeout at TLS handshake from {}", addr);
                    }

                    let conn = IncomingStream::new(stream);
                    let fut = conn_handler.serve_connection_with_upgrades(conn, service);

                    // we serve single connection for max 120 seconds
                    // todo: connection based timeout
                    let ret = timeout(Duration::from_secs(120), fut).await;
                    match ret {
                        Ok(Ok(())) => {},
                        Ok(Err(e)) => log::warn!("error serving connection from {}: {}", addr, e),
                        Err(_) => log::debug!("connection from {} timeout", addr),
                    }
                });
            }
        }
    }
}

#[derive(Clone)]
pub struct ServerContext {
    pub client: Arc<ClientContext>,
    pub tls: Arc<RwLock<SslAcceptor>>,
}

impl ServerContext {
    pub async fn new(file: File, ctx: &Arc<ClientContext>) -> Result<ServerContext> {
        let tls = new_tls_acceptor(file, &ctx.key).await?;
        Ok(ServerContext {
            client: ctx.clone(),
            tls: Arc::new(RwLock::new(tls)),
        })
    }

    pub async fn reload_cert(&self, file: File) -> Result<()> {
        let tls = new_tls_acceptor(file, &self.client.key).await?;
        *self.tls.write().unwrap() = tls;
        Ok(())
    }
}

async fn new_tls_acceptor(mut file: File, key: &str) -> Result<SslAcceptor> {
    use openssl::pkcs12::{ParsedPkcs12_2, Pkcs12};
    use openssl::ssl::{AlpnError, select_next_proto};

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await?;

    let pkcs12 = Pkcs12::from_der(&bytes)?;
    let pkcs12_2 = pkcs12.parse2(key)?;

    if let ParsedPkcs12_2 { pkey: Some(pkey), cert: Some(cert), ca: Some(mut ca) } = pkcs12_2
    {
        let mut builder = SslAcceptor::mozilla_modern(SslMethod::tls_server())?;
        builder.set_certificate(&cert)?;
        builder.set_private_key(&pkey)?;
        while let Some(ca) = ca.pop() {
            builder.add_extra_chain_cert(ca)?;
        }
        builder.set_alpn_select_callback(|_, alpn| {
            select_next_proto(crate::ALPN, alpn).ok_or(AlpnError::NOACK)
        });
        Ok(builder.build())
    } else {
        Err(Error::IncompleteCertFile)
    }
}
