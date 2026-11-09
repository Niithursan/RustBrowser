use rust_browser::diagnostics::{Code, MAX_BYTES, MAX_ENTRIES, Recorder};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rust-browser-diagnostics-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join(".diagnostic-test-root"), b"owned synthetic data").unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn wait(recorder: &Recorder, condition: impl Fn(&rust_browser::diagnostics::Report) -> bool) {
    let start = Instant::now();
    while !condition(&recorder.snapshot()) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "Diagnostic state timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn typed_journal_is_bounded_persistent_and_private_windows_open_nothing() {
    let root = Root::new();
    let recorder = Recorder::start(Some(&root.0), |_| {}).unwrap();
    wait(&recorder, |r| r.enabled && r.storage_ok);
    recorder.record(Code::Started);
    for _ in 0..3000 {
        recorder.record(Code::NavigationFailed);
        recorder.record(Code::SessionSaveFailed);
    }
    wait(&recorder, |r| r.entries.len() > 1);
    drop(recorder);
    let bytes = fs::read(root.0.join("local-diagnostics/journal.json")).unwrap();
    assert!(bytes.len() as u64 <= MAX_BYTES);
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let entries = document["entries"].as_array().unwrap();
    assert!(entries.len() <= MAX_ENTRIES);
    assert!(entries.iter().any(|e| e["code"] == "clean_shutdown"));
    assert!(!root.0.join("local-diagnostics/running.marker").exists());
    let before = bytes;
    let private = Recorder::start(None, |_| {
        panic!("Private recorder sent a persistent callback")
    })
    .unwrap();
    private.record(Code::NavigationFailed);
    assert!(!private.snapshot().enabled);
    assert!(private.clear(true).is_err());
    drop(private);
    assert_eq!(
        before,
        fs::read(root.0.join("local-diagnostics/journal.json")).unwrap()
    );
}

#[test]
fn unreadable_data_is_preserved_until_confirmed_clear_and_other_data_survives() {
    let root = Root::new();
    fs::create_dir(root.0.join("local-diagnostics")).unwrap();
    let raw = b"not valid diagnostic JSON: https://private.invalid/token=secret";
    fs::write(root.0.join("local-diagnostics/journal.json"), raw).unwrap();
    fs::write(root.0.join("library.sqlite"), b"synthetic unrelated data").unwrap();
    let recorder = Recorder::start(Some(&root.0), |_| {}).unwrap();
    wait(&recorder, |r| r.enabled);
    assert!(!recorder.snapshot().storage_ok);
    assert!(recorder.clear(false).is_err());
    assert_eq!(
        raw,
        fs::read(root.0.join("local-diagnostics/journal.json"))
            .unwrap()
            .as_slice()
    );
    recorder.clear(true).unwrap();
    wait(&recorder, |r| {
        r.storage_ok && r.entries.iter().any(|e| e.code == Code::Cleared)
    });
    recorder.record(Code::Started);
    drop(recorder);
    let bytes = fs::read(root.0.join("local-diagnostics/journal.json")).unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains("secret"));
    assert_eq!(
        fs::read(root.0.join("library.sqlite")).unwrap(),
        b"synthetic unrelated data"
    );
}

#[test]
fn second_recorder_cannot_clear_the_active_journal_and_write_failure_is_visible() {
    let root = Root::new();
    let first = Recorder::start(Some(&root.0), |_| {}).unwrap();
    wait(&first, |r| r.enabled && r.storage_ok);
    let second = Recorder::start(Some(&root.0), |_| {}).unwrap();
    wait(&second, |r| r.enabled);
    assert!(!second.snapshot().storage_ok);
    second.clear(true).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    drop(second);
    assert!(root.0.join("local-diagnostics/running.marker").is_file());
    fs::create_dir(root.0.join("local-diagnostics/journal.tmp")).unwrap();
    first.record(Code::SessionSaveFailed);
    wait(&first, |r| !r.storage_ok);
    drop(first);
    assert!(root.0.join("local-diagnostics/running.marker").is_file());
}

#[test]
fn invalid_schema_and_oversized_diagnostics_are_not_overwritten() {
    for bytes in [
        br#"{"version":99,"entries":[]}"#.to_vec(),
        br#"{"version":1,"entries":[{"first":1,"last":2,"count":1,"code":"arbitrary-secret"}]}"#
            .to_vec(),
        vec![b'x'; MAX_BYTES as usize + 1],
    ] {
        let root = Root::new();
        fs::create_dir(root.0.join("local-diagnostics")).unwrap();
        let path = root.0.join("local-diagnostics/journal.json");
        fs::write(&path, &bytes).unwrap();
        let recorder = Recorder::start(Some(&root.0), |_| {}).unwrap();
        wait(&recorder, |r| r.enabled);
        assert!(!recorder.snapshot().storage_ok);
        drop(recorder);
        assert_eq!(bytes, fs::read(path).unwrap());
    }
}

#[test]
fn abnormal_process_exit_is_reported_on_reopen_without_page_data() {
    let root = Root::new();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["abruptly_terminated_recorder_child", "--ignored", "--exact"])
        .env("RUST_BROWSER_DIAGNOSTIC_TEST_ROOT", &root.0)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !root.0.join("child.ready").exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "Owned child exited before readiness"
        );
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Child readiness timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let recorder = Recorder::start(Some(&root.0), |_| {}).unwrap();
    wait(&recorder, |r| r.previous_exit_unclean);
    assert!(
        recorder
            .snapshot()
            .entries
            .iter()
            .any(|e| e.code == Code::PreviousExitUnclean)
    );
    recorder.record(Code::Started);
    drop(recorder);
    assert!(!root.0.join("local-diagnostics/running.marker").exists());
}

#[test]
#[ignore = "Owned helper process used by abnormal_process_exit_is_reported_on_reopen_without_page_data"]
fn abruptly_terminated_recorder_child() {
    let root = PathBuf::from(std::env::var_os("RUST_BROWSER_DIAGNOSTIC_TEST_ROOT").unwrap());
    assert!(root.join(".diagnostic-test-root").is_file());
    let recorder = Recorder::start(Some(&root), |_| {}).unwrap();
    wait(&recorder, |r| r.storage_ok);
    recorder.record(Code::Started);
    while !root.join("local-diagnostics/journal.json").is_file() {
        std::thread::sleep(Duration::from_millis(10));
    }
    fs::write(root.join("child.ready"), b"ready").unwrap();
    std::thread::sleep(Duration::from_secs(60));
}
