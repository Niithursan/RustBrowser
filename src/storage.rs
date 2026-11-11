//! A bounded, coalescing background writer. Session files contain browsing URLs:
//! keep them inside the user's profile, never log their contents or commit them.
use crate::{navigation, settings::Settings, tabs::MAX_TABS};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Snapshot {
    pub version: u32,
    pub tabs: Vec<String>,
    pub active: usize,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub pinned: Vec<bool>,
    #[serde(default)]
    pub titles: Vec<String>,
    #[serde(default)]
    pub keep_awake: Vec<bool>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            version: 1,
            tabs: vec![navigation::HOME.into()],
            active: 0,
            settings: Settings::default(),
            pinned: Vec::new(),
            titles: Vec::new(),
            keep_awake: Vec::new(),
        }
    }
}

impl Snapshot {
    pub fn validate(&self) -> io::Result<()> {
        if self.version != 1
            || self.tabs.is_empty()
            || self.tabs.len() > MAX_TABS
            || self.active >= self.tabs.len()
            || self.tabs.iter().any(|url| {
                url != navigation::HOME
                    && (url.len() > 8192 || navigation::address(url).as_deref() != Ok(url.as_str()))
            })
            || (!self.pinned.is_empty()
                && (self.pinned.len() != self.tabs.len()
                    || self.pinned.windows(2).any(|pair| !pair[0] && pair[1])))
            || (!self.keep_awake.is_empty() && self.keep_awake.len() != self.tabs.len())
            || (!self.titles.is_empty()
                && (self.titles.len() != self.tabs.len()
                    || self.titles.iter().any(|title| {
                        title.chars().count() > 120 || title.chars().any(char::is_control)
                    })))
            || self.settings.validate().is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid or unsupported session data",
            ));
        }
        Ok(())
    }
}

pub fn load(path: &Path) -> io::Result<Snapshot> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Snapshot::default()),
        Err(error) => return Err(error),
    };
    if file.metadata()?.len() > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Session file is too large",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Session file is too large",
        ));
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid session JSON"))?;
    snapshot.validate()?;
    Ok(snapshot)
}

pub fn save(path: &Path, snapshot: &Snapshot) -> io::Result<()> {
    let sanitized;
    let snapshot = if snapshot.settings.restore_session {
        snapshot
    } else {
        sanitized = Snapshot {
            version: snapshot.version,
            settings: snapshot.settings.clone(),
            ..Snapshot::default()
        };
        &sanitized
    };
    snapshot.validate()?;
    let bytes = serde_json::to_vec_pretty(snapshot)?;
    let temp = path.with_extension("json.tmp");
    let mut file = File::create(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    // std::fs::rename replaces the destination atomically on Windows (MoveFileExW).
    fs::rename(&temp, path)
}

pub struct Store {
    path: PathBuf,
    write_gate: Arc<Mutex<()>>,
    latest: Arc<Mutex<Option<Snapshot>>>,
    stop: Arc<AtomicBool>,
    wake: SyncSender<()>,
    worker: Option<JoinHandle<()>>,
    _lock: File,
}

impl Store {
    pub fn open(
        profile: &Path,
        on_error: impl Fn() + Send + 'static,
    ) -> io::Result<(Self, Snapshot, bool)> {
        let lock = lock(profile)?;
        Self::open_locked(profile, lock, on_error)
    }

    /// Open with a profile lock already taken by `lock` (before the event loop).
    pub fn open_locked(
        profile: &Path,
        lock: File,
        on_error: impl Fn() + Send + 'static,
    ) -> io::Result<(Self, Snapshot, bool)> {
        let path = profile.join("session.json");
        let temporary = path.with_extension("json.tmp");
        let missing = !path.try_exists()?;
        let (snapshot, recovered) = match load(&path) {
            Ok(_) if missing && temporary.try_exists()? => match load(&temporary) {
                Ok(snapshot) => {
                    fs::rename(&temporary, &path)?;
                    (snapshot, true)
                }
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    preserve_invalid(&temporary, profile)?;
                    (Snapshot::default(), true)
                }
                Err(error) => return Err(error),
            },
            Ok(snapshot) => (snapshot, false),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                // Preserve invalid data for recovery; do not silently overwrite it.
                preserve_invalid(&path, profile)?;
                if temporary.try_exists()? {
                    match load(&temporary) {
                        Ok(snapshot) => {
                            fs::rename(&temporary, &path)?;
                            (snapshot, true)
                        }
                        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                            preserve_invalid(&temporary, profile)?;
                            (Snapshot::default(), true)
                        }
                        Err(error) => return Err(error),
                    }
                } else {
                    (Snapshot::default(), true)
                }
            }
            Err(error) => return Err(error),
        };
        let latest = Arc::new(Mutex::new(None::<Snapshot>));
        let stored_path = path.clone();
        let write_gate = Arc::new(Mutex::new(()));
        let worker_gate = write_gate.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let (wake, receiver) = mpsc::sync_channel(1);
        let pending = latest.clone();
        let stopping = stop.clone();
        let worker = thread::Builder::new()
            .name("session-writer".into())
            .spawn(move || {
                let write_pending = || {
                    // Serializing disk writes here does not block ordinary queue updates.
                    if let Ok(_guard) = worker_gate.lock()
                        && let Some(snapshot) =
                            pending.lock().ok().and_then(|mut latest| latest.take())
                        && save(&path, &snapshot).is_err()
                    {
                        on_error();
                    }
                };
                while receiver.recv().is_ok() {
                    if !stopping.load(Ordering::Acquire) {
                        thread::sleep(Duration::from_millis(350));
                    }
                    write_pending();
                    if stopping.load(Ordering::Acquire) {
                        // A newer queue can arrive during the preceding write. Flush it too.
                        write_pending();
                        break;
                    }
                }
            })?;
        Ok((
            Self {
                path: stored_path,
                write_gate,
                latest,
                stop,
                wake,
                worker: Some(worker),
                _lock: lock,
            },
            snapshot,
            recovered,
        ))
    }

    pub fn queue(&self, snapshot: Snapshot) {
        if let Ok(mut latest) = self.latest.lock() {
            *latest = Some(snapshot);
        }
        let _ = self.wake.try_send(());
    }

    /// Used after session retention has been disabled in application state. An old
    /// queued write cannot restore URLs after this operation returns successfully.
    pub fn forget_session(&self, settings: Settings) -> io::Result<()> {
        if settings.restore_session {
            return Err(io::Error::other(
                "Disable session retention before clearing.",
            ));
        }
        let _guard = self
            .write_gate
            .lock()
            .map_err(|_| io::Error::other("Session writer unavailable."))?;
        *self
            .latest
            .lock()
            .map_err(|_| io::Error::other("Session writer unavailable."))? = None;
        save(
            &self.path,
            &Snapshot {
                settings,
                ..Snapshot::default()
            },
        )
    }
}

fn preserve_invalid(path: &Path, profile: &Path) -> io::Result<()> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let backup = profile.join(format!("session-invalid-{nonce}.json"));
    // Reserve a previously unused name; replace only our own empty reservation.
    // This also works on filesystems that do not implement hard links.
    drop(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)?,
    );
    if let Err(error) = fs::rename(path, &backup) {
        let _ = fs::remove_file(&backup);
        return Err(error);
    }
    Ok(())
}

impl Drop for Store {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.wake.try_send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Take the profile's exclusive lock. `WouldBlock` means a normal window of this
/// profile is running. The operating system releases the lock when its holder
/// exits for any reason, so a crash cannot leave a stale lock behind.
pub fn lock(profile: &Path) -> io::Result<File> {
    fs::create_dir_all(profile)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(profile.join("session.lock"))?;
    lock.try_lock().map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "This profile is already open. Close the other browser or use RUST_BROWSER_PROFILE.",
        )
    })?;
    Ok(lock)
}

/// One value from `installation.ini` next to the running executable, which Setup
/// writes; a portable copy has none.
pub fn installation_value(key: &str) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let text = fs::read_to_string(exe.parent()?.join("installation.ini")).ok()?;
    if text.len() > 16 * 1024 {
        return None;
    }
    let mut section = "";
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = line;
        } else if section == "[Installation]"
            && let Some((name, value)) = line.split_once('=')
            && name.trim() == key
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// A validation install (never the production identity) records its own
/// disposable profile, because links Windows starts through its registration get
/// no environment.
fn validation_profile() -> Option<PathBuf> {
    let identity = installation_value("Identity")?;
    if !identity.starts_with("RustBrowser.Validation.") {
        return None;
    }
    let profile = PathBuf::from(installation_value("Profile")?);
    profile.is_absolute().then_some(profile)
}

pub fn profile_dir() -> io::Result<PathBuf> {
    if let Some(value) = std::env::var_os("RUST_BROWSER_PROFILE") {
        return Ok(value.into());
    }
    if let Some(profile) = validation_profile() {
        return Ok(profile);
    }
    // Keep the v0.1 location so upgrading preserves existing cookies and site storage.
    Ok(PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?,
    )
    .join("RustBrowser")
    .join("v0.1"))
}
