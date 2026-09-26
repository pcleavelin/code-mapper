//! Little-endian writing and reading for the index cache, the hash that file text and anchored
//! slices are keyed by, and a write that never leaves a file half written, which the cache and
//! the map's files both use.

use std::path::Path;

/// FNV-1a: what a file's text and an anchored slice are keyed by.
pub fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn w_str(b: &mut Vec<u8>, s: &str) {
    w_u32(b, s.len() as u32);
    b.extend_from_slice(s.as_bytes());
}

pub fn w_u32(b: &mut Vec<u8>, n: u32) {
    b.extend_from_slice(&n.to_le_bytes());
}

pub struct Reader<'a> {
    pub data: &'a [u8],
    pub off: usize,
}

impl Reader<'_> {
    pub fn bytes(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.data.get(self.off..self.off + n)?;
        self.off += n;
        Some(s)
    }
    pub fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }
    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.bytes(8)?.try_into().ok()?))
    }
    pub fn str(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        Some(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }
}

/// Write `data` whole: to a temporary file beside `path`, then renamed over it, so a reader
/// or a second writer never sees a half-written file. Retries while Windows refuses the file
/// for a moment (errors 1224, 32 and 5, while another process or a scanner has it open).
pub fn write_retry(path: &Path, data: &[u8]) -> std::io::Result<()> {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let tmp = path.with_extension(format!(
        "tmp{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut last = None;
    for _ in 0..6 {
        match std::fs::write(&tmp, data).and_then(|()| std::fs::rename(&tmp, path)) {
            Ok(()) => return Ok(()),
            Err(e) if matches!(e.raw_os_error(), Some(1224) | Some(32) | Some(5)) => {
                std::thread::sleep(std::time::Duration::from_millis(40));
                last = Some(e);
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(last.unwrap())
}
