//! The "Who's browsing?" window: shown by a plain launch when there is more than
//! one profile. It only chooses; the chosen profile then starts in a process of
//! its own (`--profile=<id>`), because a process can run one event loop and each
//! profile keeps its own lock, WebView2 environment and link channel.
use rust_browser::profiles::{self, Registry};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

const HTML: &str = include_str!("ui/picker.html");

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
enum Choice {
    Open(String),
    Add,
    Close,
}

/// The picker page with the profiles and theme filled in. The data is JSON with
/// `<` escaped, so no profile name can end the script element.
fn page(registry: &Registry, theme: rust_browser::settings::Theme) -> String {
    let data = serde_json::json!({
        "theme": theme,
        "can_add": registry.profiles.len() < profiles::MAX_PROFILES,
        "profiles": registry.profiles,
    })
    .to_string()
    .replace('<', "\\u003c");
    HTML.replacen("/*DATA*/null", &data, 1)
}

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
                .with_title("Rust Browser - Who's browsing?")
                .with_inner_size(LogicalSize::new(640.0, 460.0))
                .with_min_inner_size(LogicalSize::new(360.0, 320.0)),
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
                if request.body().len() <= 256
                    && let Ok(choice) = serde_json::from_str::<Choice>(request.body())
                {
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
            && let Ok(text) = serde_json::to_string(message)
        {
            let _ = view.evaluate_script(&format!("window.showError({text})"));
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
        match choice {
            Choice::Open(id) => {
                // Only a profile that still exists; another process may have
                // deleted it since the picker opened.
                match profiles::load(&self.root) {
                    Ok(registry) if registry.get(&id).is_some() => {
                        self.chosen = Some(id);
                        event_loop.exit();
                    }
                    _ => self.show_error("That profile no longer exists."),
                }
            }
            Choice::Add => {
                let created = profiles::update(&self.root, |registry| {
                    let name = registry.suggested_name();
                    let color = registry.suggested_color();
                    registry.create(&name, color).map(|p| p.id)
                });
                match created {
                    Ok(id) => {
                        self.chosen = Some(id);
                        event_loop.exit();
                    }
                    Err(error) => self.show_error(&error.to_string()),
                }
            }
            Choice::Close => event_loop.exit(),
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
    let theme = rust_browser::storage::load(&root.join("session.json"))
        .map(|snapshot| snapshot.settings.theme)
        .unwrap_or_default();
    let event_loop = EventLoop::<Choice>::with_user_event()
        .build()
        .map_err(|_| ())?;
    let mut picker = Picker {
        root: root.to_path_buf(),
        html: page(registry, theme),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_cannot_escape_the_picker_script() {
        let mut registry = Registry::default();
        registry
            .create("</script><img src=x onerror=alert(1)>", profiles::COLORS[1])
            .unwrap();
        let html = page(&registry, rust_browser::settings::Theme::Light);
        assert!(!html.contains("/*DATA*/null"));
        assert_eq!(
            html.matches("</script>").count(),
            1,
            "only the page's own end tag"
        );
        assert!(html.contains("\\u003c/script>\\u003cimg"));
        assert!(html.contains("\"theme\":\"light\""));
        assert!(html.contains("\"can_add\":true"));
    }

    #[test]
    fn only_known_choices_are_accepted() {
        assert_eq!(
            serde_json::from_str::<Choice>(r#"{"action":"open","value":"p2"}"#).unwrap(),
            Choice::Open("p2".into())
        );
        assert_eq!(
            serde_json::from_str::<Choice>(r#"{"action":"add"}"#).unwrap(),
            Choice::Add
        );
        assert!(serde_json::from_str::<Choice>(r#"{"action":"delete","value":"p2"}"#).is_err());
    }
}
