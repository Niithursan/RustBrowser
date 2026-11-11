//! Two real browser processes on one disposable profile. Opens real windows, so it
//! is a native suite: run it on its own with `-- --ignored --test-threads=1`.
#![cfg(windows)]
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
    },
};

const EXE: &str = env!("CARGO_BIN_EXE_rust-browser");

struct Profile(PathBuf);
impl Profile {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "rust-browser-single-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        // WebView2 may still be releasing files for a moment after exit.
        for _ in 0..20 {
            if std::fs::remove_dir_all(&self.0).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

fn launch(profile: &Path, args: &[&str]) -> Child {
    Command::new(EXE)
        .args(args)
        .env("RUST_BROWSER_PROFILE", profile)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn finish(mut child: Child, limit: Duration) -> (i32, String) {
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Process did not finish within {limit:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
    (status.code().unwrap_or(-1), stderr)
}

/// Visible top-level windows of `pid`.
fn windows_of(pid: u32) -> Vec<HWND> {
    struct Search {
        pid: u32,
        found: Vec<HWND>,
    }
    unsafe extern "system" fn each(window: HWND, data: LPARAM) -> windows::core::BOOL {
        // SAFETY: `data` is the `Search` passed below and outlives EnumWindows.
        let search = unsafe { &mut *(data.0 as *mut Search) };
        let mut owner = 0;
        // SAFETY: plain queries on a window handle supplied by EnumWindows.
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut owner));
            if owner == search.pid && IsWindowVisible(window).as_bool() {
                search.found.push(window);
            }
        }
        true.into()
    }
    let mut search = Search {
        pid,
        found: Vec::new(),
    };
    // SAFETY: the callback only touches `search` during this call.
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

/// A browser started by a test; stopped if the test fails before closing it.
struct Browser(Option<Child>);
impl Browser {
    fn id(&self) -> u32 {
        self.0.as_ref().map_or(0, Child::id)
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn close(mut browser: Browser) {
    let child = browser.0.take().unwrap();
    let pid = child.id();
    for window in windows_of(pid) {
        // SAFETY: asks this test's own browser window to close normally.
        unsafe {
            let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
    let (code, stderr) = finish(child, Duration::from_secs(20));
    assert_eq!(code, 0, "Browser did not exit cleanly: {stderr}");
}

fn saved_tabs(profile: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(profile.join("session.json")).unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    value["tabs"]
        .as_array()
        .map(|tabs| {
            tabs.iter()
                .filter_map(|t| t.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn wait_for(limit: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// Launch `args` until the running browser accepts it (it may still be starting).
fn hand_over(profile: &Path, args: &[&str]) {
    let started = Instant::now();
    loop {
        let (code, stderr) = finish(launch(profile, args), Duration::from_secs(20));
        if code == 0 {
            return;
        }
        assert_eq!(code, 3, "Unexpected exit {code}: {stderr}");
        assert!(stderr.contains("No second window was opened"), "{stderr}");
        assert!(
            started.elapsed() < Duration::from_secs(45),
            "Hand-off never accepted: {stderr}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[test]
#[ignore = "Opens real browser windows; run as a native suite"]
fn native_second_launch_hands_links_to_the_running_browser() {
    let profile = Profile::new("handoff");
    let first = Browser(Some(launch(&profile.0, &[])));
    assert!(
        wait_for(Duration::from_secs(30), || !windows_of(first.id())
            .is_empty()),
        "First window did not appear"
    );
    let link = "http://127.0.0.1:9/handed-over";
    hand_over(&profile.0, &["--single-argument", link]);
    hand_over(&profile.0, &[]);
    // A refused link never reaches the running browser.
    let (code, stderr) = finish(
        launch(&profile.0, &["--single-argument", "javascript:alert(1)"]),
        Duration::from_secs(20),
    );
    assert_eq!(code, 2, "{stderr}");
    assert!(
        wait_for(Duration::from_secs(10), || saved_tabs(&profile.0)
            .iter()
            .any(|t| t == link)),
        "Handed-over link was not opened: {:?}",
        saved_tabs(&profile.0)
    );
    close(first);
    // Start tab, the handed-over link, and the tab the new-window request opened
    // (this version opens a tab for it). Nothing from the refused launch.
    assert_eq!(
        saved_tabs(&profile.0),
        vec!["rust-browser:new-tab", link, "rust-browser:new-tab"]
    );
}

#[test]
#[ignore = "Opens real browser windows; run as a native suite"]
fn native_launch_after_a_crash_takes_over_the_profile() {
    let profile = Profile::new("crash");
    let mut first = Browser(Some(launch(&profile.0, &[])));
    assert!(
        wait_for(Duration::from_secs(30), || !windows_of(first.id())
            .is_empty()),
        "First window did not appear"
    );
    assert!(wait_for(Duration::from_secs(10), || profile
        .0
        .join("session.json")
        .exists()));
    // An abrupt end: the lock must not outlive the process.
    let mut crashed = first.0.take().unwrap();
    crashed.kill().unwrap();
    let _ = crashed.wait();
    let link = "http://127.0.0.1:9/after-crash";
    let second = Browser(Some(launch(&profile.0, &["--single-argument", link])));
    assert!(
        wait_for(Duration::from_secs(30), || !windows_of(second.id())
            .is_empty()),
        "The next launch did not take over the profile"
    );
    assert!(
        wait_for(Duration::from_secs(10), || saved_tabs(&profile.0)
            .iter()
            .any(|t| t == link)),
        "{:?}",
        saved_tabs(&profile.0)
    );
    close(second);
}
