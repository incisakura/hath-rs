use std::io;
use std::time::Duration;

use openssl::sha::Sha1;
use tokio::io::AsyncReadExt;

pub mod body;
pub use self::body::BoxBody;

pub mod limiter;
pub use self::limiter::{LimitedStream, Limiter};

pub fn hex_to_u8(h0: u8, h1: u8) -> Option<u8> {
    let n0 = match h0 {
        b'0'..=b'9' => h0 - b'0',
        b'a'..=b'f' => h0 - b'a' + 0x0a,
        _ => return None,
    };
    let n1 = match h1 {
        b'0'..=b'9' => h1 - b'0',
        b'a'..=b'f' => h1 - b'a' + 0x0a,
        _ => return None,
    };

    Some(n0 << 4 | n1)
}

/// # Return
/// A `[u8; 2]` which is valid UTF-8
pub fn u8_to_hex(n: u8) -> [u8; 2] {
    let n0 = n >> 4;
    let n1 = n << 4 >> 4;

    let c0 = match n0 {
        0x00..=0x09 => n0 + b'0',
        0x0a..=0x0f => n0 + b'a' - 0x0a,
        _ => unreachable!(),
    };
    let c1 = match n1 {
        0x00..=0x09 => n1 + b'0',
        0x0a..=0x0f => n1 + b'a' - 0x0a,
        _ => unreachable!(),
    };
    [c0, c1]
}

/// The size of `String` would always be `2 * N`
pub fn slice_to_hex<const N: usize>(slice: &[u8; N]) -> String {
    let mut str = vec![0; N * 2];
    for (buf, n) in str.chunks_exact_mut(2).zip(slice) {
        let src = u8_to_hex(*n);
        let dest: &mut [u8; 2] = buf.try_into().unwrap();
        *dest = src;
    }

    // SAFETY: All bytes is valid UTF-8
    unsafe { String::from_utf8_unchecked(str) }
}

/// Return a hexadecimal sha1 digest of hyphen joined `data`.
pub fn sha1_digest(data: &[&str]) -> String {
    let mut hasher = Sha1::new();
    let mut iter = data.iter();
    
    // ref: Iterator::intersperse

    // first element
    if let Some(item) = iter.next() {
        hasher.update(item.as_bytes());
    }

    // any rest element
    for item in data {
        hasher.update(b"-");
        hasher.update(item.as_bytes());
    }

    let digest = hasher.finish();
    slice_to_hex(&digest)
}

pub async fn file_sha1(file: &mut tokio::fs::File) -> io::Result<String> {
    let mut buf = vec![0; 4096];
    let mut hasher = Sha1::new();
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finish();

    Ok(slice_to_hex(&digest))
}


// todo: u64 or string ?
pub fn unix_time() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now();
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    duration.as_secs()
}
