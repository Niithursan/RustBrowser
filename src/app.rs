use crate::{
    engine::{self, Engine},
    library_worker::{self, Worker},
    native_background, native_downloads, native_popup, native_security,
    ui::{self, Command, Panel, State, TabState, ToolbarState},
};
use base64::Engine as _;
use rust_browser::{
    background::{Lifecycle, Phase, Protection},
    diagnostics::{self, Code as DiagnosticCode},
    engine_api::{ContentEngine, LoadTarget, NavigationCommand},
    library::{Action, Data, Filter},
    navigation,
    privacy::{self, ClearRequest, PrivateLease},
    security::{self, Decision},
    settings::Settings,
    storage::{self, Snapshot, Store},
    tabs::{MAX_TABS, TabId, Tabs},
};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use wry::{Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

pub use rust_browser::engine_api::PageEvent;
#[derive(Debug)]
pub enum Event {
    SuspensionResult {
        id: TabId,
        ticket: u64,
        success: bool,
    },
    PermissionPrompt,
    EngineCleared {
        startup: bool,
        success: bool,
        ticket: u64,
    },
    Command(Command),
    Page(TabId, PageEvent),
    PopupRequested(u64),
    PopupBlocked,
    PageClose(TabId),
    Fullscreen(TabId, bool),
    StorageError,
    DiagnosticsUpdated(diagnostics::Report),
    RendererFailed,
    Library(library_worker::Reply),
    DownloadRequested(u64, TabId),
    DownloadStopped(u64, i32),
    DownloadTick,
    DownloadRejected,
    DownloadDestination(u64, Result<Option<PathBuf>, String>),
    DownloadFinished(u64, Result<PathBuf, String>),
    DownloadRecovery(String),
}

fn content_target(url: &str) -> Result<LoadTarget, rust_browser::engine_api::InvalidLocation> {
    if matches!(url, navigation::HOME | "about:blank") {
        Ok(LoadTarget::Blank)
    } else {
        LoadTarget::web(url)
    }
}

struct Page {
    engine: Engine,
    state: State,
    lifecycle: Lifecycle,
    opener: Option<TabId>,
}
struct Clearing {
    failed: bool,
    ticket: u64,
    request: ClearRequest,
    revision: u64,
    library_done: bool,
    engine_done: bool,
    started: Instant,
    started_native: bool,
    results: Vec<String>,
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
    panel: Option<Panel>,
    library_data: Data,
    filter: Filter,
    revision: u64,
    run_id: String,
    downloads: native_downloads::Shared,
    library: Worker,
    diagnostics: diagnostics::Recorder,
    diagnostic_warning_shown: bool,
    store: Option<Store>,
    permissions: native_security::Shared,
    popups: native_popup::Shared,
    fullscreen: Option<TabId>,
    focus_pending: bool,
    profile: PathBuf,
    private: bool,
    ready: bool,
    clearing: Option<Clearing>,
    next_clear: u64,
    permission_lockout: bool,
    started: Instant,
    next_background: Instant,
    last_background: Instant,
    chrome_height: f64,
    last_snapshot: RefCell<Option<Snapshot>>,
    last_render: RefCell<String>,
    last_title: RefCell<String>,
    // Last field: native views/contexts and workers must release before cleanup.
    _private_lease: Option<PrivateLease>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.permissions.borrow_mut().cancel_all();
        self.popups.borrow_mut().clear();
        self.downloads.borrow_mut().shutdown();
    }
}

impl Session {
    fn create(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::create_at(
            event_loop,
            proxy,
            storage::profile_dir()?,
            std::env::args().any(|arg| arg == "--private"),
            std::env::args().skip(1).find(|arg| arg != "--private"),
        )
    }
    fn create_at(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
        normal_profile: PathBuf,
        private: bool,
        launch_url: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Sweep only dead, recognized leases; live private windows retain their locks.
        let sweep_profile = normal_profile.clone();
        let _ = std::thread::Builder::new()
            .name("private-recovery".into())
            .spawn(move || {
                let _ = PrivateLease::sweep(&sweep_profile);
            });
        let private_lease = if private {
            Some(PrivateLease::create(&normal_profile)?)
        } else {
            None
        };
        let profile = private_lease
            .as_ref()
            .map(|lease| lease.path.clone())
            .unwrap_or(normal_profile);
        let error_proxy = proxy.clone();
        let (store, snapshot, recovered) = if private {
            (None, Snapshot::default(), false)
        } else {
            let (store, snapshot, recovered) = Store::open(&profile, move || {
                let _ = error_proxy.send_event(Event::StorageError);
            })?;
            (Some(store), snapshot, recovered)
        };
        let diagnostic_proxy = proxy.clone();
        let diagnostics =
            diagnostics::Recorder::start((!private).then_some(profile.as_path()), move |report| {
                let _ = diagnostic_proxy.send_event(Event::DiagnosticsUpdated(report));
            })
            .unwrap_or_else(|_| diagnostics::Recorder::disabled());
        let library = Worker::start(
            if private {
                PathBuf::from(":memory:")
            } else {
                profile.join("library.sqlite")
            },
            proxy.clone(),
        )?;
        use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
        let icon =
            winit::window::Icon::from_resource(1, Some(winit::dpi::PhysicalSize::new(32, 32))).ok();
        let window = event_loop.create_window(
            Window::default_attributes()
                .with_window_icon(icon.clone())
                .with_taskbar_icon(icon)
                .with_title(format!("Rust Browser {}", env!("CARGO_PKG_VERSION")))
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
            .with_incognito(private)
            .with_permission_handler(|_| wry::PermissionResponse::Deny)
            .with_download_started_handler(|_, _| false)
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
        if private {
            native_security::verify_private(&toolbar)?;
        }
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
                "Session recovery performed. Invalid originals were preserved; the last valid snapshot was used when available.".into()
            } else {
                String::new()
            },
            panel: None,
            library_data: Data::default(),
            filter: Filter::default(),
            revision: 0,
            run_id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_string(),
            downloads: Rc::new(RefCell::new(native_downloads::Manager::default())),
            library,
            diagnostics,
            diagnostic_warning_shown: false,
            store,
            profile,
            private,
            permissions: Rc::new(RefCell::new(native_security::Permissions::default())),
            popups: Rc::new(RefCell::new(native_popup::Manager::default())),
            fullscreen: None,
            focus_pending: false,
            ready: false,
            clearing: None,
            next_clear: 0,
            permission_lockout: false,
            started: Instant::now(),
            next_background: Instant::now(),
            last_background: Instant::now(),
            chrome_height: ui::HEIGHT,
            last_snapshot: RefCell::new(None),
            last_render: RefCell::new(String::new()),
            last_title: RefCell::new(String::new()),
            _private_lease: private_lease,
        };
        let restored = if !session.settings.restore_session {
            vec![navigation::HOME.into()]
        } else {
            snapshot.tabs
        };
        for (index, url) in restored.into_iter().enumerate() {
            session.open_tab(url)?;
            if let Some(tab) = session.tabs.active_mut() {
                tab.pinned = snapshot.pinned.get(index).copied().unwrap_or(false);
                tab.keep_awake = snapshot.keep_awake.get(index).copied().unwrap_or(false);
                if let Some(title) = snapshot.titles.get(index) {
                    tab.title = title.clone();
                }
            }
        }
        if let Some(tab) = session.tabs.items.get(snapshot.active) {
            let id = tab.id;
            session.tabs.select(id);
        }
        // An explicit launch address opens an additional tab rather than discarding restoration.
        if let Some(input) = launch_url {
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
        if session.library_action(Action::Refresh).is_err() {
            session.notice="Library database is unavailable. Browsing remains available; check profile access or use the browser version that created this library.".into();
        }
        // Reset WebView2's legacy remembered permission states before any website
        // loads. All new decisions are scoped in our library with SavesInProfile=false.
        if let Some(tab) = session.tabs.active() {
            native_security::clear(
                &tab.content.engine.view,
                &ClearRequest {
                    permissions: true,
                    confirmed: true,
                    ..ClearRequest::default()
                },
                true,
                0,
                session.proxy.clone(),
            )?;
        }
        Ok(session)
    }

    fn open_tab(&mut self, url: String) -> Result<(), Box<dyn std::error::Error>> {
        let url = if url == "about:blank" || url == navigation::HOME {
            url
        } else {
            navigation::address(&url)?
        };
        if url.len() > 8192 {
            return Err("The tab address is too long.".into());
        }
        if self.tabs.items.len() >= MAX_TABS {
            return Err("The 64-tab limit has been reached.".into());
        }
        let page = self.prepare_page(&url)?;
        self.tabs.push(url, page)?;
        Ok(())
    }

    fn prepare_page(&mut self, url: &str) -> Result<Page, Box<dyn std::error::Error>> {
        self.prepare_related(url, None)
    }
    fn prepare_related(
        &mut self,
        url: &str,
        environment: Option<
            webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment,
        >,
    ) -> Result<Page, Box<dyn std::error::Error>> {
        let id = self.tabs.next_id();
        let engine = Engine::new(
            &self.window,
            &mut self.content_context,
            self.proxy.clone(),
            id,
            self.downloads.clone(),
            self.permissions.clone(),
            engine::Options {
                private: self.private,
                environment: environment.clone(),
            },
        )?;
        engine::accelerators(&engine.view, self.proxy.clone())?;
        native_popup::install(&engine.view, self.popups.clone(), self.proxy.clone(), id)?;
        if self.ready && environment.is_none() {
            engine.load(&content_target(url)?)?;
        }
        let state = State {
            url: url.into(),
            loading: true,
            message: "Connecting…".into(),
            ..State::default()
        };
        Ok(Page {
            engine,
            state,
            lifecycle: Lifecycle::default(),
            opener: None,
        })
    }

    fn effective_panel(&self) -> Option<Panel> {
        self.panel.or_else(|| {
            self.tabs
                .active()
                .filter(|tab| tab.url == navigation::HOME)
                .map(|_| Panel::NewTab)
        })
    }

    fn open_popup(&mut self, ticket: u64) -> Result<(), Box<dyn std::error::Error>> {
        let Some(request) = self.popups.borrow_mut().take(ticket) else {
            return Ok(());
        };
        if !self.ready
            || self.clearing.is_some()
            || self.tabs.items.len() >= MAX_TABS
            || !self.tabs.items.iter().any(|t| t.id == request.source)
        {
            return Err("Popup could not be opened safely.".into());
        }
        let mut page = self.prepare_related(&request.url, Some(request.environment.clone()))?;
        page.opener = Some(request.source);
        request.attach(&page.engine.view)?;
        crate::native_activity::watch_resources(&page.engine.view, page.engine.activity.clone())?;
        self.tabs.push(request.url.clone(), page)?;
        self.panel = None;
        self.activate()?;
        self.persist();
        Ok(())
    }

    fn resize(&self) -> wry::Result<()> {
        let size = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor());
        self.toolbar.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(
                size.width,
                if self.effective_panel().is_some() {
                    size.height
                } else {
                    self.chrome_height
                },
            )
            .into(),
        })?;
        if let Some(tab) = self.tabs.active() {
            tab.content.engine.view.set_bounds(Rect {
                position: LogicalPosition::new(0.0, self.chrome_height).into(),
                size: LogicalSize::new(size.width, (size.height - self.chrome_height).max(1.0))
                    .into(),
            })?;
        }
        Ok(())
    }

    fn activate(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let next = self.tabs.active().map(|tab| tab.id);
        let fullscreen = self
            .tabs
            .active()
            .filter(|t| t.content.engine.activity.fullscreen.get())
            .map(|t| t.id);
        if fullscreen != self.fullscreen {
            self.fullscreen = fullscreen;
            self.window
                .set_fullscreen(fullscreen.map(|_| winit::window::Fullscreen::Borderless(None)));
        }
        {
            let mut permissions = self.permissions.borrow_mut();
            if permissions.active != next {
                permissions.cancel_all();
            }
            permissions.active = next;
        }
        let active = self.tabs.active_index();
        let panel = self.effective_panel();
        let now = self.started.elapsed();
        for (index, tab) in self.tabs.items.iter_mut().enumerate() {
            tab.content.lifecycle.visible(index == active, now);
            if index == active
                && matches!(
                    tab.content.lifecycle.phase,
                    Phase::Sleeping | Phase::Pending | Phase::ResumeFailed
                )
            {
                let result = native_background::resume(&tab.content.engine.view);
                tab.content.lifecycle.resumed(result.is_ok());
                if result.is_err() {
                    self.notice="This tab could not resume. Use Wake tab to retry; the page was kept intact.".into();
                }
            }
            tab.content
                .engine
                .set_visible(index == active && panel.is_none())?;
        }
        self.resize()?;
        self.focus_active()
    }

    fn focus_active(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // WebView2 MoveFocus rejects a minimized parent (E_INVALIDARG). Keep
        // visibility/tab changes, then request focus once Windows restores it.
        self.focus_pending = self.window.is_minimized() == Some(true);
        if self.focus_pending {
            return Ok(());
        }
        if self.effective_panel().is_some() {
            self.toolbar.focus()?;
        } else if let Some(tab) = self.tabs.active() {
            tab.content.engine.focus()?;
        }
        Ok(())
    }

    fn persist(&self) {
        if self.private || self.clearing.is_some() {
            return;
        }
        let Some(store) = &self.store else { return };
        let snapshot = Snapshot {
            version: 1,
            tabs: self
                .tabs
                .items
                .iter()
                .map(|tab| {
                    if tab.url == navigation::HOME || navigation::address(&tab.url).is_ok() {
                        tab.url.clone()
                    } else {
                        navigation::HOME.into()
                    }
                })
                .collect(),
            active: self.tabs.active_index(),
            settings: self.settings.clone(),
            pinned: self.tabs.items.iter().map(|tab| tab.pinned).collect(),
            titles: self
                .tabs
                .items
                .iter()
                .map(|tab| {
                    tab.title
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(120)
                        .collect()
                })
                .collect(),
            keep_awake: self.tabs.items.iter().map(|tab| tab.keep_awake).collect(),
        };
        if let Some(snapshot) = privacy::persistent_snapshot(snapshot, self.private) {
            let mut last = self.last_snapshot.borrow_mut();
            if last.as_ref() != Some(&snapshot) {
                store.queue(snapshot.clone());
                *last = Some(snapshot);
            }
        }
    }

    fn render(&self) {
        let Some(active) = self.tabs.active() else {
            return;
        };
        let source = active.content.engine.view.url().unwrap_or_default();
        let indicator_url = if active.url == navigation::HOME && source == "about:blank" {
            navigation::HOME
        } else {
            source.as_str()
        };
        let state = ToolbarState {
            settings: &self.settings,
            private: self.private,
            restore_session: self.settings.restore_session,
            security: security::indicator(
                indicator_url,
                active.content.state.loading,
                active.content.state.failed,
            ),
            permission_prompt: self.permissions.borrow().prompt(),
            busy: self.clearing.is_some() || !self.ready,
            page: &active.content.state,
            active_id: active.id,
            search_engine: self.settings.search_engine,
            notice: &self.notice,
            panel: self.effective_panel(),
            library: &self.library_data,
            history_enabled: self.settings.history_enabled,
            downloads: self.downloads.borrow().records(),
            diagnostics: (self.panel == Some(Panel::Settings)).then(|| self.diagnostics.snapshot()),
            tabs: self
                .tabs
                .items
                .iter()
                .map(|tab| TabState {
                    id: tab.id,
                    title: &tab.title,
                    active: tab.id == active.id,
                    pinned: tab.pinned,
                    keep_awake: tab.keep_awake,
                    phase: tab.content.lifecycle.phase,
                    loading: tab.content.state.loading,
                })
                .collect(),
        };
        match serde_json::to_string(&state) {
            Ok(json) => {
                let changed = *self.last_render.borrow() != json;
                if changed
                    && self
                        .toolbar
                        .evaluate_script(&format!("window.render && window.render({json})"))
                        .is_err()
                {
                    self.diagnostics.record(DiagnosticCode::ToolbarFailed);
                } else {
                    *self.last_render.borrow_mut() = json;
                }
            }
            Err(_) => self.diagnostics.record(DiagnosticCode::ToolbarFailed),
        }
        let prefix = if self.private {
            "Private — "
        } else if active.content.state.loading {
            "Loading… — "
        } else {
            ""
        };
        let title = format!("{prefix}Rust Browser {}", env!("CARGO_PKG_VERSION"));
        if self.last_title.borrow().as_str() != title {
            self.window.set_title(&title);
            *self.last_title.borrow_mut() = title;
        }
    }

    fn close_tab(&mut self, id: TabId) -> Result<(), Box<dyn std::error::Error>> {
        let return_to = self
            .tabs
            .active()
            .filter(|t| t.id == id)
            .and_then(|t| t.content.opener);
        self.popups.borrow_mut().cancel_tab(id);
        self.permissions.borrow_mut().cancel_tab(id);
        if self.tabs.items.len() == 1 && self.tabs.active().is_some_and(|tab| tab.id == id) {
            self.open_tab(navigation::HOME.into())?;
        }
        self.tabs.close(id);
        if let Some(opener) = return_to {
            self.tabs.select(opener);
        }
        let activated = self.activate();
        #[cfg(test)]
        if let Err(error) = &activated {
            eprintln!(
                "Controlled tab-close activation failed: {error}; active lifecycle/native suspension: {:?}",
                self.tabs.active().map(|tab| (
                    tab.content.lifecycle.phase,
                    native_background::suspended(&tab.content.engine.view)
                ))
            );
        }
        activated?;
        Ok(())
    }

    fn command(&mut self, command: Command) -> Result<(), Box<dyn std::error::Error>> {
        if self.clearing.is_some() {
            return Err("Data clearing is in progress; wait for its result.".into());
        }
        if !self.ready && !matches!(command, Command::Ready | Command::ToolbarHeight(_)) {
            return Err("Security initialization is not complete.".into());
        }
        match command {
            Command::RefreshDiagnostics => self.diagnostics.refresh()?,
            Command::DismissNotice => self.notice.clear(),
            Command::ClearDiagnostics { confirmed } => self.diagnostics.clear(confirmed)?,
            Command::ToolbarHeight(height) => {
                if !height.is_finite() || !(80.0..=320.0).contains(&height) {
                    return Err("Invalid toolbar size.".into());
                }
                self.chrome_height = height;
                self.resize()?;
            }
            Command::MoveTab { id, to } => {
                self.tabs.move_to(id, to);
                self.activate()?;
            }
            Command::MoveActive(backwards) => {
                if let Some(tab) = self.tabs.active() {
                    let id = tab.id;
                    let index = self.tabs.active_index();
                    let to = if backwards {
                        index.saturating_sub(1)
                    } else {
                        (index + 1).min(self.tabs.items.len() - 1)
                    };
                    self.tabs.move_to(id, to);
                    self.activate()?;
                }
            }
            Command::PinActive => {
                if let Some(tab) = self.tabs.active() {
                    let id = tab.id;
                    let pinned = !tab.pinned;
                    self.tabs.pin(id, pinned);
                    self.activate()?;
                }
            }
            Command::KeepAwake => {
                if let Some(tab) = self.tabs.active_mut() {
                    tab.keep_awake = !tab.keep_awake;
                }
                self.activate()?;
            }
            Command::WakeTab => {
                if let Some(tab) = self.tabs.active_mut() {
                    let result = native_background::resume(&tab.content.engine.view);
                    tab.content.lifecycle.resumed(result.is_ok());
                    result?;
                }
            }
            Command::SetAutomaticSuspension(enabled) => {
                self.settings.automatic_suspension = enabled;
                if !enabled {
                    for tab in &mut self.tabs.items {
                        if matches!(
                            tab.content.lifecycle.phase,
                            Phase::Sleeping | Phase::Pending
                        ) {
                            let result = native_background::resume(&tab.content.engine.view);
                            tab.content.lifecycle.resumed(result.is_ok());
                            if result.is_err() {
                                self.notice="A sleeping tab could not resume. Select it and use Wake tab to retry.".into();
                            }
                        }
                    }
                }
            }
            Command::SetSuspendDelay(seconds) => {
                if !(15..=3600).contains(&seconds) {
                    return Err("Choose a suspension delay between 15 and 3600 seconds.".into());
                }
                self.settings.suspend_after_seconds = seconds;
            }
            Command::SetTheme(theme) => self.settings.theme = theme,
            Command::SetShowRecent(value) => {
                if self.private && value {
                    return Err("Private windows do not show normal browsing history.".into());
                }
                self.settings.show_recent = value;
                self.library_action(Action::Refresh)?;
            }
            Command::AddFavorite { title, url } => {
                let mut settings = self.settings.clone();
                settings.favorites.push(rust_browser::settings::Favorite {
                    title: title.trim().into(),
                    url: navigation::address(&url)?,
                });
                settings.validate()?;
                self.settings = settings;
            }
            Command::RemoveFavorite(index) => {
                if index >= self.settings.favorites.len() {
                    return Err("Favorite no longer exists.".into());
                }
                self.settings.favorites.remove(index);
            }
            Command::NewPrivateWindow => {
                std::process::Command::new(std::env::current_exe()?)
                    .arg("--private")
                    .spawn()?;
            }
            Command::SetRestoreSession(value) => {
                if self.private {
                    return Err("Private windows never retain sessions.".into());
                }
                self.settings.restore_session = value;
                if !value {
                    if let Some(store) = &self.store {
                        store.forget_session(self.settings.clone())?;
                    }
                    privacy::clear_session_backups(&self.profile)?;
                }
            }
            Command::ClearData(request) => self.begin_clear(request, None)?,
            Command::RevokePermission {
                origin,
                capability,
                confirmed,
            } => {
                if !confirmed
                    || security::permission_origin(&origin).as_deref() != Some(origin.as_str())
                {
                    return Err("Invalid or unconfirmed permission revocation.".into());
                }
                self.begin_clear(
                    ClearRequest {
                        permissions: true,
                        confirmed: true,
                        ..ClearRequest::default()
                    },
                    Some((origin, capability)),
                )?;
            }
            Command::PermissionDecision {
                id,
                decision,
                remember,
            } => {
                let prompt = self.permissions.borrow().prompt();
                if let Some(prompt) = prompt.filter(|p| p.id == id) {
                    let current = self
                        .tabs
                        .active()
                        .and_then(|tab| tab.content.engine.view.url().ok())
                        .and_then(|url| security::origin(&url));
                    if current.as_deref() != Some(prompt.origin.as_str()) {
                        self.permissions.borrow_mut().resolve(id, Decision::Deny)?;
                        return Err("Permission request expired after navigation.".into());
                    }
                    if remember {
                        self.library_action(Action::SetPermission {
                            origin: prompt.origin,
                            capability: prompt.capability,
                            decision: Some(decision),
                        })?;
                    }
                    self.permissions.borrow_mut().resolve(id, decision)?;
                    self.panel = Some(Panel::Settings);
                    self.activate()?;
                }
            }
            Command::ShowPanel(panel) => {
                if panel == Panel::NewTab {
                    return self.command(Command::Home);
                }
                self.panel = Some(panel);
                self.filter = Filter::default();
                self.activate()?;
                self.library_action(Action::Refresh)?;
            }
            Command::ClosePanel => {
                self.panel = None;
                self.activate()?;
            }
            Command::QueryLibrary(filter) => {
                self.filter = filter;
                self.library_action(Action::Refresh)?;
            }
            Command::Library(action) => {
                if matches!(&action,Action::RemoveFolder{id} if self.filter.folder==Some(*id)) {
                    self.filter.folder = None;
                }
                self.library_action(action)?;
            }
            Command::BookmarkCurrent => {
                if self.private {
                    return Err("Bookmarks are unavailable in private windows; normal bookmarks remain unchanged.".into());
                }
                if let Some(tab) = self.tabs.active() {
                    let title = if tab.title == "New tab" {
                        tab.url.clone()
                    } else {
                        tab.title.clone()
                    };
                    let action = Action::SaveBookmark {
                        id: None,
                        title,
                        url: tab.url.clone(),
                        folder: None,
                    };
                    self.panel = Some(Panel::Bookmarks);
                    self.filter = Filter::default();
                    self.activate()?;
                    self.library_action(action)?;
                }
            }
            Command::OpenSaved { url, new_tab } => {
                let url = rust_browser::library::bookmark_url(&url).map_err(|e| e.to_string())?;
                self.panel = None;
                if new_tab {
                    self.open_tab(url)?;
                    self.activate()?;
                } else {
                    self.activate()?;
                    self.command(Command::Navigate(url))?;
                }
            }
            Command::SetHistoryEnabled(value) => {
                self.settings.history_enabled = value;
            }
            Command::CancelDownload(id) => {
                self.downloads.borrow_mut().cancel(id)?;
            }
            Command::OpenDownloadFolder(id) => {
                if let Some(folder) = self.downloads.borrow().folder(id) {
                    native_downloads::open_folder(&folder)?;
                }
            }
            Command::NewTab => {
                self.panel = None;
                self.open_tab(navigation::HOME.into())?;
                self.activate()?;
                self.command(Command::FocusAddress)?;
                if self.settings.show_recent && !self.private {
                    self.library_action(Action::Refresh)?;
                }
            }
            Command::SelectTab(id) => {
                self.panel = None;
                if self.tabs.select(id) {
                    self.activate()?;
                }
            }
            Command::CycleTab(backwards) => {
                self.panel = None;
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
                let panel_open = self.effective_panel().is_some();
                self.permissions.borrow_mut().cancel_all();
                let url = navigation::resolve(&input, self.settings.search_engine)?;
                self.notice.clear();
                self.panel = None;
                if let Some(tab) = self.tabs.active_mut() {
                    tab.content.engine.load(&LoadTarget::web(&url)?)?;
                    tab.url = url.clone();
                    tab.content.state.url = url;
                    tab.content.state.message = "Connecting…".into();
                    tab.content.state.loading = true;
                    tab.content.state.failed = false;
                    tab.content.engine.focus()?;
                }
                if panel_open {
                    self.activate()?;
                }
            }
            Command::Home => {
                self.panel = None;
                self.permissions.borrow_mut().cancel_all();
                if let Some(tab) = self.tabs.active_mut() {
                    let result = native_background::resume(&tab.content.engine.view);
                    tab.content.lifecycle.resumed(result.is_ok());
                    result?;
                    tab.url = navigation::HOME.into();
                    tab.title = "New tab".into();
                    tab.content.state.url = tab.url.clone();
                    tab.content.state.failed = false;
                    tab.content.engine.load(&LoadTarget::Blank)?;
                }
                self.activate()?;
                if self.settings.show_recent && !self.private {
                    self.library_action(Action::Refresh)?;
                }
            }
            Command::Ready => {
                self.last_render.borrow_mut().clear();
                self.activate()?;
            }
            Command::FocusAddress => {
                self.toolbar.focus()?;
                self.toolbar.evaluate_script("window.focusAddress()")?;
            }
            Command::FocusTabs => {
                self.toolbar.focus()?;
                self.toolbar
                    .evaluate_script("window.focusActiveTab && window.focusActiveTab()")?;
            }
            Command::FocusPage => {
                self.panel = None;
                self.activate()?;
                if self
                    .tabs
                    .active()
                    .is_some_and(|tab| tab.url == navigation::HOME)
                {
                    self.toolbar
                        .evaluate_script("document.getElementById('home-query').focus()")?;
                } else if let Some(tab) = self.tabs.active() {
                    tab.content.engine.focus()?;
                }
            }
            other => {
                if let Some(tab) = self.tabs.active_mut() {
                    if matches!(other, Command::Back) && !tab.content.state.back
                        || matches!(other, Command::Forward) && !tab.content.state.forward
                    {
                        return Ok(());
                    }
                    let action = match other {
                        Command::Back => NavigationCommand::Back,
                        Command::Forward => NavigationCommand::Forward,
                        Command::Reload => NavigationCommand::Reload,
                        Command::Stop => NavigationCommand::Stop,
                        _ => return Ok(()),
                    };
                    tab.content.engine.navigate(action)?;
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
        let was_local = self
            .tabs
            .active()
            .is_some_and(|tab| tab.url == navigation::HOME);
        let Some(tab) = self.tabs.get_mut(id) else {
            return;
        };
        match event {
            PageEvent::Loading => {
                // Keep critical-operation protection until the document is closed: a POST
                // navigation can still be uploading while NavigationStarting fires.
                tab.content.lifecycle.resumed(true);
                tab.content.lifecycle.visible(true, self.started.elapsed());
                tab.content.lifecycle.visible(
                    self.permissions.borrow().active == Some(id),
                    self.started.elapsed(),
                );
                self.permissions.borrow_mut().cancel_tab(id);
                tab.content.state.loading = true;
                tab.content.state.failed = false;
                tab.content.state.message = "Loading…".into();
            }
            PageEvent::Cancelled => {
                if tab.content.state.loading {
                    tab.content.state.loading = false;
                    tab.content.state.failed = false;
                    tab.content.state.message =
                        "Navigation cancelled. The previous page is kept when available.".into();
                }
            }
            PageEvent::Changed => {}
            PageEvent::Title(title) => {
                tab.title = if tab.url == navigation::HOME {
                    "New tab".into()
                } else {
                    title
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(120)
                        .collect()
                }
            }
            PageEvent::Completed {
                error,
                navigation_id,
                url,
                title,
            } => {
                if error.is_some() {
                    self.diagnostics.record(DiagnosticCode::NavigationFailed);
                }
                if error.is_none()
                    && self.settings.history_enabled
                    && !self.private
                    && self.clearing.is_none()
                {
                    let action = Action::Record {
                        key: format!("{}-{id}-{navigation_id}", self.run_id),
                        url,
                        title,
                        timestamp: rust_browser::library::now(),
                    };
                    if let Err(message) = self.library.submit(library_worker::Job {
                        revision: 0,
                        action,
                        filter: Filter::default(),
                    }) {
                        self.notice = message.into();
                    }
                }
                tab.content.state.loading = false;
                tab.content.state.failed = error.is_some();
                tab.content.state.message =
                    error.map(|e| format!("Error: {e}")).unwrap_or_else(|| {
                        "Ready · Ctrl+L address · Ctrl+T new tab · Ctrl+Tab switch · F7 page".into()
                    });
            }
            PageEvent::Error(error) => {
                self.diagnostics.record(DiagnosticCode::NavigationFailed);
                tab.content.state.loading = false;
                tab.content.state.failed = true;
                tab.content.state.message = format!("Error: {error}");
            }
        }
        if let Ok(url) = tab.content.engine.view.url()
            && url.len() <= 8192
            && navigation::address(&url).as_deref() == Ok(url.as_str())
        {
            tab.url = url.clone();
            tab.content.state.url = url;
        }
        (tab.content.state.back, tab.content.state.forward) = tab.content.engine.history();
        let is_local = self
            .tabs
            .active()
            .is_some_and(|tab| tab.url == navigation::HOME);
        if was_local != is_local {
            let _ = self.activate();
        }
        self.persist();
        self.start_engine_clear();
    }

    fn library_action(&mut self, action: Action) -> Result<(), Box<dyn std::error::Error>> {
        if self.private
            && matches!(
                action,
                Action::SaveBookmark { .. }
                    | Action::RemoveBookmark { .. }
                    | Action::SaveFolder { .. }
                    | Action::RemoveFolder { .. }
                    | Action::Record { .. }
            )
        {
            return Err("Private windows do not write bookmarks or history.".into());
        }
        self.revision += 1;
        self.library.submit(library_worker::Job {
            revision: self.revision,
            action,
            filter: self.filter.clone(),
        })?;
        Ok(())
    }

    fn suspension_protection(&self, id: TabId, audio: bool) -> Protection {
        let tab = self.tabs.items.iter().find(|tab| tab.id == id);
        Protection {
            active: self.tabs.active().is_some_and(|tab| tab.id == id),
            pinned: tab.is_some_and(|t| t.pinned),
            keep_awake: tab.is_some_and(|t| t.keep_awake),
            loading: tab.is_none_or(|t| t.content.state.loading),
            downloads: self.downloads.borrow().active_count() > 0,
            permissions: self.permissions.borrow().prompt().is_some(),
            capture: self.permissions.borrow().protected(id)
                || tab.is_some_and(|t| {
                    t.content.engine.activity.protected.get()
                        || !t.content.engine.activity.observable.get()
                        || t.content.engine.activity.fullscreen.get()
                        || t.content.opener.is_some()
                })
                || self.tabs.items.iter().any(|t| t.content.opener == Some(id)),
            audio,
            busy: !self.ready || self.clearing.is_some() || self.popups.borrow().busy(),
        }
    }
    fn background_tick(&mut self) -> bool {
        if Instant::now() < self.next_background {
            return false;
        }
        let gap = self.last_background.elapsed() > Duration::from_secs(30);
        self.last_background = Instant::now();
        self.next_background = Instant::now() + Duration::from_secs(1);
        if gap {
            let active = self.tabs.active().map(|t| t.id);
            for tab in &mut self.tabs.items {
                if matches!(
                    tab.content.lifecycle.phase,
                    Phase::Sleeping | Phase::Pending | Phase::ResumeFailed
                ) {
                    let success = native_background::resume(&tab.content.engine.view).is_ok();
                    tab.content.lifecycle.resumed(success);
                    if !success {
                        self.notice="A tab did not resume after an event-loop pause. Select it and use Wake tab; the page was preserved.".into();
                    }
                }
                tab.content.lifecycle.visible(true, self.started.elapsed());
                tab.content
                    .lifecycle
                    .visible(active == Some(tab.id), self.started.elapsed());
                tab.content.lifecycle.defer(self.started.elapsed());
            }
            return true;
        }
        let now = self.started.elapsed();
        let mut changed = false;
        let ids: Vec<_> = self.tabs.items.iter().map(|tab| tab.id).collect();
        for id in ids {
            let Some(tab) = self.tabs.items.iter().find(|tab| tab.id == id) else {
                continue;
            };
            let protection = self.suspension_protection(id, false);
            if protection.protected() || !self.settings.automatic_suspension {
                if matches!(
                    tab.content.lifecycle.phase,
                    Phase::Sleeping | Phase::Pending
                ) {
                    let success = native_background::resume(&tab.content.engine.view).is_ok();
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.content.lifecycle.resumed(success);
                        tab.content.lifecycle.defer(now);
                    }
                    changed = true;
                }
                continue;
            }
            if tab.content.lifecycle.pending_expired(now) {
                let resumed = native_background::resume(&tab.content.engine.view).is_ok();
                if let Some(tab) = self.tabs.get_mut(id) {
                    if resumed {
                        tab.content.lifecycle.unsupported();
                    } else {
                        tab.content.lifecycle.resumed(false);
                        self.notice = "A timed-out sleeping tab could not resume. Select it and use Wake tab to retry.".into();
                    }
                }
                changed = true;
                continue;
            }
            if tab.content.lifecycle.phase != Phase::Awake {
                continue;
            }
            let audio = match native_background::playing_audio(&tab.content.engine.view) {
                Ok(value) => value,
                Err(_) => {
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.content.lifecycle.unsupported();
                    }
                    changed = true;
                    continue;
                }
            };
            let protection = self.suspension_protection(id, audio);
            if let Some(tab) = self.tabs.get_mut(id)
                && let Some(ticket) = tab.content.lifecycle.request(
                    now,
                    Duration::from_secs(self.settings.suspend_after_seconds),
                    true,
                    protection,
                )
            {
                if native_background::suspend(
                    &tab.content.engine.view,
                    self.proxy.clone(),
                    id,
                    ticket,
                )
                .is_err()
                {
                    tab.content.lifecycle.unsupported();
                }
                changed = true;
            }
        }
        changed
    }
    fn suspension_result(&mut self, id: TabId, ticket: u64, success: bool) {
        let protection = self.suspension_protection(id, false);
        if let Some(tab) = self.tabs.get_mut(id) {
            let eligible = self.settings.automatic_suspension && !protection.protected();
            if !eligible
                || tab.content.lifecycle.generation != ticket
                || tab.content.lifecycle.phase != Phase::Pending
            {
                if success {
                    let resumed = native_background::resume(&tab.content.engine.view).is_ok();
                    tab.content.lifecycle.resumed(resumed);
                    tab.content.lifecycle.defer(self.started.elapsed());
                }
            } else {
                tab.content
                    .lifecycle
                    .complete(ticket, success, self.started.elapsed());
            }
        }
    }

    fn begin_clear(
        &mut self,
        request: ClearRequest,
        revoke: Option<(String, security::Capability)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        request.validate()?;
        if self.downloads.borrow().active_count() > 0 {
            return Err("Cancel or finish all downloads before clearing browsing data.".into());
        }
        let neutral = self.prepare_page("about:blank")?;
        self.filter = Filter::default();
        self.permissions.borrow_mut().cancel_all();
        self.popups.borrow_mut().clear();
        self.next_clear += 1;
        self.permissions.borrow_mut().enabled = false;
        self.clearing = Some(Clearing {
            failed: false,
            ticket: self.next_clear,
            request: request.clone(),
            revision: 0,
            library_done: !(request.history || request.permissions),
            engine_done: false,
            started: Instant::now(),
            started_native: false,
            results: Vec::new(),
        });
        // Keep monotonically increasing tab IDs so late events cannot target a new tab.
        self.tabs.items.clear();
        self.tabs.push("about:blank".into(), neutral)?;
        self.panel = Some(Panel::Settings);
        self.activate()?;
        if request.session {
            self.settings.restore_session = false;
            let result = self
                .store
                .as_ref()
                .map(|store| store.forget_session(self.settings.clone()))
                .unwrap_or(Ok(()))
                .and_then(|()| privacy::clear_session_backups(&self.profile).map(|_| ()));
            if let Some(clear) = &mut self.clearing {
                clear.failed |= result.is_err();
                clear.results.push(if result.is_ok(){"Saved session URLs and session recovery files cleared; restoration is now disabled."}else{"Session clearing failed or was partial; recovery files may remain."}.into());
            }
        }
        if request.history || request.permissions {
            let action = if let Some((origin, capability)) = revoke {
                Action::SetPermission {
                    origin,
                    capability,
                    decision: None,
                }
            } else {
                Action::ClearLibrary {
                    history: request.history,
                    permissions: request.permissions,
                    confirmed: true,
                }
            };
            if self.library_action(action).is_ok() {
                if let Some(clear) = &mut self.clearing {
                    clear.revision = self.revision;
                }
            } else if let Some(clear) = &mut self.clearing {
                clear.failed = true;
                clear.library_done = true;
                clear
                    .results
                    .push("Application database clearing failed.".into());
            }
        }
        self.notice="Clearing selected data. Tabs were closed; waiting for the blank page and engine confirmation.".into();
        Ok(())
    }
    fn start_engine_clear(&mut self) {
        let Some(clear) = &mut self.clearing else {
            return;
        };
        if clear.started_native {
            return;
        }
        let Some(tab) = self.tabs.active() else {
            return;
        };
        if tab.content.state.loading
            || tab.content.engine.view.url().ok().as_deref() != Some("about:blank")
        {
            return;
        }
        clear.started_native = true;
        if native_security::clear(
            &tab.content.engine.view,
            &clear.request,
            false,
            clear.ticket,
            self.proxy.clone(),
        )
        .is_err()
        {
            clear.engine_done = true;
            clear.failed = true;
            clear
                .results
                .push("Engine clearing could not start; its data may remain.".into());
        }
    }
    fn finish_clear(&mut self) {
        let done = self
            .clearing
            .as_ref()
            .is_some_and(|clear| clear.library_done && clear.engine_done);
        if done && let Some(clear) = self.clearing.take() {
            self.permission_lockout |= clear.failed && clear.request.permissions;
            self.permissions.borrow_mut().enabled = !self.permission_lockout;
            self.notice = format!(
                "{} Bookmarks and downloaded files were kept. Library recovery backups were retained and may contain old history or permissions. New browsing can create new data; this is not secure disk erasure.",
                clear.results.join(" ")
            );
            if clear.failed && clear.request.permissions {
                self.notice.push_str(" Website permission requests remain denied until restart because revocation was not fully confirmed.");
            }
            self.persist();
        }
    }
}

#[cfg(test)]
#[path = "app_validation.rs"]
mod validation;

#[cfg(test)]
#[path = "benchmark_probe.rs"]
mod benchmark_probe;

#[cfg(test)]
#[path = "ux_validation.rs"]
mod ux_validation;

#[cfg(test)]
#[path = "compat_validation.rs"]
mod compat_validation;

#[cfg(test)]
#[path = "daily_validation.rs"]
mod daily_validation;

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
                    Key::Named(NamedKey::PageUp) => 0x21,
                    Key::Named(NamedKey::PageDown) => 0x22,
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
                if let Some(session) = &mut self.session
                    && session.clearing.is_some()
                {
                    session.notice =
                        "Wait for the clearing result before closing the browser.".into();
                    session.render();
                    return;
                }
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
                if let Some(session) = &mut self.session
                    && session.focus_pending
                    && session.focus_active().is_err()
                {
                    session.notice =
                        "Could not focus the selected tab. Select it again to retry.".into();
                    session.render();
                }
            }
            _ => {}
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        if self
            .session
            .as_ref()
            .is_some_and(|s| s.private && s.tabs.items.len() == 1 && s.clearing.is_none())
            && (matches!(&event, Event::Command(Command::CloseActive))
                || matches!(&event,Event::PageClose(id) if self.session.as_ref().is_some_and(|s|s.tabs.active().is_some_and(|t|t.id==*id && t.content.opener.is_some()))))
        {
            self.session = None;
            event_loop.exit();
            return;
        }
        let Some(session) = &mut self.session else {
            return;
        };
        match event {
            Event::DiagnosticsUpdated(report) => {
                if report.previous_exit_unclean && !session.diagnostic_warning_shown {
                    session.diagnostic_warning_shown = true;
                    session.notice = "The previous normal window did not finish cleanly. Only committed session data can be restored; unsaved page work may be lost.".into();
                }
            }
            Event::RendererFailed => session.diagnostics.record(DiagnosticCode::RendererFailed),
            Event::PopupRequested(ticket) => {
                if let Err(error) = session.open_popup(ticket) {
                    session.notice = format!("Error: {error}");
                }
            }
            Event::PopupBlocked => {
                session.notice =
                    "Popup blocked: only user-initiated HTTP(S) or blank popups are supported."
                        .into()
            }
            Event::PageClose(id) => {
                if session
                    .tabs
                    .items
                    .iter()
                    .any(|t| t.id == id && t.content.opener.is_some())
                {
                    let _ = session.close_tab(id);
                }
            }
            Event::Fullscreen(id, enabled) => {
                if session.tabs.active().is_some_and(|t| t.id == id) {
                    session.fullscreen = enabled.then_some(id);
                    session.window.set_fullscreen(if enabled {
                        Some(winit::window::Fullscreen::Borderless(None))
                    } else {
                        None
                    });
                    let _ = session.resize();
                }
            }
            Event::SuspensionResult{id,ticket,success}=>session.suspension_result(id,ticket,success),
            Event::PermissionPrompt=>{session.filter=Filter::default();session.panel=Some(Panel::Settings);let _=session.activate();}
            Event::EngineCleared{startup,success,ticket}=>{
                if startup {
                    if success {session.diagnostics.record(DiagnosticCode::Started);session.permissions.borrow_mut().enabled=true;session.ready=true;for tab in &session.tabs.items {if content_target(&tab.url).map_err(Box::<dyn std::error::Error>::from).and_then(|target|tab.content.engine.load(&target)).is_err(){session.notice="A restored tab could not load.".into();}}}
                    else {session.diagnostics.record(DiagnosticCode::EngineClearFailed);session.notice="Security initialization failed. Websites remain unloaded; update WebView2 and restart.".into();}
                } else if let Some(clear)=&mut session.clearing && clear.ticket==ticket {
                    clear.engine_done=true;clear.failed |= !success;
                    if clear.request.history || clear.request.cookies || clear.request.cache || clear.request.permissions {
                        clear.results.push(if success {"WebView2 confirmed clearing the selected engine data."}else{"WebView2 reported a clearing failure; some engine data may remain."}.into());
                    }
                }
            }
            Event::Library(reply) => {
                if let Some(data)=&reply.data {session.permissions.borrow_mut().rules=data.permissions.clone();}
                if let Some(clear)=&mut session.clearing && clear.revision==reply.revision && !clear.library_done {
                    clear.library_done=true;clear.failed |= reply.message.is_some();
                    clear.results.push(if reply.message.is_none(){"Selected application history/permission records cleared."}else{"Application database clearing failed; records may remain."}.into());
                }
                if let Some(message) = reply.message {
                    session.diagnostics.record(DiagnosticCode::LibraryFailed);
                    session.notice = message;
                }
                if reply.revision == session.revision
                    && let Some(data) = reply.data
                {
                    session.library_data = data;
                }
            }
            Event::DownloadStopped(id, reason) => {
                session.diagnostics.record(DiagnosticCode::DownloadStopped);
                session.downloads.borrow_mut().stopped(id, reason)
            }
            Event::DownloadTick => {
                session.downloads.borrow_mut().poll(&session.proxy);
            }
            Event::DownloadRequested(id, tab_id) => {
                session.notice.clear();
                if let Some(tab) = session.tabs.get_mut(tab_id) {
                    tab.content.state.loading = false;
                    tab.content.state.message = "Download handed to manager.".into();
                }
                let owner=match session.window.window_handle().map(|handle|handle.as_raw()){
                    Ok(RawWindowHandle::Win32(handle))=>handle.hwnd.get(),
                    _=>{session.downloads.borrow_mut().destination(id,Err("Could not identify the browser window for destination selection.".into()));return;}
                };
                let proxy = session.proxy.clone();
                if std::thread::Builder::new()
                    .name("download-folder-picker".into())
                    .spawn(move || {
                        let result = native_downloads::choose_folder(owner);
                        if let Ok(Some(folder))=&result {
                            let notice=match rust_browser::download_recovery::clean_empty_abandoned(folder){
                                Ok(r) if r.cleaned>0 || r.preserved>0=>Some(format!("Download staging check: {} empty abandoned folders cleaned; {} nonempty or unrecognized owned folders preserved for manual review. Live transfers were kept.",r.cleaned,r.preserved)),
                                Ok(_)=>None,
                                Err(_)=>Some("Download staging could not be checked; existing files were preserved.".into()),
                            };
                            if let Some(notice)=notice {let _=proxy.send_event(Event::DownloadRecovery(notice));}
                        }
                        let _ = proxy.send_event(Event::DownloadDestination(id, result));
                    })
                    .is_err()
                {
                    session
                        .downloads
                        .borrow_mut()
                        .destination(id, Err("Could not start the folder picker.".into()));
                }
                session.panel = Some(Panel::Downloads);
                let _ = session.activate();
            }
            Event::DownloadDestination(id, result) => {
                session.downloads.borrow_mut().destination(id, result)
            }
            Event::DownloadRejected=>session.notice="A download from this address is already active, or the download limit was reached.".into(),
            Event::DownloadFinished(id, result) => {
                if result.is_err() { session.diagnostics.record(DiagnosticCode::DownloadFailed); }
                session.downloads.borrow_mut().finished(id, result)
            }
            Event::DownloadRecovery(notice)=>session.notice=notice,
            Event::Command(command) => {
                if let Err(error) = session.command(command) {
                    session.diagnostics.record(DiagnosticCode::CommandFailed);
                    session.notice = format!("Error: {error}");
                }
            }
            Event::Page(id, event) => session.page_event(id, event),
            Event::StorageError => {
                session.diagnostics.record(DiagnosticCode::SessionSaveFailed);
                *session.last_snapshot.borrow_mut()=None;
                session.notice =
                    "Error: Session could not be saved. Check profile access and disk space.".into()
            }
        }
        session.finish_clear();
        session.render();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(session) = &mut self.session {
            session.popups.borrow_mut().expire();
            if session.background_tick() {
                session.render();
            }
            if session.permissions.borrow_mut().expire() {
                session.render();
            }
            if session
                .clearing
                .as_ref()
                .is_some_and(|c| c.started.elapsed() > Duration::from_secs(60))
            {
                session.clearing = None;
                session.permission_lockout = true;
                session.notice =
                    "Clearing timed out. Completion is unconfirmed; some data may remain. Website permission requests remain denied until restart.".into();
                session.render();
            }
            if session.downloads.borrow_mut().poll(&session.proxy) {
                session.render();
            }
            if session.downloads.borrow().active_count() > 0
                || session.permissions.borrow().prompt().is_some()
                || session.clearing.is_some()
            {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(250),
                ));
            } else {
                event_loop.set_control_flow(
                    if session.settings.automatic_suspension && session.tabs.items.len() > 1 {
                        ControlFlow::WaitUntil(session.next_background)
                    } else {
                        ControlFlow::Wait
                    },
                );
            }
        }
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
