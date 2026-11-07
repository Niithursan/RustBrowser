//! Windows engine adapter. Keep COM and WebView2 details out of application state.
use crate::{app::Event, navigation, ui::Command};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, HistoryChangedEventHandler, NavigationCompletedEventHandler,
    SourceChangedEventHandler,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU};
use windows::core::BOOL;
use winit::{event_loop::EventLoopProxy, window::Window};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

pub struct Engine {
    pub view: WebView,
}

impl Engine {
    pub fn new(
        window: &Window,
        context: &mut WebContext,
        proxy: EventLoopProxy<Event>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let nav_proxy = proxy.clone();
        let popup_proxy = proxy.clone();
        let load_proxy = proxy.clone();
        let view = WebViewBuilder::new_with_web_context(context)
            // Preserve WebView2 security defaults instead of Wry's default browser arguments.
            .with_additional_browser_args("")
            .with_autoplay(false)
            .with_focused(false)
            .with_navigation_handler(move |url| {
                let allowed = navigation::allowed(&url);
                if !allowed {
                    let _ = nav_proxy
                        .send_event(Event::Error("Blocked a non-HTTP(S) navigation.".into()));
                }
                allowed
            })
            .with_new_window_req_handler(move |url, _| {
                let _ = popup_proxy.send_event(Event::Command(Command::Navigate(url)));
                wry::NewWindowResponse::Deny
            })
            .with_permission_handler(|_| wry::PermissionResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .with_on_page_load_handler(move |event, _| {
                if matches!(event, wry::PageLoadEvent::Started) {
                    let _ = load_proxy.send_event(Event::Loading);
                }
            })
            .build_as_child(window)?;
        unsafe {
            let core = view.webview();
            let mut token = 0;
            let p = proxy.clone();
            core.add_SourceChanged(
                &SourceChangedEventHandler::create(Box::new(move |_, _| {
                    let _ = p.send_event(Event::Changed);
                    Ok(())
                })),
                &mut token,
            )?;
            let p = proxy.clone();
            core.add_HistoryChanged(
                &HistoryChangedEventHandler::create(Box::new(move |_, _| {
                    let _ = p.send_event(Event::Changed);
                    Ok(())
                })),
                &mut token,
            )?;
            core.add_NavigationCompleted(&NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut success = BOOL::default();
                    args.IsSuccess(&mut success)?;
                    let error = if success.as_bool() { None } else {
                        let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                        args.WebErrorStatus(&mut status)?;
                        Some(format!("Navigation failed (WebView2 status {}). Check the address or connection and retry.", status.0))
                    };
                    let _ = proxy.send_event(Event::Completed(error));
                }
                Ok(())
            })), &mut token)?;
        }
        Ok(Self { view })
    }

    pub fn history(&self) -> (bool, bool) {
        // SAFETY: UI thread owns a live WebView2 COM object; outputs are valid BOOLs.
        unsafe {
            let mut back = BOOL::default();
            let mut forward = BOOL::default();
            let core = self.view.webview();
            let _ = core.CanGoBack(&mut back);
            let _ = core.CanGoForward(&mut forward);
            (back.as_bool(), forward.as_bool())
        }
    }

    pub fn command(&self, command: &Command) -> Result<(), Box<dyn std::error::Error>> {
        // Native history preserves redirects, POST navigation and same-document entries.
        // SAFETY: all calls happen on the event-loop/COM apartment thread.
        unsafe {
            let core = self.view.webview();
            match command {
                Command::Back => core.GoBack()?,
                Command::Forward => core.GoForward()?,
                Command::Reload => core.Reload()?,
                Command::Stop => core.Stop()?,
                _ => {}
            }
        }
        Ok(())
    }
}

/// Register on both controllers: embedded webviews consume keys before winit sees them.
pub fn accelerators(view: &WebView, proxy: EventLoopProxy<Event>) -> windows::core::Result<()> {
    // SAFETY: registration and callback execute in the owning UI apartment.
    unsafe {
        let mut token = 0;
        view.controller().add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                    args.KeyEventKind(&mut kind)?;
                    if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                        && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                    {
                        return Ok(());
                    }
                    let mut key = 0;
                    args.VirtualKey(&mut key)?;
                    let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                    let alt = GetKeyState(VK_MENU.0 as i32) < 0;
                    let command = match (key, ctrl, alt) {
                        (0x4c, true, _) | (0x44, _, true) | (0x75, _, _) => {
                            Some(Command::FocusAddress)
                        }
                        (0x52, true, _) | (0x74, _, _) => Some(Command::Reload),
                        (0x25, _, true) => Some(Command::Back),
                        (0x27, _, true) => Some(Command::Forward),
                        (0x1b, _, _) => Some(Command::Stop),
                        (0x09, true, _) => Some(Command::FocusPage),
                        _ => None,
                    };
                    if let Some(command) = command {
                        args.SetHandled(true)?;
                        let _ = proxy.send_event(Event::Command(command));
                    }
                }
                Ok(())
            })),
            &mut token,
        )
    }
}
