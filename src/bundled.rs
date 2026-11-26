//! The built-in ad blocker on Windows: uBlock Origin
//! (<https://github.com/gorhill/uBlock>, GPL-3.0), the official Chromium
//! release, embedded in the executable and unpacked into the profile, where
//! WebView2 runs it like an extension the user added from a folder.
//!
//! The ZIP in `third_party/ublock-origin` is the release file unchanged; a test
//! checks it against the SHA-256 GitHub publishes for it. Unpacking reads only
//! that embedded file, but still refuses anything a ZIP could use to write
//! outside its folder.
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

/// The uBlock Origin release that comes with this build.
pub const VERSION: &str = "1.77.2";
/// SHA-256 of `ZIP`, as published with the release.
pub const SHA256: &str = "636cfa6c640b29ab7251b765225d6c0f580bd9fe830d1319005ea7093cf4303c";
/// Where its source and license are published.
pub const SOURCE: &str = "https://github.com/gorhill/uBlock/releases/tag/1.77.2";
const ZIP: &[u8] = include_bytes!("../third_party/ublock-origin/uBlock0_1.77.2.chromium.zip");
/// The folder every file in the release ZIP sits in.
const PREFIX: &str = "uBlock0.chromium/";
/// The folder it is unpacked to, inside the folder given to `unpack`.
const FOLDER: &str = "ublock-origin";
/// Written last into an unpacked copy: which release it holds.
const MARKER: &str = ".rust-browser-bundle";
const MAX_FILES: usize = 5000;
const MAX_TOTAL: usize = 128 * 1024 * 1024;

/// One file in a ZIP.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    pub name: &'a str,
    pub data: Vec<u8>,
}

fn u16_at(bytes: &[u8], at: usize) -> Option<usize> {
    bytes
        .get(at..at + 2)
        .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<usize> {
    bytes
        .get(at..at + 4)
        .and_then(|b| usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok())
}

/// A relative path of plain names separated by `/`.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', ':', '\0'])
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// Every file in `zip` (folders are left out), read through its central
/// directory. Only stored and deflated files are accepted; encrypted files,
/// ZIP64, spanned archives and unsafe names are refused.
pub fn read(zip: &[u8]) -> Result<Vec<Entry<'_>>, String> {
    let bad = || "The built-in ad blocker's archive is damaged.".to_string();
    // The end record is 22 bytes, followed by a comment of at most 64 KiB.
    let floor = zip.len().saturating_sub(22 + 0xffff);
    let end = (floor..=zip.len().saturating_sub(22))
        .rev()
        .find(|&at| zip[at..].starts_with(b"PK\x05\x06"))
        .ok_or_else(bad)?;
    let (count, size, start) = (
        u16_at(zip, end + 10).ok_or_else(bad)?,
        u32_at(zip, end + 12).ok_or_else(bad)?,
        u32_at(zip, end + 16).ok_or_else(bad)?,
    );
    if u16_at(zip, end + 4) != Some(0) || count > MAX_FILES || start + size > end {
        return Err(bad());
    }
    let mut entries = Vec::new();
    let (mut at, mut total) = (start, 0usize);
    for _ in 0..count {
        if !zip.get(at..).is_some_and(|b| b.starts_with(b"PK\x01\x02")) {
            return Err(bad());
        }
        let flags = u16_at(zip, at + 8).ok_or_else(bad)?;
        let method = u16_at(zip, at + 10).ok_or_else(bad)?;
        let packed = u32_at(zip, at + 20).ok_or_else(bad)?;
        let unpacked = u32_at(zip, at + 24).ok_or_else(bad)?;
        let name_len = u16_at(zip, at + 28).ok_or_else(bad)?;
        let skip = u16_at(zip, at + 30).ok_or_else(bad)? + u16_at(zip, at + 32).ok_or_else(bad)?;
        let local = u32_at(zip, at + 42).ok_or_else(bad)?;
        let name = zip
            .get(at + 46..at + 46 + name_len)
            .and_then(|b| std::str::from_utf8(b).ok())
            .ok_or_else(bad)?;
        at += 46 + name_len + skip;
        if flags & 1 != 0 || packed == 0xffff_ffff || unpacked == 0xffff_ffff {
            return Err(bad());
        }
        if name.ends_with('/') {
            continue;
        }
        if !safe_name(name) {
            return Err(format!(
                "The built-in ad blocker's archive has an unsafe name: {name}"
            ));
        }
        total += unpacked;
        if total > MAX_TOTAL {
            return Err(bad());
        }
        if !zip
            .get(local..)
            .is_some_and(|b| b.starts_with(b"PK\x03\x04"))
        {
            return Err(bad());
        }
        let data_at = local
            + 30
            + u16_at(zip, local + 26).ok_or_else(bad)?
            + u16_at(zip, local + 28).ok_or_else(bad)?;
        let packed = zip.get(data_at..data_at + packed).ok_or_else(bad)?;
        let data = match method {
            0 => packed.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(packed, unpacked)
                .map_err(|_| bad())?,
            _ => return Err(bad()),
        };
        if data.len() != unpacked {
            return Err(bad());
        }
        entries.push(Entry { name, data });
    }
    Ok(entries)
}

/// Write `entries` (all inside `PREFIX`) into `folder`, then the marker.
fn write_all(entries: &[Entry<'_>], folder: &Path) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("The built-in ad blocker could not be unpacked: {e}");
    for entry in entries {
        let name = entry
            .name
            .strip_prefix(PREFIX)
            .filter(|n| safe_name(n))
            .ok_or("The built-in ad blocker's archive has an unexpected layout.")?;
        let path = folder.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(failed)?;
        }
        let mut file = fs::File::create_new(&path).map_err(failed)?;
        file.write_all(&entry.data).map_err(failed)?;
    }
    fs::write(folder.join(MARKER), SHA256).map_err(failed)
}

fn unique(parent: &Path, label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    parent.join(format!(".{FOLDER}-{label}-{}-{nanos}", std::process::id()))
}

/// The unpacked copy of this build's uBlock Origin in `parent`, unpacking it
/// first when it is missing or holds another release. The new copy is written
/// beside the old one and swapped in only when complete; an old copy that is
/// still in use (another browser process of this profile) is kept.
pub fn unpack(parent: &Path) -> Result<PathBuf, String> {
    let target = parent.join(FOLDER);
    if fs::read_to_string(target.join(MARKER)).is_ok_and(|kept| kept == SHA256) {
        return Ok(target);
    }
    let failed = |e: std::io::Error| format!("The built-in ad blocker could not be unpacked: {e}");
    fs::create_dir_all(parent).map_err(failed)?;
    let fresh = unique(parent, "new");
    fs::create_dir(&fresh).map_err(failed)?;
    let written = read(ZIP).and_then(|entries| write_all(&entries, &fresh));
    if let Err(error) = written {
        let _ = fs::remove_dir_all(&fresh);
        return Err(error);
    }
    let old = unique(parent, "old");
    if target.exists()
        && let Err(error) = fs::rename(&target, &old)
    {
        let _ = fs::remove_dir_all(&fresh);
        return Err(failed(error));
    }
    if let Err(error) = fs::rename(&fresh, &target) {
        let _ = fs::rename(&old, &target);
        let _ = fs::remove_dir_all(&fresh);
        return Err(failed(error));
    }
    let _ = fs::remove_dir_all(&old);
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    /// A minimal ZIP: each file stored (method 0) or deflated (method 8).
    fn zip(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for (name, data, deflate) in files {
            let packed = if *deflate {
                miniz_oxide::deflate::compress_to_vec(data, 6)
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            let header = |sig: &[u8], central: bool| {
                let mut h = sig.to_vec();
                if central {
                    h.extend_from_slice(&20u16.to_le_bytes());
                }
                h.extend_from_slice(&20u16.to_le_bytes());
                h.extend_from_slice(&0u16.to_le_bytes());
                h.extend_from_slice(&method.to_le_bytes());
                h.extend_from_slice(&[0; 8]); // time, date, CRC (not checked)
                h.extend_from_slice(&(packed.len() as u32).to_le_bytes());
                h.extend_from_slice(&(data.len() as u32).to_le_bytes());
                h.extend_from_slice(&(name.len() as u16).to_le_bytes());
                h.extend_from_slice(&0u16.to_le_bytes());
                if central {
                    // Comment length, disk, internal and external attributes.
                    h.extend_from_slice(&[0; 10]);
                    h.extend_from_slice(&offset.to_le_bytes());
                }
                h.extend_from_slice(name.as_bytes());
                h
            };
            out.extend(header(b"PK\x03\x04", false));
            out.extend(&packed);
            central.extend(header(b"PK\x01\x02", true));
        }
        let start = out.len() as u32;
        out.extend(&central);
        out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn the_embedded_zip_is_the_pinned_release() {
        let digest = sha2::Sha256::digest(ZIP);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, SHA256);
        let entries = read(ZIP).unwrap();
        assert!(entries.iter().all(|e| e.name.starts_with(PREFIX)));
        let manifest = entries
            .iter()
            .find(|e| e.name == "uBlock0.chromium/manifest.json")
            .unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest.data).unwrap();
        assert_eq!(manifest["version"], VERSION);
        assert_eq!(manifest["name"], "uBlock Origin");
        assert!(
            entries
                .iter()
                .any(|e| e.name == "uBlock0.chromium/LICENSE.txt")
        );
    }

    #[test]
    fn stored_and_deflated_files_are_read() {
        let text = b"hello hello hello hello".repeat(20);
        let bytes = zip(&[("a/one.txt", b"1", false), ("a/two.txt", &text, true)]);
        let entries = read(&bytes).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0],
            Entry {
                name: "a/one.txt",
                data: b"1".to_vec()
            }
        );
        assert_eq!(entries[1].data, text);
    }

    #[test]
    fn unsafe_or_damaged_archives_are_refused() {
        for name in ["../evil", "/abs", "a/../../b", "C:/x", "a\\b", "a/./b/../c"] {
            assert!(read(&zip(&[(name, b"x", false)])).is_err(), "{name}");
        }
        let good = zip(&[("a.txt", b"x", false)]);
        assert!(read(&good[..good.len() - 1]).is_err());
        assert!(read(b"not a zip").is_err());
        let mut encrypted = good.clone();
        encrypted[6] = 1; // the local header's flags; the central one follows
        let central = good.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        encrypted[central + 8] = 1;
        assert!(read(&encrypted).is_err());
    }

    #[test]
    fn the_release_is_unpacked_once_and_replaced_when_it_changes() {
        let parent = std::env::temp_dir().join(format!("rb-bundled-{}", std::process::id()));
        let _ = fs::remove_dir_all(&parent);
        let folder = unpack(&parent).unwrap();
        assert!(folder.join("manifest.json").is_file());
        assert_eq!(fs::read_to_string(folder.join(MARKER)).unwrap(), SHA256);
        // Unchanged: kept as it is.
        fs::write(folder.join("extra"), "kept").unwrap();
        assert_eq!(unpack(&parent).unwrap(), folder);
        assert!(folder.join("extra").exists());
        // Another release's copy (or an unfinished one): replaced whole.
        fs::write(folder.join(MARKER), "other").unwrap();
        assert_eq!(unpack(&parent).unwrap(), folder);
        assert!(!folder.join("extra").exists());
        let leftovers = fs::read_dir(&parent).unwrap().count();
        assert_eq!(leftovers, 1, "only the unpacked folder is left");
        fs::remove_dir_all(&parent).unwrap();
    }
}
