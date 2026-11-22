//! The Linux front end: one browser window drawn by winit on X11 (XWayland in a
//! Wayland session), with WebKitGTK views from wry.
//!
//! It reuses everything that is not tied to WebView2: the trusted toolbar page and
//! its command protocol (`ui`), tabs, navigation, sessions, settings and the
//! bookmarks and history library (`library_worker`). What WebView2 alone provides on
//! Windows (extensions and the shield, downloads, permission prompts, private-data
//! clearing, multiple windows, profiles, sleeping tabs, password autofill) is not
//! offered here yet: such commands answer with a notice instead.
//!
//! Security model, as on Windows: website views only navigate to HTTP(S) and
//! `about:blank`, have no command bridge, and are denied every permission and
//! download; the toolbar is a separate WebKit context that may only show its own
//! local document; a link a website opens in a new window becomes a tab.
use crate::{
    library_worker::{self, Worker},
    ui::{self, Command, Panel, ResizeEdge, State, TabState, ToolbarState},
};
use rust_browser::{
    library::{Action, Data, Filter},
    navigation,
    settings::Settings,
    storage::{self, Snapshot, Store},
    tabs::{ClosedTabs, MAX_TABS, TabId, Tabs},
};
use std::{
    cell::RefCell,
    error::Error,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use webkit2gtk::{PermissionRequestExt, WebViewExt};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Fullscreen, ResizeDirection, Window, WindowId},
};
use wry::{
    PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtUnix,
    WebViewExtUnix,
};

/// How often GTK (which runs WebKitGTK) is serviced while winit waits.
const GTK_TICK: Duration = Duration::from_millis(8);
/// The toolbar's height until it reports its own.
const DEFAULT_CHROME: f64 = 119.0;
/// Zoom steps, as on Windows.
const ZOOM_STEPS: [f64; 13] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0,
];
const UNSUPPORTED: &str = "That is not available on Linux yet.";

/// What reaches the event loop from views, GTK handlers and worker threads.
#[derive(Debug)]
pub enum Event {
    /// From the trusted toolbar view.
    Command(Command),
    Page(TabId, PageEvent),
    /// A website asked for a new window: the address opens as a tab.
    OpenTab(String),
    Library(library_worker::Reply),
    StorageError,
}

#[derive(Debug)]
pub enum PageEvent {
    Started(String),
    Finished(String),
    Title(String),
    Fullscreen(bool),
    Blocked,
}

/// One tab's website view and what the toolbar shows about it.
struct Page {
    view: WebView,
    state: State,
    zoom: f64,
}

/// Launch options from the command line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub private: bool,
    pub version: bool,
    pub links: Vec<String>,
}

/// Read `--private`, `--version` and links (website addresses only) from the
/// arguments after the program name.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Launch, String> {
    let mut launch = Launch::default();
    for arg in args {
        match arg.as_str() {
            "--private" => launch.private = true,
            "--version" => launch.version = true,
            other if other.starts_with("--") => {
                return Err(format!("Unknown option {other}."));
            }
            link => launch
                .links
                .push(navigation::external(link).map_err(|e| e.to_string())?),
        }
        if launch.links.len() > MAX_TABS {
            return Err("Too many links.".into());
        }
    }
    Ok(launch)
}

/// Where each view goes: `None` hides it.
#[derive(Debug, PartialEq)]
struct Plan {
    toolbar: (f64, f64, f64, f64),
    page: Option<(f64, f64, f64, f64)>,
}

/// The layout: the toolbar on top and the page below it; a full panel, an open
/// dropdown or the new-tab page makes the toolbar fill the window and hides the
/// page (WebKitGTK child views cannot be clipped to a shape as on Windows); a
/// fullscreen page fills the window alone.
fn plan(width: f64, height: f64, chrome: f64, covered: bool, fullscreen: bool) -> Plan {
    let (width, height) = (width.max(1.0), height.max(1.0));
    if fullscreen && !covered {
        return Plan {
            toolbar: (0.0, 0.0, width, 0.0),
            page: Some((0.0, 0.0, width, height)),
        };
    }
    if covered {
        return Plan {
            toolbar: (0.0, 0.0, width, height),
            page: None,
        };
    }
    let chrome = chrome.clamp(1.0, height);
    Plan {
        toolbar: (0.0, 0.0, width, chrome),
        page: Some((0.0, chrome, width, (height - chrome).max(1.0))),
    }
}

fn rect((x, y, w, h): (f64, f64, f64, f64)) -> Rect {
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(w.max(1.0), h.max(1.0)).into(),
    }
}

fn next_zoom(current: f64, increase: bool) -> f64 {
    if increase {
        ZOOM_STEPS
            .iter()
            .copied()
            .find(|z| *z > current + 0.001)
            .unwrap_or(ZOOM_STEPS[ZOOM_STEPS.len() - 1])
    } else {
        ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|z| *z < current - 0.001)
            .unwrap_or(ZOOM_STEPS[0])
    }
}

struct Browser {
    window: Window,
    toolbar: WebView,
    tabs: Tabs<Page>,
    closed: ClosedTabs,
    content: WebContext,
    proxy: EventLoopProxy<Event>,
    settings: Settings,
    store: Option<Store>,
    library: Worker,
    data: Data,
    filter: Filter,
    revision: u64,
    private: bool,
    panel: Option<Panel>,
    menu_open: bool,
    fullscreen: bool,
    chrome: f64,
    notice: String,
    last_render: RefCell<String>,
    run_id: u128,
    loads: u64,
}

impl Browser {
    fn open(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
        profile: &Path,
        private: bool,
        links: Vec<String>,
    ) -> Result<Self, Box<dyn Error>> {
        let (store, snapshot) = if private {
            (None, Snapshot::default())
        } else {
            let errors = proxy.clone();
            let (store, snapshot, _) = Store::open(profile, move || {
                let _ = errors.send_event(Event::StorageError);
            })?;
            (Some(store), snapshot)
        };
        let window = event_loop.create_window(
            Window::default_attributes()
                .with_title("Rust Browser")
                .with_decorations(false)
                .with_inner_size(LogicalSize::new(1200.0, 820.0))
                .with_min_inner_size(LogicalSize::new(480.0, 360.0)),
        )?;
        let mut toolbar_context = WebContext::new((!private).then(|| profile.join("toolbar")));
        let toolbar = toolbar_view(&window, &mut toolbar_context, proxy.clone(), private)?;
        let library_proxy = proxy.clone();
        let library = Worker::start(
            if private {
                PathBuf::from(":memory:")
            } else {
                profile.join("library.sqlite")
            },
            move |reply| {
                let _ = library_proxy.send_event(Event::Library(reply));
            },
        )?;
        let mut browser = Self {
            window,
            toolbar,
            tabs: Tabs::default(),
            closed: ClosedTabs::default(),
            content: WebContext::new((!private).then(|| profile.join("content"))),
            proxy,
            settings: snapshot.settings.clone(),
            store,
            library,
            data: Data::default(),
            filter: Filter::default(),
            revision: 0,
            private,
            panel: None,
            menu_open: false,
            fullscreen: false,
            chrome: DEFAULT_CHROME,
            notice: String::new(),
            last_render: RefCell::new(String::new()),
            run_id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
            loads: 0,
        };
        // The toolbar view must exist for as long as its context: keep it alive.
        std::mem::forget(toolbar_context);
        let restore = !private && browser.settings.restore_session;
        if restore {
            for (index, url) in snapshot.tabs.iter().enumerate().take(MAX_TABS) {
                if browser.open_tab(url.clone()).is_ok()
                    && let Some(tab) = browser.tabs.active_mut()
                {
                    tab.pinned = snapshot.pinned.get(index).copied().unwrap_or(false);
                    if let Some(title) = snapshot.titles.get(index) {
                        tab.title.clone_from(title);
                    }
                }
            }
            if let Some(id) = browser.tabs.items.get(snapshot.active).map(|t| t.id) {
                browser.tabs.select(id);
            }
        }
        for link in links {
            let _ = browser.open_tab(link);
        }
        if browser.tabs.items.is_empty() {
            browser.open_tab(navigation::HOME.into())?;
        }
        browser.layout();
        browser.refresh_library();
        Ok(browser)
    }

    /// Add a tab at `url` (an HTTP(S) address or the new-tab page) and select it.
    fn open_tab(&mut self, url: String) -> Result<(), Box<dyn Error>> {
        let url = if url == navigation::HOME || url == "about:blank" {
            url
        } else {
            navigation::address(&url)?
        };
        if self.tabs.items.len() >= MAX_TABS {
            return Err("The 64-tab limit has been reached.".into());
        }
        let id = self.tabs.next_id();
        let view = content_view(
            &self.window,
            &mut self.content,
            self.proxy.clone(),
            id,
            self.private,
        )?;
        if url != navigation::HOME {
            view.load_url(&url)?;
        }
        let state = State {
            url: url.clone(),
            loading: url != navigation::HOME,
            ..State::default()
        };
        self.tabs.push(
            url,
            Page {
                view,
                state,
                zoom: 1.0,
            },
        )?;
        Ok(())
    }

    fn covered(&self) -> bool {
        self.panel.is_some()
            || self.menu_open
            || self
                .tabs
                .active()
                .is_some_and(|t| t.url == navigation::HOME)
    }

    /// Place and show the views for the current state.
    fn layout(&self) {
        let size = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor());
        let plan = plan(
            size.width,
            size.height,
            self.chrome,
            self.covered(),
            self.fullscreen,
        );
        let shown = plan.toolbar.3 > 0.0;
        if shown {
            let _ = self.toolbar.set_bounds(rect(plan.toolbar));
        }
        let _ = self.toolbar.set_visible(shown);
        let active = self.tabs.active().map(|t| t.id);
        for tab in &self.tabs.items {
            let visible = Some(tab.id) == active && plan.page.is_some();
            if visible && let Some(page) = plan.page {
                let _ = tab.content.view.set_bounds(rect(page));
            }
            let _ = tab.content.view.set_visible(visible);
        }
    }

    fn persist(&self) {
        let Some(store) = &self.store else {
            return;
        };
        let restore = self.settings.restore_session;
        let tabs: Vec<_> = self
            .tabs
            .items
            .iter()
            .filter(|_| restore)
            .collect();
        let snapshot = Snapshot {
            tabs: tabs.iter().map(|t| t.url.clone()).collect(),
            active: if restore {
                self.tabs.active_index()
            } else {
                0
            },
            pinned: tabs.iter().map(|t| t.pinned).collect(),
            titles: tabs.iter().map(|t| t.title.clone()).collect(),
            keep_awake: tabs.iter().map(|_| false).collect(),
            settings: self.settings.clone(),
            ..Snapshot::default()
        };
        store.queue(snapshot);
    }

    fn submit(&mut self, action: Action) -> Result<(), Box<dyn Error>> {
        if self.private && !matches!(action, Action::Refresh) {
            return Err("Private windows do not write bookmarks or history.".into());
        }
        self.revision += 1;
        let mut filter = self.filter.clone();
        filter.page = self
            .tabs
            .active()
            .filter(|t| navigation::allowed(&t.url))
            .map(|t| t.url.clone());
        self.library.submit(library_worker::Job {
            revision: self.revision,
            action,
            filter,
        })?;
        Ok(())
    }

    fn refresh_library(&mut self) {
        if let Err(error) = self.submit(Action::Refresh) {
            self.notice = error.to_string();
        }
    }

    fn render(&self) {
        let Some(active) = self.tabs.active() else {
            return;
        };
        let page = &active.content.state;
        let security = rust_browser::security::indicator(&active.url, page.loading, page.failed);
        let zoom = active.content.zoom;
        let state = ToolbarState {
            zoom_percent: ((zoom - 1.0).abs() > 0.001).then(|| (zoom * 100.0).round() as u32),
            page_tools: navigation::allowed(&active.url),
            diagnostics: None,
            settings: &self.settings,
            private: self.private,
            restore_session: self.settings.restore_session,
            security,
            permission_prompt: None,
            permission_seconds: None,
            busy: false,
            page,
            tabs: self
                .tabs
                .items
                .iter()
                .map(|tab| TabState {
                    id: tab.id,
                    title: &tab.title,
                    active: tab.id == active.id,
                    pinned: tab.pinned,
                    keep_awake: false,
                    phase: rust_browser::background::Phase::Awake,
                    loading: tab.content.state.loading,
                    favicon: &tab.content.state.favicon,
                })
                .collect(),
            active_id: active.id,
            search_engine: self.settings.search_engine,
            notice: &self.notice,
            notice_kind: ui::notice_kind(&self.notice, ""),
            panel: self.panel.or_else(|| {
                (active.url == navigation::HOME).then_some(Panel::NewTab)
            }),
            menu_open: self.menu_open,
            library_panel: None,
            vertical_tabs: false,
            maximized: self.window.is_maximized(),
            fullscreen: self.fullscreen,
            controls_in_library: false,
            blocked: None,
            count_blocked: false,
            extensions: Vec::new(),
            library: &self.data,
            library_folder: self.filter.folder,
            history_enabled: self.settings.history_enabled,
            downloads: Vec::new(),
            profiles: Vec::new(),
            profile_colors: &rust_browser::profiles::COLORS,
            can_add_profile: false,
        };
        if let Ok(json) = serde_json::to_string(&state)
            && *self.last_render.borrow() != json
            && self
                .toolbar
                .evaluate_script(&format!("window.render && window.render({json})"))
                .is_ok()
        {
            *self.last_render.borrow_mut() = json;
        }
        let title = if active.title.is_empty() {
            "Rust Browser".to_string()
        } else {
            format!("{} - Rust Browser", active.title)
        };
        self.window.set_title(&title);
    }

    fn select(&mut self, id: TabId) {
        if self.tabs.select(id) {
            self.panel = None;
        }
    }

    fn close_tab(&mut self, id: TabId) -> Result<(), Box<dyn Error>> {
        if let Some(tab) = self.tabs.close(id) {
            let _ = tab.content.view.set_visible(false);
            if navigation::allowed(&tab.url) {
                self.closed.push(tab.url);
            }
        }
        if self.tabs.items.is_empty() {
            self.open_tab(navigation::HOME.into())?;
        }
        Ok(())
    }

    fn command(&mut self, el: &ActiveEventLoop, command: Command) -> Result<(), Box<dyn Error>> {
        match command {
            Command::Ready => self.last_render.borrow_mut().clear(),
            Command::ToolbarHeight(height) => {
                if height.is_finite() && height >= 40.0 {
                    self.chrome = height.min(320.0);
                }
            }
            Command::MinimizeWindow => self.window.set_minimized(true),
            Command::ToggleMaximize => self.window.set_maximized(!self.window.is_maximized()),
            Command::CloseWindow | Command::ExitBrowser => {
                self.persist();
                el.exit();
            }
            Command::DragWindow => {
                let _ = self.window.drag_window();
            }
            Command::ResizeWindow(edge) => {
                let _ = self.window.drag_resize_window(match edge {
                    ResizeEdge::North => ResizeDirection::North,
                    ResizeEdge::NorthWest => ResizeDirection::NorthWest,
                    ResizeEdge::NorthEast => ResizeDirection::NorthEast,
                });
            }
            Command::DismissNotice => self.notice.clear(),
            Command::SetMenuOpen(open) => self.menu_open = open,
            Command::SetMenuArea { .. } | Command::RefreshDiagnostics | Command::RefreshProfiles => {}
            Command::Navigate(input) => {
                let url = navigation::resolve(&input, self.settings.search_engine)?;
                self.panel = None;
                let tab = self.tabs.active_mut().ok_or("No tab is selected.")?;
                tab.url.clone_from(&url);
                tab.content.state.url.clone_from(&url);
                tab.content.state.loading = true;
                tab.content.state.failed = false;
                tab.content.view.load_url(&url)?;
                self.persist();
            }
            Command::Home => {
                self.panel = None;
                if let Some(tab) = self.tabs.active_mut() {
                    tab.url = navigation::HOME.into();
                    tab.title.clear();
                    tab.content.state = State {
                        url: navigation::HOME.into(),
                        ..State::default()
                    };
                    let _ = tab.content.view.load_url("about:blank");
                }
                self.persist();
            }
            Command::Back | Command::Forward | Command::Reload | Command::Stop => {
                let tab = self.tabs.active().ok_or("No tab is selected.")?;
                let engine = tab.content.view.webview();
                match command {
                    Command::Back if engine.can_go_back() => engine.go_back(),
                    Command::Forward if engine.can_go_forward() => engine.go_forward(),
                    Command::Reload if navigation::allowed(&tab.url) => engine.reload(),
                    Command::Stop => engine.stop_loading(),
                    _ => {}
                }
            }
            Command::ZoomIn | Command::ZoomOut | Command::ResetZoom => {
                let tab = self.tabs.active_mut().ok_or("No tab is selected.")?;
                let zoom = match command {
                    Command::ZoomIn => next_zoom(tab.content.zoom, true),
                    Command::ZoomOut => next_zoom(tab.content.zoom, false),
                    _ => 1.0,
                };
                tab.content.view.zoom(zoom)?;
                tab.content.zoom = zoom;
            }
            Command::PrintPage => {
                self.tabs
                    .active()
                    .ok_or("No tab is selected.")?
                    .content
                    .view
                    .print()?;
            }
            Command::NewTab => {
                self.panel = None;
                self.open_tab(navigation::HOME.into())?;
                self.persist();
            }
            Command::CloseActive => {
                if let Some(id) = self.tabs.active().map(|t| t.id) {
                    self.close_tab(id)?;
                }
                self.persist();
            }
            Command::CloseTab(id) => {
                self.close_tab(id)?;
                self.persist();
            }
            Command::CloseOtherTabs(keep) => {
                let others: Vec<_> = self
                    .tabs
                    .items
                    .iter()
                    .filter(|t| t.id != keep && !t.pinned)
                    .map(|t| t.id)
                    .collect();
                for id in others {
                    self.close_tab(id)?;
                }
                self.persist();
            }
            Command::SelectTab(id) => self.select(id),
            Command::CycleTab(backwards) => {
                self.tabs.cycle(backwards);
                self.panel = None;
            }
            Command::SelectIndex(index) => {
                let index = index.min(self.tabs.items.len().saturating_sub(1));
                if let Some(id) = self.tabs.items.get(index).map(|t| t.id) {
                    self.select(id);
                }
            }
            Command::MoveTab { id, to } => {
                self.tabs.move_to(id, to);
                self.persist();
            }
            Command::MoveActive(right) => {
                if let Some(id) = self.tabs.active().map(|t| t.id) {
                    let index = self.tabs.active_index();
                    let to = if right {
                        index + 1
                    } else {
                        index.saturating_sub(1)
                    };
                    self.tabs.move_to(id, to);
                    self.persist();
                }
            }
            Command::PinActive => {
                if let Some(tab) = self.tabs.active() {
                    let (id, pinned) = (tab.id, !tab.pinned);
                    self.tabs.pin(id, pinned);
                    self.persist();
                }
            }
            Command::PinTab(id) => {
                if let Some(pinned) = self.tabs.items.iter().find(|t| t.id == id).map(|t| !t.pinned) {
                    self.tabs.pin(id, pinned);
                    self.persist();
                }
            }
            Command::DuplicateTab(id) => {
                let url = self
                    .tabs
                    .items
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.url.clone())
                    .ok_or("That tab is closed.")?;
                self.open_tab(url)?;
                self.persist();
            }
            Command::ReopenClosedTab => {
                let url = self.closed.pop().ok_or("No closed tab to reopen.")?;
                self.open_tab(url)?;
                self.persist();
            }
            Command::OpenSaved { url, new_tab } => {
                let url = rust_browser::library::bookmark_url(&url).map_err(|e| e.to_string())?;
                self.panel = None;
                if new_tab {
                    self.open_tab(url)?;
                    self.persist();
                } else {
                    return self.command(el, Command::Navigate(url));
                }
            }
            Command::OpenSavedTabs(urls) => {
                self.panel = None;
                for url in urls.into_iter().take(MAX_TABS) {
                    let url = rust_browser::library::bookmark_url(&url).map_err(|e| e.to_string())?;
                    self.open_tab(url)?;
                }
                self.persist();
            }
            Command::ShowPanel(Panel::NewTab) => return self.command(el, Command::Home),
            Command::ShowPanel(panel) => {
                self.panel = Some(panel);
                self.filter = Filter::default();
                self.refresh_library();
            }
            Command::ToggleLibrary => {
                self.panel = match self.panel {
                    Some(Panel::Bookmarks | Panel::History | Panel::Downloads) => None,
                    _ => Some(Panel::Bookmarks),
                };
                self.refresh_library();
            }
            Command::ClosePanel | Command::CloseLibrary => self.panel = None,
            Command::QueryLibrary(filter) => {
                self.filter = filter;
                self.refresh_library();
            }
            Command::Library(action) => self.submit(action)?,
            Command::BookmarkCurrent => {
                let tab = self.tabs.active().ok_or("No tab is selected.")?;
                if !navigation::allowed(&tab.url) {
                    return Err("Only website pages can be bookmarked.".into());
                }
                let action = Action::SaveBookmark {
                    id: None,
                    title: tab.title.clone(),
                    url: tab.url.clone(),
                    folder: None,
                };
                self.submit(action)?;
            }
            Command::SetSearchEngine(engine) => {
                self.settings.search_engine = engine;
                self.persist();
            }
            Command::SetTheme(theme) => {
                self.settings.theme = theme;
                self.persist();
            }
            Command::SetHistoryEnabled(on) => {
                self.settings.history_enabled = on;
                self.persist();
            }
            Command::SetRestoreSession(on) => {
                self.settings.restore_session = on;
                self.persist();
            }
            Command::SetShowRecent(on) => {
                self.settings.show_recent = on;
                self.persist();
            }
            Command::SetBookmarksBar(on) => {
                self.settings.bookmarks_bar = on;
                self.persist();
            }
            Command::SetNewtabBackground(background) => {
                self.settings.newtab_background = background;
                self.persist();
            }
            Command::FocusPage => {
                if let Some(tab) = self.tabs.active() {
                    let _ = tab.content.view.focus();
                }
            }
            Command::FocusAddress | Command::FocusTabs => {
                let _ = self.toolbar.focus();
            }
            _ => return Err(UNSUPPORTED.into()),
        }
        Ok(())
    }

    fn page_event(&mut self, id: TabId, event: PageEvent) {
        let history = self.settings.history_enabled && !self.private;
        let Some(tab) = self.tabs.get_mut(id) else {
            return;
        };
        match event {
            PageEvent::Started(url) => {
                if navigation::allowed(&url) {
                    tab.url.clone_from(&url);
                    tab.content.state.url = url;
                }
                tab.content.state.loading = true;
                tab.content.state.failed = false;
            }
            PageEvent::Finished(url) => {
                tab.content.state.loading = false;
                if navigation::allowed(&url) {
                    tab.url.clone_from(&url);
                    tab.content.state.url.clone_from(&url);
                    let engine = tab.content.view.webview();
                    tab.content.state.back = engine.can_go_back();
                    tab.content.state.forward = engine.can_go_forward();
                    if history {
                        self.loads += 1;
                        let action = Action::Record {
                            key: format!("linux-{}-{id}-{}", self.run_id, self.loads),
                            url,
                            title: tab.title.clone(),
                            timestamp: rust_browser::library::now(),
                        };
                        if let Err(error) = self.submit(action) {
                            self.notice = error.to_string();
                        }
                    }
                }
                self.persist();
            }
            PageEvent::Title(title) => {
                tab.title = title.chars().filter(|c| !c.is_control()).take(512).collect();
                self.persist();
            }
            PageEvent::Fullscreen(on) => {
                if self.tabs.active().is_some_and(|t| t.id == id) {
                    self.fullscreen = on;
                    self.window
                        .set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
                }
            }
            PageEvent::Blocked => {
                self.notice =
                    "Downloads and website permissions are not available on Linux yet.".into();
            }
        }
    }
}

/// The trusted toolbar: the same local document as on Windows, in its own WebKit
/// context, limited to that exact document, with the command bridge.
fn toolbar_view(
    window: &Window,
    context: &mut WebContext,
    proxy: EventLoopProxy<Event>,
    private: bool,
) -> Result<WebView, Box<dyn Error>> {
    use base64::Engine as _;
    let url = format!(
        "data:text/html;charset=utf-8;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(ui::HTML)
    );
    let allowed = url.clone();
    let view = WebViewBuilder::new_with_web_context(context)
        .with_url(&url)
        .with_incognito(private)
        .with_devtools(false)
        .with_clipboard(true)
        .with_hotkeys_zoom(false)
        .with_permission_handler(|_| wry::PermissionResponse::Deny)
        .with_download_started_handler(|_, _| false)
        .with_navigation_handler(move |to| to == allowed)
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_ipc_handler(move |request| {
            if request.body().len() <= 16384
                && let Ok(command) = serde_json::from_str::<Command>(request.body())
            {
                let _ = proxy.send_event(Event::Command(command));
            }
        })
        .build_as_child(window)?;
    Ok(view)
}

/// A website view: HTTP(S) and `about:blank` only, no command bridge, every
/// permission and download refused, new windows opened as tabs.
fn content_view(
    window: &Window,
    context: &mut WebContext,
    proxy: EventLoopProxy<Event>,
    id: TabId,
    private: bool,
) -> Result<WebView, Box<dyn Error>> {
    let (title, load, popup, blocked) = (proxy.clone(), proxy.clone(), proxy.clone(), proxy.clone());
    let download = proxy.clone();
    let view = WebViewBuilder::new_with_web_context(context)
        .with_incognito(private)
        .with_devtools(false)
        .with_clipboard(false)
        .with_autoplay(false)
        .with_hotkeys_zoom(true)
        .with_visible(false)
        .with_navigation_handler(move |url| {
            let allowed = url == "about:blank" || navigation::allowed(&url);
            if !allowed {
                let _ = blocked.send_event(Event::Page(id, PageEvent::Blocked));
            }
            allowed
        })
        .with_new_window_req_handler(move |url, _| {
            if navigation::allowed(&url) {
                let _ = popup.send_event(Event::OpenTab(url));
            }
            wry::NewWindowResponse::Deny
        })
        .with_download_started_handler(move |_, _| {
            let _ = download.send_event(Event::Page(id, PageEvent::Blocked));
            false
        })
        .with_document_title_changed_handler(move |text| {
            let _ = title.send_event(Event::Page(id, PageEvent::Title(text)));
        })
        .with_on_page_load_handler(move |event, url| {
            let event = match event {
                PageLoadEvent::Started => PageEvent::Started(url),
                PageLoadEvent::Finished => PageEvent::Finished(url),
            };
            let _ = load.send_event(Event::Page(id, event));
        })
        .build_as_child(window)?;
    let engine = view.webview();
    // Every permission request (camera, microphone, location, notifications) is
    // refused until Linux has the prompt Windows has.
    engine.connect_permission_request(|_, request| {
        request.deny();
        true
    });
    let (enter, leave) = (proxy.clone(), proxy);
    engine.connect_enter_fullscreen(move |_| {
        let _ = enter.send_event(Event::Page(id, PageEvent::Fullscreen(true)));
        false
    });
    engine.connect_leave_fullscreen(move |_| {
        let _ = leave.send_event(Event::Page(id, PageEvent::Fullscreen(false)));
        false
    });
    Ok(view)
}

struct App {
    proxy: EventLoopProxy<Event>,
    profile: PathBuf,
    launch: Option<Launch>,
    browser: Option<Browser>,
    failed: Option<String>,
}

impl App {
    fn after(&mut self, el: &ActiveEventLoop, result: Result<(), Box<dyn Error>>) {
        let Some(browser) = &mut self.browser else {
            return;
        };
        if let Err(error) = result {
            browser.notice = error.to_string();
        }
        browser.layout();
        browser.render();
        let _ = el;
    }
}

impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.browser.is_some() {
            return;
        }
        let Some(launch) = self.launch.take() else {
            return;
        };
        match Browser::open(
            el,
            self.proxy.clone(),
            &self.profile,
            launch.private,
            launch.links,
        ) {
            Ok(browser) => self.browser = Some(browser),
            Err(error) => {
                self.failed = Some(error.to_string());
                el.exit();
            }
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, event: Event) {
        let Some(browser) = &mut self.browser else {
            return;
        };
        let result = match event {
            Event::Command(command) => browser.command(el, command),
            Event::Page(id, event) => {
                browser.page_event(id, event);
                Ok(())
            }
            Event::OpenTab(url) => browser.open_tab(url).map(|()| browser.persist()),
            Event::Library(reply) => {
                if let Some(message) = reply.message.or(reply.summary) {
                    browser.notice = message;
                }
                if let Some(data) = reply.data
                    && reply.revision >= browser.revision.saturating_sub(64)
                {
                    browser.data = *data;
                }
                Ok(())
            }
            Event::StorageError => {
                browser.notice =
                    "Error: Session could not be saved. Check profile access and disk space."
                        .into();
                Ok(())
            }
        };
        self.after(el, result);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(browser) = &mut self.browser else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                browser.persist();
                el.exit();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.after(el, Ok(()));
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // WebKitGTK runs on GTK's main loop; service it between winit's events.
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + GTK_TICK));
    }
}

/// Start the Linux browser. Returns a message for the user when it cannot.
pub fn run(launch: Launch) -> Result<(), String> {
    use winit::platform::x11::EventLoopBuilderExtX11;
    let profile = storage::profile_dir().map_err(|e| e.to_string())?;
    // wry's child views need X11; under Wayland this runs on XWayland.
    // SAFETY: called on the main thread before GTK, winit or any other thread
    // starts, so nothing reads the environment concurrently.
    unsafe {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    gtk::init().map_err(|_| "GTK could not start. Is a display available?".to_string())?;
    let event_loop = EventLoop::<Event>::with_user_event()
        .with_x11()
        .build()
        .map_err(|e| e.to_string())?;
    let mut app = App {
        proxy: event_loop.create_proxy(),
        profile,
        launch: Some(launch),
        browser: None,
        failed: None,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    // Views close before the window and the GTK main loop go away.
    app.browser = None;
    app.failed.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_and_links_are_read() {
        let args = |list: &[&str]| parse_args(list.iter().map(|s| s.to_string()));
        assert_eq!(args(&[]).unwrap(), Launch::default());
        let launch = args(&["--private", "https://example.com/"]).unwrap();
        assert!(launch.private && !launch.version);
        assert_eq!(launch.links, ["https://example.com/"]);
        assert!(args(&["--version"]).unwrap().version);
        assert!(args(&["--unknown"]).is_err());
        assert!(args(&["javascript:alert(1)"]).is_err());
        assert!(args(&["file:///etc/passwd"]).is_err());
    }

    #[test]
    fn the_page_sits_below_the_toolbar_unless_covered_or_fullscreen() {
        let normal = plan(1200.0, 800.0, 119.0, false, false);
        assert_eq!(normal.toolbar, (0.0, 0.0, 1200.0, 119.0));
        assert_eq!(normal.page, Some((0.0, 119.0, 1200.0, 681.0)));
        let covered = plan(1200.0, 800.0, 119.0, true, false);
        assert_eq!(covered.toolbar, (0.0, 0.0, 1200.0, 800.0));
        assert_eq!(covered.page, None);
        let full = plan(1920.0, 1080.0, 119.0, false, true);
        assert_eq!(full.toolbar.3, 0.0);
        assert_eq!(full.page, Some((0.0, 0.0, 1920.0, 1080.0)));
        // A dropdown opened over a fullscreen page brings the toolbar back.
        assert_eq!(plan(1920.0, 1080.0, 119.0, true, true).page, None);
    }

    #[test]
    fn zoom_moves_between_steps_and_stops_at_the_ends() {
        assert_eq!(next_zoom(1.0, true), 1.1);
        assert_eq!(next_zoom(1.0, false), 0.9);
        assert_eq!(next_zoom(2.0, true), 2.0);
        assert_eq!(next_zoom(0.25, false), 0.25);
    }
}
