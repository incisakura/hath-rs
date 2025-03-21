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
    /// Local speedlimit override
    speedlimit: Option<u32>,

    // Mutable Context
    pub limiter: Limiter,
    pub mut_context: RwLock<MutContext>,

    pub client: HttpClient,
}

impl AppContext {
    pub fn from_config(config: Config) -> Result<AppContext, openssl::error::ErrorStack> {
        let limiter = match config.speedlimit {
            Some(n) if n > 0 => Limiter::new((n * 1024) as f64),
            _ => Limiter::new(f64::INFINITY),
        };
        let client = HttpClient::new(limiter.clone())?;
        let mut_context = RwLock::new(MutContext::default());

        Ok(AppContext {
            id: config.id,
            key: config.key,
            cache_dir: config.cache_dir,
            data_dir: config.data_dir,
            speedlimit: config.speedlimit,
            limiter,
            mut_context,
            client,
        })
    }

    pub fn in_static_range(&self, range: u16) -> bool {
        let guard = self.mut_context.read().unwrap();

        guard.static_range.contains(&range)
    }

    pub fn update(&self, vec: Vec<String>) -> Result<(), Error> {
        let mut speedlimit = 0f64; // unit: KiB/s

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
                "disable_bwm" => {
                    if self.speedlimit.is_none() && val == "true" {
                        speedlimit = f64::INFINITY;
                    }
                }
                "throttle_bytes" => {
                    if self.speedlimit.is_none() && speedlimit == 0f64 {
                        speedlimit = val.parse::<u32>()? as f64;
                    }
                }
                "use_less_memory" => {}
                "disable_logging" => {}
                _ => log::debug!("unimplemented setting: {}: {}", key, val),
            }
        }
        drop(guard);

        if speedlimit != 0f64 {
            self.limiter.set_limit(speedlimit * 1024.0);
        }
        Ok(())
    }
}
