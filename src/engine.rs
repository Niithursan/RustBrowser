//! Windows engine adapter. Keep COM and WebView2 details out of application state.
use crate::{
    app::{Event, PageEvent},
    ui::Command,
};
use rust_browser::{navigation, tabs::TabId};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, HistoryChangedEventHandler,
    LaunchingExternalUriSchemeEventHandler, NavigationCompletedEventHandler,
    SourceChangedEventHandler,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::core::{BOOL, Interface};
use winit::{event_loop::EventLoopProxy, window::Window};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

pub struct Engine {
    pub view: WebView,
    pub activity: std::rc::Rc<crate::native_activity::Activity>,
}
pub struct Options {
    pub private: bool,
    pub environment: Option<ICoreWebView2Environment>,
}

impl Engine {
    pub fn new(
        window: &Window,
        context: &mut WebContext,
        proxy: EventLoopProxy<Event>,
        id: TabId,
        downloads: crate::native_downloads::Shared,
        permissions: crate::native_security::Shared,
        options: Options,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let nav_proxy = proxy.clone();
        let load_proxy = proxy.clone();
        let private = options.private;
        let related = options.environment.is_some();
        let builder = WebViewBuilder::new_with_web_context(context);
        let builder = if let Some(environment) = options.environment {
            builder.with_environment(environment)
        } else {
            builder
        };
        let view = builder
            // Preserve WebView2 security defaults instead of Wry's default browser arguments.
            .with_additional_browser_args("")
            .with_autoplay(false)
            .with_clipboard(false)
            .with_incognito(private)
            .with_focused(false)
            .with_navigation_handler(move |url| {
                let allowed = url == "about:blank" || navigation::allowed(&url);
                if !allowed {
                    let _ = nav_proxy.send_event(Event::Page(
                        id,
                        PageEvent::Error("Blocked a non-HTTP(S) navigation.".into()),
                    ));
                }
                allowed
            })
            .with_on_page_load_handler(move |event, _| {
                if matches!(event, wry::PageLoadEvent::Started) {
                    let _ = load_proxy.send_event(Event::Page(id, PageEvent::Loading));
                }
            })
            .with_visible(false)
            .with_document_title_changed_handler({
                let p = proxy.clone();
                move |title| {
                    let _ = p.send_event(Event::Page(id, PageEvent::Title(title)));
                }
            })
            .build_as_child(window)?;
        if private {
            crate::native_security::verify_private(&view)?;
        }
        crate::native_security::install(&view, permissions, proxy.clone(), id)?;
        crate::native_downloads::install(&view, downloads, proxy.clone(), id)?;
        // SAFETY: handlers are registered against live WebView2 objects on the UI/COM
        // apartment thread. They enqueue owned events; no COM object crosses threads.
        unsafe {
            let core = view.webview();
            let mut token = 0;
            let external: ICoreWebView2_18 = core.cast()?;
            external.add_LaunchingExternalUriScheme(
                &LaunchingExternalUriSchemeEventHandler::create(Box::new(|_, args| {
                    if let Some(args) = args {
                        args.SetCancel(true)?;
                    }
                    Ok(())
                })),
                &mut token,
            )?;
            let p = proxy.clone();
            core.add_SourceChanged(
                &SourceChangedEventHandler::create(Box::new(move |_, _| {
                    let _ = p.send_event(Event::Page(id, PageEvent::Changed));
                    Ok(())
                })),
                &mut token,
            )?;
            let p = proxy.clone();
            core.add_HistoryChanged(
                &HistoryChangedEventHandler::create(Box::new(move |_, _| {
                    let _ = p.send_event(Event::Page(id, PageEvent::Changed));
                    Ok(())
                })),
                &mut token,
            )?;
            let completed_proxy = proxy.clone();
            core.add_NavigationCompleted(&NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                if let Some(args) = args {
                    let mut success = BOOL::default();
                    args.IsSuccess(&mut success)?;
                    let error = if success.as_bool() { None } else {
                        let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                        args.WebErrorStatus(&mut status)?;
                        Some(format!("Navigation failed (WebView2 status {}). Check the address or connection and retry.", status.0))
                    };
                    let mut navigation_id=0;args.NavigationId(&mut navigation_id)?;
                    let mut url=String::new();let mut title=String::new();
                    if let Some(sender)=sender {
                        let mut source=windows::core::PWSTR::null();sender.Source(&mut source)?;url=webview2_com::take_pwstr(source);
                        let mut value=windows::core::PWSTR::null();sender.DocumentTitle(&mut value)?;title=webview2_com::take_pwstr(value);
                    }
                    let _ = completed_proxy.send_event(Event::Page(id, PageEvent::Completed { error,navigation_id,url,title }));
                }
                Ok(())
            })), &mut token)?;
        }
        let activity = std::rc::Rc::new(crate::native_activity::Activity::default());
        crate::native_activity::install(&view, activity.clone(), proxy.clone(), id)?;
        if !related {
            crate::native_activity::watch_resources(&view, activity.clone())?;
        }
        Ok(Self { view, activity })
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
                    let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                    let command = crate::ui::shortcut(key, ctrl, alt, shift);
                    if let Some(command) = command {
                        // Escape must also reach HTML fullscreen, selects and dialogs.
                        args.SetHandled(key != 0x1b)?;
                        let _ = proxy.send_event(Event::Command(command));
                    }
                }
                Ok(())
            })),
            &mut token,
        )
    }
}
