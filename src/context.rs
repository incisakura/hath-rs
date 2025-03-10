use std::collections::HashSet;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::client::HttpClient;
use crate::error::Error;
use crate::utils::Limiter;
use crate::Config;

pub struct ClientContext {
    pub id: u32,
    pub key: String,
    pub limiter: Limiter,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub mut_context: RwLock<MutContext>,

    pub client: HttpClient,
}

impl ClientContext {
    pub fn from_config(mut config: Config) -> Result<Arc<ClientContext>, openssl::error::ErrorStack> {
        // speedlimit remap
        if config.speedlimit <= 0.0 {
            config.speedlimit = f64::INFINITY;
        }

        let limiter = Limiter::new(config.speedlimit);
        let client = HttpClient::new(limiter.clone())?;

        let ctx = Arc::new(ClientContext {
            id: config.id,
            key: config.key,
            limiter,
            cache_dir: config.cache_dir,
            data_dir: config.data_dir,
            mut_context: RwLock::new(MutContext::default()),
            client,
        });
        Ok(ctx)
    }

    pub fn in_static_range(&self, range: &u16) -> bool {
        let guard = self.mut_context.read().unwrap();

        guard.static_range.contains(range)
    }

    pub fn update(&self, vec: Vec<String>) -> Result<(), Error> {
        let mut guard = self.mut_context.write().unwrap();

        let iter = vec.iter().filter_map(|s| s.split_once('='));
        for (key, val) in iter {
            match key {
                "static_ranges" => {
                    guard.static_range.clear();
                    val.trim_end_matches(';').split(';').try_for_each(|x| -> Result<(), std::num::ParseIntError> {
                        let num = u16::from_str_radix(x, 16)?;
                        guard.static_range.insert(num);
                        Ok(())
                    })?;
                }
                _ => log::debug!("unimplemented setting: {}", key),
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct MutContext {
    pub static_range: HashSet<u16>,
    pub server_ip: HashSet<IpAddr>,
}
