use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::RwLock;

use crate::client::HttpClient;
use crate::utils::Limiter;
use crate::{Config, Error};

#[derive(Default)]
pub struct MutContext {
    pub static_range: HashSet<u16>,
}

pub struct AppContext {
    // Immutable Context
    pub id: u32,
    pub key: String,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,

    // Mutable Context
    pub limiter: Limiter,
    pub mut_context: RwLock<MutContext>,

    pub client: HttpClient,
}

impl AppContext {
    pub fn from_config(mut config: Config) -> Result<AppContext, openssl::error::ErrorStack> {
        // speedlimit remap
        if config.speedlimit <= 0.0 {
            config.speedlimit = f64::INFINITY;
        }

        let limiter = Limiter::new(config.speedlimit);
        let client = HttpClient::new(limiter.clone())?;

        Ok(AppContext {
            id: config.id,
            key: config.key,
            limiter,
            cache_dir: config.cache_dir,
            data_dir: config.data_dir,
            mut_context: RwLock::new(MutContext::default()),
            client,
        })
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
                _ => log::debug!("unimplemented setting: {}: {}", key, val),
            }
        }
        Ok(())
    }
}
