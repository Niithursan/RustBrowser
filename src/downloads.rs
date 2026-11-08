//! Engine-independent destination handling. Never trust a server-suggested filename.
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Count forward progress only: retrying the same bytes must not reset a stalled transfer.
#[derive(Default)]
pub struct ProgressWatch {
    highest: i64,
    last_advance: std::time::Duration,
}
impl ProgressWatch {
    pub fn observe(&mut self, received: i64, elapsed: std::time::Duration) {
        if received > self.highest {
            self.highest = received;
            self.last_advance = elapsed;
        }
    }
    pub fn bytes(&self) -> i64 {
        self.highest
    }
    pub fn stalled(&self, elapsed: std::time::Duration) -> bool {
        elapsed.saturating_sub(self.last_advance) >= std::time::Duration::from_secs(120)
    }
}

pub fn safe_filename(suggested: &str) -> String {
    let name = suggested.rsplit(['/', '\\']).next().unwrap_or("");
    let name: String = name.chars().filter(|c| !c.is_control() && !matches!(c,
        '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')).take(100).collect();
    let mut name = name.trim_matches([' ', '.']).to_owned();
    if name.is_empty() {
        name = "download".into();
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL", "CLOCK$", "CONIN$", "CONOUT$"].contains(&stem.as_str())
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
    {
        name.insert(0, '_');
    }
    name
}

#[derive(Clone, Debug)]
pub struct Staged {
    pub folder: PathBuf,
    pub temporary: PathBuf,
    pub name: String,
    lease: std::sync::Arc<crate::download_recovery::Lease>,
}

impl Staged {
    pub fn create(folder: &Path, suggested: &str) -> io::Result<Self> {
        if !folder.is_absolute() || !folder.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Choose an existing absolute destination folder.",
            ));
        }
        let folder = folder.canonicalize()?;
        let name = safe_filename(suggested);
        for attempt in 0..100 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let directory = folder.join(format!(
                ".rust-browser-download-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    let lease =
                        std::sync::Arc::new(crate::download_recovery::Lease::create(&directory)?);
                    return Ok(Self {
                        folder,
                        temporary: directory.join(&name),
                        name,
                        lease,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Could not reserve a download staging folder.",
        ))
    }

    pub fn publish(&self) -> io::Result<PathBuf> {
        if !self.temporary.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Downloaded file is missing.",
            ));
        }
        let stem = Path::new(&self.name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("download");
        let extension = Path::new(&self.name).extension().and_then(|s| s.to_str());
        for suffix in 0..10_000 {
            let name = if suffix == 0 {
                self.name.clone()
            } else {
                format!(
                    "{stem} ({suffix}){}",
                    extension.map(|s| format!(".{s}")).unwrap_or_default()
                )
            };
            let target = self.folder.join(name);
            match move_without_replace(&self.temporary, &target) {
                Ok(()) => {
                    self.lease.clean_empty();
                    return Ok(target);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists || target.exists() => {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Too many filename collisions.",
        ))
    }

    /// Only call after WebView2 has reported a terminal/cancelled operation.
    pub fn clean_partial(&self) {
        let _ = fs::remove_file(&self.temporary);
        self.lease.clean_empty();
    }
}

#[cfg(windows)]
fn move_without_replace(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW},
        core::PCWSTR,
    };
    let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both null-terminated paths live for the call. REPLACE_EXISTING is
    // deliberately absent: atomic rename cannot clobber a concurrent creator.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| io::Error::from_raw_os_error(error.code().0 & 0xffff))
}

#[cfg(not(windows))]
fn move_without_replace(source: &Path, target: &Path) -> io::Result<()> {
    fs::hard_link(source, target)?;
    fs::remove_file(source)
}
