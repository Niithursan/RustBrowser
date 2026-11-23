//! Linux download adapter over WebKitGTK's own download signals, with the same
//! rules as the Windows one: every download asks for a folder, only HTTP(S)
//! sources are accepted, the server's filename is never trusted, and the file is
//! written into a leased staging folder and moved into place only once complete,
//! never over an existing file.
//!
//! WebKit's signal handlers only post events: the manager is changed from the
//! event loop, never while WebKit is inside a signal, because `Download::cancel`
//! may report the failure before it returns.
use crate::linux::Event;
use gtk::prelude::{FileChooserExt, NativeDialogExt};
use rust_browser::{
    downloads::{ProgressWatch, Record, Staged, safe_filename},
    navigation,
};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};
use webkit2gtk::{DownloadExt, URIRequestExt, URIResponseExt, WebContextExt};
use winit::event_loop::EventLoopProxy;

pub type Shared = Rc<RefCell<Manager>>;

/// The most transfers at once, and the most rows kept, as on Windows.
const MAX_ACTIVE: usize = 16;
const MAX_ENTRIES: usize = 256;

struct Entry {
    record: Record,
    /// The transfer while WebKit may still write to it.
    download: Option<webkit2gtk::Download>,
    /// The folder chooser while it is open.
    chooser: Option<gtk::FileChooserNative>,
    stage: Option<Staged>,
    source: String,
    started: Instant,
    progress: ProgressWatch,
}

impl Entry {
    fn live(&self) -> bool {
        self.download.is_some() || self.record.status == "Finishing"
    }
}

#[derive(Default)]
pub struct Manager {
    entries: Vec<Entry>,
    next_id: u64,
}

/// Take over every download of the website views' WebKit context.
pub fn install(context: &webkit2gtk::WebContext, manager: Shared, proxy: EventLoopProxy<Event>) {
    context.connect_download_started(move |_, download| {
        // wry gives every view it builds a download handler that saves straight
        // into the Downloads folder (its default), and the first view's runs
        // before this one: remove what it put on this download first.
        take_destination(download);
        let source = download
            .request()
            .and_then(|request| request.uri())
            .map(|uri| uri.to_string())
            .unwrap_or_default();
        let Some(id) = manager.borrow_mut().add(download, source) else {
            download.cancel();
            let _ = proxy.send_event(Event::DownloadRejected);
            return;
        };
        // A file never replaces another: the staging folder is new and empty.
        download.set_allow_overwrite(false);
        let decide = proxy.clone();
        download.connect_decide_destination(move |_, suggested| {
            let _ = decide.send_event(Event::DownloadRequested(id, suggested.to_owned()));
            // Handled, which also keeps handlers connected later (wry's, from
            // later views) from running: WebKit waits for `set_destination` or
            // `cancel`.
            true
        });
        let failed = proxy.clone();
        download.connect_failed(move |_, error| {
            let cancelled = error.matches(webkit2gtk::DownloadError::CancelledByUser);
            let _ = failed.send_event(Event::DownloadStopped(id, cancelled));
        });
        // Also emitted after `failed`, which then has already ended the entry.
        let done = proxy.clone();
        download.connect_finished(move |_| {
            let _ = done.send_event(Event::DownloadDone(id));
        });
    });
}

/// Disconnect every `decide-destination` handler already on `download`, so the
/// browser alone chooses where it is saved.
fn take_destination(download: &webkit2gtk::Download) {
    use gtk::glib::{
        gobject_ffi,
        prelude::{ObjectType, StaticType},
        subclass::signal::SignalId,
        translate::IntoGlib,
    };
    let Some(signal) = SignalId::lookup("decide-destination", webkit2gtk::Download::static_type())
    else {
        return;
    };
    // SAFETY: `download` is a live GObject borrowed for the call, and only
    // handlers of this one signal on this one object are matched; no closure
    // pointer is read.
    unsafe {
        gobject_ffi::g_signal_handlers_disconnect_matched(
            download.as_ptr().cast(),
            gobject_ffi::G_SIGNAL_MATCH_ID,
            signal.into_glib(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
    }
}

impl Manager {
    /// Record a new transfer from `source`, or refuse it: not HTTP(S), already
    /// being downloaded, or too many at once.
    fn add(&mut self, download: &webkit2gtk::Download, source: String) -> Option<u64> {
        if !navigation::allowed(&source)
            || self.active_count() >= MAX_ACTIVE
            || self.entries.iter().any(|e| e.source == source && e.live())
        {
            return None;
        }
        if self.entries.len() >= MAX_ENTRIES {
            let index = self.entries.iter().position(|e| !e.live())?;
            self.entries.remove(index);
        }
        self.next_id += 1;
        let id = self.next_id;
        self.entries.push(Entry {
            record: Record {
                id,
                name: safe_filename(&source),
                status: "Starting".into(),
                received: 0,
                total: 0,
                folder: None,
                path: None,
                detail: String::new(),
            },
            download: Some(download.clone()),
            chooser: None,
            stage: None,
            source,
            started: Instant::now(),
            progress: ProgressWatch::default(),
        });
        Some(id)
    }

    fn entry(&mut self, id: u64) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.record.id == id)
    }

    pub fn records(&self) -> Vec<Record> {
        self.entries
            .iter()
            .rev()
            .map(|e| e.record.clone())
            .collect()
    }

    pub fn active_count(&self) -> usize {
        self.entries.iter().filter(|e| e.live()).count()
    }

    pub fn folder(&self, id: u64) -> Option<PathBuf> {
        self.entries
            .iter()
            .find(|e| e.record.id == id)?
            .stage
            .as_ref()
            .map(|s| s.folder.clone())
    }

    /// The website view that started a transfer, to find its tab.
    pub fn view(&self, id: u64) -> Option<webkit2gtk::WebView> {
        self.entries
            .iter()
            .find(|e| e.record.id == id)?
            .download
            .as_ref()?
            .web_view()
    }

    /// WebKit has the server's filename and waits for a destination: ask for a
    /// folder. Returns whether the transfer still wants one.
    pub fn requested(&mut self, id: u64, suggested: &str, proxy: &EventLoopProxy<Event>) -> bool {
        let Some(entry) = self.entry(id).filter(|e| e.download.is_some()) else {
            return false;
        };
        entry.record.name = safe_filename(suggested);
        entry.record.status = "Choose destination".into();
        let chooser = gtk::FileChooserNative::new(
            Some("Save download to"),
            None::<&gtk::Window>,
            gtk::FileChooserAction::SelectFolder,
            Some("Save here"),
            Some("Cancel"),
        );
        if let Some(folder) = gtk::glib::user_special_dir(gtk::glib::UserDirectory::Downloads) {
            chooser.set_current_folder(folder);
        }
        let chosen = proxy.clone();
        chooser.connect_response(move |chooser, response| {
            let folder = (response == gtk::ResponseType::Accept)
                .then(|| chooser.filename())
                .flatten();
            let _ = chosen.send_event(Event::DownloadDestination(id, folder));
        });
        chooser.show();
        entry.chooser = Some(chooser);
        true
    }

    /// The folder chooser closed with `folder`, or was cancelled.
    pub fn destination(&mut self, id: u64, folder: Option<PathBuf>) {
        let Some(entry) = self.entry(id) else {
            return;
        };
        if let Some(chooser) = entry.chooser.take() {
            chooser.destroy();
        }
        let Some(download) = entry.download.clone() else {
            return;
        };
        let Some(folder) = folder else {
            entry.record.status = "Cancelled".into();
            entry.download = None;
            download.cancel();
            return;
        };
        let stage = match Staged::create(&folder, &entry.record.name) {
            Ok(stage) => stage,
            Err(_) => {
                entry.record.status = "Failed".into();
                entry.record.detail = "Could not prepare the chosen destination folder.".into();
                entry.download = None;
                download.cancel();
                return;
            }
        };
        let Some(path) = stage.temporary.to_str() else {
            stage.clean_partial();
            entry.record.status = "Failed".into();
            entry.record.detail = "The chosen folder's name cannot be used.".into();
            entry.download = None;
            download.cancel();
            return;
        };
        download.set_destination(path);
        entry.record.folder = Some(stage.folder.display().to_string());
        entry.record.status = "Downloading".into();
        entry.started = Instant::now();
        entry.progress = ProgressWatch::default();
        entry.stage = Some(stage);
    }

    pub fn cancel(&mut self, id: u64) {
        let Some(entry) = self.entry(id) else {
            return;
        };
        let Some(download) = entry.download.clone() else {
            return;
        };
        if let Some(chooser) = entry.chooser.take() {
            chooser.destroy();
            entry.download = None;
            entry.record.status = "Cancelled".into();
        } else {
            entry.record.status = "Cancelling".into();
        }
        download.cancel();
    }

    /// WebKit gave up on the transfer, or it was cancelled.
    pub fn stopped(&mut self, id: u64, cancelled: bool) {
        let Some(entry) = self.entry(id) else {
            return;
        };
        if let Some(chooser) = entry.chooser.take() {
            chooser.destroy();
        }
        entry.download = None;
        if let Some(stage) = &entry.stage {
            stage.clean_partial();
        }
        // An outcome the browser already chose (a stall, a refused folder) stands.
        if matches!(
            entry.record.status.as_str(),
            "Starting" | "Choose destination" | "Downloading" | "Cancelling"
        ) {
            entry.record.status = if cancelled { "Cancelled" } else { "Failed" }.into();
            if !cancelled {
                entry.record.detail = "The transfer was interrupted.".into();
            }
        }
    }

    /// WebKit finished writing: move the file into its folder off the UI thread.
    pub fn done(&mut self, id: u64, proxy: &EventLoopProxy<Event>) {
        let Some(entry) = self.entry(id) else {
            return;
        };
        if entry.record.status != "Downloading" {
            return;
        }
        if let Some(download) = entry.download.take() {
            let received = i64::try_from(download.received_data_length()).unwrap_or(i64::MAX);
            entry.record.received = received;
            entry.record.total = entry.record.total.max(received);
        }
        let Some(stage) = entry.stage.clone() else {
            entry.record.status = "Failed".into();
            return;
        };
        entry.record.status = "Finishing".into();
        let proxy = proxy.clone();
        let spawned = std::thread::Builder::new()
            .name("download-finalizer".into())
            .spawn(move || {
                let result = stage.publish().map_err(|_| {
                    "Could not safely move the completed file. The staged file was preserved."
                        .to_string()
                });
                let _ = proxy.send_event(Event::DownloadFinished(id, result));
            });
        if spawned.is_err() {
            entry.record.status = "Failed".into();
            entry.record.detail =
                "Could not start file finalization; the staged file was preserved.".into();
        }
    }

    pub fn finished(&mut self, id: u64, result: Result<PathBuf, String>) {
        let Some(entry) = self.entry(id) else {
            return;
        };
        match result {
            Ok(path) => {
                entry.record.status = "Completed".into();
                entry.record.name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into();
                entry.record.path = Some(path.display().to_string());
            }
            Err(error) => {
                entry.record.status = "Failed".into();
                entry.record.detail = error;
            }
        }
    }

    /// Read progress; cancel a transfer that has not advanced for two minutes.
    /// Returns whether any row changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for entry in &mut self.entries {
            if entry.record.status != "Downloading" {
                continue;
            }
            let Some(download) = entry.download.clone() else {
                continue;
            };
            let received = i64::try_from(download.received_data_length()).unwrap_or(i64::MAX);
            let total = download
                .response()
                .map(|r| i64::try_from(r.content_length()).unwrap_or(0))
                .unwrap_or(0);
            entry.progress.observe(received, entry.started.elapsed());
            if entry.progress.stalled(entry.started.elapsed()) {
                entry.record.status = "Failed".into();
                entry.record.detail =
                    "No progress for two minutes; the transfer was cancelled.".into();
                entry.download = None;
                download.cancel();
                if let Some(stage) = &entry.stage {
                    stage.clean_partial();
                }
                changed = true;
                continue;
            }
            let received = entry.progress.bytes();
            if received != entry.record.received || total != entry.record.total {
                entry.record.received = received;
                entry.record.total = total;
                changed = true;
            }
        }
        changed
    }

    /// Cancel every transfer, as the browser closes.
    pub fn shutdown(&mut self) {
        for entry in &mut self.entries {
            if let Some(chooser) = entry.chooser.take() {
                chooser.destroy();
            }
            if let Some(download) = entry.download.take() {
                download.cancel();
                if let Some(stage) = &entry.stage {
                    stage.clean_partial();
                }
            }
        }
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Show a finished download's folder in the file manager. Only the folder is
/// opened, never the downloaded file, and no shell runs.
pub fn open_folder(path: &Path) -> Result<(), String> {
    use gtk::gio::prelude::FileExt;
    let path = path
        .canonicalize()
        .map_err(|_| "Download folder no longer exists.")?;
    if !path.is_dir() {
        return Err("Download folder no longer exists.".into());
    }
    let uri = gtk::gio::File::for_path(&path).uri();
    gtk::gio::AppInfo::launch_default_for_uri(&uri, None::<&gtk::gio::AppLaunchContext>)
        .map_err(|_| "No file manager could open the download folder.".into())
}
