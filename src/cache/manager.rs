use std::collections::BTreeMap;
use std::fs::{DirEntry, Metadata};
use std::io;
use std::path::Path;
use std::time::SystemTime;

use crate::utils::{LruItem, LruTable};

use super::file::{CacheFile, FileHash};

impl LruItem for CacheFile {
    type Key = FileHash;

    fn key(&self) -> Self::Key {
        self.hash
    }

    fn key_ref(&self) -> &Self::Key {
        &self.hash
    }
}

pub struct CacheManager {
    max_size: u64,
    current_size: u64,
    table: LruTable<CacheFile>,
}

impl CacheManager {
    pub fn new() -> CacheManager {
        CacheManager {
            max_size: 0,
            current_size: 0,
            table: LruTable::new(),
        }
    }

    pub fn set_max_size(&mut self, max_size: u64) {
        self.max_size = max_size;
    }

    // Sychrononse
    pub fn build<P: AsRef<Path>>(&mut self, cache_dir: P) -> io::Result<()> {
        let mut sort_map: BTreeMap<SystemTime, CacheFile> = BTreeMap::new();

        for entry in dir_iter(cache_dir)? {
            for entry in dir_iter(entry.path())? {
                for (entry, file, meta) in file_iter(entry.path())? {
                    if file.info.size == meta.len() {
                        sort_map.insert(meta.accessed().unwrap(), file);
                    } else {
                        log::info!("removing file {} for broken data", entry.file_name().to_string_lossy());
                        std::fs::remove_file(entry.path())?;
                    }
                }
            }
        }

        while let Some((_, file)) = sort_map.pop_last() {
            self.current_size += file.info.size;
            self.table.push_front(file);
        }
        Ok(())
    }

    pub fn add(&mut self, cache_dir: &Path, file: CacheFile) {
        self.current_size += file.info.size;
        self.table.push_front(file);

        while self.current_size > self.max_size {
            let Some(file) = self.table.pop_back() else { break };

            let path = file.path(cache_dir);
            tokio::spawn(async move {
                if let Err(e) = tokio::fs::remove_file(path).await {
                    log::error!("unable to remove file: {}: {}", file.filename(false), e);
                }
            });
        }
    }

    pub fn update(&mut self, file: &CacheFile) {
        self.table.get(file.key_ref());
    }
}

fn dir_iter<P: AsRef<Path>>(path: P) -> io::Result<impl Iterator<Item = DirEntry>> {
    fn is_u8_hex(bytes: &[u8]) -> bool {
        bytes.len() == 2 && bytes.iter().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
    }

    let read_dir = std::fs::read_dir(path)?;
    Ok(read_dir.into_iter().filter_map(|entry| {
        let entry = entry.ok()?;
        let file_type = entry.file_type().ok()?;
        if file_type.is_dir() && is_u8_hex(entry.file_name().as_encoded_bytes()) {
            return Some(entry);
        }
        None
    }))
}

fn file_iter<P: AsRef<Path>>(path: P) -> io::Result<impl Iterator<Item = (DirEntry, CacheFile, Metadata)>> {
    let read_dir = std::fs::read_dir(path)?;
    Ok(read_dir.into_iter().filter_map(|entry| {
        let entry = entry.ok()?;
        let meta = entry.metadata().ok()?;
        if meta.is_file() {
            let path = entry.path();
            let name = path.file_name().and_then(|s| s.to_str())?;
            let file = CacheFile::from_filename(name)?;
            return Some((entry, file, meta));
        }
        None
    }))
}

unsafe impl Send for CacheManager {}
unsafe impl Sync for CacheManager {}
