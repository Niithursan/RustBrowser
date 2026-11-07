use rust_browser::{
    library::{Action, Database, Filter},
    privacy::{self, ClearRequest, PrivateLease},
    security::{self, Capability, Decision},
    settings::Settings,
    storage::{self, Snapshot, Store},
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
        let p = std::env::temp_dir().join(format!(
            "rust-browser-v04-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn private_leases_isolate_data_and_sweep_only_abandoned_owned_directories() {
    let normal = Profile::new();
    fs::write(normal.0.join("keep"), b"normal").unwrap();
    let a = PrivateLease::create(&normal.0).unwrap();
    let b = PrivateLease::create(&normal.0).unwrap();
    assert_ne!(a.path, b.path);
    assert!(a.path.starts_with(normal.0.canonicalize().unwrap()));
    fs::write(a.path.join("private-state"), b"private").unwrap();
    assert!(!b.path.join("private-state").exists());
    assert_eq!(PrivateLease::sweep(&normal.0).unwrap(), 0);
    let abandoned = normal
        .0
        .join("private-contexts/private-00000000000000000000000000000001-00000001-00");
    fs::create_dir(&abandoned).unwrap();
    fs::write(abandoned.join("lease.lock"), b"").unwrap();
    fs::write(abandoned.join("cache"), b"stale").unwrap();
    let unrelated = normal.0.join("private-contexts/user-files");
    fs::create_dir(&unrelated).unwrap();
    assert_eq!(PrivateLease::sweep(&normal.0).unwrap(), 1);
    assert!(!abandoned.exists());
    assert!(a.path.exists() && b.path.exists() && unrelated.exists());
    let path = a.path.clone();
    drop(a);
    assert!(!path.exists());
    drop(b);
    assert_eq!(fs::read(normal.0.join("keep")).unwrap(), b"normal");
}

#[test]
fn private_snapshots_never_reach_disk_and_disabled_restoration_strips_urls() {
    let snapshot = Snapshot {
        tabs: vec!["https://example.com/private?token=secret".into()],
        ..Snapshot::default()
    };
    assert!(privacy::persistent_snapshot(snapshot.clone(), true).is_none());
    let mut no_restore = snapshot;
    no_restore.settings.restore_session = false;
    let p = Profile::new();
    storage::save(&p.0.join("session.json"), &no_restore).unwrap();
    let text = fs::read_to_string(p.0.join("session.json")).unwrap();
    assert!(!text.contains("secret"));
    assert_eq!(
        storage::load(&p.0.join("session.json")).unwrap().tabs,
        vec![rust_browser::navigation::HOME]
    );
}

#[test]
fn clearing_session_is_a_barrier_against_queued_writes_and_preserves_bookmarks() {
    let p = Profile::new();
    let (store, _, _) = Store::open(&p.0, || {}).unwrap();
    store.queue(Snapshot {
        tabs: vec!["https://example.com/old-secret".into()],
        ..Snapshot::default()
    });
    fs::write(p.0.join("session-invalid-123.json"), b"old-secret").unwrap();
    fs::write(p.0.join("session.json.tmp"), b"partial-secret").unwrap();
    fs::write(p.0.join("library.corrupt-123"), b"bookmark recovery").unwrap();
    let settings = Settings {
        restore_session: false,
        ..Settings::default()
    };
    store.forget_session(settings.clone()).unwrap();
    privacy::clear_session_backups(&p.0).unwrap();
    store.queue(Snapshot {
        tabs: vec!["https://example.com/new-secret".into()],
        settings,
        ..Snapshot::default()
    });
    drop(store);
    assert!(
        !fs::read_to_string(p.0.join("session.json"))
            .unwrap()
            .contains("secret")
    );
    assert!(!p.0.join("session-invalid-123.json").exists());
    assert!(p.0.join("library.corrupt-123").exists());
}

#[test]
fn permission_rules_persist_by_exact_origin_and_capability_and_revoke_individually() {
    let p = Profile::new();
    let (mut db, _) = Database::open(&p.0.join("library.sqlite")).unwrap();
    let set = |origin: &str, capability, decision| Action::SetPermission {
        origin: origin.into(),
        capability,
        decision,
    };
    db.apply(set(
        "https://example.com",
        Capability::Camera,
        Some(Decision::Allow),
    ))
    .unwrap();
    db.apply(set(
        "https://example.com:8443",
        Capability::Microphone,
        Some(Decision::Deny),
    ))
    .unwrap();
    drop(db);
    let (mut db, _) = Database::open(&p.0.join("library.sqlite")).unwrap();
    let data = db.query(&Filter::default()).unwrap();
    assert_eq!(data.permissions.len(), 2);
    assert_eq!(data.permissions[0].origin, "https://example.com");
    assert_eq!(data.permissions[0].decision, Decision::Allow);
    db.apply(set("https://example.com", Capability::Camera, None))
        .unwrap();
    let data = db.query(&Filter::default()).unwrap();
    assert_eq!(data.permissions.len(), 1);
    assert_eq!(data.permissions[0].capability, Capability::Microphone);
    let (private, _) = Database::open(std::path::Path::new(":memory:")).unwrap();
    assert!(
        private
            .query(&Filter::default())
            .unwrap()
            .permissions
            .is_empty()
    );
    for invalid in [
        "http://example.com",
        "https://example.com/path",
        "https://example.com:443",
        "https://user:pass@example.com",
        "file:///C:/x",
        "null",
    ] {
        assert!(
            db.apply(set(invalid, Capability::Camera, Some(Decision::Allow)))
                .is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn confirmed_library_clearing_preserves_bookmarks_and_other_profiles() {
    let p = Profile::new();
    let (mut db, _) = Database::open(&p.0.join("library.sqlite")).unwrap();
    db.apply(Action::SaveBookmark {
        id: None,
        title: "Keep".into(),
        url: "https://example.com/".into(),
        folder: None,
    })
    .unwrap();
    db.apply(Action::Record {
        key: "visit".into(),
        title: "Visit".into(),
        url: "https://example.com/".into(),
        timestamp: 123,
    })
    .unwrap();
    db.apply(Action::SetPermission {
        origin: "https://example.com".into(),
        capability: Capability::Notifications,
        decision: Some(Decision::Deny),
    })
    .unwrap();
    assert!(
        db.apply(Action::ClearLibrary {
            history: true,
            permissions: true,
            confirmed: false
        })
        .is_err()
    );
    assert_eq!(db.query(&Filter::default()).unwrap().history.len(), 1);
    db.apply(Action::ClearLibrary {
        history: true,
        permissions: true,
        confirmed: true,
    })
    .unwrap();
    let data = db.query(&Filter::default()).unwrap();
    assert!(data.history.is_empty() && data.permissions.is_empty());
    assert_eq!(data.bookmarks.len(), 1);
    assert!(ClearRequest::default().validate().is_err());
    assert!(
        ClearRequest {
            history: true,
            ..ClearRequest::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        ClearRequest {
            history: true,
            confirmed: true,
            ..ClearRequest::default()
        }
        .validate()
        .is_ok()
    );
}

#[test]
fn origins_and_connection_indicators_do_not_overstate_security() {
    assert_eq!(
        security::origin("https://EXAMPLE.com:443/path?q=private#x").as_deref(),
        Some("https://example.com")
    );
    assert_eq!(
        security::origin("https://例え.テスト/").unwrap(),
        "https://xn--r8jz45g.xn--zckzah"
    );
    for invalid in [
        "javascript:alert(1)",
        "data:text/html,x",
        "file:///C:/x",
        "about:blank",
        "https://user:pass@example.com/",
    ] {
        assert!(security::origin(invalid).is_none());
    }
    assert!(security::permission_origin("http://example.com").is_none());
    assert!(security::permission_origin("http://127.0.0.1:8765/path").is_some());
    assert!(
        security::indicator("https://example.com/", false, false)
            .message
            .contains("does not")
    );
    assert!(
        security::indicator("https://example.com/", false, true)
            .message
            .contains("failed")
    );
    assert!(
        security::indicator("https://example.com/", true, false)
            .message
            .contains("Connecting")
    );
    assert!(
        security::indicator("http://example.com/", false, false)
            .message
            .contains("not encrypted")
    );
    assert_eq!(
        security::indicator("about:blank", false, false).scheme,
        "none"
    );
    assert_eq!(
        security::indicator(rust_browser::navigation::HOME, false, false).scheme,
        "local"
    );
}

#[test]
fn malformed_settings_and_storage_failure_do_not_enable_privilege() {
    assert!(serde_json::from_str::<Settings>(r#"{"restore_session":"yes"}"#).is_err());
    assert!(serde_json::from_str::<Settings>("not json").is_err());
    let legacy: Settings = serde_json::from_str(r#"{"search_engine":"bing"}"#).unwrap();
    assert!(legacy.restore_session && legacy.history_enabled);
    let p = Profile::new();
    fs::create_dir(p.0.join("library.sqlite")).unwrap();
    assert!(Database::open(&p.0.join("library.sqlite")).is_err());
    assert!(p.0.join("library.sqlite").is_dir());
}
