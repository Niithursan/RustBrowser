//! Deferred native popup requests stay on the UI apartment. No COM value crosses threads.
use crate::app::Event;
use rust_browser::{navigation, tabs::TabId};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, NewWindowRequestedEventHandler};
use windows::core::BOOL;
use winit::event_loop::EventLoopProxy;
use wry::{WebView, WebViewExtWindows};

pub type Shared = Rc<RefCell<Manager>>;
#[derive(Default)]
pub struct Manager {
    next: u64,
    pending: Vec<(u64, Request)>,
}
pub struct Request {
    pub source: TabId,
    pub url: String,
    pub environment: ICoreWebView2Environment,
    args: ICoreWebView2NewWindowRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
    created: Instant,
}
impl Request {
    pub fn attach(&self, view: &WebView) -> windows::core::Result<()> {
        // SAFETY: newly constructed, un-navigated view in the opener's environment/profile.
        unsafe { self.args.SetNewWindow(&view.webview()) }
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        // SAFETY: a deferral must complete on every success, cancellation and error path.
        unsafe {
            let _ = self.deferral.Complete();
        }
    }
}
impl Manager {
    pub fn take(&mut self, id: u64) -> Option<Request> {
        let index = self.pending.iter().position(|(key, _)| *key == id)?;
        Some(self.pending.remove(index).1)
    }
    /// The tab that asked for popup `id`, so it opens in that tab's window.
    pub fn source(&self, id: u64) -> Option<TabId> {
        self.pending
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, r)| r.source)
    }
    pub fn cancel_tab(&mut self, tab: TabId) {
        self.pending.retain(|(_, r)| r.source != tab);
    }
    pub fn clear(&mut self) {
        self.pending.clear();
    }
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn expire(&mut self) {
        self.pending
            .retain(|(_, r)| r.created.elapsed() < Duration::from_secs(30));
    }
}
pub fn install(
    view: &WebView,
    manager: Shared,
    proxy: EventLoopProxy<Event>,
    source: TabId,
) -> windows::core::Result<()> {
    let environment = view.environment();
    // SAFETY: callback owns live interfaces on the owning UI thread, only a scalar ticket is posted.
    unsafe {
        let mut token = 0;
        view.webview().add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                args.SetHandled(true)?;
                let mut uri = windows::core::PWSTR::null();
                args.Uri(&mut uri)?;
                let url = webview2_com::take_pwstr(uri);
                let mut initiated = BOOL::default();
                args.IsUserInitiated(&mut initiated)?;
                #[cfg(test)]
                println!(
                    "Native popup callback; user initiated: {}",
                    initiated.as_bool()
                );
                if !initiated.as_bool()
                    || (url != "about:blank" && !navigation::allowed(&url))
                    || manager.borrow().pending.len() >= 8
                {
                    let _ = proxy.send_event(Event::PopupBlocked);
                    return Ok(());
                }
                let deferral = args.GetDeferral()?;
                let mut state = manager.borrow_mut();
                state.next += 1;
                let id = state.next;
                state.pending.push((
                    id,
                    Request {
                        source,
                        url,
                        environment: environment.clone(),
                        args,
                        deferral,
                        created: Instant::now(),
                    },
                ));
                if proxy.send_event(Event::PopupRequested(id)).is_err() {
                    state.take(id);
                }
                Ok(())
            })),
            &mut token,
        )
    }
}
