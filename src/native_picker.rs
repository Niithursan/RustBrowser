//! The "Who's browsing?" window on Windows: shown by a plain launch when there
//! is more than one profile. Its page and choices are shared (`picker`).
use crate::picker::{self, Choice, Outcome};
use rust_browser::profiles::Registry;
use std::path::{Path, PathBuf};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

struct Picker {
    root: PathBuf,
    html: String,
    proxy: EventLoopProxy<Choice>,
    window: Option<Window>,
    view: Option<WebView>,
    context: WebContext,
    chosen: Option<String>,
    failed: bool,
}

impl Picker {
    fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
        let icon =
            winit::window::Icon::from_resource(1, Some(winit::dpi::PhysicalSize::new(32, 32))).ok();
        let window = event_loop.create_window(
            Window::default_attributes()
                .with_window_icon(icon.clone())
                .with_taskbar_icon(icon)
                .with_title(picker::TITLE)
                .with_inner_size(LogicalSize::<f64>::from(picker::SIZE))
                .with_min_inner_size(LogicalSize::<f64>::from(picker::MIN_SIZE)),
        )?;
        let url = format!(
            "data:text/html;charset=utf-8;base64,{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.html)
        );
        let allowed = url.clone();
        let proxy = self.proxy.clone();
        let view = WebViewBuilder::new_with_web_context(&mut self.context)
            .with_url(&url)
            .with_focused(true)
            .with_devtools(false)
            .with_default_context_menus(false)
            .with_browser_accelerator_keys(false)
            .with_hotkeys_zoom(false)
            .with_permission_handler(|_| wry::PermissionResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .with_navigation_handler(move |to| to == allowed)
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                if let Some(choice) = picker::read(request.body()) {
                    let _ = proxy.send_event(choice);
                }
            })
            .build(&window)?;
        // Started from a shortcut, the picker is allowed to come to the front.
        window.focus_window();
        self.window = Some(window);
        self.view = Some(view);
        Ok(())
    }

    fn show_error(&self, message: &str) {
        if let Some(view) = &self.view
            && let Some(script) = picker::error_script(message)
        {
            let _ = view.evaluate_script(&script);
        }
    }
}

impl ApplicationHandler<Choice> for Picker {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() && self.open(event_loop).is_err() {
            self.failed = true;
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, choice: Choice) {
        match picker::decide(&self.root, choice) {
            Outcome::Chosen(id) => {
                self.chosen = Some(id);
                event_loop.exit();
            }
            Outcome::Error(message) => self.show_error(&message),
            Outcome::Closed => event_loop.exit(),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(view) = &self.view {
                    let _ = view.set_bounds(wry::Rect {
                        position: winit::dpi::PhysicalPosition::new(0, 0).into(),
                        size: size.into(),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Show the picker. `Ok(Some(id))` is the chosen (or newly added) profile;
/// `Ok(None)` means the picker was closed without choosing.
pub fn choose(root: &Path, registry: &Registry) -> Result<Option<String>, ()> {
    let event_loop = EventLoop::<Choice>::with_user_event()
        .build()
        .map_err(|_| ())?;
    let mut picker = Picker {
        root: root.to_path_buf(),
        html: picker::page(root, registry),
        proxy: event_loop.create_proxy(),
        window: None,
        view: None,
        // Its own WebView2 folder: the picker never touches a profile's data.
        context: WebContext::new(Some(root.join("profiles").join("picker"))),
        chosen: None,
        failed: false,
    };
    event_loop.run_app(&mut picker).map_err(|_| ())?;
    // Release the view before the event loop's window goes away.
    picker.view = None;
    if picker.failed {
        return Err(());
    }
    Ok(picker.chosen)
}
