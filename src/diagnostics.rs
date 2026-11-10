//! Local application events only. This API deliberately accepts no free-form text.
//! The application profile lock must be held while a persistent recorder is alive.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const MAX_ENTRIES: usize = 128;
pub const MAX_BYTES: u64 = 32768;
const MARKER: &[u8] = b"Rust Browser local diagnostics normal run v1\n";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    Starting,
    Started,
    CleanShutdown,
    PreviousExitUnclean,
    StartupFailed,
    NavigationFailed,
    RendererFailed,
    EngineRestarted,
    SessionSaveFailed,
    LibraryFailed,
    EngineClearFailed,
    TabResumeFailed,
    DownloadFailed,
    DownloadStopped,
    CommandFailed,
    ToolbarFailed,
    Cleared,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub first: u64,
    pub last: u64,
    pub code: Code,
    pub count: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    entries: Vec<Entry>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub enabled: bool,
    pub storage_ok: bool,
    pub previous_exit_unclean: bool,
    pub dropped_events: u64,
    pub entries: Vec<Entry>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn ordinary(path: &Path, directory: bool) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("Diagnostic reparse point refused"));
        }
    }
    if metadata.file_type().is_symlink() || metadata.is_dir() != directory {
        return Err(io::Error::other("Unexpected diagnostic object"));
    }
    Ok(true)
}

fn bounded_read(path: &Path) -> io::Result<Vec<u8>> {
    if !ordinary(path, false)? {
        return Ok(Vec::new());
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::other("Diagnostic size limit"));
    }
    Ok(bytes)
}

struct Journal {
    root: PathBuf,
    _lock: File,
}

impl Journal {
    fn open(profile: &Path) -> io::Result<Self> {
        let root = profile.join("local-diagnostics");
        if !ordinary(&root, true)? {
            fs::create_dir(&root)?;
        }
        let lock_path = root.join("journal.lock");
        ordinary(&lock_path, false)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.try_lock()?;
        Ok(Self { root, _lock: lock })
    }

    fn load(&self, report: &mut Report) -> io::Result<()> {
        let bytes = bounded_read(&self.root.join("journal.json"))?;
        if !bytes.is_empty() {
            let document: Document = serde_json::from_slice(&bytes)
                .map_err(|_| io::Error::other("Invalid diagnostic data"))?;
            if document.version != 1
                || document.entries.len() > MAX_ENTRIES
                || document
                    .entries
                    .iter()
                    .any(|entry| entry.count == 0 || entry.last < entry.first)
            {
                return Err(io::Error::other("Unsupported diagnostic data"));
            }
            report.entries = document.entries;
        } else if ordinary(&self.root.join("journal.json"), false)? {
            return Err(io::Error::other("Empty diagnostic data"));
        }
        let marker = self.root.join("running.marker");
        let previous = bounded_read(&marker)?;
        if !previous.is_empty() {
            if previous != MARKER {
                return Err(io::Error::other("Unknown diagnostic marker"));
            }
            report.previous_exit_unclean = true;
        } else if ordinary(&marker, false)? {
            return Err(io::Error::other("Empty diagnostic marker"));
        } else {
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(marker)?
                .write_all(MARKER)?;
        }
        Ok(())
    }

    fn reset(&self, report: &mut Report) -> io::Result<()> {
        ordinary(&self.root, true)?;
        for name in ["journal.json", "journal.tmp", "running.marker"] {
            let path = self.root.join(name);
            if ordinary(&path, false)? {
                fs::remove_file(path)?;
            }
        }
        let mut empty = Report {
            enabled: true,
            ..Report::default()
        };
        self.load(&mut empty)?;
        *report = empty;
        Ok(())
    }

    fn save(&self, report: &Report) -> io::Result<()> {
        let document = Document {
            version: 1,
            entries: report.entries.clone(),
        };
        let bytes = serde_json::to_vec(&document).map_err(io::Error::other)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(io::Error::other("Diagnostic size limit"));
        }
        let path = self.root.join("journal.json");
        let temp = self.root.join("journal.tmp");
        ordinary(&path, false)?;
        ordinary(&temp, false)?;
        let mut file = File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(temp, path)
    }

    fn clean_marker(&self) -> io::Result<()> {
        let marker = self.root.join("running.marker");
        if bounded_read(&marker)? != MARKER {
            return Err(io::Error::other("Diagnostic marker changed"));
        }
        fs::remove_file(marker)
    }
}

fn append(report: &mut Report, code: Code) {
    let timestamp = now();
    if let Some(entry) = report.entries.last_mut()
        && entry.code == code
        && timestamp.saturating_sub(entry.last) < 60
    {
        entry.last = entry.last.max(timestamp);
        entry.count = entry.count.saturating_add(1);
        return;
    }
    if report.entries.len() == MAX_ENTRIES {
        report.entries.remove(0);
    }
    report.entries.push(Entry {
        first: timestamp,
        last: timestamp,
        code,
        count: 1,
    });
}

enum Request {
    Record(Code),
    Snapshot,
    Clear,
}

pub struct Recorder {
    sender: Option<SyncSender<Request>>,
    report: Arc<Mutex<Report>>,
    dropped: Arc<AtomicU64>,
    clean: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Recorder {
    pub fn disabled() -> Self {
        Self {
            sender: None,
            report: Arc::new(Mutex::new(Report::default())),
            dropped: Arc::new(AtomicU64::new(0)),
            clean: Arc::new(AtomicBool::new(true)),
            started: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }
    /// None is the private-window path: no directory, marker or worker is opened.
    pub fn start(
        profile: Option<&Path>,
        notify: impl Fn(Report) + Send + 'static,
    ) -> io::Result<Self> {
        let report = Arc::new(Mutex::new(Report::default()));
        let dropped = Arc::new(AtomicU64::new(0));
        let clean = Arc::new(AtomicBool::new(true));
        let started = Arc::new(AtomicBool::new(false));
        let Some(profile) = profile else {
            return Ok(Self {
                sender: None,
                report,
                dropped,
                clean,
                started,
                worker: None,
            });
        };
        let profile = profile.to_path_buf();
        let (sender, receiver) = mpsc::sync_channel(32);
        let (shared, lost, clean_exit, startup_complete) = (
            report.clone(),
            dropped.clone(),
            clean.clone(),
            started.clone(),
        );
        let worker = thread::Builder::new()
            .name("local-diagnostics".into())
            .spawn(move || {
                let mut data = Report {
                    enabled: true,
                    ..Report::default()
                };
                let journal = Journal::open(&profile).ok();
                let mut writable = journal.as_ref().is_some_and(|j| j.load(&mut data).is_ok());
                data.storage_ok = writable;
                if data.previous_exit_unclean {
                    append(&mut data, Code::PreviousExitUnclean);
                }
                append(&mut data, Code::Starting);
                let publish = |data: &Report| {
                    if let Ok(mut slot) = shared.lock() {
                        *slot = data.clone();
                    }
                    notify(data.clone());
                };
                publish(&data);
                let mut dirty = true;
                let mut last_flush = Instant::now();
                loop {
                    match receiver.recv_timeout(Duration::from_secs(2)) {
                        Ok(Request::Record(code)) => {
                            append(&mut data, code);
                            dirty = true;
                        }
                        Ok(Request::Snapshot) => {
                            dirty = true;
                            publish(&data);
                        }
                        Ok(Request::Clear) => {
                            // An unreadable journal is preserved until this explicit command.
                            writable = journal.as_ref().is_some_and(|j| j.reset(&mut data).is_ok());
                            data.storage_ok = writable;
                            if data.storage_ok {
                                append(&mut data, Code::Cleared);
                            }
                            dirty = true;
                            publish(&data);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    data.dropped_events = lost.load(Ordering::Relaxed);
                    if dirty && last_flush.elapsed() >= Duration::from_secs(2) {
                        data.storage_ok =
                            writable && journal.as_ref().is_some_and(|j| j.save(&data).is_ok());
                        dirty = false;
                        last_flush = Instant::now();
                        publish(&data);
                    }
                    if let Ok(mut slot) = shared.lock() {
                        *slot = data.clone();
                    }
                }
                if clean_exit.load(Ordering::Acquire) {
                    append(
                        &mut data,
                        if startup_complete.load(Ordering::Acquire) {
                            Code::CleanShutdown
                        } else {
                            Code::StartupFailed
                        },
                    );
                }
                if writable
                    && let Some(journal) = journal
                    && journal.save(&data).is_ok()
                    && clean_exit.load(Ordering::Acquire)
                {
                    let _ = journal.clean_marker();
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            report,
            dropped,
            clean,
            started,
            worker: Some(worker),
        })
    }

    pub fn record(&self, code: Code) {
        if code == Code::Started {
            self.started.store(true, Ordering::Release);
        }
        if let Some(sender) = &self.sender
            && sender.try_send(Request::Record(code)).is_err()
        {
            let _ = self
                .dropped
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                    Some(n.saturating_add(1))
                });
        }
    }
    pub fn snapshot(&self) -> Report {
        self.report
            .lock()
            .map(|report| report.clone())
            .unwrap_or_default()
    }
    pub fn refresh(&self) -> Result<(), &'static str> {
        self.send(Request::Snapshot)
    }
    pub fn clear(&self, confirmed: bool) -> Result<(), &'static str> {
        if !confirmed {
            return Err("Confirm clearing local diagnostics first.");
        }
        self.send(Request::Clear)
    }
    fn send(&self, request: Request) -> Result<(), &'static str> {
        self.sender
            .as_ref()
            .ok_or("Local diagnostics are unavailable in this window.")?
            .try_send(request)
            .map_err(|_| "Local diagnostics are busy. Try again.")
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.clean.store(!thread::panicking(), Ordering::Release);
        let _ = self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
