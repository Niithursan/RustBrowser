//! Unix-socket transport for the single-instance hand-off on Linux. The message
//! format, limits and rate limit are `rust_browser::handoff`'s, as on Windows: a
//! later launch of a normal window sends one fixed-format request (open these
//! links, or open a window) and reads one status byte back.
//!
//! Endpoint protections:
//! - the socket lives in a directory checked before every use: a real directory
//!   owned by this user with no access for group or others (`$XDG_RUNTIME_DIR`,
//!   private by specification, or else a `handoff` folder in the profile). So
//!   only this user (or root) can connect to the socket or put one there;
//! - the socket itself is owned by this user, readable and writable by it only;
//! - the browser binds it only while holding the profile lock, so a socket
//!   already there was left by a browser that ended, and is replaced;
//! - a same-user process could imitate either side, which is outside the
//!   boundary (it already has all of the user's rights), as on Windows.
use rust_browser::handoff::{self, MAX_MESSAGE, RateLimiter, Request, Status};
use std::{
    fs::{self, Permissions},
    io::{self, Read, Write},
    net::Shutdown,
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

/// Time a client may take to send its message.
const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// Time the browser's UI may take to accept a request before the client is told
/// it is not responding.
pub const UI_TIMEOUT: Duration = Duration::from_secs(5);
/// Clients served at the same time; more are told the browser is busy.
const MAX_ACTIVE: usize = 4;

/// This process's user ID (the owner of its `/proc` entry).
fn my_uid() -> Option<u32> {
    fs::metadata("/proc/self").ok().map(|m| m.uid())
}

/// Whether `dir` is a real directory (not a link) owned by this user that no one
/// else may enter.
fn private_dir(dir: &Path) -> bool {
    let me = my_uid();
    fs::symlink_metadata(dir)
        .is_ok_and(|m| m.is_dir() && Some(m.uid()) == me && m.mode() & 0o077 == 0)
}

/// Where the socket for `profile` lives, or `None` when no private directory is
/// available for it.
pub fn endpoint(profile: &Path) -> Option<PathBuf> {
    endpoint_in(
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        profile,
    )
}

/// `endpoint`, with the user's runtime folder given.
fn endpoint_in(runtime: Option<PathBuf>, profile: &Path) -> Option<PathBuf> {
    let name = handoff::socket_name(&profile.to_string_lossy());
    if let Some(runtime) = runtime.filter(|dir| dir.is_absolute() && private_dir(dir)) {
        return Some(runtime.join(name));
    }
    let dir = profile.join("handoff");
    let _ = fs::DirBuilder::new().mode(0o700).create(&dir);
    private_dir(&dir).then(|| dir.join(name))
}

/// Turns an accepted request into a status. The browser's implementation hands
/// it to the UI thread and waits at most `UI_TIMEOUT`.
pub type Dispatch = Arc<dyn Fn(Request) -> Status + Send + Sync>;

/// Listens for later launches until dropped.
pub struct Server {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Listen at `path`. The caller holds the profile lock, so anything already
    /// at `path` was left by a browser that ended without removing it.
    pub fn start(path: &Path, dispatch: Dispatch) -> io::Result<Self> {
        match fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_socket() => fs::remove_file(path)?,
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Something that is not a socket is in the way.",
                ));
            }
            Err(_) => {}
        }
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, Permissions::from_mode(0o600))?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("handoff-server".into())
            .spawn(move || {
                let limiter = Arc::new(Mutex::new(RateLimiter::default()));
                let active = Arc::new(AtomicUsize::new(0));
                for stream in listener.incoming() {
                    if flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(mut stream) = stream else {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    };
                    if active.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIVE {
                        active.fetch_sub(1, Ordering::SeqCst);
                        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                        let _ = stream.write_all(&[Status::Busy as u8]);
                        continue;
                    }
                    let (limiter, dispatch, active) =
                        (limiter.clone(), dispatch.clone(), active.clone());
                    let spawned = std::thread::Builder::new()
                        .name("handoff-client".into())
                        .spawn({
                            let active = active.clone();
                            move || {
                                serve(stream, &limiter, &dispatch);
                                active.fetch_sub(1, Ordering::SeqCst);
                            }
                        });
                    if spawned.is_err() {
                        active.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            })?;
        Ok(Self {
            path: path.to_path_buf(),
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the listener, which waits in `accept`. If the socket was removed
        // meanwhile it cannot be woken; it is then left to end with the process
        // rather than holding up the browser's exit.
        let woken = UnixStream::connect(&self.path).is_ok();
        if let Some(thread) = self.thread.take()
            && woken
        {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.path);
    }
}

/// Read one request (the client then closes its side), answer it, and close.
/// Nothing is answered to a client that does not finish sending in time.
fn serve(mut stream: UnixStream, limiter: &Mutex<RateLimiter>, dispatch: &Dispatch) {
    let Some(message) = read_message(&mut stream) else {
        return;
    };
    let status = match handoff::decode(&message) {
        Ok(request) => {
            let admitted = limiter
                .lock()
                .map(|mut l| l.admit(Instant::now()))
                .unwrap_or(false);
            if admitted {
                dispatch(request)
            } else {
                Status::Busy
            }
        }
        Err(_) => Status::Refused,
    };
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let _ = stream.write_all(&[status as u8]);
}

/// The whole message, read within `IO_TIMEOUT` in all, or `None`. A message
/// longer than any valid one is read no further and decodes as too large.
fn read_message(stream: &mut UnixStream) -> Option<Vec<u8>> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut message = Vec::new();
    let mut buffer = [0u8; 4096];
    while message.len() <= MAX_MESSAGE {
        let left = deadline.checked_duration_since(Instant::now())?;
        stream
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .ok()?;
        match stream.read(&mut buffer) {
            Ok(0) => return Some(message),
            Ok(read) => message.extend_from_slice(&buffer[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    Some(message)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The browser answered with this status.
    Answered(Status),
    /// Nothing is listening (the browser is starting, closing or hung before its
    /// endpoint existed).
    NoServer,
    /// Something at the endpoint is not this user's.
    Untrusted,
    /// The browser did not answer in time.
    NotResponding,
    Failed,
}

/// Send one request to the browser holding `profile` and return its answer.
pub fn send(profile: &Path, request: &Request) -> Outcome {
    match endpoint(profile) {
        Some(path) => send_to(&path, request),
        None => Outcome::Failed,
    }
}

fn send_to(path: &Path, request: &Request) -> Outcome {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_socket() && Some(m.uid()) == my_uid() => {}
        Ok(_) => return Outcome::Untrusted,
        Err(_) => return Outcome::NoServer,
    }
    let Ok(mut stream) = UnixStream::connect(path) else {
        return Outcome::NoServer;
    };
    let sent = stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .and_then(|()| stream.write_all(&handoff::encode(request)))
        .and_then(|()| stream.shutdown(Shutdown::Write));
    if sent.is_err() {
        return Outcome::Failed;
    }
    if stream
        .set_read_timeout(Some(UI_TIMEOUT + IO_TIMEOUT))
        .is_err()
    {
        return Outcome::Failed;
    }
    let mut status = [0u8; 1];
    match stream.read(&mut status) {
        Ok(1) => Status::from_byte(status[0]).map_or(Outcome::Failed, Outcome::Answered),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            Outcome::NotResponding
        }
        _ => Outcome::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A private directory for one test.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rust-browser-handoff-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        dir
    }

    fn open(link: &str) -> Request {
        Request::Open(vec![link.to_string()])
    }

    #[test]
    fn a_request_reaches_the_browser_and_its_answer_comes_back() {
        let dir = folder("answer");
        let path = dir.join("s.sock");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let server = Server::start(
            &path,
            Arc::new(move |request| {
                record.lock().unwrap().push(request);
                Status::Accepted
            }),
        )
        .unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(
            send_to(&path, &open("https://example.com/")),
            Outcome::Answered(Status::Accepted)
        );
        assert_eq!(
            send_to(&path, &Request::NewWindow),
            Outcome::Answered(Status::Accepted)
        );
        assert_eq!(
            *seen.lock().unwrap(),
            [open("https://example.com/"), Request::NewWindow]
        );
        drop(server);
        // The browser closed: the socket is gone and nothing answers.
        assert!(!path.exists());
        assert_eq!(send_to(&path, &Request::NewWindow), Outcome::NoServer);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_and_too_many_requests_are_refused_without_reaching_the_browser() {
        let dir = folder("refuse");
        let path = dir.join("s.sock");
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let _server = Server::start(
            &path,
            Arc::new(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Status::Accepted
            }),
        )
        .unwrap();
        // A link the command line would refuse, then a message that is not one.
        assert_eq!(
            send_to(&path, &open("file:///etc/passwd")),
            Outcome::Answered(Status::Refused)
        );
        let mut raw = UnixStream::connect(&path).unwrap();
        raw.write_all(b"not a hand-off message").unwrap();
        raw.shutdown(Shutdown::Write).unwrap();
        let mut answer = [0u8; 1];
        raw.read_exact(&mut answer).unwrap();
        assert_eq!(answer[0], Status::Refused as u8);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        // The rate limit answers Busy once it is reached.
        let answers: Vec<_> = (0..=handoff::RATE_LIMIT)
            .map(|_| send_to(&path, &Request::NewWindow))
            .collect();
        assert_eq!(calls.load(Ordering::SeqCst), handoff::RATE_LIMIT);
        assert_eq!(answers.last(), Some(&Outcome::Answered(Status::Busy)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_browser_can_stop_listening_even_after_its_socket_was_removed() {
        let dir = folder("removed");
        let path = dir.join("s.sock");
        let server = Server::start(&path, Arc::new(|_| Status::Accepted)).unwrap();
        fs::remove_file(&path).unwrap();
        let (done, stopped) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(server);
            let _ = done.send(());
        });
        assert!(stopped.recv_timeout(Duration::from_secs(5)).is_ok());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_socket_left_by_a_crash_is_replaced_and_anything_else_is_not() {
        let dir = folder("stale");
        let path = dir.join("s.sock");
        // A crashed browser's socket: bound, then its listener gone.
        drop(UnixListener::bind(&path).unwrap());
        assert_eq!(send_to(&path, &Request::NewWindow), Outcome::NoServer);
        let server = Server::start(&path, Arc::new(|_| Status::Accepted)).unwrap();
        assert_eq!(
            send_to(&path, &Request::NewWindow),
            Outcome::Answered(Status::Accepted)
        );
        drop(server);
        // A plain file in its place is neither removed nor trusted.
        fs::write(&path, b"").unwrap();
        assert!(Server::start(&path, Arc::new(|_| Status::Accepted)).is_err());
        assert!(path.is_file());
        assert_eq!(send_to(&path, &Request::NewWindow), Outcome::Untrusted);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn only_a_private_directory_holds_the_socket() {
        let dir = folder("private");
        assert!(private_dir(&dir));
        fs::set_permissions(&dir, Permissions::from_mode(0o755)).unwrap();
        assert!(!private_dir(&dir));
        let profile = folder("profile");
        // A runtime folder others may enter is not used: the profile's own is.
        let path = endpoint_in(Some(dir.clone()), &profile).unwrap();
        assert_eq!(path.parent(), Some(profile.join("handoff").as_path()));
        assert!(private_dir(&profile.join("handoff")));
        fs::set_permissions(&dir, Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            endpoint_in(Some(dir.clone()), &profile).unwrap().parent(),
            Some(dir.as_path())
        );
        // Without one, the profile's own folder is used.
        assert_eq!(
            endpoint_in(None, &profile).unwrap().parent(),
            Some(profile.join("handoff").as_path())
        );
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(profile);
    }
}
