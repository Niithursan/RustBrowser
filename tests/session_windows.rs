//! Session format 1 with several windows (v0.18, owner decision: option B).
use rust_browser::{
    navigation,
    settings::Settings,
    storage::{self, MAX_WINDOWS, Snapshot, Store, WindowSnapshot},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Profile(PathBuf);
impl Profile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rust-browser-windows-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn window(urls: &[&str], active: usize) -> WindowSnapshot {
    WindowSnapshot {
        tabs: urls.iter().map(|u| u.to_string()).collect(),
        active,
        pinned: vec![false; urls.len()],
        titles: vec!["Page".into(); urls.len()],
        keep_awake: vec![false; urls.len()],
    }
}

fn three_windows() -> Snapshot {
    Snapshot {
        tabs: vec!["https://focused.example/".into(), navigation::HOME.into()],
        active: 1,
        other_windows: vec![
            window(&["https://second.example/a", "https://second.example/b"], 0),
            window(&[navigation::HOME], 0),
        ],
        ..Snapshot::default()
    }
}

#[test]
fn a_single_window_file_is_unchanged_and_has_no_new_field() {
    let profile = Profile::new();
    let path = profile.0.join("session.json");
    let snapshot = Snapshot {
        tabs: vec!["https://example.com/".into()],
        ..Snapshot::default()
    };
    storage::save(&path, &snapshot).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("other_windows"), "{text}");
    assert_eq!(storage::load(&path).unwrap(), snapshot);
    // A file without the field (any earlier version) loads as one window.
    fs::write(
        &path,
        r#"{"version":1,"tabs":["rust-browser:new-tab"],"active":0}"#,
    )
    .unwrap();
    assert!(storage::load(&path).unwrap().other_windows.is_empty());
}

#[test]
fn several_windows_round_trip() {
    let profile = Profile::new();
    let path = profile.0.join("session.json");
    let snapshot = three_windows();
    storage::save(&path, &snapshot).unwrap();
    assert!(fs::read_to_string(&path).unwrap().contains("other_windows"));
    assert_eq!(storage::load(&path).unwrap(), snapshot);
}

#[test]
fn window_and_tab_limits() {
    let profile = Profile::new();
    let path = profile.0.join("session.json");
    let mut snapshot = Snapshot {
        other_windows: vec![window(&[navigation::HOME], 0); MAX_WINDOWS - 1],
        ..Snapshot::default()
    };
    assert!(storage::save(&path, &snapshot).is_ok());
    snapshot.other_windows.push(window(&[navigation::HOME], 0));
    assert!(storage::save(&path, &snapshot).is_err(), "nine windows");
    let too_many_tabs = window(&vec![navigation::HOME; 65], 0);
    let snapshot = Snapshot {
        other_windows: vec![too_many_tabs],
        ..Snapshot::default()
    };
    assert!(
        storage::save(&path, &snapshot).is_err(),
        "65 tabs in another window"
    );
}

#[test]
fn invalid_other_windows_are_rejected() {
    let profile = Profile::new();
    let path = profile.0.join("session.json");
    let bad = [
        window(&[], 0),
        window(&["https://example.com/"], 1),
        window(&["javascript:alert(1)"], 0),
        window(&["https://user:pw@example.com/"], 0),
        WindowSnapshot {
            pinned: vec![false, true],
            ..window(&["https://a.example/", "https://b.example/"], 0)
        },
        WindowSnapshot {
            titles: vec!["bad\u{7}".into()],
            ..window(&["https://a.example/"], 0)
        },
        WindowSnapshot {
            keep_awake: vec![true, false, true],
            ..window(&["https://a.example/"], 0)
        },
    ];
    for other in bad {
        let snapshot = Snapshot {
            other_windows: vec![other.clone()],
            ..Snapshot::default()
        };
        assert!(storage::save(&path, &snapshot).is_err(), "{other:?}");
    }
}

#[test]
fn an_invalid_file_on_disk_is_preserved_and_not_used() {
    let profile = Profile::new();
    let path = profile.0.join("session.json");
    // Valid main window, invalid other window: the whole file is refused, kept
    // for recovery, and the browser starts from a default session (as before).
    fs::write(
        &path,
        r#"{"version":1,"tabs":["https://example.com/"],"active":0,
            "other_windows":[{"tabs":["javascript:alert(1)"],"active":0}]}"#,
    )
    .unwrap();
    let (store, snapshot, recovered) = Store::open(&profile.0, || {}).unwrap();
    drop(store);
    assert!(recovered);
    assert_eq!(snapshot, Snapshot::default());
    let preserved = fs::read_dir(&profile.0)
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("session-invalid-")
        });
    assert!(preserved);
}

#[test]
fn snapshots_that_would_exceed_the_file_limit_are_detected() {
    let long = format!("https://example.com/{}", "a".repeat(8000));
    let full = window(&vec![long.as_str(); 64], 0);
    let snapshot = Snapshot {
        other_windows: vec![full; MAX_WINDOWS - 1],
        ..Snapshot::default()
    };
    assert!(snapshot.validate().is_ok());
    assert!(!storage::fits(&snapshot));
    assert!(storage::fits(&Snapshot {
        other_windows: Vec::new(),
        ..snapshot
    }));
}

/// The session fields and rules of 0.17, which had no `other_windows` and did not
/// refuse unknown fields.
#[derive(serde::Deserialize)]
struct Snapshot017 {
    version: u32,
    tabs: Vec<String>,
    active: usize,
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    pinned: Vec<bool>,
    #[serde(default)]
    titles: Vec<String>,
    #[serde(default)]
    keep_awake: Vec<bool>,
}

#[test]
fn a_file_written_by_0_18_restores_the_focused_window_in_0_17() {
    let profile = Profile::new();
    let path = profile.0.join("session.json");
    let written = three_windows();
    storage::save(&path, &written).unwrap();
    let old: Snapshot017 = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    // 0.17's validation is the same check without the new field.
    let as_017 = Snapshot {
        version: old.version,
        tabs: old.tabs,
        active: old.active,
        settings: old.settings,
        pinned: old.pinned,
        titles: old.titles,
        keep_awake: old.keep_awake,
        other_windows: Vec::new(),
    };
    assert!(as_017.validate().is_ok());
    assert_eq!(as_017.tabs, written.tabs);
    assert_eq!(as_017.active, written.active);
}
