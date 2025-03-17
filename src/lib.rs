use std::fs::File;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use context::ClientContext;
use openssl::error::ErrorStack;
use server::ServerContext;

mod cache;
mod client;
mod context;
mod error;
mod server;
mod utils;

use crate::error::Error;
use crate::server::Server;
use crate::utils::unix_time;

pub type Result<T, E = error::Error> = std::result::Result<T, E>;

/// ALPN protocols supported by HTTP backend.
pub const ALPN: &[u8] = b"\x02h2\x08http/1.1";

pub const CLIENT_VER: u16 = 169;

#[derive(serde::Deserialize)]
pub struct Config {
    pub log_level: log::LevelFilter,
    pub id: u32,
    pub key: String,
    pub bind: SocketAddr,

    pub speedlimit: f64,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Config> {
        let file = File::open(path)?;
        serde_json::from_reader(file).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

pub async fn main(config: Config) -> Result<()> {
    unsafe { simple_logger::init().unwrap_unchecked() };
    log::set_max_level(config.log_level);

    init_openssl()?;

    let bind_addr = config.bind;
    // start client & login
    let ctx = ClientContext::from_config(config)?;
    log::info!("login to H@H network");
    ctx.login().await?;

    // start server
    let file = ctx.download_cert().await?;

    let server_ctx = ServerContext::new(file, &ctx).await?;
    let server = Server::new(bind_addr, server_ctx).await?;
    tokio::spawn(server.run());

    // client event loop
    ctx.notify_start().await?;
    let alive = async {
        loop {
            let _ = ctx.alive().await;
            tokio::time::sleep(Duration::from_secs(100)).await;
        }
    };
    tokio::select! {
        _ = alive => (),
        _ = tokio::signal::ctrl_c() => ()
    };

    log::info!("signal exit, shutting down...");
    ctx.shutdown().await?;
    Ok(())
}

/// Load OpenSSL legacy and default providers.
fn init_openssl() -> Result<(), ErrorStack> {
    use openssl::provider::Provider;
    use std::mem::forget;
    forget(Provider::load(None, "legacy")?);
    forget(Provider::load(None, "default")?);
    Ok(())
}
