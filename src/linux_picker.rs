//! The "Who's browsing?" window on Linux: shown by a plain launch when there is
//! more than one profile. Its page and choices are shared (`picker`); this is
//! the window, a WebKitGTK view that may show only that page, in an in-memory
//! WebKit context, so the picker never touches a profile's data.
use crate::picker::{self, Choice, Outcome};
use rust_browser::profiles::Registry;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    window::{Window, WindowId},
};
use wry::{Rect, WebView, WebViewBuilder};

/// The picker's private address. As for the toolbar, a `data:` URL would not
/// do: wry delivers IPC only from pages whose URL is a valid HTTP URI.
const SCHEME: &str = "rust-browser-picker";
const URL: &str = "rust-browser-picker://picker/";
const GTK_TICK: Duration = Duration::from_millis(8);

/// What the picker's scheme serves: the page at `/`, nothing else.
fn document(path: &str, html: &str) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    let (status, body) = if path == "/" {
        (200, html.as_bytes().to_vec())
    } else {
        (404, Vec::new())
    };
    wry::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Cache-Control", "no-store")
        .body(body.into())
        .unwrap_or_else(|_| wry::http::Response::new((&[] as &[u8]).into()))
}

struct Picker {
    root: PathBuf,
    html: String,
    proxy: EventLoopProxy<Choice>,
    // The view drops before its window.
    view: Option<WebView>,
    window: Option<Window>,
    chosen: Option<String>,
    failed: bool,
}

impl Picker {
    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        let window = el.create_window(
            Window::default_attributes()
                .with_title(picker::TITLE)
                .with_inner_size(LogicalSize::<f64>::from(picker::SIZE))
                .with_min_inner_size(LogicalSize::<f64>::from(picker::MIN_SIZE)),
        )?;
        let html = self.html.clone();
        let proxy = self.proxy.clone();
        let size = window.inner_size().to_logical::<f64>(window.scale_factor());
        let view = WebViewBuilder::new()
            .with_custom_protocol(SCHEME.into(), move |_, request| {
                document(request.uri().path(), &html)
            })
            .with_url(URL)
            .with_bounds(bounds(size))
            .with_incognito(true)
            .with_focused(true)
            .with_devtools(false)
            .with_clipboard(false)
            .with_hotkeys_zoom(false)
            .with_permission_handler(|_| wry::PermissionResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .with_navigation_handler(|to| to == URL)
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                if let Some(choice) = picker::read(request.body()) {
                    let _ = proxy.send_event(choice);
                }
            })
            .build_as_child(&window)?;
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

fn bounds(size: LogicalSize<f64>) -> Rect {
    Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: LogicalSize::new(size.width.max(1.0), size.height.max(1.0)).into(),
    }
}

impl ApplicationHandler<Choice> for Picker {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none() && self.open(el).is_err() {
            self.failed = true;
            el.exit();
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, choice: Choice) {
        match picker::decide(&self.root, choice) {
            Outcome::Chosen(id) => {
                self.chosen = Some(id);
                el.exit();
            }
            Outcome::Error(message) => self.show_error(&message),
            Outcome::Closed => el.exit(),
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let (Some(window), Some(view)) = (&self.window, &self.view) {
                    let size = window.inner_size().to_logical::<f64>(window.scale_factor());
                    let _ = view.set_bounds(bounds(size));
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // WebKitGTK runs on GTK's main loop; service it between winit's events.
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if !el.exiting() {
            el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + GTK_TICK));
        }
    }
}

/// Show the picker. `Ok(Some(id))` is the chosen (or newly added) profile;
/// `Ok(None)` means the picker was closed without choosing.
pub fn choose(root: &Path, registry: &Registry) -> Result<Option<String>, String> {
    let event_loop = crate::linux::event_loop::<Choice>()?;
    let mut picker = Picker {
        root: root.to_path_buf(),
        html: picker::page(root, registry),
        proxy: event_loop.create_proxy(),
        view: None,
        window: None,
        chosen: None,
        failed: false,
    };
    event_loop.run_app(&mut picker).map_err(|e| e.to_string())?;
    picker.view = None;
    picker.window = None;
    if picker.failed {
        return Err("The profile picker could not be shown.".into());
    }
    Ok(picker.chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scheme_serves_only_the_page() {
        let page = document("/", "<p>picker</p>");
        assert_eq!(page.status(), 200);
        assert_eq!(page.body().as_ref(), b"<p>picker</p>");
        assert_eq!(document("/other", "x").status(), 404);
        assert_eq!(document("/../etc/passwd", "x").status(), 404);
        assert!(URL.parse::<wry::http::Uri>().is_ok());
        assert!(URL.starts_with(SCHEME));
    }
}
