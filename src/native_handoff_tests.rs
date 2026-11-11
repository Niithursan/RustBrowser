//! Transport tests for the hand-off pipe: real pipes, injected dispatch, no GUI.
//! A different Windows user cannot be created here; the user rule is covered by
//! `rust_browser::handoff` unit tests and by the DACL check below.
use super::*;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicU64;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A unique endpoint name for one test (same shape as the real one).
fn name() -> String {
    format!(
        r"\\.\pipe\RustBrowser.HandOff.test.{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    )
}

/// A server whose dispatch records requests and answers `status`.
fn server(name: &str, status: Status) -> (Server, Arc<Mutex<Vec<Request>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let dispatch: Dispatch = Arc::new(move |request| {
        record.lock().unwrap().push(request);
        status
    });
    (Server::start(name, dispatch).unwrap(), seen)
}

/// Write raw bytes as a client and return the reply byte, if any.
fn send_raw(name: &str, message: &[u8]) -> Option<u8> {
    let name = HSTRING::from(name);
    // SAFETY: test-only client; the handle is owned and buffers outlive the I/O.
    unsafe {
        let pipe = Owned(
            CreateFileW(
                &name,
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
            .ok()?,
        );
        let mode = PIPE_READMODE_MESSAGE;
        SetNamedPipeHandleState(pipe.0, Some(&mode), None, None).ok()?;
        overlapped(pipe.0, IO_TIMEOUT, |ov| {
            WriteFile(pipe.0, Some(message), None, Some(ov))
        })
        .ok()?;
        let mut status = [0u8; 1];
        match overlapped(pipe.0, Duration::from_secs(8), |ov| {
            ReadFile(pipe.0, Some(&mut status), None, Some(ov))
        }) {
            Ok(1) => Some(status[0]),
            _ => None,
        }
    }
}

fn open(link: &str) -> Request {
    Request::Open(vec![link.into()])
}

/// A pipe server in another program (PowerShell) that owns `name` first.
fn foreign_server(name: &str) -> Child {
    let short = name.trim_start_matches(r"\\.\pipe\");
    let script = format!(
        "$p = New-Object System.IO.Pipes.NamedPipeServerStream('{short}', 'InOut', 4, 'Message'); \
         [Console]::Out.WriteLine('ready'); [Console]::Out.Flush(); \
         $p.WaitForConnection(); $b = New-Object byte[] 65536; [void]$p.Read($b, 0, $b.Length); \
         $p.WriteByte(1); $p.Flush(); Start-Sleep -Seconds 2"
    );
    let mut child = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(child.stdout.take().unwrap()),
        &mut line,
    )
    .unwrap();
    assert_eq!(line.trim(), "ready");
    child
}

#[test]
fn accepted_requests_reach_the_dispatcher() {
    let name = name();
    let (_server, seen) = server(&name, Status::Accepted);
    assert_eq!(
        send(&name, &open("https://example.com/a")),
        Outcome::Answered(Status::Accepted)
    );
    assert_eq!(
        send(&name, &Request::NewWindow),
        Outcome::Answered(Status::Accepted)
    );
    assert_eq!(
        *seen.lock().unwrap(),
        vec![open("https://example.com/a"), Request::NewWindow]
    );
}

#[test]
fn malformed_messages_are_refused_before_dispatch() {
    let name = name();
    let (_server, seen) = server(&name, Status::Accepted);
    let refused = Some(Status::Refused as u8);
    let mut wrong_version = handoff::encode(&open("https://example.com/"));
    wrong_version[4] = 2;
    assert_eq!(send_raw(&name, &wrong_version), refused);
    assert_eq!(send_raw(&name, b"not a hand-off message"), refused);
    assert_eq!(
        send_raw(&name, &handoff::encode(&open("javascript:alert(1)"))),
        refused
    );
    assert_eq!(
        send_raw(&name, &handoff::encode(&open("file:///C:/Windows/win.ini"))),
        refused
    );
    let too_many = Request::Open((0..17).map(|i| format!("https://{i}.example/")).collect());
    assert_eq!(send_raw(&name, &handoff::encode(&too_many)), refused);
    // Larger than the server's buffer: refused, never truncated and accepted.
    let mut oversized = handoff::encode(&open("https://example.com/"));
    oversized.resize(MAX_MESSAGE + 4096, b'a');
    assert_eq!(send_raw(&name, &oversized), refused);
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn a_flood_is_rate_limited() {
    let name = name();
    let (_server, seen) = server(&name, Status::Accepted);
    let answers: Vec<Outcome> = (0..handoff::RATE_LIMIT + 6)
        .map(|i| send(&name, &open(&format!("https://{i}.example/"))))
        .collect();
    let count = |want: Status| {
        answers
            .iter()
            .filter(|a| **a == Outcome::Answered(want))
            .count()
    };
    assert_eq!(count(Status::Accepted), handoff::RATE_LIMIT);
    assert_eq!(count(Status::Busy), 6);
    assert_eq!(seen.lock().unwrap().len(), handoff::RATE_LIMIT);
}

#[test]
fn a_hung_browser_is_reported_not_accepted() {
    let name = name();
    let dispatch: Dispatch = Arc::new(|_| {
        std::thread::sleep(UI_TIMEOUT + IO_TIMEOUT + Duration::from_secs(2));
        Status::Accepted
    });
    let _server = Server::start(&name, dispatch).unwrap();
    let started = Instant::now();
    assert_eq!(
        send(&name, &open("https://example.com/")),
        Outcome::NotResponding
    );
    assert!(started.elapsed() < UI_TIMEOUT + IO_TIMEOUT + Duration::from_secs(2));
}

#[test]
fn missing_endpoint_is_reported() {
    assert_eq!(
        send(&name(), &open("https://example.com/")),
        Outcome::NoServer
    );
}

#[test]
fn an_endpoint_owned_by_anything_else_is_not_used() {
    let name = name();
    let (_first, _) = server(&name, Status::Accepted);
    let dispatch: Dispatch = Arc::new(|_| Status::Accepted);
    assert_eq!(
        Server::start(&name, dispatch).err(),
        Some(ServerError::Taken)
    );
    // Another program squatting on the name first.
    let other = name.clone() + ".squat";
    let mut squatter = foreign_server(&other);
    let dispatch: Dispatch = Arc::new(|_| Status::Accepted);
    assert_eq!(
        Server::start(&other, dispatch).err(),
        Some(ServerError::Taken)
    );
    let _ = squatter.kill();
    let _ = squatter.wait();
}

#[test]
fn a_fake_server_is_not_sent_the_request() {
    let name = name();
    let mut fake = foreign_server(&name);
    assert_eq!(
        send(&name, &open("https://example.com/secret")),
        Outcome::Untrusted
    );
    let _ = fake.kill();
    let _ = fake.wait();
}

#[test]
fn the_endpoint_allows_only_this_user() {
    use windows::Win32::Security::{
        Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
        },
        DACL_SECURITY_INFORMATION,
    };
    let name = name();
    let (_server, _) = server(&name, Status::Accepted);
    let me = me().unwrap();
    // SAFETY: test-only queries on an owned handle; the descriptor and string are
    // freed with LocalFree.
    unsafe {
        let pipe = Owned(
            CreateFileW(
                &HSTRING::from(&name),
                GENERIC_READ.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
            .unwrap(),
        );
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        let status = GetSecurityInfo(
            pipe.0,
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            Some(&mut descriptor),
        );
        assert!(status.is_ok());
        let mut text = PWSTR::null();
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            None,
        )
        .unwrap();
        let sddl = text.to_string().unwrap();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        // Protected DACL with exactly one allow entry, for this user.
        assert!(sddl.starts_with("D:P"), "{sddl}");
        assert_eq!(sddl.matches("(A;").count(), 1, "{sddl}");
        assert!(sddl.contains(&me.user), "{sddl}");
        for group in [";WD)", ";AU)", ";BU)", ";AN)"] {
            assert!(!sddl.contains(group), "{sddl}");
        }
    }
}
