use rust_browser::downloads::{Staged, safe_filename};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Folder(PathBuf);
impl Folder {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rust-browser-dl-{}-{}-{}",
            std::process::id(),
            rust_browser::library::now(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn hostile_filenames_cannot_escape_or_use_windows_devices_and_streams() {
    for input in ["../../report.txt", r"C:\secret\report.txt", "report.txt"] {
        assert_eq!(safe_filename(input), "report.txt");
    }
    for input in ["..", "... ", "", ":*?<>|"] {
        assert_eq!(safe_filename(input), "download");
    }
    assert_eq!(safe_filename("CON.txt"), "_CON.txt");
    assert_eq!(safe_filename("LPT9"), "_LPT9");
    assert_eq!(safe_filename("hello.txt:secret"), "hello.txtsecret");
    assert!(!safe_filename("invoice\u{202e}exe.pdf").contains('\u{202e}'));
    assert!(safe_filename(&"日".repeat(1000)).chars().count() <= 100);
}
#[test]
fn collision_safe_publish_preserves_existing_data() {
    let folder = Folder::new();
    fs::write(folder.0.join("report.txt"), b"existing").unwrap();
    let stage = Staged::create(&folder.0, "../../report.txt").unwrap();
    fs::write(&stage.temporary, b"new file").unwrap();
    let final_path = stage.publish().unwrap();
    assert_eq!(final_path.file_name().unwrap(), "report (1).txt");
    assert_eq!(fs::read(folder.0.join("report.txt")).unwrap(), b"existing");
    assert_eq!(fs::read(final_path).unwrap(), b"new file");
    assert!(!stage.temporary.exists());
}
#[test]
fn cancellation_and_missing_files_are_safe() {
    let folder = Folder::new();
    assert!(Staged::create(std::path::Path::new("relative"), "a.txt").is_err());
    let stage = Staged::create(&folder.0, "a.txt").unwrap();
    assert!(stage.publish().is_err());
    fs::write(&stage.temporary, b"partial").unwrap();
    stage.clean_partial();
    assert!(!stage.temporary.exists());
    assert!(!folder.0.join("a.txt").exists());
}
