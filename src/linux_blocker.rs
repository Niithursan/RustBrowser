//! The built-in ad blocker on Linux: hBlock's list (`rust_browser::hblock`),
//! downloaded by the browser and kept as WebKit filters beside the shield's
//! other filters (`content-filters`), with a small record of when it was
//! downloaded (`hblock.json`).
//!
//! Only a normal window downloads or prepares the list; private windows of the
//! profile load the prepared filters and never write. The download runs on its
//! own thread with libsoup's blocking API (its own session, HTTPS validated by
//! GIO's TLS), reads at most `hblock::MAX_BYTES`, and sends nothing but the
//! request for the list.
use rust_browser::hblock::{self, Kept};
use std::path::{Path, PathBuf};

const RECORD: &str = "hblock.json";

/// Seconds since 1970, UTC.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The record kept in `folder`, if it is there and usable.
pub fn read(folder: &Path) -> Option<Kept> {
    let text = std::fs::read(folder.join(RECORD)).ok()?;
    if text.len() > 4096 {
        return None;
    }
    serde_json::from_slice::<Kept>(&text)
        .ok()
        .filter(|kept| kept.valid(now()))
}

/// Keep `kept` in `folder` (written whole, then renamed into place).
pub fn write(folder: &Path, kept: &Kept) -> std::io::Result<()> {
    let text = serde_json::to_vec(kept).map_err(std::io::Error::other)?;
    let partial = folder.join(format!("{RECORD}.partial"));
    std::fs::write(&partial, text)?;
    std::fs::rename(partial, folder.join(RECORD))
}

/// Forget the record (the filters are removed through the shield).
pub fn forget(folder: &Path) {
    let _ = std::fs::remove_file(folder.join(RECORD));
}

/// Download hBlock's list and prepare it, off the UI thread; `done` receives the
/// prepared filters or why not.
pub fn fetch(
    done: impl FnOnce(Result<hblock::Prepared, String>) + Send + 'static,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("built-in-blocker".into())
        .spawn(move || done(download().and_then(|bytes| hblock::prepare(&bytes))))
        .map(|_| ())
}

fn download() -> Result<Vec<u8>, String> {
    use soup::prelude::*;
    use webkit2gtk::gio::prelude::InputStreamExt;
    let failed =
        |e: webkit2gtk::glib::Error| format!("hBlock's list could not be downloaded ({e}).");
    let session = soup::Session::new();
    session.set_timeout(60);
    session.set_user_agent("RustBrowser");
    let message = soup::Message::new("GET", hblock::LIST_URL)
        .map_err(|_| "hBlock's address could not be used.".to_string())?;
    let stream = session
        .send(&message, None::<&webkit2gtk::gio::Cancellable>)
        .map_err(failed)?;
    if message.status() != soup::Status::Ok {
        return Err(format!(
            "hBlock's site did not send the list ({:?}).",
            message.status()
        ));
    }
    let mut bytes = Vec::new();
    loop {
        let chunk = stream
            .read_bytes(64 * 1024, None::<&webkit2gtk::gio::Cancellable>)
            .map_err(failed)?;
        if chunk.is_empty() {
            break;
        }
        bytes.extend_from_slice(&chunk);
        if bytes.len() > hblock::MAX_BYTES {
            return Err("hBlock's list is larger than expected.".into());
        }
    }
    Ok(bytes)
}

/// The folder a profile keeps its filters in.
pub fn folder(profile: &Path) -> PathBuf {
    profile.join("content-filters")
}
