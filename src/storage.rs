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
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            version: 1,
            tabs: vec![navigation::HOME.into()],
            active: 0,
            settings: Settings::default(),
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
                url.len() > 8192 || navigation::address(url).as_deref() != Ok(url.as_str())
            })
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
    let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
    snapshot.validate()?;
    Ok(snapshot)
}

pub fn save(path: &Path, snapshot: &Snapshot) -> io::Result<()> {
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
        fs::create_dir_all(profile)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(profile.join("session.lock"))?;
        lock.try_lock().map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "This profile is already open. Close the other browser or use RUST_BROWSER_PROFILE."))?;
        let path = profile.join("session.json");
        let (snapshot, recovered) = match load(&path) {
            Ok(snapshot) => (snapshot, false),
            Err(_) => {
                // Preserve invalid data for recovery; do not silently overwrite it.
                let backup = profile.join(format!(
                    "session-invalid-{}.json",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                ));
                fs::rename(&path, backup)?;
                (Snapshot::default(), true)
            }
        };
        let latest = Arc::new(Mutex::new(None::<Snapshot>));
        let stop = Arc::new(AtomicBool::new(false));
        let (wake, receiver) = mpsc::sync_channel(1);
        let pending = latest.clone();
        let stopping = stop.clone();
        let worker = thread::Builder::new()
            .name("session-writer".into())
            .spawn(move || {
                while receiver.recv().is_ok() {
                    if !stopping.load(Ordering::Acquire) {
                        thread::sleep(Duration::from_millis(350));
                    }
                    let snapshot = pending.lock().ok().and_then(|mut latest| latest.take());
                    if let Some(snapshot) = snapshot
                        && save(&path, &snapshot).is_err()
                    {
                        on_error();
                    }
                    if stopping.load(Ordering::Acquire) {
                        // A newer queue can arrive during the preceding write. Flush it too.
                        let final_snapshot =
                            pending.lock().ok().and_then(|mut latest| latest.take());
                        if let Some(snapshot) = final_snapshot
                            && save(&path, &snapshot).is_err()
                        {
                            on_error();
                        }
                        break;
                    }
                }
            })?;
        Ok((
            Self {
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

pub fn profile_dir() -> io::Result<PathBuf> {
    if let Some(value) = std::env::var_os("RUST_BROWSER_PROFILE") {
        return Ok(value.into());
    }
    // Keep the v0.1 location so upgrading preserves existing cookies and site storage.
    Ok(PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?,
    )
    .join("RustBrowser")
    .join("v0.1"))
}
