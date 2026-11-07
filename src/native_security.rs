//! WebView2 permissions and data clearing. COM objects stay on the UI apartment.
use crate::app::Event;
use rust_browser::{
    privacy::ClearRequest,
    security::{self, Capability, Decision, Rule},
    tabs::TabId,
};
use serde::Serialize;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use webview2_com::{
    ClearBrowsingDataCompletedHandler, Microsoft::Web::WebView2::Win32::*,
    PermissionRequestedEventHandler,
};
use windows::core::{BOOL, Interface, PWSTR};
use winit::event_loop::EventLoopProxy;
use wry::{WebView, WebViewExtWindows};

pub type Shared = Rc<RefCell<Permissions>>;
#[derive(Clone, Debug, Serialize)]
pub struct Prompt {
    pub id: u64,
    pub tab: TabId,
    pub origin: String,
    pub capability: Capability,
}
struct Pending {
    prompt: Prompt,
    args: ICoreWebView2PermissionRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
    created: Instant,
}
#[derive(Default)]
pub struct Permissions {
    pub enabled: bool,
    pub rules: Vec<Rule>,
    pending: Vec<Pending>,
    next: u64,
    pub active: Option<TabId>,
}
fn kind(value: COREWEBVIEW2_PERMISSION_KIND) -> Option<Capability> {
    match value {
        COREWEBVIEW2_PERMISSION_KIND_CAMERA => Some(Capability::Camera),
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => Some(Capability::Microphone),
        COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION => Some(Capability::Geolocation),
        COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS => Some(Capability::Notifications),
        _ => None,
    }
}
pub fn install(
    view: &WebView,
    shared: Shared,
    proxy: EventLoopProxy<Event>,
    tab: TabId,
) -> windows::core::Result<()> {
    // SAFETY: registration and callbacks use live COM interfaces on their UI thread.
    unsafe {
        let mut token = 0;
        view.webview().add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |sender, args| {
                let Some(args) = args else { return Ok(()) };
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                let Ok(extended) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() else {
                    return Ok(());
                };
                extended.SetSavesInProfile(false)?;
                // Handle child-frame requests here as well. Cross-origin frames are denied below.
                extended.SetHandled(true)?;
                let mut raw = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut raw)?;
                let Some(capability) = kind(raw) else {
                    return Ok(());
                };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let Some(origin) = security::permission_origin(&webview2_com::take_pwstr(uri))
                else {
                    return Ok(());
                };
                let Some(sender) = sender else { return Ok(()) };
                let mut source = PWSTR::null();
                sender.Source(&mut source)?;
                if security::origin(&webview2_com::take_pwstr(source)).as_deref()
                    != Some(origin.as_str())
                {
                    return Ok(());
                }
                let mut manager = shared.borrow_mut();
                if !manager.enabled {
                    return Ok(());
                }
                if manager.active != Some(tab) {
                    return Ok(());
                }
                if let Some(rule) = manager
                    .rules
                    .iter()
                    .find(|r| r.origin == origin && r.capability == capability)
                {
                    if rule.decision == Decision::Allow {
                        args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                    }
                    return Ok(());
                }
                let mut gesture = BOOL::default();
                args.IsUserInitiated(&mut gesture)?;
                if !gesture.as_bool()
                    || manager.pending.len() >= 4
                    || manager.pending.iter().any(|p| {
                        p.prompt.tab == tab
                            && p.prompt.origin == origin
                            && p.prompt.capability == capability
                    })
                {
                    return Ok(());
                }
                let deferral = args.GetDeferral()?;
                manager.next += 1;
                let prompt = Prompt {
                    id: manager.next,
                    tab,
                    origin,
                    capability,
                };
                manager.pending.push(Pending {
                    prompt,
                    args,
                    deferral,
                    created: Instant::now(),
                });
                let _ = proxy.send_event(Event::PermissionPrompt);
                Ok(())
            })),
            &mut token,
        )
    }
}
impl Permissions {
    pub fn prompt(&self) -> Option<Prompt> {
        self.pending.first().map(|p| p.prompt.clone())
    }
    pub fn resolve(&mut self, id: u64, decision: Decision) -> windows::core::Result<()> {
        let Some(index) = self.pending.iter().position(|p| p.prompt.id == id) else {
            return Ok(());
        };
        let pending = self.pending.remove(index);
        // SAFETY: retained event args/deferral never leave this apartment.
        unsafe {
            let state = if decision == Decision::Allow
                && self.active == Some(pending.prompt.tab)
                && pending.created.elapsed() < Duration::from_secs(60)
            {
                COREWEBVIEW2_PERMISSION_STATE_ALLOW
            } else {
                COREWEBVIEW2_PERMISSION_STATE_DENY
            };
            let result = pending.args.SetState(state);
            let complete = pending.deferral.Complete();
            result.and(complete)
        }
    }
    pub fn cancel_tab(&mut self, tab: TabId) {
        let ids: Vec<_> = self
            .pending
            .iter()
            .filter(|p| p.prompt.tab == tab)
            .map(|p| p.prompt.id)
            .collect();
        for id in ids {
            let _ = self.resolve(id, Decision::Deny);
        }
    }
    pub fn cancel_all(&mut self) {
        let ids: Vec<_> = self.pending.iter().map(|p| p.prompt.id).collect();
        for id in ids {
            let _ = self.resolve(id, Decision::Deny);
        }
    }
    pub fn expire(&mut self) -> bool {
        let ids: Vec<_> = self
            .pending
            .iter()
            .filter(|p| p.created.elapsed() >= Duration::from_secs(60))
            .map(|p| p.prompt.id)
            .collect();
        let changed = !ids.is_empty();
        for id in ids {
            let _ = self.resolve(id, Decision::Deny);
        }
        changed
    }
}
impl Drop for Permissions {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

pub fn verify_private(view: &WebView) -> windows::core::Result<()> {
    // Wry silently falls back on old runtimes. Refuse that fallback for private windows.
    unsafe {
        let core: ICoreWebView2_13 = view.webview().cast()?;
        let profile = core.Profile()?;
        let mut enabled = BOOL::default();
        profile.IsInPrivateModeEnabled(&mut enabled)?;
        if !enabled.as_bool() {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80004005u32 as i32,
            )));
        }
    }
    Ok(())
}
pub fn clear(
    view: &WebView,
    request: &ClearRequest,
    startup: bool,
    ticket: u64,
    proxy: EventLoopProxy<Event>,
) -> windows::core::Result<()> {
    let mut kinds = 0;
    if request.history {
        kinds |= COREWEBVIEW2_BROWSING_DATA_KINDS_BROWSING_HISTORY.0;
    }
    if request.cookies {
        kinds |= COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_SITE.0;
    }
    if request.cache {
        kinds |= COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE.0
            | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE.0;
    }
    if request.permissions {
        kinds |= COREWEBVIEW2_BROWSING_DATA_KINDS_SETTINGS.0;
    }
    if kinds == 0 {
        let _ = proxy.send_event(Event::EngineCleared {
            ticket,
            startup,
            success: true,
        });
        return Ok(());
    }
    // SAFETY: asynchronous handler only sends an owned result; the profile stays live.
    unsafe {
        let core: ICoreWebView2_13 = view.webview().cast()?;
        let profile: ICoreWebView2Profile2 = core.Profile()?.cast()?;
        profile.ClearBrowsingData(
            COREWEBVIEW2_BROWSING_DATA_KINDS(kinds),
            &ClearBrowsingDataCompletedHandler::create(Box::new(move |result| {
                let _ = proxy.send_event(Event::EngineCleared {
                    ticket,
                    startup,
                    success: result.is_ok(),
                });
                Ok(())
            })),
        )
    }
}
