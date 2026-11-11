//! Windows-only download adapter. COM operations never leave their owning UI apartment.
use crate::app::Event;
use rust_browser::{
    downloads::{ProgressWatch, Staged, safe_filename},
    navigation,
};
use serde::Serialize;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};
use webview2_com::{
    DownloadStartingEventHandler, Microsoft::Web::WebView2::Win32::*, StateChangedEventHandler,
};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
        UI::Shell::{
            Common::COMDLG_FILTERSPEC, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
            FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
        },
    },
    core::{HSTRING, Interface, PWSTR},
};
use winit::event_loop::EventLoopProxy;
use wry::{WebView, WebViewExtWindows};

pub type Shared = Rc<RefCell<Manager>>;
#[derive(Clone, Debug, Serialize)]
pub struct Record {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub received: i64,
    pub total: i64,
    pub folder: Option<String>,
    pub path: Option<String>,
    pub detail: String,
}
struct Entry {
    record: Record,
    operation: Option<ICoreWebView2DownloadOperation>,
    request: Option<(
        ICoreWebView2DownloadStartingEventArgs,
        ICoreWebView2Deferral,
    )>,
    stage: Option<Staged>,
    source: String,
    started: Instant,
    progress: ProgressWatch,
}
#[derive(Default)]
pub struct Manager {
    entries: Vec<Entry>,
    next_id: u64,
}

// SAFETY: only call for COM-allocated, null-terminated output strings on this thread.
pub unsafe fn take_string(value: PWSTR) -> String {
    let owned = webview2_com::CoTaskMemPWSTR::from(value);
    owned.to_string()
}

pub fn install(
    view: &WebView,
    manager: Shared,
    proxy: EventLoopProxy<Event>,
    tab_id: rust_browser::tabs::TabId,
) -> windows::core::Result<()> {
    // SAFETY: register against the live WebView2 object in its owning UI apartment.
    unsafe {
        let core: ICoreWebView2_4 = view.webview().cast()?;
        let mut token = 0;
        core.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                args.SetCancel(true)?; // Fail closed if any later initialization fails.
                args.SetHandled(true)?; // No default UI offers to open/execute the file.
                let operation = args.DownloadOperation()?;
                let mut uri = PWSTR::null();
                operation.Uri(&mut uri)?;
                let url = take_string(uri);
                let mut state = manager.borrow_mut();
                if !navigation::allowed(&url)
                    || state.active_count() >= 16
                    || state.entries.iter().any(|entry| {
                        entry.source == url
                            && (entry.operation.is_some() || entry.record.status == "Finishing")
                    })
                {
                    args.SetCancel(true)?;
                    let _ = proxy.send_event(Event::DownloadRejected);
                    return Ok(());
                }
                if state.entries.len() >= 256 {
                    if let Some(index) = state.entries.iter().position(|e| {
                        e.operation.is_none()
                            && e.request.is_none()
                            && e.record.status != "Finishing"
                    }) {
                        state.entries.remove(index);
                    } else {
                        args.SetCancel(true)?;
                        return Ok(());
                    }
                }
                let mut suggested = PWSTR::null();
                args.ResultFilePath(&mut suggested)?;
                let name = safe_filename(&take_string(suggested));
                state.next_id += 1;
                let id = state.next_id;
                let notified = Rc::new(std::cell::Cell::new(false));
                let events = proxy.clone();
                let mut state_token = 0;
                operation.add_StateChanged(
                    &StateChangedEventHandler::create(Box::new(move |operation, _| {
                        let Some(operation) = operation else {
                            return Ok(());
                        };
                        let mut status = COREWEBVIEW2_DOWNLOAD_STATE::default();
                        operation.State(&mut status)?;
                        if status != COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS
                            && !notified.replace(true)
                        {
                            if status == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
                                let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
                                operation.InterruptReason(&mut reason)?;
                                // Capture the first interruption and stop automatic retries.
                                let _ = operation.Cancel();
                                let _ = events.send_event(Event::DownloadStopped(id, reason.0));
                            } else {
                                let _ = events.send_event(Event::DownloadTick);
                            }
                        }
                        Ok(())
                    })),
                    &mut state_token,
                )?;
                let deferral = args.GetDeferral()?;
                state.entries.push(Entry {
                    source: url,
                    started: Instant::now(),
                    progress: ProgressWatch::default(),
                    record: Record {
                        id,
                        name,
                        status: "Choose destination".into(),
                        received: 0,
                        total: 0,
                        folder: None,
                        path: None,
                        detail: String::new(),
                    },
                    operation: Some(operation),
                    request: Some((args, deferral)),
                    stage: None,
                });
                if proxy
                    .send_event(Event::DownloadRequested(id, tab_id))
                    .is_err()
                {
                    let _ = state.cancel(id);
                }
                Ok(())
            })),
            &mut token,
        )
    }
}

impl Manager {
    pub fn records(&self) -> Vec<Record> {
        self.entries
            .iter()
            .rev()
            .map(|e| e.record.clone())
            .collect()
    }
    pub fn active_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.operation.is_some() || e.record.status == "Finishing")
            .count()
    }
    pub fn folder(&self, id: u64) -> Option<PathBuf> {
        self.entries
            .iter()
            .find(|e| e.record.id == id)?
            .stage
            .as_ref()
            .map(|s| s.folder.clone())
    }

    pub fn destination(&mut self, id: u64, result: Result<Option<PathBuf>, String>) {
        let Some(entry) = self.entries.iter_mut().find(|e| e.record.id == id) else {
            return;
        };
        let Some((args, deferral)) = entry.request.take() else {
            return;
        };
        // SAFETY: deferral and event args were retained and used only on the UI thread.
        unsafe {
            let prepared = match result {
                Ok(Some(folder)) => Staged::create(&folder, &entry.record.name)
                    .map(Some)
                    .map_err(|_| "Could not prepare the chosen destination folder.".to_string()),
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            match prepared {
                Ok(Some(stage)) => {
                    if args
                        .SetResultFilePath(&HSTRING::from(
                            dunce::simplified(&stage.temporary).as_os_str(),
                        ))
                        .is_ok()
                        && args.SetCancel(false).is_ok()
                    {
                        entry.record.folder =
                            Some(dunce::simplified(&stage.folder).display().to_string());
                        entry.record.status = "Downloading".into();
                        entry.started = Instant::now();
                        entry.progress = ProgressWatch::default();
                        entry.stage = Some(stage);
                    } else {
                        stage.clean_partial();
                        let _ = args.SetCancel(true);
                        entry.operation = None;
                        entry.record.status = "Failed".into();
                        entry.record.detail = "WebView2 rejected the destination.".into();
                    }
                }
                other => {
                    let _ = args.SetCancel(true);
                    entry.operation = None;
                    entry.record.status = if other.is_err() {
                        "Failed"
                    } else {
                        "Cancelled"
                    }
                    .into();
                    if let Err(message) = other {
                        entry.record.detail = message;
                    }
                }
            }
            if deferral.Complete().is_err() {
                entry.record.status = "Failed".into();
                entry.record.detail = "Could not complete the download request.".into();
                if let Some(operation) = entry.operation.take() {
                    let _ = operation.Cancel();
                }
            }
        }
    }

    pub fn cancel(&mut self, id: u64) -> windows::core::Result<()> {
        let Some(entry) = self.entries.iter_mut().find(|e| e.record.id == id) else {
            return Ok(());
        };
        // SAFETY: all operations remain on their owning UI apartment.
        unsafe {
            if let Some((args, deferral)) = entry.request.take() {
                args.SetCancel(true)?;
                deferral.Complete()?;
                entry.operation = None;
                entry.record.status = "Cancelled".into();
            } else if let Some(operation) = &entry.operation {
                operation.Cancel()?;
                entry.record.status = "Cancelling".into();
            }
        }
        Ok(())
    }

    pub fn poll(&mut self, proxy: &EventLoopProxy<Event>) -> bool {
        let mut changed = false;
        for entry in &mut self.entries {
            if entry.request.is_some() {
                continue;
            }
            let Some(operation) = &entry.operation else {
                continue;
            };
            // SAFETY: live operations queried on UI thread; all output pointers are valid.
            unsafe {
                let mut received = 0;
                let mut total = 0;
                let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
                if operation.State(&mut state).is_err() {
                    let _ = operation.Cancel();
                    entry.operation = None;
                    entry.record.status = "Failed".into();
                    entry.record.detail =
                        "Download operation became unavailable; any partial file was preserved."
                            .into();
                    changed = true;
                    continue;
                }
                let _ = operation.BytesReceived(&mut received);
                let _ = operation.TotalBytesToReceive(&mut total);
                entry.progress.observe(received, entry.started.elapsed());
                received = entry.progress.bytes();
                if state == COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS
                    && entry.progress.stalled(entry.started.elapsed())
                {
                    let _ = operation.Cancel();
                    entry.operation = None;
                    entry.record.status = "Failed".into();
                    entry.record.detail =
                        "No progress for two minutes; the transfer was cancelled.".into();
                    if let Some(stage) = &entry.stage {
                        stage.clean_partial();
                    }
                    changed = true;
                    continue;
                }
                if received != entry.record.received || total != entry.record.total {
                    changed = true;
                    entry.record.received = received;
                    entry.record.total = total;
                }
                if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
                    entry.operation = None;
                    entry.record.status = "Finishing".into();
                    changed = true;
                    if let Some(stage) = entry.stage.clone() {
                        let id = entry.record.id;
                        let proxy = proxy.clone();
                        let spawned=std::thread::Builder::new().name("download-finalizer".into()).spawn(move||{
                            let result=stage.publish().map_err(|_|"Could not safely move the completed file. The staged file was preserved.".to_string());
                            let _=proxy.send_event(Event::DownloadFinished(id,result));
                        });
                        if spawned.is_err() {
                            entry.record.status = "Failed".into();
                            entry.record.detail =
                                "Could not start file finalization; the staged file was preserved."
                                    .into();
                        }
                    }
                } else if state == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
                    let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
                    let _ = operation.InterruptReason(&mut reason);
                    let _ = operation.Cancel(); // Do not allow an interrupted transfer to auto-resume after cleanup.
                    entry.record.status =
                        if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED {
                            "Cancelled"
                        } else {
                            "Failed"
                        }
                        .into();
                    entry.record.detail = format!("WebView2 interruption code {}", reason.0);
                    entry.operation = None;
                    if let Some(stage) = &entry.stage {
                        stage.clean_partial();
                    }
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn finished(&mut self, id: u64, result: Result<PathBuf, String>) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.record.id == id) {
            match result {
                Ok(path) => {
                    entry.record.status = "Completed".into();
                    entry.record.name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into();
                    entry.record.path = Some(dunce::simplified(&path).display().to_string());
                }
                Err(error) => {
                    entry.record.status = "Failed".into();
                    entry.record.detail = error;
                }
            }
        }
    }

    pub fn shutdown(&mut self) {
        let ids: Vec<_> = self.entries.iter().map(|e| e.record.id).collect();
        for id in ids {
            let _ = self.cancel(id);
        }
        for entry in &mut self.entries {
            entry.operation = None;
        }
    }

    pub fn stopped(&mut self, id: u64, reason: i32) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.record.id == id) {
            if matches!(
                entry.record.status.as_str(),
                "Finishing" | "Completed" | "Failed"
            ) {
                return;
            }
            entry.record.status =
                if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED.0 {
                    "Cancelled"
                } else {
                    "Failed"
                }
                .into();
            entry.record.detail = format!("WebView2 interruption code {reason}");
            entry.operation = None;
            if let Some(stage) = &entry.stage {
                stage.clean_partial();
            }
        }
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn choose_folder(owner: isize) -> Result<Option<PathBuf>, String> {
    // SAFETY: this helper runs on its own STA thread. The dialog and returned shell
    // items never leave that apartment; only an owned filesystem path is returned.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|_| "Could not initialize the folder picker.".to_string())?;
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| "Could not create the destination picker.".to_string())?;
        dialog
            .SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .map_err(|_| "Could not configure destination picker.".to_string())?;
        dialog
            .SetTitle(&HSTRING::from("Choose a folder for this download"))
            .map_err(|_| "Could not configure destination picker.".to_string())?;
        if let Err(error) = dialog.Show(Some(HWND(owner as *mut _))) {
            if error.code().0 as u32 == 0x800704c7 {
                return Ok(None);
            }
            return Err("Destination selection failed.".into());
        }
        let item = dialog
            .GetResult()
            .map_err(|_| "No destination was selected.".to_string())?;
        let path = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|_| "Choose a filesystem folder.".to_string())?;
        let value = path
            .to_string()
            .map_err(|_| "Invalid destination path.".to_string());
        CoTaskMemFree(Some(path.0 as *const _));
        Ok(Some(PathBuf::from(value?)))
    }
}

/// Ask for an exported bookmarks file and read it, refusing anything larger than
/// the import limit before reading it all. Runs on its own STA thread like
/// `choose_folder`. Returns `None` when the user cancels.
pub fn choose_bookmarks_file(owner: isize) -> Result<Option<Vec<u8>>, String> {
    use std::io::Read;
    let limit = rust_browser::bookmark_import::MAX_FILE_BYTES;
    // SAFETY: as in `choose_folder`, the dialog and shell items stay on this STA
    // thread; only an owned path leaves the block.
    let path = unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|_| "Could not open the file picker.".to_string())?;
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| "Could not open the file picker.".to_string())?;
        let html = HSTRING::from("Exported bookmarks (*.html; *.htm)");
        let pattern = HSTRING::from("*.html;*.htm");
        dialog
            .SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: windows::core::PCWSTR(html.as_ptr()),
                pszSpec: windows::core::PCWSTR(pattern.as_ptr()),
            }])
            .and_then(|()| {
                dialog.SetOptions(FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            })
            .and_then(|()| {
                dialog.SetTitle(&HSTRING::from(
                    "Import bookmarks from an exported HTML file",
                ))
            })
            .map_err(|_| "Could not configure the file picker.".to_string())?;
        if let Err(error) = dialog.Show(Some(HWND(owner as *mut _))) {
            if error.code().0 as u32 == 0x800704c7 {
                return Ok(None);
            }
            return Err("Choosing the file failed.".into());
        }
        let item = dialog
            .GetResult()
            .map_err(|_| "No file was chosen.".to_string())?;
        let name = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|_| "Choose a file on this computer.".to_string())?;
        let value = name
            .to_string()
            .map_err(|_| "Invalid file name.".to_string());
        CoTaskMemFree(Some(name.0 as *const _));
        PathBuf::from(value?)
    };
    let file =
        std::fs::File::open(&path).map_err(|_| "The file could not be opened.".to_string())?;
    let metadata = file
        .metadata()
        .map_err(|_| "The file could not be read.".to_string())?;
    if !metadata.is_file() {
        return Err("Choose a file, not a folder.".into());
    }
    if metadata.len() > limit {
        return Err(rust_browser::bookmark_import::FileError::TooLarge.message());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "The file could not be read.".to_string())?;
    Ok(Some(bytes))
}

pub fn open_folder(path: &Path) -> std::io::Result<()> {
    let path = path.canonicalize()?;
    if !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Download folder no longer exists.",
        ));
    }
    // No shell, URL, downloaded executable, or user-supplied command is invoked.
    std::process::Command::new("explorer.exe")
        .arg(dunce::simplified(&path))
        .spawn()?;
    Ok(())
}
