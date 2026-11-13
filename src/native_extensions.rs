//! Browser extensions in the normal website profile, through WebView2's own
//! extension support. Rust Browser never downloads or bundles an extension: the
//! user adds an unpacked folder they downloaded. WebView2 keeps the installed and
//! enabled state in the profile; the settings only remember which extensions the
//! user added, so WebView2's built-in extensions are never listed or removed.
use crate::app::Event;
use serde::Serialize;
use std::path::Path;
use webview2_com::{
    BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler,
    Microsoft::Web::WebView2::Win32::*, ProfileAddBrowserExtensionCompletedHandler,
    ProfileGetBrowserExtensionsCompletedHandler,
};
use windows::core::{BOOL, HSTRING, Interface, PWSTR};
use winit::event_loop::EventLoopProxy;
use wry::{WebView, WebViewExtWindows};

/// Largest `manifest.json` read when checking a folder.
const MAX_MANIFEST: u64 = 1024 * 1024;

/// An extension as WebView2 reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Info {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

/// Results of extension operations, delivered on the UI thread.
#[derive(Debug)]
pub enum Reply {
    Folder(Result<Option<std::path::PathBuf>, String>),
    Added(Result<(Info, String), String>),
    Listed(Result<Vec<Info>, String>),
    Changed(String, Result<bool, String>),
    Removed(String, Result<(), String>),
    /// Every extension of a profile, built-in ones included (tests only).
    #[cfg(test)]
    Installed(Result<Vec<Info>, String>),
}

/// Whether `id` looks like a WebView2 extension ID (32 letters a to p).
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// Whether a manifest asks for a permission that lets an extension block or
/// redirect requests (what a content blocker needs). Asking is not proof that it
/// blocks ads; it is what Rust Browser can tell without running it.
pub fn asks_to_block(manifest: &serde_json::Value) -> bool {
    const BLOCKING: [&str; 3] = [
        "declarativeNetRequest",
        "declarativeNetRequestWithHostAccess",
        "webRequestBlocking",
    ];
    manifest
        .get("permissions")
        .and_then(|p| p.as_array())
        .is_some_and(|list| {
            list.iter()
                .any(|p| p.as_str().is_some_and(|p| BLOCKING.contains(&p)))
        })
}

/// Whether the extension in `folder` asks to block requests, if its manifest can
/// still be read (extensions added before v0.21 did not record it).
pub fn folder_asks_to_block(folder: &Path) -> Option<bool> {
    read_manifest(folder).ok().map(|m| asks_to_block(&m))
}

fn read_manifest(folder: &Path) -> Result<serde_json::Value, String> {
    if !folder.is_dir() {
        return Err("Choose the extension's folder (the one containing manifest.json).".into());
    }
    let manifest = folder.join("manifest.json");
    let metadata = std::fs::metadata(&manifest).map_err(|_| {
        "This folder has no manifest.json. Choose the unpacked extension folder.".to_string()
    })?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST {
        return Err("This folder's manifest.json is not a valid extension manifest.".into());
    }
    let text = std::fs::read(&manifest)
        .map_err(|_| "The extension's manifest.json could not be read.".to_string())?;
    let text = text.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&text);
    serde_json::from_slice(text)
        .map_err(|_| "This folder's manifest.json is not valid JSON.".to_string())
}

/// Check that `folder` holds an unpacked extension before WebView2 sees it.
/// Returns the manifest's version for display and whether it asks to block
/// requests.
pub fn check_folder(folder: &Path) -> Result<(String, bool), String> {
    let value = read_manifest(folder)?;
    let version = value.get("version").and_then(|v| v.as_str());
    let manifest_version = value.get("manifest_version").and_then(|v| v.as_u64());
    match (
        value.get("name").and_then(|v| v.as_str()),
        version,
        manifest_version,
    ) {
        (Some(_), Some(version), Some(2 | 3)) => {
            Ok((version.chars().take(32).collect(), asks_to_block(&value)))
        }
        _ => Err("This folder's manifest.json is not a valid extension manifest.".into()),
    }
}

fn profile(view: &WebView) -> windows::core::Result<ICoreWebView2Profile7> {
    // SAFETY: interface queries on a live view, on the UI thread that owns it.
    unsafe { view.webview().cast::<ICoreWebView2_13>()?.Profile()?.cast() }
}

/// # Safety
/// `extension` must be a live WebView2 extension object.
unsafe fn info(extension: &ICoreWebView2BrowserExtension) -> windows::core::Result<Info> {
    let (mut id, mut name, mut enabled) = (PWSTR::null(), PWSTR::null(), BOOL::default());
    // SAFETY: plain property reads on a live object; strings are freed by take_pwstr.
    unsafe {
        extension.Id(&mut id)?;
        extension.Name(&mut name)?;
        extension.IsEnabled(&mut enabled)?;
    }
    Ok(Info {
        id: webview2_com::take_pwstr(id),
        name: webview2_com::take_pwstr(name)
            .chars()
            .filter(|c| !c.is_control())
            .take(80)
            .collect(),
        enabled: enabled.as_bool(),
    })
}

fn send(proxy: &EventLoopProxy<Event>, reply: Reply) {
    let _ = proxy.send_event(Event::Extension(reply));
}

fn failed(error: windows::core::Error) -> String {
    format!("WebView2 reported an error ({:#010x}).", error.code().0)
}

/// Add an unpacked extension folder to the website profile.
pub fn add(view: &WebView, folder: &Path, proxy: EventLoopProxy<Event>) -> Result<(), String> {
    let path = folder.display().to_string();
    let profile = profile(view).map_err(|_| "Extensions are not available here.".to_string())?;
    let handler =
        ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |result, extension| {
            let reply = match (result, extension) {
                // SAFETY: the extension object is live for this callback.
                (Ok(()), Some(extension)) => unsafe { info(&extension) }
                    .map(|info| (info, path.clone()))
                    .map_err(failed),
                (Ok(()), None) => Err("WebView2 did not return the added extension.".into()),
                (Err(error), _) => Err(format!(
                    "WebView2 could not add this extension. {}",
                    failed(error)
                )),
            };
            send(&proxy, Reply::Added(reply));
            Ok(())
        }));
    // SAFETY: the handler only sends owned results; the profile stays live.
    unsafe { profile.AddBrowserExtension(&HSTRING::from(folder.as_os_str()), &handler) }
        .map_err(|_| "WebView2 could not add this extension.".to_string())
}

/// Visit every installed extension (including WebView2's built-in ones).
fn with_extensions(
    view: &WebView,
    each: impl FnOnce(Vec<ICoreWebView2BrowserExtension>) + 'static,
    proxy: EventLoopProxy<Event>,
    on_error: impl FnOnce(String) -> Reply + 'static,
) -> Result<(), String> {
    let profile = profile(view).map_err(|_| "Extensions are not available here.".to_string())?;
    let mut each = Some(each);
    let mut on_error = Some(on_error);
    let handler =
        ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |result, list| {
            let collected = result.and_then(|()| {
                let mut found = Vec::new();
                if let Some(list) = list {
                    let mut count = 0;
                    // SAFETY: reads on a live collection during the callback.
                    unsafe {
                        list.Count(&mut count)?;
                        for index in 0..count {
                            found.push(list.GetValueAtIndex(index)?);
                        }
                    }
                }
                Ok(found)
            });
            match collected {
                Ok(found) => {
                    if let Some(each) = each.take() {
                        each(found);
                    }
                }
                Err(error) => {
                    if let Some(on_error) = on_error.take() {
                        send(&proxy, on_error(failed(error)));
                    }
                }
            }
            Ok(())
        }));
    // SAFETY: the handler only uses owned values; the profile stays live.
    unsafe { profile.GetBrowserExtensions(&handler) }
        .map_err(|_| "WebView2 could not list extensions.".to_string())
}

/// List the installed extensions whose IDs are in `ids` (the ones the user added).
pub fn list(view: &WebView, ids: Vec<String>, proxy: EventLoopProxy<Event>) -> Result<(), String> {
    let reply = proxy.clone();
    with_extensions(
        view,
        move |all| {
            let found = all
                .iter()
                // SAFETY: live objects from the completed list.
                .filter_map(|extension| unsafe { info(extension) }.ok())
                .filter(|info| ids.contains(&info.id))
                .collect();
            send(&reply, Reply::Listed(Ok(found)));
        },
        proxy,
        |error| Reply::Listed(Err(error)),
    )
}

/// Turn an extension the user added on or off.
pub fn set_enabled(
    view: &WebView,
    id: String,
    enabled: bool,
    proxy: EventLoopProxy<Event>,
) -> Result<(), String> {
    let reply = proxy.clone();
    let key = id.clone();
    with_extensions(
        view,
        move |all| {
            // SAFETY: live objects from the completed list.
            let target = all
                .into_iter()
                .find(|extension| unsafe { info(extension) }.is_ok_and(|i| i.id == id));
            let Some(target) = target else {
                send(
                    &reply,
                    Reply::Changed(id, Err("This extension is not installed.".into())),
                );
                return;
            };
            let done = reply.clone();
            let changed = id.clone();
            let handler = BrowserExtensionEnableCompletedHandler::create(Box::new(move |result| {
                send(
                    &done,
                    Reply::Changed(changed.clone(), result.map(|()| enabled).map_err(failed)),
                );
                Ok(())
            }));
            // SAFETY: the handler only sends owned results.
            if unsafe { target.Enable(enabled, &handler) }.is_err() {
                send(
                    &reply,
                    Reply::Changed(id, Err("WebView2 could not change this extension.".into())),
                );
            }
        },
        proxy,
        move |error| Reply::Changed(key, Err(error)),
    )
}

/// Remove an extension the user added from the profile.
pub fn remove(view: &WebView, id: String, proxy: EventLoopProxy<Event>) -> Result<(), String> {
    let reply = proxy.clone();
    let key = id.clone();
    with_extensions(
        view,
        move |all| {
            // SAFETY: live objects from the completed list.
            let target = all
                .into_iter()
                .find(|extension| unsafe { info(extension) }.is_ok_and(|i| i.id == id));
            let Some(target) = target else {
                // Already gone: forgetting it is enough.
                send(&reply, Reply::Removed(id, Ok(())));
                return;
            };
            let done = reply.clone();
            let removed = id.clone();
            let handler = BrowserExtensionRemoveCompletedHandler::create(Box::new(move |result| {
                send(
                    &done,
                    Reply::Removed(removed.clone(), result.map_err(failed)),
                );
                Ok(())
            }));
            // SAFETY: the handler only sends owned results.
            if unsafe { target.Remove(&handler) }.is_err() {
                send(
                    &reply,
                    Reply::Removed(id, Err("WebView2 could not remove this extension.".into())),
                );
            }
        },
        proxy,
        move |error| Reply::Removed(key, Err(error)),
    )
}

/// Every extension installed in a view's profile, for tests (toolbar and private
/// profiles must have none the user added).
#[cfg(test)]
pub fn installed(view: &WebView, proxy: EventLoopProxy<Event>) -> Result<(), String> {
    let reply = proxy.clone();
    with_extensions(
        view,
        move |all| {
            let found = all
                .iter()
                // SAFETY: live objects from the completed list.
                .filter_map(|extension| unsafe { info(extension) }.ok())
                .collect();
            send(&reply, Reply::Installed(Ok(found)));
        },
        proxy,
        |error| Reply::Installed(Err(error)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_ids_are_checked() {
        assert!(valid_id("nhhbkmgedjdemoadhmhiinbgbhhgbgnk"));
        assert!(!valid_id("nhhbkmgedjdemoadhmhiinbgbhhgbgn"));
        assert!(!valid_id("NHHBKMGEDJDEMOADHMHIINBGBHHGBGNK"));
        assert!(!valid_id("zhhbkmgedjdemoadhmhiinbgbhhgbgnk"));
        assert!(!valid_id("../bkmgedjdemoadhmhiinbgbhhgbgnk"));
    }

    #[test]
    fn only_blocking_permissions_count_as_asking_to_block() {
        let ask = |json: &str| asks_to_block(&serde_json::from_str(json).unwrap());
        assert!(ask(r#"{"permissions":["declarativeNetRequest"]}"#));
        assert!(ask(
            r#"{"permissions":["storage","declarativeNetRequestWithHostAccess"]}"#
        ));
        assert!(ask(
            r#"{"permissions":["webRequest","webRequestBlocking"]}"#
        ));
        assert!(!ask(r#"{"permissions":["webRequest","storage"]}"#));
        assert!(!ask(
            r#"{"optional_permissions":["declarativeNetRequest"]}"#
        ));
        assert!(!ask(r#"{"content_scripts":[]}"#));
        assert!(!ask(r#"{"permissions":"declarativeNetRequest"}"#));
        let fixtures =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions");
        assert_eq!(folder_asks_to_block(&fixtures.join("blocker")), Some(true));
        assert_eq!(
            folder_asks_to_block(&fixtures.join("marker-a")),
            Some(false)
        );
        assert_eq!(folder_asks_to_block(&fixtures.join("missing")), None);
    }

    #[test]
    fn folders_without_a_valid_manifest_are_refused() {
        let root = std::env::temp_dir().join(format!(
            "rust-browser-ext-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert!(check_folder(&root).is_err(), "no manifest");
        assert!(check_folder(&root.join("missing")).is_err(), "no folder");
        for bad in [
            "not json",
            r#"{"name":"x","version":"1"}"#,
            r#"{"name":"x","version":"1","manifest_version":1}"#,
            r#"{"version":"1","manifest_version":3}"#,
            r#"[1,2,3]"#,
        ] {
            std::fs::write(root.join("manifest.json"), bad).unwrap();
            assert!(check_folder(&root).is_err(), "{bad}");
        }
        std::fs::write(
            root.join("manifest.json"),
            "\u{feff}{\"name\":\"Test\",\"version\":\"1.2.3\",\"manifest_version\":3}",
        )
        .unwrap();
        assert_eq!(check_folder(&root).unwrap(), ("1.2.3".to_string(), false));
        let big = format!(
            "{{\"name\":\"x\",\"version\":\"1\",\"manifest_version\":3,\"pad\":\"{}\"}}",
            "a".repeat(MAX_MANIFEST as usize)
        );
        std::fs::write(root.join("manifest.json"), big).unwrap();
        assert!(check_folder(&root).is_err(), "too large");
        let _ = std::fs::remove_dir_all(root);
    }
}
