use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

static TEMPORARY_COUNT: AtomicU32 = AtomicU32::new(0);

const ATTEMPTS: Attempts = Attempts(6);

const RETRY_PAUSE: Duration = Duration::from_millis(40);

#[derive(Clone, Copy, Debug)]
struct Attempts(u32);

impl Attempts {
    fn range(self) -> impl Iterator<Item = u32> {
        0..self.0
    }
}

#[derive(Clone, Copy, Debug)]
struct SystemCode(i32);

impl SystemCode {
    const LOCKED_BY_USER_MAPPING: Self = Self(1224);
    const SHARING_VIOLATION: Self = Self(32);
    const ACCESS_DENIED: Self = Self(5);

    fn of(error: &io::Error) -> Option<Self> {
        error.raw_os_error().map(Self)
    }

    fn transient(self) -> bool {
        [
            Self::LOCKED_BY_USER_MAPPING,
            Self::SHARING_VIOLATION,
            Self::ACCESS_DENIED,
        ]
        .iter()
        .any(|code| code.0 == self.0)
    }
}

fn temporary_beside(path: &Path) -> PathBuf {
    let count = TEMPORARY_COUNT.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("tmp{}-{count}", process::id()))
}

#[expect(
    clippy::disallowed_methods,
    reason = "io-store is the one place that writes and removes files"
)]
pub fn write<Contents: AsRef<[u8]>>(path: &Path, contents: Contents) -> io::Result<()> {
    let temporary = temporary_beside(path);
    let mut last = None;
    for _ in ATTEMPTS.range() {
        match fs::write(&temporary, contents.as_ref()).and_then(|()| fs::rename(&temporary, path)) {
            Ok(()) => return Ok(()),
            Err(error) if SystemCode::of(&error).is_some_and(SystemCode::transient) => {
                thread::sleep(RETRY_PAUSE);
                last = Some(error);
            }
            Err(error) => {
                drop(fs::remove_file(&temporary));
                return Err(error);
            }
        }
    }
    drop(fs::remove_file(&temporary));
    Err(last.unwrap_or_else(|| io::Error::other("no write attempt was made")))
}

#[expect(
    clippy::disallowed_methods,
    reason = "io-store is the one place that writes and removes files"
)]
pub fn remove(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}

#[cfg(test)]
mod tests;
