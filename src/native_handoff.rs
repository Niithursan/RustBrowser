//! Named-pipe transport for the single-instance hand-off (see `rust_browser::handoff`
//! for the message format and trust rules, and SECURITY.md for the threat model).
//!
//! Endpoint protections:
//! - local only: `PIPE_REJECT_REMOTE_CLIENTS`;
//! - one user: a protected DACL that grants access only to this user's SID;
//! - no squatting: the first instance uses `FILE_FLAG_FIRST_PIPE_INSTANCE`, so the
//!   server refuses to start if anything already owns the name, and a new
//!   instance is created before each client is served so the name never lapses;
//! - the name is derived from the Windows session, user SID and profile path;
//! - both sides check the peer process: same user and session, and for the server
//!   the same executable; the client opens the pipe at identification level so a
//!   server cannot impersonate it.
use rust_browser::handoff::{self, MAX_MESSAGE, Malformed, Peer, RateLimiter, Request, Status};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_IO_PENDING, ERROR_MORE_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
            GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, LocalFree, WAIT_OBJECT_0,
        },
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                SDDL_REVISION_1,
            },
            GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
            TOKEN_USER, TokenUser,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
            FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
            ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
        },
        System::{
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe,
                GetNamedPipeClientProcessId, GetNamedPipeServerProcessId, PIPE_READMODE_MESSAGE,
                PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
                SetNamedPipeHandleState, WaitNamedPipeW,
            },
            RemoteDesktop::ProcessIdToSessionId,
            Threading::{
                CreateEventW, GetCurrentProcess, GetCurrentProcessId, OpenProcess,
                OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW, WaitForSingleObject,
            },
        },
        UI::WindowsAndMessaging::AllowSetForegroundWindow,
    },
    core::{HSTRING, PCWSTR, PWSTR},
};

/// Time a client may take to send its message, and to read the reply.
const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// Time the browser's UI may take to accept a request before the client is told it
/// is not responding.
pub const UI_TIMEOUT: Duration = Duration::from_secs(5);
/// Clients served at the same time; more are told the browser is busy.
const MAX_ACTIVE: usize = 4;

/// Owns a Win32 handle and closes it once.
struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the handle is owned by this value and closed exactly once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}
// SAFETY: kernel handles may be used from any thread.
unsafe impl Send for Owned {}

#[derive(Debug)]
enum Io {
    TimedOut,
    MoreData,
    Failed,
}

/// Run one overlapped operation on `handle` and wait at most `timeout`.
fn overlapped(
    handle: HANDLE,
    timeout: Duration,
    start: impl FnOnce(*mut OVERLAPPED) -> windows::core::Result<()>,
) -> Result<u32, Io> {
    // SAFETY: the event and OVERLAPPED outlive the operation: on timeout the
    // operation is cancelled and waited for before they are dropped.
    unsafe {
        let event = Owned(CreateEventW(None, true, false, PCWSTR::null()).map_err(|_| Io::Failed)?);
        let mut ov = OVERLAPPED {
            hEvent: event.0,
            ..Default::default()
        };
        if let Err(error) = start(&mut ov) {
            let code = error.code();
            if code == ERROR_PIPE_CONNECTED.to_hresult() {
                return Ok(0);
            }
            if code == ERROR_MORE_DATA.to_hresult() {
                return Err(Io::MoreData);
            }
            if code != ERROR_IO_PENDING.to_hresult() {
                return Err(Io::Failed);
            }
            let millis = timeout.as_millis().min(u32::MAX as u128) as u32;
            if WaitForSingleObject(event.0, millis) != WAIT_OBJECT_0 {
                let _ = CancelIoEx(handle, Some(&ov));
                let mut done = 0;
                let _ = GetOverlappedResult(handle, &ov, &mut done, true);
                return Err(Io::TimedOut);
            }
        }
        let mut done = 0;
        match GetOverlappedResult(handle, &ov, &mut done, false) {
            Ok(()) => Ok(done),
            Err(error) if error.code() == ERROR_MORE_DATA.to_hresult() => Err(Io::MoreData),
            Err(_) => Err(Io::Failed),
        }
    }
}

fn sid_string(token: HANDLE) -> Option<String> {
    // SAFETY: two-call GetTokenInformation into an owned, sufficiently large buffer;
    // the SID string is freed with LocalFree.
    unsafe {
        let mut needed = 0;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
        if needed == 0 || needed > 4096 {
            return None;
        }
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
        .ok()?;
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).ok()?;
        let value = text.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        value
    }
}

/// Identify a process by user, session and executable path.
fn identify(process: HANDLE, pid: u32) -> Option<Peer> {
    // SAFETY: queries on a handle owned by the caller; the token is closed here.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let token = Owned(token);
        let user = sid_string(token.0)?;
        let mut session = 0;
        ProcessIdToSessionId(pid, &mut session).ok()?;
        let mut path = vec![0u16; 1024];
        let mut size = path.len() as u32;
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut size,
        )
        .ok()?;
        Some(Peer {
            user,
            session,
            executable: String::from_utf16(&path[..size as usize]).ok()?,
        })
    }
}

pub fn me() -> Option<Peer> {
    // SAFETY: the pseudo handle for the current process needs no closing.
    unsafe { identify(GetCurrentProcess(), GetCurrentProcessId()) }
}

fn peer(pid: u32) -> Option<Peer> {
    // SAFETY: a query-only handle, closed by `Owned`.
    unsafe {
        let process = Owned(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        identify(process.0, pid)
    }
}

/// The endpoint for this user, session and profile.
pub fn endpoint(profile: &Path) -> Option<String> {
    let me = me()?;
    let profile = std::path::absolute(profile).ok()?;
    Some(handoff::endpoint(
        &me.user,
        me.session,
        &profile.to_string_lossy(),
    ))
}

/// Something that turns an accepted request into a status. The browser's
/// implementation forwards it to the UI thread and waits at most `UI_TIMEOUT`.
pub type Dispatch = Arc<dyn Fn(Request) -> Status + Send + Sync>;

pub struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServerError {
    /// Something else already owns the endpoint name.
    Taken,
    Failed,
}

fn create_instance(name: &HSTRING, user: &str, first: bool) -> Result<Owned, ServerError> {
    // SAFETY: the security descriptor is created from a constant-shaped SDDL string,
    // used only during CreateNamedPipeW and then freed.
    unsafe {
        // Owner and protected DACL: full access for this user only.
        let sddl = HSTRING::from(format!("O:{user}D:P(A;;GA;;;{user})"));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &sddl,
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
        .map_err(|_| ServerError::Failed)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let mut mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
        if first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let handle = CreateNamedPipeW(
            name,
            mode,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            16,
            MAX_MESSAGE as u32,
            0,
            Some(&attributes),
        );
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        if handle.is_invalid() {
            return Err(if first {
                ServerError::Taken
            } else {
                ServerError::Failed
            });
        }
        Ok(Owned(handle))
    }
}

fn reply(pipe: &Owned, status: Status) {
    let byte = [status as u8];
    // SAFETY: the buffer outlives the overlapped write (waited or cancelled).
    let _ = overlapped(pipe.0, IO_TIMEOUT, |ov| unsafe {
        WriteFile(pipe.0, Some(&byte), None, Some(ov))
    });
    // Wait for the client to read and close, so disconnecting cannot discard the reply.
    let mut sink = [0u8; 1];
    let _ = overlapped(pipe.0, IO_TIMEOUT, |ov| unsafe {
        ReadFile(pipe.0, Some(&mut sink), None, Some(ov))
    });
}

fn serve(pipe: Owned, me: &Peer, limiter: &Mutex<RateLimiter>, dispatch: &Dispatch) {
    let mut client = 0;
    // SAFETY: queries the connected pipe owned by this function.
    let identified = unsafe { GetNamedPipeClientProcessId(pipe.0, &mut client) }.is_ok();
    if !identified || !peer(client).is_some_and(|client| handoff::trusted_client(&client, me)) {
        // No reply to an unknown client.
        return;
    }
    let mut buffer = vec![0u8; MAX_MESSAGE];
    // SAFETY: the buffer outlives the overlapped read (waited or cancelled).
    let read = overlapped(pipe.0, IO_TIMEOUT, |ov| unsafe {
        ReadFile(pipe.0, Some(&mut buffer), None, Some(ov))
    });
    let status = match read {
        Ok(length) => match handoff::decode(&buffer[..length as usize]) {
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
        },
        Err(Io::MoreData) => {
            // Too large: discard the rest of the message (bounded) so the client's
            // write completes and it can read the refusal.
            let mut drained = 0;
            loop {
                // SAFETY: the buffer outlives the overlapped read (waited or cancelled).
                let more = overlapped(pipe.0, IO_TIMEOUT, |ov| unsafe {
                    ReadFile(pipe.0, Some(&mut buffer), None, Some(ov))
                });
                drained += 1;
                match more {
                    Err(Io::MoreData) if drained < 16 => continue,
                    Ok(_) => break,
                    _ => return,
                }
            }
            Status::Refused
        }
        Err(_) => return,
    };
    reply(&pipe, status);
    // SAFETY: the pipe is owned here.
    unsafe {
        let _ = DisconnectNamedPipe(pipe.0);
    }
}

impl Server {
    /// Start listening on `name`. Fails with `Taken` if anything already owns it.
    pub fn start(name: &str, dispatch: Dispatch) -> Result<Self, ServerError> {
        let me = me().ok_or(ServerError::Failed)?;
        let name = HSTRING::from(name);
        let first = create_instance(&name, &me.user, true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("handoff-server".into())
            .spawn(move || {
                let limiter = Arc::new(Mutex::new(RateLimiter::default()));
                let active = Arc::new(AtomicUsize::new(0));
                let me = Arc::new(me);
                let mut current = first;
                while !flag.load(Ordering::SeqCst) {
                    // SAFETY: waits for a client on the instance owned by `current`.
                    let connected =
                        overlapped(current.0, Duration::from_millis(250), |ov| unsafe {
                            ConnectNamedPipe(current.0, Some(ov))
                        });
                    match connected {
                        Ok(_) => {}
                        Err(Io::TimedOut) => continue,
                        Err(_) => {
                            std::thread::sleep(Duration::from_millis(100));
                            continue;
                        }
                    }
                    // Keep the name owned before serving this client.
                    let Ok(next) = create_instance(&name, &me.user, false) else {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    };
                    let pipe = std::mem::replace(&mut current, next);
                    if active.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIVE {
                        active.fetch_sub(1, Ordering::SeqCst);
                        reply(&pipe, Status::Busy);
                        continue;
                    }
                    let (me, limiter, dispatch, active) = (
                        me.clone(),
                        limiter.clone(),
                        dispatch.clone(),
                        active.clone(),
                    );
                    let _ = std::thread::Builder::new()
                        .name("handoff-client".into())
                        .spawn(move || {
                            serve(pipe, &me, &limiter, &dispatch);
                            active.fetch_sub(1, Ordering::SeqCst);
                        });
                }
            })
            .map_err(|_| ServerError::Failed)?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The browser answered with this status.
    Answered(Status),
    /// Nothing is listening (the browser is starting, closing or hung before its
    /// endpoint existed).
    NoServer,
    /// Something answered that is not this user's Rust Browser.
    Untrusted,
    /// The browser did not answer in time.
    NotResponding,
    Failed,
}

/// Send one request to the browser on `name` and return its answer.
pub fn send(name: &str, request: &Request) -> Outcome {
    let Some(me) = me() else {
        return Outcome::Failed;
    };
    let name = HSTRING::from(name);
    let message = handoff::encode(request);
    // SAFETY: handles are owned by `Owned`; buffers outlive their operations.
    unsafe {
        let open = || {
            CreateFileW(
                &name,
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED
                    | FILE_FLAGS_AND_ATTRIBUTES(
                        SECURITY_SQOS_PRESENT.0 | SECURITY_IDENTIFICATION.0,
                    ),
                None,
            )
        };
        let pipe = match open() {
            Ok(pipe) => Owned(pipe),
            Err(error) if error.code() == ERROR_PIPE_BUSY.to_hresult() => {
                if !WaitNamedPipeW(&name, 1000).as_bool() {
                    return Outcome::NotResponding;
                }
                match open() {
                    Ok(pipe) => Owned(pipe),
                    Err(_) => return Outcome::NotResponding,
                }
            }
            Err(_) => return Outcome::NoServer,
        };
        let mode = PIPE_READMODE_MESSAGE;
        if SetNamedPipeHandleState(pipe.0, Some(&mode), None, None).is_err() {
            return Outcome::Failed;
        }
        let mut server = 0;
        if GetNamedPipeServerProcessId(pipe.0, &mut server).is_err()
            || !peer(server).is_some_and(|server| handoff::trusted_server(&server, &me))
        {
            return Outcome::Untrusted;
        }
        // Let the browser bring its window forward (Windows focus rules).
        let _ = AllowSetForegroundWindow(server);
        if overlapped(pipe.0, IO_TIMEOUT, |ov| {
            WriteFile(pipe.0, Some(&message), None, Some(ov))
        })
        .is_err()
        {
            return Outcome::Failed;
        }
        let mut status = [0u8; 1];
        match overlapped(pipe.0, UI_TIMEOUT + IO_TIMEOUT, |ov| {
            ReadFile(pipe.0, Some(&mut status), None, Some(ov))
        }) {
            Ok(1) => Status::from_byte(status[0]).map_or(Outcome::Failed, Outcome::Answered),
            Err(Io::TimedOut) => Outcome::NotResponding,
            _ => Outcome::Failed,
        }
    }
}

/// Decode problems are reported only as `Refused`; kept for diagnostics in tests.
#[allow(dead_code)]
pub fn explain(problem: &Malformed) -> String {
    format!("{problem:?}")
}

#[cfg(test)]
#[path = "native_handoff_tests.rs"]
mod tests;
