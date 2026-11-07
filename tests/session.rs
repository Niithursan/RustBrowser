use rust_browser::{
    navigation,
    settings::{SearchEngine, Settings},
    storage::{self, Snapshot, Store},
    tabs::Tabs,
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
            "rust-browser-test-{}-{}-{}",
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

#[test]
fn tabs_search_settings_and_active_session_roundtrip() {
    let profile = Profile::new();
    let (store, initial, recovered) = Store::open(&profile.0, || panic!("write failed")).unwrap();
    assert!(!recovered);
    assert_eq!(initial, Snapshot::default());
    let mut tabs = Tabs::default();
    let first = tabs.push(navigation::HOME.into(), ()).unwrap();
    tabs.push(
        navigation::resolve("rust & browsers 日本", SearchEngine::DuckDuckGo).unwrap(),
        (),
    )
    .unwrap();
    tabs.select(first);
    let snapshot = Snapshot {
        version: 1,
        tabs: tabs.items.iter().map(|tab| tab.url.clone()).collect(),
        active: tabs.active_index(),
        settings: Settings {
            search_engine: SearchEngine::DuckDuckGo,
        },
    };
    store.queue(snapshot.clone());
    drop(store); // Shutdown must flush even before the debounce expires.
    let (store, restored, _) = Store::open(&profile.0, || {}).unwrap();
    assert_eq!(snapshot, restored);
    assert_eq!(restored.active, 0);
    assert!(restored.tabs[1].starts_with("https://duckduckgo.com/?q="));
    drop(store);
}

#[test]
fn profile_lock_releases_and_last_queued_snapshot_wins() {
    let profile = Profile::new();
    let (store, _, _) = Store::open(&profile.0, || {}).unwrap();
    assert!(Store::open(&profile.0, || {}).is_err());
    for active in 0..2 {
        store.queue(Snapshot {
            tabs: vec![navigation::HOME.into(), "https://rust-lang.org/".into()],
            active,
            ..Snapshot::default()
        });
    }
    drop(store);
    let (store, snapshot, _) = Store::open(&profile.0, || {}).unwrap();
    assert_eq!(snapshot.active, 1);
    drop(store);
}

#[test]
fn atomic_replacement_and_corruption_recovery() {
    let profile = Profile::new();
    let path = profile.0.join("session.json");
    storage::save(&path, &Snapshot::default()).unwrap();
    let changed = Snapshot {
        settings: Settings {
            search_engine: SearchEngine::Bing,
        },
        ..Snapshot::default()
    };
    storage::save(&path, &changed).unwrap();
    assert_eq!(storage::load(&path).unwrap(), changed);
    fs::write(&path, b"{broken").unwrap();
    let (store, snapshot, recovered) = Store::open(&profile.0, || {}).unwrap();
    assert!(recovered);
    assert_eq!(snapshot, Snapshot::default());
    let backup = fs::read_dir(&profile.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("session-invalid-")
        })
        .unwrap();
    assert_eq!(fs::read(backup).unwrap(), b"{broken");
    store.queue(snapshot);
    drop(store);
    assert!(storage::load(&path).is_ok());
}

#[test]
fn invalid_and_oversized_sessions_are_rejected_without_overwriting_valid_data() {
    let profile = Profile::new();
    let path = profile.0.join("session.json");
    storage::save(&path, &Snapshot::default()).unwrap();
    for snapshot in [
        Snapshot {
            version: 999,
            ..Snapshot::default()
        },
        Snapshot {
            tabs: vec![],
            ..Snapshot::default()
        },
        Snapshot {
            active: 10,
            ..Snapshot::default()
        },
        Snapshot {
            tabs: vec!["javascript:alert(1)".into()],
            ..Snapshot::default()
        },
        Snapshot {
            tabs: vec!["https://user:pass@example.com/".into()],
            ..Snapshot::default()
        },
        Snapshot {
            tabs: vec![navigation::HOME.into(); 65],
            ..Snapshot::default()
        },
    ] {
        assert!(storage::save(&path, &snapshot).is_err());
    }
    assert_eq!(storage::load(&path).unwrap(), Snapshot::default());
    fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(storage::load(&path).is_err());
}

#[test]
fn background_write_failure_is_reported() {
    let profile = Profile::new();
    let (send, receive) = std::sync::mpsc::channel();
    let (store, _, _) = Store::open(&profile.0, move || {
        let _ = send.send(());
    })
    .unwrap();
    fs::create_dir(profile.0.join("session.json.tmp")).unwrap();
    store.queue(Snapshot::default());
    drop(store);
    receive
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
}
