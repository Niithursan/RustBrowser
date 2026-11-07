//! Only explicitly selected folders and recognized, unlocked staging leases are inspected.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
const MARKER: &str = ".rust-browser-stage-lease";
const MAGIC: &[u8] = b"RustBrowser staging lease v1\n";
#[derive(Debug)]
pub struct Lease {
    directory: PathBuf,
    file: Mutex<Option<File>>,
}
impl Lease {
    pub fn create(directory: &Path) -> io::Result<Self> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(directory.join(MARKER))?;
        file.try_lock()?;
        file.write_all(MAGIC)?;
        file.sync_all()?;
        Ok(Self {
            directory: directory.into(),
            file: Mutex::new(Some(file)),
        })
    }
    pub fn clean_empty(&self) {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return;
        };
        if entries
            .into_iter()
            .any(|entry| !entry.is_ok_and(|entry| entry.file_name() == MARKER))
        {
            return;
        }
        if let Ok(mut file) = self.file.lock() {
            drop(file.take());
            let _ = fs::remove_file(self.directory.join(MARKER));
            let _ = fs::remove_dir(&self.directory);
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.clean_empty();
    }
}
#[derive(Default, Debug)]
pub struct Report {
    pub cleaned: usize,
    pub preserved: usize,
    pub live: usize,
}
fn plain(path: &Path, directory: bool) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !meta.file_type().is_symlink()
        && if directory {
            meta.is_dir()
        } else {
            meta.is_file()
        }
}
/// Never recursively deletes or removes payload files. Old/unrecognized folders stay intact.
pub fn clean_empty_abandoned(folder: &Path) -> io::Result<Report> {
    let root = folder.canonicalize()?;
    if !plain(&root, true) {
        return Err(io::Error::other("Not a plain download folder"));
    }
    let mut report = Report::default();
    for entry in fs::read_dir(&root)?.take(256) {
        let entry = entry?;
        let name = entry.file_name();
        let Some(rest) = name
            .to_str()
            .and_then(|s| s.strip_prefix(".rust-browser-download-"))
        else {
            continue;
        };
        if rest.split('-').count() != 3 || !rest.chars().all(|c| c.is_ascii_digit() || c == '-') {
            continue;
        }
        let directory = entry.path();
        if !plain(&directory, true) || directory.canonicalize()?.parent() != Some(root.as_path()) {
            continue;
        }
        let marker = directory.join(MARKER);
        if !plain(&marker, false) {
            continue;
        }
        let mut file = match OpenOptions::new().read(true).write(true).open(&marker) {
            Ok(f) => f,
            Err(_) => {
                report.preserved += 1;
                continue;
            }
        };
        if file.try_lock().is_err() {
            report.live += 1;
            continue;
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file).take(128).read_to_end(&mut bytes)?;
        if bytes != MAGIC {
            report.preserved += 1;
            continue;
        }
        if fs::read_dir(&directory)?.any(|entry| !entry.is_ok_and(|e| e.file_name() == MARKER)) {
            report.preserved += 1;
            continue;
        }
        drop(file);
        if fs::remove_file(marker)
            .and_then(|()| fs::remove_dir(directory))
            .is_ok()
        {
            report.cleaned += 1
        } else {
            report.preserved += 1
        }
    }
    Ok(report)
}
