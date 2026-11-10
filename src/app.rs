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
    tabs::{ClosedTabs, MAX_TABS, TabId, Tabs},
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
    /// A page asked for a non-HTTP(S) top-level address; its document is unchanged.
    NavigationBlocked(TabId),
    PageClose(TabId),
    Fullscreen(TabId, bool),
    StorageError,
    DiagnosticsUpdated(diagnostics::Report),
    EngineProcess(TabId, crate::native_activity::Failure),
    /// The toolbar view's own WebView2 process failed.
    ToolbarProcess(crate::native_activity::Failure),
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

/// A closed tab's view stays hidden this long before it is destroyed. Destroying a
/// view that was visible moments earlier can crash the shared WebView2 browser
/// process (measured in VALIDATION.md); 300 ms was the shortest safe delay observed.
const RETIRE_DELAY: Duration = Duration::from_secs(1);
/// Hidden views kept for their delay. Closing every possible tab within the delay
/// (holding Ctrl+W, for example) fits; only opening and closing more than this many
/// tabs within one delay destroys the oldest hidden view early. A cap of 16 did that
/// for a 17-tab burst and crashed the engine (measured in VALIDATION.md).
const MAX_RETIRED: usize = MAX_TABS;
/// Automatic engine restarts allowed within `ENGINE_RESTART_WINDOW`.
const MAX_ENGINE_RESTARTS: usize = 3;
/// How long a failed navigation waits for a download request before it is logged.
/// Only the local diagnostic record is delayed; the tab shows the failure at once.
const FAILURE_RECORD_DELAY: Duration = Duration::from_secs(2);
const ENGINE_RESTART_WINDOW: Duration = Duration::from_secs(120);

/// Create the trusted toolbar view: local HTML only, no permissions, downloads,
/// navigation, new windows or developer tools, and size-limited IPC commands. Used at
/// startup and to replace the toolbar after its own engine process failed.
fn build_toolbar(
    window: &Window,
    context: &mut WebContext,
    proxy: &EventLoopProxy<Event>,
    private: bool,
) -> Result<WebView, Box<dyn std::error::Error>> {
    let ipc_proxy = proxy.clone();
    let toolbar_url = format!(
        "data:text/html;charset=utf-8;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(ui::HTML)
    );
    let allowed_url = toolbar_url.clone();
    let toolbar = WebViewBuilder::new_with_web_context(context)
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
        .build_as_child(window)?;
    if private {
        native_security::verify_private(&toolbar)?;
    }
    engine::accelerators(&toolbar, proxy.clone())?;
    crate::native_activity::watch_toolbar(&toolbar, proxy.clone())?;
    Ok(toolbar)
}

/// The toolbar keeps failing (or cannot be rebuilt): the window can no longer be
/// controlled, so say so once in a native dialog. Tabs are already saved.
fn toolbar_unrecoverable(window: &Window) {
    #[cfg(not(test))]
    {
        let owner = match window.window_handle().map(|handle| handle.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => 0,
        };
        // SAFETY: a static message owned by this process's own live window.
        unsafe {
            use windows::{
                Win32::{
                    Foundation::HWND,
                    UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
                },
                core::w,
            };
            let _ = MessageBoxW(
                Some(HWND(owner as *mut _)),
                w!(
                    "The browser toolbar keeps stopping and was not restarted again.\n\nClose and reopen Rust Browser. Open tabs are saved."
                ),
                w!("Rust Browser"),
                MB_ICONERROR | MB_OK,
            );
        }
    }
    #[cfg(test)]
    let _ = window;
}

/// Bound automatic restarts so a persistently crashing engine cannot loop.
fn admit_engine_restart(history: &mut Vec<Instant>, now: Instant) -> bool {
    history.retain(|at| now.duration_since(*at) < ENGINE_RESTART_WINDOW);
    if history.len() >= MAX_ENGINE_RESTARTS {
        return false;
    }
    history.push(now);
    true
}

fn http_status_message(code: u16) -> String {
    let meaning = match code {
        400 => "the request was not understood",
        401 | 407 => "authentication is required",
        403 => "access is not allowed",
        404 | 410 => "the page was not found",
        408 | 504 => "the website took too long",
        429 => "too many requests were sent; wait before retrying",
        500..=599 => "the website has a problem; retry later",
        _ => "the request was refused",
    };
    format!("The website answered with HTTP {code}: {meaning}.")
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
    /// Hidden views of closed tabs awaiting destruction, oldest first.
    retired: Vec<(Instant, Page)>,
    closed: ClosedTabs,
    toolbar: WebView,
    /// A failed toolbar view, hidden and kept for `RETIRE_DELAY` like a closed tab.
    retired_toolbar: Option<(Instant, WebView)>,
    content_context: WebContext,
    toolbar_context: WebContext,
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
    /// The content browser process ended; views are rebuilt on the next idle turn.
    engine_lost: bool,
    engine_restarts: Vec<Instant>,
    toolbar_restarts: Vec<Instant>,
    /// Failed navigations not yet written to diagnostics. A navigation that becomes a
    /// download ends as "aborted" shortly before its download request arrives.
    unrecorded_failures: Vec<(Instant, TabId)>,
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
        let toolbar = build_toolbar(&window, &mut toolbar_context, &proxy, private)?;
        let mut session = Self {
            tabs: Tabs::default(),
            retired: Vec::new(),
            closed: ClosedTabs::default(),
            toolbar,
            retired_toolbar: None,
            content_context,
            toolbar_context,
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
            engine_lost: false,
            engine_restarts: Vec::new(),
            toolbar_restarts: Vec::new(),
            unrecorded_failures: Vec::new(),
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
        session.reset_engine_permissions()?;
        Ok(session)
    }

    /// Reset WebView2's legacy remembered permission states before any website
    /// loads. All new decisions are scoped in our library with SavesInProfile=false.
    fn reset_engine_permissions(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tab) = self.tabs.active() {
            native_security::clear(
                &tab.content.engine.view,
                &ClearRequest {
                    permissions: true,
                    confirmed: true,
                    ..ClearRequest::default()
                },
                true,
                0,
                self.proxy.clone(),
            )?;
        }
        Ok(())
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
        let engine = self.build_engine(id, environment.clone())?;
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

    fn build_engine(
        &mut self,
        id: TabId,
        environment: Option<
            webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment,
        >,
    ) -> Result<Engine, Box<dyn std::error::Error>> {
        let engine = Engine::new(
            &self.window,
            &mut self.content_context,
            self.proxy.clone(),
            id,
            self.downloads.clone(),
            self.permissions.clone(),
            engine::Options {
                private: self.private,
                environment,
            },
        )?;
        engine::accelerators(&engine.view, self.proxy.clone())?;
        native_popup::install(&engine.view, self.popups.clone(), self.proxy.clone(), id)?;
        Ok(engine)
    }

    /// Hide a closed tab's view now and destroy it after `RETIRE_DELAY`.
    fn retire(&mut self, page: Page) {
        // A view that cannot be hidden is already unusable; it is still only dropped.
        let _ = page.engine.set_visible(false);
        if self.retired.len() >= MAX_RETIRED {
            self.retired.remove(0);
        }
        self.retired.push((Instant::now() + RETIRE_DELAY, page));
    }

    /// Destroy retired views whose delay has passed. Returns the next deadline.
    fn reap_retired(&mut self) -> Option<Instant> {
        let now = Instant::now();
        let before = self.retired.len();
        self.retired.retain(|(deadline, _)| *deadline > now);
        if self.retired.len() != before {
            // Engine data clearing waits until no document of a closed tab remains.
            self.start_engine_clear();
        }
        if self
            .retired_toolbar
            .as_ref()
            .is_some_and(|(deadline, _)| *deadline <= now)
        {
            self.retired_toolbar = None;
        }
        let pages = self.retired.first().map(|(deadline, _)| *deadline);
        let toolbar = self.retired_toolbar.as_ref().map(|(deadline, _)| *deadline);
        match (pages, toolbar) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Write navigation failures that no download request claimed within the delay
    /// (all of them when `flush` is set). Returns the next deadline.
    fn record_navigation_failures(&mut self, flush: bool) -> Option<Instant> {
        let now = Instant::now();
        let before = self.unrecorded_failures.len();
        self.unrecorded_failures
            .retain(|(deadline, _)| !flush && *deadline > now);
        for _ in self.unrecorded_failures.len()..before {
            self.diagnostics.record(DiagnosticCode::NavigationFailed);
        }
        self.unrecorded_failures
            .first()
            .map(|(deadline, _)| *deadline)
    }

    /// A navigation turned into a download. WebView2 has just reported that
    /// navigation as aborted, but the tab still shows its previous, intact document.
    fn download_requested(&mut self, tab_id: TabId) {
        self.notice.clear();
        // Only the newest failure of this tab is the download's own abort.
        if let Some(index) = self
            .unrecorded_failures
            .iter()
            .rposition(|(_, id)| *id == tab_id)
        {
            self.unrecorded_failures.remove(index);
        }
        if let Some(tab) = self.tabs.get_mut(tab_id) {
            tab.content.state.loading = false;
            tab.content.state.failed = false;
            tab.content.state.message = "Download handed to manager.".into();
        }
    }

    fn process_failed(&mut self, id: TabId, failure: crate::native_activity::Failure) {
        use crate::native_activity::Failure;
        match failure {
            // Every view of the profile reports this once; recover a single time.
            Failure::Browser => self.engine_lost = true,
            Failure::Renderer => {
                self.diagnostics.record(DiagnosticCode::RendererFailed);
                self.page_event(
                    id,
                    PageEvent::Error(
                        "This page's WebView2 process failed. Use Reload to recover it; unsaved page changes may be lost."
                            .into(),
                    ),
                );
            }
            Failure::Unresponsive => {
                if self.tabs.items.iter().any(|tab| tab.id == id) {
                    self.notice =
                        "A page is not responding. Wait for it, or use Reload or close its tab."
                            .into();
                }
            }
            // WebView2 restarts these helpers itself; the page is still usable.
            Failure::Helper => {}
        }
    }

    /// The toolbar's own engine process failed. Its browser process and renderer are
    /// separate from the content engine's, so pages keep working, but the dead view
    /// would leave the window without an address bar, tabs or menus. Replace it.
    fn toolbar_failed(&mut self, failure: crate::native_activity::Failure) {
        use crate::native_activity::Failure;
        if matches!(failure, Failure::Helper | Failure::Unresponsive) {
            return;
        }
        self.diagnostics.record(DiagnosticCode::ToolbarFailed);
        if !admit_engine_restart(&mut self.toolbar_restarts, Instant::now()) {
            toolbar_unrecoverable(&self.window);
            return;
        }
        let toolbar = match build_toolbar(
            &self.window,
            &mut self.toolbar_context,
            &self.proxy,
            self.private,
        ) {
            Ok(toolbar) => toolbar,
            Err(_) => {
                toolbar_unrecoverable(&self.window);
                return;
            }
        };
        let failed = std::mem::replace(&mut self.toolbar, toolbar);
        // A view that cannot be hidden is already unusable; it is still only dropped.
        let _ = failed.set_visible(false);
        // Restarts are bounded, so at most one failed toolbar waits at a time in
        // practice; a newer one replaces (and destroys) an older one.
        self.retired_toolbar = Some((Instant::now() + RETIRE_DELAY, failed));
        // The new toolbar announces itself with `Command::Ready`, which re-renders.
        self.last_render.borrow_mut().clear();
        self.notice =
            "The browser toolbar stopped and was restarted. Open tabs were not affected.".into();
        let _ = self.resize();
    }

    /// Replace every content view after the shared browser process ended. Tab
    /// identity, order, pins and addresses are kept; documents are reloaded.
    fn recover_engine(&mut self) {
        self.engine_lost = false;
        self.diagnostics.record(DiagnosticCode::EngineRestarted);
        if !admit_engine_restart(&mut self.engine_restarts, Instant::now()) {
            self.notice = "The browser engine keeps stopping and was not restarted again. Restart Rust Browser; open tabs are saved.".into();
            return;
        }
        // Deferrals and operations of the ended process can no longer complete.
        self.permissions.borrow_mut().cancel_all();
        self.popups.borrow_mut().clear();
        self.retired.clear();
        self.fullscreen = None;
        self.window.set_fullscreen(None);
        let mut failed = false;
        for index in 0..self.tabs.items.len() {
            let id = self.tabs.items[index].id;
            match self.build_engine(id, None) {
                Ok(engine) => {
                    let ready = self.ready;
                    let tab = &mut self.tabs.items[index];
                    let loaded = !ready
                        || content_target(&tab.url)
                            .map_err(Box::<dyn std::error::Error>::from)
                            .and_then(|target| engine.load(&target))
                            .is_ok();
                    failed |= !loaded;
                    tab.content = Page {
                        engine,
                        state: State {
                            url: tab.url.clone(),
                            loading: loaded,
                            failed: !loaded,
                            message: if loaded {
                                "Connecting…"
                            } else {
                                "Error: This tab could not be reloaded."
                            }
                            .into(),
                            ..State::default()
                        },
                        lifecycle: Lifecycle::default(),
                        opener: None,
                    };
                }
                Err(_) => failed = true,
            }
        }
        // Startup security initialization was lost with the old process.
        if !self.ready && self.reset_engine_permissions().is_err() {
            failed = true;
        }
        failed |= self.activate().is_err();
        self.notice = if failed {
            "The browser engine stopped and could not be fully restarted. Restart Rust Browser; open tabs are saved."
        } else {
            "The browser engine stopped unexpectedly and was restarted. Open pages were reloaded; unsaved page work and unfinished downloads may be lost."
        }
        .into();
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
        if let Some(tab) = self.tabs.close(id) {
            if tab.url != navigation::HOME && navigation::address(&tab.url).is_ok() {
                self.closed.push(tab.url);
            }
            self.retire(tab.content);
        }
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
            Command::SelectIndex(index) => {
                self.panel = None;
                let last = self.tabs.items.len().saturating_sub(1);
                if let Some(tab) = self.tabs.items.get(index.min(last)) {
                    let id = tab.id;
                    self.tabs.select(id);
                    self.activate()?;
                }
            }
            Command::ReopenClosedTab => {
                let Some(url) = self.closed.pop() else {
                    self.notice = "No recently closed tab to reopen in this window.".into();
                    return Ok(());
                };
                self.panel = None;
                if let Err(error) = self.open_tab(url.clone()) {
                    self.closed.push(url);
                    return Err(error);
                }
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
                http_status,
                navigation_id,
                url,
                title,
            } => {
                if error.is_some() && self.unrecorded_failures.len() < MAX_TABS {
                    self.unrecorded_failures
                        .push((Instant::now() + FAILURE_RECORD_DELAY, id));
                }
                if error.is_none()
                    && http_status.is_none()
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
                tab.content.state.message = match (error, http_status) {
                    (Some(error), _) => format!("Error: {error}"),
                    (None, Some(code)) => http_status_message(code),
                    (None, None) => {
                        "Ready · Ctrl+L address · Ctrl+T new tab · Ctrl+Tab switch · F7 page".into()
                    }
                };
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
        // Cleared data must not be reachable again through "reopen closed tab".
        self.closed.clear();
        // Keep monotonically increasing tab IDs so late events cannot target a new tab.
        for tab in std::mem::take(&mut self.tabs.items) {
            self.retire(tab.content);
        }
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
        // Documents of the closed tabs must be gone before their data is cleared.
        if clear.started_native || !self.retired.is_empty() {
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
mod tests {
    use super::*;
    #[test]
    fn a_profile_held_by_another_window_is_distinguished_from_startup_faults() {
        // Store::open reports the held session lock as WouldBlock.
        let root = std::env::temp_dir().join(format!(
            "rust-browser-start-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (first, _, _) = Store::open(&root, || {}).unwrap();
        let second: Box<dyn std::error::Error> = match Store::open(&root, || {}) {
            Ok(_) => panic!("a second store opened a locked profile"),
            Err(error) => error.into(),
        };
        assert_eq!(start_failure(second.as_ref()), StartFailure::ProfileInUse);
        drop(first);
        let missing: Box<dyn std::error::Error> =
            std::io::Error::from(std::io::ErrorKind::NotFound).into();
        assert_eq!(start_failure(missing.as_ref()), StartFailure::Other);
        let text: Box<dyn std::error::Error> = "WebView2 is unavailable".into();
        assert_eq!(start_failure(text.as_ref()), StartFailure::Other);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn engine_restarts_are_bounded_within_the_window_and_allowed_again_after_it() {
        let start = Instant::now();
        let mut history = Vec::new();
        for attempt in 0..MAX_ENGINE_RESTARTS {
            assert!(admit_engine_restart(
                &mut history,
                start + Duration::from_secs(attempt as u64)
            ));
        }
        assert!(!admit_engine_restart(
            &mut history,
            start + Duration::from_secs(10)
        ));
        assert!(admit_engine_restart(
            &mut history,
            start + ENGINE_RESTART_WINDOW + Duration::from_secs(5)
        ));
    }
    #[test]
    fn server_error_statuses_have_specific_non_connection_messages() {
        assert!(http_status_message(404).contains("HTTP 404: the page was not found"));
        assert!(http_status_message(429).contains("too many requests"));
        assert!(http_status_message(503).contains("retry later"));
        for code in [400, 401, 403, 404, 418, 500, 503] {
            let message = http_status_message(code);
            assert!(!message.starts_with("Error:") && !message.contains("connection"));
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

#[cfg(test)]
#[path = "survey_validation.rs"]
mod survey_validation;

#[cfg(test)]
#[path = "reliability_validation.rs"]
mod reliability_validation;

#[cfg(test)]
#[path = "toolbar_validation.rs"]
mod toolbar_validation;

/// Why the browser window could not be created.
#[derive(Debug, PartialEq, Eq)]
pub enum StartFailure {
    /// Another normal window already holds this profile. Not a fault.
    ProfileInUse,
    Other,
}
fn start_failure(error: &(dyn std::error::Error + 'static)) -> StartFailure {
    match error.downcast_ref::<std::io::Error>() {
        Some(error) if error.kind() == std::io::ErrorKind::WouldBlock => StartFailure::ProfileInUse,
        _ => StartFailure::Other,
    }
}

struct App {
    session: Option<Session>,
    proxy: EventLoopProxy<Event>,
    failure: Option<StartFailure>,
    modifiers: ModifiersState,
    /// Set once the window is closing: when the hidden views may be destroyed.
    closing: Option<Instant>,
}
impl App {
    /// Hide everything now and destroy the views after `RETIRE_DELAY`, for the same
    /// reason closed tabs are retired: destroying a visible view can crash the
    /// WebView2 browser process while it is writing profile data.
    fn begin_shutdown(&mut self) {
        if self.closing.is_some() {
            return;
        }
        if let Some(session) = &mut self.session {
            session.record_navigation_failures(true);
            session.persist();
            session.window.set_visible(false);
            for tab in &session.tabs.items {
                let _ = tab.content.engine.set_visible(false);
            }
            let _ = session.toolbar.set_visible(false);
        }
        self.closing = Some(Instant::now() + RETIRE_DELAY);
    }
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
                self.failure = Some(start_failure(error.as_ref()));
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
                    Key::Named(NamedKey::F4) => 0x73,
                    Key::Named(NamedKey::Home) => 0x24,
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
                self.begin_shutdown();
                event_loop.set_control_flow(ControlFlow::Poll);
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
        if self.closing.is_some() {
            return;
        }
        if self
            .session
            .as_ref()
            .is_some_and(|s| s.private && s.tabs.items.len() == 1 && s.clearing.is_none())
            && (matches!(&event, Event::Command(Command::CloseActive))
                || matches!(&event,Event::PageClose(id) if self.session.as_ref().is_some_and(|s|s.tabs.active().is_some_and(|t|t.id==*id && t.content.opener.is_some()))))
        {
            self.begin_shutdown();
            event_loop.set_control_flow(ControlFlow::Poll);
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
            Event::EngineProcess(id, failure) => session.process_failed(id, failure),
            Event::ToolbarProcess(failure) => session.toolbar_failed(failure),
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
            Event::NavigationBlocked(id) => {
                if session.tabs.items.iter().any(|tab| tab.id == id) {
                    session.notice = "A link to an app or unsupported address was blocked. Only http:// and https:// addresses open here; the page is unchanged.".into();
                }
            }
            Event::DownloadRequested(id, tab_id) => {
                session.download_requested(tab_id);
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
        if let Some(deadline) = self.closing {
            if Instant::now() >= deadline {
                self.session = None;
                event_loop.exit();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
            return;
        }
        if let Some(session) = &mut self.session {
            if session.engine_lost {
                session.recover_engine();
                session.render();
            }
            let recording = session.record_navigation_failures(false);
            let retiring = match (session.reap_retired(), recording) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
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
            let periodic = if session.downloads.borrow().active_count() > 0
                || session.permissions.borrow().prompt().is_some()
                || session.clearing.is_some()
            {
                Some(Instant::now() + Duration::from_millis(250))
            } else if session.settings.automatic_suspension && session.tabs.items.len() > 1 {
                Some(session.next_background)
            } else {
                None
            };
            // Wake for whichever comes first; idle sessions still wait without polling.
            event_loop.set_control_flow(match (periodic, retiring) {
                (Some(a), Some(b)) => ControlFlow::WaitUntil(a.min(b)),
                (Some(at), None) | (None, Some(at)) => ControlFlow::WaitUntil(at),
                (None, None) => ControlFlow::Wait,
            });
        }
    }
}

pub fn run() -> Result<(), StartFailure> {
    let event_loop = EventLoop::<Event>::with_user_event()
        .build()
        .map_err(|_| StartFailure::Other)?;
    let mut app = App {
        session: None,
        proxy: event_loop.create_proxy(),
        failure: None,
        modifiers: ModifiersState::default(),
        closing: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|_| StartFailure::Other)?;
    app.failure.map_or(Ok(()), Err)
}
