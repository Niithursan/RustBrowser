use crate::{
    engine::{self, Engine},
    ui::{self, Command, State, TabState, ToolbarState},
};
use base64::Engine as _;
use rust_browser::{
    navigation,
    settings::Settings,
    storage::{self, Snapshot, Store},
    tabs::{MAX_TABS, TabId, Tabs},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};
use wry::{Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

#[derive(Debug)]
pub enum PageEvent {
    Loading,
    Changed,
    Completed(Option<String>),
    Error(String),
    Title(String),
}
#[derive(Debug)]
pub enum Event {
    Command(Command),
    Page(TabId, PageEvent),
    OpenTab(TabId, String),
    StorageError,
}

struct Page {
    engine: Engine,
    state: State,
}

// Drop views before contexts and the parent window. Store flushes and releases its lock last.
struct Session {
    tabs: Tabs<Page>,
    toolbar: WebView,
    content_context: WebContext,
    _toolbar_context: WebContext,
    window: Window,
    proxy: EventLoopProxy<Event>,
    settings: Settings,
    notice: String,
    store: Store,
}

impl Session {
    fn create(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let profile = storage::profile_dir()?;
        let error_proxy = proxy.clone();
        let (store, snapshot, recovered) = Store::open(&profile, move || {
            let _ = error_proxy.send_event(Event::StorageError);
        })?;
        let window = event_loop.create_window(
            Window::default_attributes()
                .with_title("Rust Browser 0.2.0")
                .with_inner_size(LogicalSize::new(1200.0, 820.0))
                .with_min_inner_size(LogicalSize::new(560.0, 320.0)),
        )?;
        let content_context = WebContext::new(Some(profile.join("content")));
        let mut toolbar_context = WebContext::new(Some(profile.join("toolbar")));
        let ipc_proxy = proxy.clone();
        let toolbar_url = format!(
            "data:text/html;charset=utf-8;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(ui::HTML)
        );
        let allowed_url = toolbar_url.clone();
        let toolbar = WebViewBuilder::new_with_web_context(&mut toolbar_context)
            .with_additional_browser_args("")
            .with_url(&toolbar_url)
            .with_devtools(false)
            .with_navigation_handler(move |url| url == allowed_url)
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                if request.body().len() <= 16384
                    && let Ok(command) = serde_json::from_str::<Command>(request.body())
                {
                    let _ = ipc_proxy.send_event(Event::Command(command));
                }
            })
            .build_as_child(&window)?;
        engine::accelerators(&toolbar, proxy.clone())?;
        let mut session = Self {
            tabs: Tabs::default(),
            toolbar,
            content_context,
            _toolbar_context: toolbar_context,
            window,
            proxy,
            settings: snapshot.settings,
            notice: if recovered {
                "Invalid session was backed up; started with Home.".into()
            } else {
                String::new()
            },
            store,
        };
        for url in snapshot.tabs {
            session.open_tab(url)?;
        }
        if let Some(tab) = session.tabs.items.get(snapshot.active) {
            let id = tab.id;
            session.tabs.select(id);
        }
        // An explicit launch address opens an additional tab rather than discarding restoration.
        if let Some(input) = std::env::args().nth(1) {
            match navigation::resolve(&input, session.settings.search_engine) {
                Ok(url) => {
                    if let Err(error) = session.open_tab(url) {
                        session.notice = error.to_string();
                    }
                }
                Err(error) => session.notice = error,
            }
        }
        session.activate()?;
        session.persist();
        Ok(session)
    }

    fn open_tab(&mut self, url: String) -> Result<(), Box<dyn std::error::Error>> {
        let url = navigation::address(&url)?;
        if url.len() > 8192 {
            return Err("The tab address is too long.".into());
        }
        if self.tabs.items.len() >= MAX_TABS {
            return Err("The 64-tab limit has been reached.".into());
        }
        let id = self.tabs.next_id();
        let engine = Engine::new(
            &self.window,
            &mut self.content_context,
            self.proxy.clone(),
            id,
        )?;
        engine::accelerators(&engine.view, self.proxy.clone())?;
        engine.view.load_url(&url)?;
        let state = State {
            url: url.clone(),
            loading: true,
            message: "Connecting…".into(),
            ..State::default()
        };
        self.tabs.push(url, Page { engine, state })?;
        Ok(())
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
        if let Some(tab) = self.tabs.active() {
            tab.content.engine.view.set_bounds(Rect {
                position: LogicalPosition::new(0.0, ui::HEIGHT).into(),
                size: LogicalSize::new(size.width, (size.height - ui::HEIGHT).max(1.0)).into(),
            })?;
        }
        Ok(())
    }

    fn activate(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let active = self.tabs.active_index();
        for (index, tab) in self.tabs.items.iter().enumerate() {
            tab.content.engine.view.set_visible(index == active)?;
        }
        self.resize()?;
        if let Some(tab) = self.tabs.active() {
            tab.content.engine.view.focus()?;
        }
        Ok(())
    }

    fn persist(&self) {
        self.store.queue(Snapshot {
            version: 1,
            tabs: self.tabs.items.iter().map(|tab| tab.url.clone()).collect(),
            active: self.tabs.active_index(),
            settings: self.settings.clone(),
        });
    }

    fn render(&self) {
        let Some(active) = self.tabs.active() else {
            return;
        };
        let state = ToolbarState {
            page: &active.content.state,
            active_id: active.id,
            search_engine: self.settings.search_engine,
            notice: &self.notice,
            tabs: self
                .tabs
                .items
                .iter()
                .map(|tab| TabState {
                    id: tab.id,
                    title: &tab.title,
                    active: tab.id == active.id,
                })
                .collect(),
        };
        match serde_json::to_string(&state) {
            Ok(json) => {
                if self
                    .toolbar
                    .evaluate_script(&format!("window.render && window.render({json})"))
                    .is_err()
                {
                    eprintln!("Could not update browser toolbar.");
                }
            }
            Err(_) => eprintln!("Could not encode browser toolbar state."),
        }
        self.window.set_title(if active.content.state.loading {
            "Loading… — Rust Browser 0.2.0"
        } else {
            "Rust Browser 0.2.0"
        });
    }

    fn close_tab(&mut self, id: TabId) -> Result<(), Box<dyn std::error::Error>> {
        if self.tabs.items.len() == 1 && self.tabs.active().is_some_and(|tab| tab.id == id) {
            self.open_tab(navigation::HOME.into())?;
        }
        self.tabs.close(id);
        self.activate()?;
        Ok(())
    }

    fn command(&mut self, command: Command) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            Command::NewTab => {
                self.open_tab(navigation::HOME.into())?;
                self.activate()?;
                self.command(Command::FocusAddress)?;
            }
            Command::SelectTab(id) => {
                if self.tabs.select(id) {
                    self.activate()?;
                }
            }
            Command::CycleTab(backwards) => {
                self.tabs.cycle(backwards);
                self.activate()?;
            }
            Command::CloseTab(id) => self.close_tab(id)?,
            Command::CloseActive => {
                if let Some(tab) = self.tabs.active() {
                    self.close_tab(tab.id)?;
                }
            }
            Command::SetSearchEngine(engine) => {
                self.settings.search_engine = engine;
            }
            Command::Navigate(input) => {
                let url = navigation::resolve(&input, self.settings.search_engine)?;
                if let Some(tab) = self.tabs.active_mut() {
                    tab.content.engine.view.load_url(&url)?;
                    tab.url = url.clone();
                    tab.content.state.url = url;
                    tab.content.state.message = "Connecting…".into();
                    tab.content.state.loading = true;
                    tab.content.engine.view.focus()?;
                }
            }
            Command::Home => self.command(Command::Navigate(navigation::HOME.into()))?,
            Command::Ready => self.activate()?,
            Command::FocusAddress => {
                self.toolbar.focus()?;
                self.toolbar.evaluate_script("window.focusAddress()")?;
            }
            Command::FocusPage => {
                if let Some(tab) = self.tabs.active() {
                    tab.content.engine.view.focus()?;
                }
            }
            other => {
                if let Some(tab) = self.tabs.active_mut() {
                    if matches!(other, Command::Back) && !tab.content.state.back
                        || matches!(other, Command::Forward) && !tab.content.state.forward
                    {
                        return Ok(());
                    }
                    tab.content.engine.command(&other)?;
                    if matches!(other, Command::Stop) {
                        tab.content.state.loading = false;
                        tab.content.state.message = "Stopped.".into();
                    }
                }
            }
        }
        self.persist();
        Ok(())
    }

    fn page_event(&mut self, id: TabId, event: PageEvent) {
        let Some(tab) = self.tabs.get_mut(id) else {
            return;
        };
        match event {
            PageEvent::Loading => {
                tab.content.state.loading = true;
                tab.content.state.message = "Loading…".into();
            }
            PageEvent::Changed => {}
            PageEvent::Title(title) => tab.title = title.chars().take(120).collect(),
            PageEvent::Completed(error) => {
                tab.content.state.loading = false;
                tab.content.state.message =
                    error.map(|e| format!("Error: {e}")).unwrap_or_else(|| {
                        "Ready · Ctrl+L address · Ctrl+T new tab · Ctrl+Tab switch · F7 page".into()
                    });
            }
            PageEvent::Error(error) => tab.content.state.message = format!("Error: {error}"),
        }
        if let Ok(url) = tab.content.engine.view.url()
            && url.len() <= 8192
            && navigation::address(&url).as_deref() == Ok(url.as_str())
        {
            tab.url = url.clone();
            tab.content.state.url = url;
        }
        (tab.content.state.back, tab.content.state.forward) = tab.content.engine.history();
        self.persist();
    }
}

struct App {
    session: Option<Session>,
    proxy: EventLoopProxy<Event>,
    failure: Option<String>,
    modifiers: ModifiersState,
}
impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.session.is_some() {
            return;
        }
        match Session::create(event_loop, self.proxy.clone()) {
            Ok(session) => {
                session.render();
                self.session = Some(session);
            }
            Err(error) => {
                self.failure = Some(format!(
                    "Could not start browser: {error}. Check WebView2 Runtime and profile access."
                ));
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            // Child controllers handle focused-webpage input. This covers the native
            // parent receiving focus before a child has been selected after startup.
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                let key = match event.logical_key {
                    Key::Character(value) if value.len() == 1 => {
                        value.as_bytes()[0].to_ascii_uppercase() as u32
                    }
                    Key::Named(NamedKey::F5) => 0x74,
                    Key::Named(NamedKey::F6) => 0x75,
                    Key::Named(NamedKey::F7) => 0x76,
                    Key::Named(NamedKey::Tab) => 0x09,
                    Key::Named(NamedKey::Escape) => 0x1b,
                    Key::Named(NamedKey::ArrowLeft) => 0x25,
                    Key::Named(NamedKey::ArrowRight) => 0x27,
                    _ => 0,
                };
                if let Some(command) = ui::shortcut(
                    key,
                    self.modifiers.control_key(),
                    self.modifiers.alt_key(),
                    self.modifiers.shift_key(),
                ) {
                    self.user_event(event_loop, Event::Command(command));
                }
            }
            WindowEvent::CloseRequested => {
                if let Some(session) = &self.session {
                    session.persist();
                }
                self.session = None;
                event_loop.exit();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(session) = &mut self.session
                    && session.resize().is_err()
                {
                    session.notice = "Could not resize webpage.".into();
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
                    session.notice = format!("Error: {error}");
                }
            }
            Event::Page(id, event) => session.page_event(id, event),
            Event::OpenTab(source, url) => {
                if session.tabs.items.iter().any(|tab| tab.id == source)
                    && navigation::address(&url).is_ok()
                    && let Err(error) = session.open_tab(url).and_then(|()| session.activate())
                {
                    session.notice = format!("Error: {error}");
                }
                session.persist();
            }
            Event::StorageError => {
                session.notice =
                    "Error: Session could not be saved. Check profile access and disk space.".into()
            }
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
        modifiers: ModifiersState::default(),
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    Ok(())
}
