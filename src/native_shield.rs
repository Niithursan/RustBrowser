//! The shield's blocked-request count, read from the engine's own network events.
//!
//! WebView2's extension interface reports only an extension's ID, name and whether it
//! is on: no counts and no per-site switch. What the engine does report, through the
//! DevTools protocol on the page's own view, is each request a client (an extension)
//! stopped: `Network.loadingFailed` with `net::ERR_BLOCKED_BY_CLIENT`, or a redirect
//! from a website address into `chrome-extension://` (a stand-in script). Counting
//! those two gives the exact number of requests extensions stopped for this page's
//! document; it cannot say which extension stopped them.
//!
//! Only counts are kept. Addresses are compared and dropped, never stored or logged.
//! The Network domain is enabled with no response buffering, and only for views in
//! the normal website profile when the user has added an extension.
use crate::app::Event;
use rust_browser::tabs::TabId;
use std::{cell::Cell, rc::Rc};
use webview2_com::{
    CallDevToolsProtocolMethodCompletedHandler, DevToolsProtocolEventReceivedEventHandler,
    Microsoft::Web::WebView2::Win32::*, NavigationStartingEventHandler,
};
use windows::core::{BOOL, HSTRING, PWSTR};
use winit::event_loop::EventLoopProxy;
use wry::{WebView, WebViewExtWindows};

/// The count for one page view.
#[derive(Default)]
pub struct Blocked {
    count: Cell<u32>,
    /// The engine accepted `Network.enable`: the count is real, not just zero.
    counting: Cell<bool>,
    /// Handlers were registered (at most once per view).
    watching: Cell<bool>,
    /// A change was announced and not yet rendered (one event per burst).
    announced: Cell<bool>,
}

impl Blocked {
    /// Requests stopped for the current document, if the engine is counting.
    pub fn count(&self) -> Option<u32> {
        self.counting.get().then(|| self.count.get())
    }

    /// The toolbar showed the latest count; the next change announces itself again.
    pub fn rendered(&self) {
        self.announced.set(false);
    }

    fn add(&self, proxy: &EventLoopProxy<Event>, id: TabId) {
        self.count.set(self.count.get().saturating_add(1));
        if !self.announced.replace(true) {
            let _ = proxy.send_event(Event::Blocked(id));
        }
    }

    fn reset(&self, proxy: &EventLoopProxy<Event>, id: TabId) {
        if self.count.replace(0) != 0 && !self.announced.replace(true) {
            let _ = proxy.send_event(Event::Blocked(id));
        }
    }
}

/// A `Network.loadingFailed` parameter object that a client (extension) caused.
pub fn blocked_failure(json: &str) -> bool {
    // Cheap test first: most failures are not blocks, and none need parsing then.
    json.contains("ERR_BLOCKED_BY_CLIENT")
        && serde_json::from_str::<serde_json::Value>(json).is_ok_and(|v| {
            v.get("errorText").and_then(|e| e.as_str()) == Some("net::ERR_BLOCKED_BY_CLIENT")
        })
}

/// A `Network.requestWillBeSent` parameter object for a redirect from a website
/// address into an extension's own resources (a request an extension replaced).
pub fn blocked_redirect(json: &str) -> bool {
    const EXTENSION: &str = "chrome-extension://";
    // Every request passes here; parse only the rare candidates.
    if !json.contains(EXTENSION) || !json.contains("\"redirectResponse\"") {
        return false;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return false;
    };
    let to = value
        .pointer("/request/url")
        .and_then(|u| u.as_str())
        .unwrap_or("");
    let from = value
        .pointer("/redirectResponse/url")
        .and_then(|u| u.as_str());
    to.starts_with(EXTENSION) && from.is_some_and(|from| !from.starts_with(EXTENSION))
}

/// # Safety
/// `args` must be the live event arguments of the callback being handled.
unsafe fn parameters(args: &ICoreWebView2DevToolsProtocolEventReceivedEventArgs) -> String {
    let mut json = PWSTR::null();
    // SAFETY: a property read on live arguments; take_pwstr frees the string.
    if unsafe { args.ParameterObjectAsJson(&mut json) }.is_err() {
        return String::new();
    }
    webview2_com::take_pwstr(json)
}

/// Start counting requests extensions stop in `view`. Does nothing if it already
/// counts. The count restarts with every new top-level document.
pub fn watch(
    view: &WebView,
    blocked: Rc<Blocked>,
    proxy: EventLoopProxy<Event>,
    id: TabId,
) -> windows::core::Result<()> {
    if blocked.watching.replace(true) {
        return Ok(());
    }
    // SAFETY: registration and callbacks run in this content view's UI apartment;
    // the handlers keep only an Rc to the count and an event proxy.
    unsafe {
        let core = view.webview();
        let mut token = 0;
        let (b, p) = (blocked.clone(), proxy.clone());
        core.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    // A refused navigation keeps the current document and its count.
                    let mut cancelled = BOOL::default();
                    args.Cancel(&mut cancelled)?;
                    let mut redirected = BOOL::default();
                    args.IsRedirected(&mut redirected)?;
                    if !cancelled.as_bool() && !redirected.as_bool() {
                        b.reset(&p, id);
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        let (b, p) = (blocked.clone(), proxy.clone());
        core.GetDevToolsProtocolEventReceiver(&HSTRING::from("Network.loadingFailed"))?
            .add_DevToolsProtocolEventReceived(
                &DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args
                        && blocked_failure(&parameters(&args))
                    {
                        b.add(&p, id);
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        let (b, p) = (blocked.clone(), proxy.clone());
        core.GetDevToolsProtocolEventReceiver(&HSTRING::from("Network.requestWillBeSent"))?
            .add_DevToolsProtocolEventReceived(
                &DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args
                        && blocked_redirect(&parameters(&args))
                    {
                        b.add(&p, id);
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        // No response bodies or upload data are kept for DevTools.
        let (b, p) = (blocked, proxy);
        core.CallDevToolsProtocolMethod(
            &HSTRING::from("Network.enable"),
            &HSTRING::from(
                r#"{"maxTotalBufferSize":0,"maxResourceBufferSize":0,"maxPostDataSize":0}"#,
            ),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _| {
                if result.is_ok() {
                    b.counting.set(true);
                    let _ = p.send_event(Event::Blocked(id));
                }
                Ok(())
            })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_client_blocks_are_counted() {
        assert!(blocked_failure(
            r#"{"requestId":"1","errorText":"net::ERR_BLOCKED_BY_CLIENT","canceled":false}"#
        ));
        // Other failures, and the text appearing elsewhere, are not blocks.
        assert!(!blocked_failure(
            r#"{"requestId":"1","errorText":"net::ERR_CONNECTION_REFUSED"}"#
        ));
        assert!(!blocked_failure(
            r#"{"requestId":"1","errorText":"net::ERR_ABORTED","note":"ERR_BLOCKED_BY_CLIENT"}"#
        ));
        assert!(!blocked_failure(
            r#"{"errorText":"net::ERR_BLOCKED_BY_RESPONSE"}"#
        ));
        assert!(!blocked_failure("not json ERR_BLOCKED_BY_CLIENT"));
    }

    #[test]
    fn only_redirects_from_websites_into_extensions_are_counted() {
        let ext = "chrome-extension://abcdefghijklmnopabcdefghijklmnop/stand-in.js";
        let redirect = |from: &str, to: &str| {
            format!(
                r#"{{"request":{{"url":"{to}"}},"redirectResponse":{{"url":"{from}","status":307}}}}"#
            )
        };
        assert!(blocked_redirect(&redirect(
            "http://ads.example.test/a.js",
            ext
        )));
        // A first request (no redirect), a website redirect, and a redirect inside
        // the extension are not blocks.
        assert!(!blocked_redirect(&format!(
            r#"{{"request":{{"url":"{ext}"}}}}"#
        )));
        assert!(!blocked_redirect(&redirect(
            "http://a.example.test/",
            "https://a.example.test/"
        )));
        assert!(!blocked_redirect(&redirect(ext, ext)));
        assert!(!blocked_redirect(
            r#"{"request":{"url":"http://x.test/?q=chrome-extension://"},"redirectResponse":{"url":"http://x.test/"}}"#
        ));
    }
}
