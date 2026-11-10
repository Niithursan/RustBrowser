//! Conservative view-lifetime protection, without reading request bodies or logging URLs.
use crate::app::Event;
use rust_browser::tabs::TabId;
use std::{cell::Cell, rc::Rc};
use webview2_com::{
    ContainsFullScreenElementChangedEventHandler, Microsoft::Web::WebView2::Win32::*,
    ProcessFailedEventHandler, WebResourceRequestedEventHandler, WindowCloseRequestedEventHandler,
};
use windows::core::{BOOL, Interface};
use winit::event_loop::EventLoopProxy;
use wry::{WebView, WebViewExtWindows};
/// What a WebView2 process failure means for the tab that reported it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The shared browser process ended: every view in the profile is unusable.
    Browser,
    /// This tab's renderer ended; Reload creates a new one.
    Renderer,
    /// The renderer stopped responding and may still recover by itself.
    Unresponsive,
    /// Frame, GPU, utility and similar helpers. WebView2 restarts these itself.
    Helper,
}
pub fn classify(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> Failure {
    match kind {
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => Failure::Browser,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => Failure::Renderer,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => Failure::Unresponsive,
        _ => Failure::Helper,
    }
}
#[derive(Default)]
pub struct Activity {
    pub protected: Cell<bool>,
    pub observable: Cell<bool>,
    pub fullscreen: Cell<bool>,
}
/// Report failures of the toolbar view's own engine processes. The toolbar runs in a
/// separate WebView2 environment (profile/toolbar), so the content engine's handlers
/// never see them.
pub fn watch_toolbar(view: &WebView, proxy: EventLoopProxy<Event>) -> windows::core::Result<()> {
    // SAFETY: the handler and its proxy remain in the view's UI apartment.
    unsafe {
        let mut token = 0;
        view.webview().add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    args.ProcessFailedKind(&mut kind)?;
                    let _ = proxy.send_event(Event::ToolbarProcess(classify(kind)));
                }
                Ok(())
            })),
            &mut token,
        )
    }
}

pub fn install(
    view: &WebView,
    activity: Rc<Activity>,
    proxy: EventLoopProxy<Event>,
    id: TabId,
) -> windows::core::Result<()> {
    // SAFETY: handlers and their state remain in the view's UI apartment.
    unsafe {
        let core = view.webview();
        let mut token = 0;
        let p = proxy.clone();
        core.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                let _ = p.send_event(Event::PageClose(id));
                Ok(())
            })),
            &mut token,
        )?;
        let p = proxy.clone();
        core.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    args.ProcessFailedKind(&mut kind)?;
                    let _ = p.send_event(Event::EngineProcess(id, classify(kind)));
                }
                Ok(())
            })),
            &mut token,
        )?;
        let a = activity.clone();
        let p = proxy.clone();
        core.add_ContainsFullScreenElementChanged(
            &ContainsFullScreenElementChangedEventHandler::create(Box::new(move |view, _| {
                if let Some(view) = view {
                    let mut value = BOOL::default();
                    view.ContainsFullScreenElement(&mut value)?;
                    a.fullscreen.set(value.as_bool());
                    let _ = p.send_event(Event::Fullscreen(id, value.as_bool()));
                }
                Ok(())
            })),
            &mut token,
        )?;
        Ok(())
    }
}
pub fn watch_resources(view: &WebView, activity: Rc<Activity>) -> windows::core::Result<()> {
    // SAFETY: called after SetNewWindow for linked popups, on the owning UI thread.
    unsafe {
        let core = view.webview();
        let mut token = 0;
        // Include service workers. If unavailable, refuse automatic sleeping for this view.
        if let Ok(resources) = core.cast::<ICoreWebView2_22>()
            && resources
                .AddWebResourceRequestedFilterWithRequestSourceKinds(
                    windows::core::w!("*"),
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                )
                .is_ok()
        {
            let a = activity.clone();
            core.add_WebResourceRequested(
                &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        let request = args.Request()?;
                        let mut method = windows::core::PWSTR::null();
                        request.Method(&mut method)?;
                        let method = webview2_com::take_pwstr(method);
                        let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
                        args.ResourceContext(&mut context)?;
                        if !matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS")
                            || context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA
                            || context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET
                        {
                            a.protected.set(true);
                        }
                    }
                    Ok(())
                })),
                &mut token,
            )?;
            activity.observable.set(true);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_browser_and_renderer_exits_invalidate_views() {
        assert_eq!(
            classify(COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED),
            Failure::Browser
        );
        assert_eq!(
            classify(COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED),
            Failure::Renderer
        );
        assert_eq!(
            classify(COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE),
            Failure::Unresponsive
        );
        // Self-healing helpers must never mark a healthy page as failed.
        for kind in [
            COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
            COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
            COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED,
            COREWEBVIEW2_PROCESS_FAILED_KIND_SANDBOX_HELPER_PROCESS_EXITED,
            COREWEBVIEW2_PROCESS_FAILED_KIND_PPAPI_PLUGIN_PROCESS_EXITED,
            COREWEBVIEW2_PROCESS_FAILED_KIND_PPAPI_BROKER_PROCESS_EXITED,
            COREWEBVIEW2_PROCESS_FAILED_KIND_UNKNOWN_PROCESS_EXITED,
        ] {
            assert_eq!(classify(kind), Failure::Helper);
        }
    }
}
