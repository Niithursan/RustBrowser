//! Optional native lifecycle APIs. No view is destroyed to reduce resource use.
use crate::app::Event;
use rust_browser::tabs::TabId;
use webview2_com::{Microsoft::Web::WebView2::Win32::*, TrySuspendCompletedHandler};
use windows::core::{BOOL, Interface};
use winit::event_loop::EventLoopProxy;
use wry::{WebView, WebViewExtWindows};

pub fn playing_audio(view: &WebView) -> windows::core::Result<bool> {
    // SAFETY: live interfaces and valid output storage on the UI apartment.
    unsafe {
        let core: ICoreWebView2_8 = view.webview().cast()?;
        let mut playing = BOOL::default();
        core.IsDocumentPlayingAudio(&mut playing)?;
        Ok(playing.as_bool())
    }
}
pub fn suspended(view: &WebView) -> windows::core::Result<bool> {
    // SAFETY: the view remains live on this thread throughout the call.
    unsafe {
        let core: ICoreWebView2_3 = view.webview().cast()?;
        let mut value = BOOL::default();
        core.IsSuspended(&mut value)?;
        Ok(value.as_bool())
    }
}
pub fn resume(view: &WebView) -> windows::core::Result<()> {
    // SAFETY: synchronous native call in the owning UI apartment.
    unsafe {
        let core: ICoreWebView2_3 = view.webview().cast()?;
        core.Resume()?;
    }
    if suspended(view)? {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x80004005u32 as i32,
        )));
    }
    Ok(())
}
pub fn suspend(
    view: &WebView,
    proxy: EventLoopProxy<Event>,
    id: TabId,
    ticket: u64,
) -> windows::core::Result<()> {
    // SAFETY: the COM callback carries only owned scalar IDs across event dispatch.
    unsafe {
        let core: ICoreWebView2_3 = view.webview().cast()?;
        core.TrySuspend(&TrySuspendCompletedHandler::create(Box::new(
            move |result, success| {
                let _ = proxy.send_event(Event::SuspensionResult {
                    id,
                    ticket,
                    success: result.is_ok() && success,
                });
                Ok(())
            },
        )))
    }
}
