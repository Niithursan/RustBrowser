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
#[derive(Default)]
pub struct Activity {
    pub protected: Cell<bool>,
    pub observable: Cell<bool>,
    pub fullscreen: Cell<bool>,
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
        core.add_ProcessFailed(&ProcessFailedEventHandler::create(Box::new(move|_,_|{let _=p.send_event(Event::Page(id,crate::app::PageEvent::Error("A WebView2 process failed. Keep this tab and try Reload; if that fails, restart the browser. Unsaved page changes may be lost.".into())));Ok(())})),&mut token)?;
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
