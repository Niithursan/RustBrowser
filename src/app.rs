use crate::{
    engine::{self, Engine},
    navigation,
    ui::{self, Command, State},
};
use base64::Engine as _;
use std::path::PathBuf;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};
use wry::{Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

#[derive(Debug)]
pub enum Event {
    Command(Command),
    Loading,
    Changed,
    Completed(Option<String>),
    Error(String),
}

// Drop webviews before their contexts and parent window.
struct Session {
    engine: Engine,
    toolbar: WebView,
    _content_context: WebContext,
    _toolbar_context: WebContext,
    window: Window,
    state: State,
}

impl Session {
    fn create(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let window = event_loop.create_window(
            Window::default_attributes()
                .with_title("Rust Browser 0.1")
                .with_inner_size(LogicalSize::new(1200.0, 820.0))
                .with_min_inner_size(LogicalSize::new(500.0, 300.0)),
        )?;
        let profile = profile_dir()?;
        std::fs::create_dir_all(&profile)?;
        let mut content_context = WebContext::new(Some(profile.join("content")));
        let mut toolbar_context = WebContext::new(Some(profile.join("toolbar")));
        let engine = Engine::new(&window, &mut content_context, proxy.clone())?;
        let ipc_proxy = proxy.clone();
        let toolbar_url = format!(
            "data:text/html;charset=utf-8;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(ui::HTML)
        );
        let allowed_toolbar_url = toolbar_url.clone();
        let toolbar = WebViewBuilder::new_with_web_context(&mut toolbar_context)
            .with_additional_browser_args("")
            .with_url(&toolbar_url)
            .with_devtools(false)
            .with_navigation_handler(move |url| url == allowed_toolbar_url)
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                if let Ok(command) = serde_json::from_str::<Command>(request.body()) {
                    let _ = ipc_proxy.send_event(Event::Command(command));
                }
            })
            .build_as_child(&window)?;
        engine::accelerators(&engine.view, proxy.clone())?;
        engine::accelerators(&toolbar, proxy)?;
        let session = Self {
            engine,
            toolbar,
            _content_context: content_context,
            _toolbar_context: toolbar_context,
            window,
            state: State::default(),
        };
        session.resize()?;
        Ok(session)
    }

    fn resize(&self) -> wry::Result<()> {
        let size = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor());
        self.toolbar.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(size.width, ui::HEIGHT).into(),
        })?;
        self.engine.view.set_bounds(Rect {
            position: LogicalPosition::new(0.0, ui::HEIGHT).into(),
            size: LogicalSize::new(size.width, (size.height - ui::HEIGHT).max(1.0)).into(),
        })
    }

    fn render(&mut self) {
        if let Ok(url) = self.engine.view.url()
            && navigation::allowed(&url)
        {
            self.state.url = url;
        }
        (self.state.back, self.state.forward) = self.engine.history();
        let json = serde_json::to_string(&self.state).expect("state serializes");
        if let Err(error) = self
            .toolbar
            .evaluate_script(&format!("window.render && window.render({json})"))
        {
            eprintln!("Toolbar: {error}");
        }
        self.window.set_title(if self.state.loading {
            "Loading… — Rust Browser 0.1"
        } else {
            "Rust Browser 0.1"
        });
    }

    fn command(&mut self, command: Command) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            Command::Navigate(input) => {
                let url = navigation::address(&input)?;
                self.state.url = url.clone();
                self.state.message = "Connecting…".into();
                self.state.loading = true;
                self.engine.view.load_url(&url)?;
                self.engine.view.focus()?;
            }
            Command::Home => self.command(Command::Navigate(navigation::HOME.into()))?,
            Command::Ready => {}
            Command::FocusAddress => {
                self.toolbar.focus()?;
                self.toolbar.evaluate_script("window.focusAddress()")?;
            }
            Command::FocusPage => self.engine.view.focus()?,
            Command::Back if !self.state.back => {}
            Command::Forward if !self.state.forward => {}
            other => {
                self.engine.command(&other)?;
                if matches!(other, Command::Stop) {
                    self.state.loading = false;
                    self.state.message = "Stopped.".into();
                }
            }
        }
        Ok(())
    }
}

fn profile_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(value) = std::env::var_os("RUST_BROWSER_PROFILE") {
        return Ok(value.into());
    }
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?)
            .join("RustBrowser")
            .join("v0.1"),
    )
}

struct App {
    session: Option<Session>,
    proxy: EventLoopProxy<Event>,
    failure: Option<String>,
}

impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.session.is_some() {
            return;
        }
        match Session::create(event_loop, self.proxy.clone()) {
            Ok(mut session) => {
                let initial = std::env::args()
                    .nth(1)
                    .unwrap_or_else(|| navigation::HOME.into());
                if let Err(error) = session.command(Command::Navigate(initial)) {
                    session.state.message = format!("Error: {error}");
                    session.state.loading = false;
                }
                session.render();
                self.session = Some(session);
            }
            Err(error) => {
                self.failure = Some(format!(
                    "Could not start browser: {error}. Check the WebView2 Runtime installation."
                ));
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.session = None;
                event_loop.exit();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(session) = &mut self.session
                    && let Err(error) = session.resize()
                {
                    session.state.message = format!("Error: {error}");
                    session.render();
                }
            }
            _ => {}
        }
    }
    fn user_event(&mut self, _: &ActiveEventLoop, event: Event) {
        let Some(session) = &mut self.session else {
            return;
        };
        match event {
            Event::Command(command) => {
                if let Err(error) = session.command(command) {
                    session.state.message = format!("Error: {error}");
                    session.state.loading = false;
                }
            }
            Event::Loading => {
                session.state.loading = true;
                session.state.message = "Loading…".into();
            }
            Event::Changed => {}
            Event::Completed(error) => {
                session.state.loading = false;
                session.state.message = error.map(|e| format!("Error: {e}")).unwrap_or_else(|| {
                    "Ready · Ctrl+L address · Alt+←/→ history · F5 reload · Ctrl+Tab page".into()
                });
            }
            Event::Error(error) => session.state.message = format!("Error: {error}"),
        }
        session.render();
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::<Event>::with_user_event().build()?;
    let mut app = App {
        session: None,
        proxy: event_loop.create_proxy(),
        failure: None,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    Ok(())
}
