//! Hostile command lines given to the real executable. Each must be refused quickly,
//! with a clear message, before any window or profile session is created.
#![cfg(windows)]
use std::{
    ffi::OsString,
    os::windows::{ffi::OsStringExt, process::CommandExt},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Profile(PathBuf);
impl Profile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rust-browser-cli-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run the browser with `raw` appended verbatim to its command line.
fn refused(raw: &OsString) -> (i32, String) {
    let profile = Profile::new();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rust-browser"))
        .raw_arg(raw)
        .env("RUST_BROWSER_PROFILE", &profile.0)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            panic!("The browser started instead of refusing {raw:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
    assert!(
        !profile.0.join("session.json").exists(),
        "A session was written for {raw:?}"
    );
    (status.code().unwrap_or(-1), stderr)
}

fn assert_refused(raw: OsString, reason: &str) {
    let (code, stderr) = refused(&raw);
    assert_ne!(code, 101, "Panicked on {raw:?}: {stderr}");
    assert_eq!(code, 2, "Unexpected exit code for {raw:?}: {stderr}");
    assert!(
        stderr.contains(reason),
        "Missing reason {reason:?} for {raw:?}: {stderr}"
    );
}

#[test]
fn invalid_utf16_is_refused_without_a_panic() {
    let mut raw = OsString::from("--single-argument https://example.com/");
    raw.push(OsString::from_wide(&[0xD800]));
    assert_refused(raw, "not valid text");
    let mut raw = OsString::from_wide(&[0xDC00]);
    raw.push(" https://example.com/");
    assert_refused(raw, "not valid text");
}

#[test]
fn unknown_options_and_unsafe_links_are_refused() {
    assert_refused("--disable-web-security".into(), "unknown option");
    assert_refused(
        "--single-argument javascript:alert(1)".into(),
        "http:// or https://",
    );
    assert_refused(
        "--single-argument file:///C:/Windows/win.ini".into(),
        "http:// or https://",
    );
    assert_refused(
        "--single-argument https://user:secret@example.com/".into(),
        "usernames or passwords",
    );
    assert_refused("example.com".into(), "http:// or https://");
}
