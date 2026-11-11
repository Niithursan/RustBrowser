//! Private window leases never open normal session/library storage.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClearRequest {
    pub history: bool,
    pub cookies: bool,
    pub cache: bool,
    pub session: bool,
    pub permissions: bool,
    pub confirmed: bool,
}
impl ClearRequest {
    pub fn validate(&self) -> io::Result<()> {
        if !self.confirmed
            || !(self.history || self.cookies || self.cache || self.session || self.permissions)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Select data and explicitly confirm clearing.",
            ));
        }
        Ok(())
    }
}
pub fn persistent_snapshot(
    mut snapshot: crate::storage::Snapshot,
    private: bool,
) -> Option<crate::storage::Snapshot> {
    if private {
        return None;
    }
    if !snapshot.settings.restore_session {
        snapshot.tabs = vec![crate::navigation::HOME.into()];
        snapshot.active = 0;
        snapshot.pinned.clear();
        snapshot.titles.clear();
        snapshot.keep_awake.clear();
        snapshot.other_windows.clear();
    }
    Some(snapshot)
}
fn plain_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other(
                "Reparse directories are not private storage.",
            ));
        }
    }
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::other("Expected an ordinary directory."));
    }
    Ok(())
}
pub struct PrivateLease {
    pub path: PathBuf,
    lock: Option<File>,
}
impl PrivateLease {
    pub fn create(normal: &Path) -> io::Result<Self> {
        fs::create_dir_all(normal)?;
        let root = normal.canonicalize()?.join("private-contexts");
        fs::create_dir_all(&root)?;
        plain_directory(&root)?;
        let root = root.canonicalize()?;
        for attempt in 0..100 {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = root.join(format!(
                "private-{nonce:032x}-{:08x}-{attempt:02x}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    let lock = OpenOptions::new()
                        .create_new(true)
                        .read(true)
                        .write(true)
                        .open(path.join("lease.lock"))?;
                    lock.try_lock()?;
                    return Ok(Self {
                        path,
                        lock: Some(lock),
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::other(
            "Could not allocate isolated private storage.",
        ))
    }
    /// Only recognized immediate child directories; live leases and reparse points stay untouched.
    pub fn sweep(normal: &Path) -> io::Result<usize> {
        let root = normal.join("private-contexts");
        if !root.exists() {
            return Ok(0);
        }
        plain_directory(&root)?;
        let root = root.canonicalize()?;
        let mut removed = 0;
        for entry in fs::read_dir(&root)?.take(256) {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(rest) = name.strip_prefix("private-") else {
                continue;
            };
            if rest.len() != 44 || !rest.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                continue;
            }
            let path = entry.path();
            if plain_directory(&path).is_err()
                || path.canonicalize()?.parent() != Some(root.as_path())
            {
                continue;
            }
            let lock = match OpenOptions::new()
                .read(true)
                .write(true)
                .open(path.join("lease.lock"))
            {
                Ok(lock) => lock,
                Err(_) => continue,
            };
            if lock.try_lock().is_err() {
                continue;
            }
            // remove_dir_all does not follow symbolic links inside the owned tree.
            if fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}
impl Drop for PrivateLease {
    fn drop(&mut self) {
        // The application drops views/contexts before this lease. Runtime processes may
        // briefly retain files; retry off the UI thread, with next-launch sweep as fallback.
        let path = self.path.clone();
        let lock = self.lock.take();
        let _ = std::thread::Builder::new()
            .name("private-cleanup".into())
            .spawn(move || {
                let _lock = lock;
                for _ in 0..20 {
                    if fs::remove_dir_all(&path).is_ok() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            })
            .map(|thread| thread.join());
    }
}
/// Session recovery files contain no bookmarks. Corrupt library backups are retained
/// because they may contain the user's only surviving bookmarks; report that limitation.
pub fn clear_session_backups(profile: &Path) -> io::Result<usize> {
    let mut count = 0;
    for entry in fs::read_dir(profile)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name == "session.json.tmp"
            || (name.starts_with("session-invalid-") && name.ends_with(".json"))
        {
            let meta = fs::symlink_metadata(entry.path())?;
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err(io::Error::other(
                    "Unexpected session recovery path; preserved.",
                ));
            }
            fs::remove_file(entry.path())?;
            count += 1;
        }
    }
    Ok(count)
}
