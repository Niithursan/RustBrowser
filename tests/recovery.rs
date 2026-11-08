use rust_browser::{
    download_recovery::{self, Lease},
    downloads::Staged,
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
        let path = std::env::temp_dir().join(format!(
            "rust-browser-recovery-{}-{}-{}",
            std::process::id(),
            rust_browser::library::now(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn state() -> Snapshot {
    Snapshot {
        tabs: vec!["https://example.com/recovered".into()],
        pinned: vec![true],
        titles: vec!["Retained fixture".into()],
        ..Snapshot::default()
    }
}
#[test]
fn interrupted_first_save_and_corrupt_main_recover_valid_temp_without_clobbering_original() {
    for corrupt in [false, true] {
        let root = Profile::new();
        let path = root.0.join("session.json");
        let temp = root.0.join("session.json.tmp");
        let expected = state();
        fs::write(&temp, serde_json::to_vec(&expected).unwrap()).unwrap();
        if corrupt {
            fs::write(&path, b"corrupt original").unwrap();
        }
        let (store, restored, recovered) = Store::open(&root.0, || {}).unwrap();
        assert!(recovered);
        assert_eq!(restored, expected);
        assert!(!temp.exists());
        assert_eq!(storage::load(&path).unwrap(), expected);
        drop(store);
        let backups: Vec<_> = fs::read_dir(&root.0)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("session-invalid-")
            })
            .collect();
        assert_eq!(backups.len(), usize::from(corrupt));
        if corrupt {
            assert_eq!(fs::read(backups[0].path()).unwrap(), b"corrupt original");
        }
    }
}
#[test]
fn committed_privacy_preference_wins_over_stale_temp_and_io_errors_preserve_paths() {
    let root = Profile::new();
    let mut committed = Snapshot::default();
    committed.settings.restore_session = false;
    storage::save(&root.0.join("session.json"), &committed).unwrap();
    fs::write(
        root.0.join("session.json.tmp"),
        serde_json::to_vec(&state()).unwrap(),
    )
    .unwrap();
    let (store, restored, recovered) = Store::open(&root.0, || {}).unwrap();
    assert!(!recovered);
    assert!(!restored.settings.restore_session);
    assert_eq!(restored.tabs, Snapshot::default().tabs);
    drop(store);
    let invalid = Profile::new();
    fs::create_dir(invalid.0.join("session.json")).unwrap();
    assert!(Store::open(&invalid.0, || {}).is_err());
    assert!(invalid.0.join("session.json").is_dir());
}
#[test]
fn invalid_temp_is_preserved_instead_of_silently_discarded() {
    for bytes in [b"invalid pending save".as_slice(), b"", b"{\"version\":"] {
        let root = Profile::new();
        fs::write(root.0.join("session.json.tmp"), bytes).unwrap();
        let (store, _, recovered) = Store::open(&root.0, || {}).unwrap();
        assert!(recovered);
        drop(store);
        assert!(fs::read_dir(&root.0).unwrap().flatten().any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("session-invalid-")
                && fs::read(e.path()).unwrap() == bytes
        }));
    }
}
#[test]
fn download_recovery_keeps_live_transfers_payloads_unrecognized_and_corrupt_markers() {
    let root = Profile::new();
    let live = Staged::create(&root.0, "live.txt").unwrap();
    assert_eq!(
        download_recovery::clean_empty_abandoned(&root.0)
            .unwrap()
            .live,
        1
    );
    assert!(live.temporary.parent().unwrap().exists());
    let abandoned = root.0.join(".rust-browser-download-1-2-3");
    fs::create_dir(&abandoned).unwrap();
    let lease = Lease::create(&abandoned).unwrap();
    fs::write(
        abandoned.join("payload.txt"),
        b"potentially complete download",
    )
    .unwrap();
    drop(lease);
    let empty = root.0.join(".rust-browser-download-1-2-4");
    fs::create_dir(&empty).unwrap();
    let _empty_lease = Lease::create(&empty).unwrap();
    let report = download_recovery::clean_empty_abandoned(&root.0).unwrap();
    assert_eq!(report.live, 2);
    assert_eq!(report.preserved, 1);
    assert_eq!(
        fs::read(abandoned.join("payload.txt")).unwrap(),
        b"potentially complete download"
    );
    let unknown = root.0.join(".rust-browser-download-1-2-5");
    fs::create_dir(&unknown).unwrap();
    fs::write(unknown.join(".rust-browser-stage-lease"), b"not ours").unwrap();
    let report = download_recovery::clean_empty_abandoned(&root.0).unwrap();
    assert_eq!(report.preserved, 2);
    assert!(unknown.exists());
}
#[test]
fn only_empty_unlocked_recognized_staging_is_cleaned() {
    let root = Profile::new();
    let empty = root.0.join(".rust-browser-download-12-34-56");
    fs::create_dir(&empty).unwrap();
    fs::write(
        empty.join(".rust-browser-stage-lease"),
        b"RustBrowser staging lease v1\n",
    )
    .unwrap();
    let old = root.0.join(".rust-browser-download-12-34-57");
    fs::create_dir(&old).unwrap();
    let report = download_recovery::clean_empty_abandoned(&root.0).unwrap();
    assert_eq!(report.cleaned, 1);
    assert!(!empty.exists());
    assert!(old.exists());
    assert!(!root.0.join("download.txt").exists());
}

#[test]
fn crash_child_fixture() {
    let Some(root) = std::env::var_os("RUST_BROWSER_CRASH_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    assert!(root.join("fixture.marker").is_file());
    let (store, _, _) = Store::open(&root, || {}).unwrap();
    store.queue(state());
    let (mut db, _) = rust_browser::library::Database::open(&root.join("library.sqlite")).unwrap();
    db.apply(rust_browser::library::Action::SaveBookmark {
        id: None,
        title: "Committed fixture bookmark".into(),
        url: "https://example.com/committed".into(),
        folder: None,
    })
    .unwrap();
    drop(db);
    let connection = rusqlite::Connection::open(root.join("library.sqlite")).unwrap();
    connection.execute_batch("CREATE TABLE crash_probe(value INTEGER); BEGIN IMMEDIATE; INSERT INTO crash_probe VALUES(1);").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(700));
    assert!(root.join("session.json").is_file());
    fs::write(root.join("child.ready"), b"synthetic crash fixture ready").unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[test]
fn process_termination_releases_locks_restores_committed_session_and_rolls_back_sqlite() {
    let root = Profile::new();
    fs::write(root.0.join("fixture.marker"), b"isolated test only").unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["crash_child_fixture", "--exact", "--test-threads=1"])
        .env("RUST_BROWSER_CRASH_TEST_ROOT", &root.0)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !root.0.join("child.ready").exists() && std::time::Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("Crash fixture exited early: {status}")
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let ready = root.0.join("child.ready").exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready, "Crash fixture was not ready");
    let (store, restored, recovered) = Store::open(&root.0, || {}).unwrap();
    assert!(!recovered);
    assert_eq!(restored, state());
    drop(store);
    let (db, recovered) =
        rust_browser::library::Database::open(&root.0.join("library.sqlite")).unwrap();
    assert!(!recovered);
    assert_eq!(
        db.query(&rust_browser::library::Filter::default())
            .unwrap()
            .bookmarks
            .len(),
        1
    );
    drop(db);
    let connection = rusqlite::Connection::open(root.0.join("library.sqlite")).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM crash_probe", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    for _ in 0..5 {
        let (store, restored, _) = Store::open(&root.0, || {}).unwrap();
        assert_eq!(restored, state());
        drop(store);
    }
}
