//! The Linux front end: one browser window drawn by winit on X11 (XWayland in a
//! Wayland session), with WebKitGTK views from wry.
//!
//! It reuses everything that is not tied to WebView2: the trusted toolbar page and
//! its command protocol (`ui`), tabs, navigation, sessions, settings and the
//! bookmarks and history library (`library_worker`), and the download staging rules
//! (`linux_downloads`). What WebView2 alone provides on Windows (extensions and the
//! shield, permission prompts, private-data clearing, multiple windows, profiles,
//! sleeping tabs, password autofill) is not offered here yet: such commands answer
//! with a notice instead.
//!
//! Security model, as on Windows: website views only navigate to HTTP(S) and
//! `about:blank`, have no command bridge, are denied every permission, and download
//! only into a folder the user picks each time; the toolbar is a separate WebKit
//! context that may only show its own local document and never downloads; a link a
//! website opens in a new window becomes a tab.
use crate::{
    library_worker::{self, Worker},
    linux_downloads,
    ui::{self, Command, Panel, ResizeEdge, State, TabState, ToolbarState},
};
use gtk::prelude::WidgetExt as _;
use rust_browser::{
    layout::{
        ClipArea, FULLSCREEN_NOTICE, Full, LIBRARY_MAX, LIBRARY_MIN, LIBRARY_WIDTH, Layout,
        fullscreen_host, fullscreen_notice_area, layout,
    },
    library::{Action, Data, Filter},
    navigation,
    settings::Settings,
    storage::{self, Snapshot, Store},
    tabs::{ClosedTabs, MAX_TABS, TabId, Tabs},
};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};
use webkit2gtk::{FindControllerExt, PermissionRequestExt, WebViewExt};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Fullscreen, ResizeDirection, Window, WindowId},
};
use wry::{PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder, WebViewExtUnix};

/// How often GTK (which runs WebKitGTK) is serviced while winit waits.
const GTK_TICK: Duration = Duration::from_millis(8);
/// The toolbar's height until it reports its own.
const DEFAULT_CHROME: f64 = 119.0;
/// Zoom steps, as on Windows.
const ZOOM_STEPS: [f64; 13] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0,
];
/// The most matches the find bar highlights and counts.
const FIND_LIMIT: u32 = 1000;
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
    /// The clipboard's text arrived for the address field's Paste.
    Paste(String),
    StorageError,
    /// A download has the server's filename and waits for a folder.
    DownloadRequested(u64, String),
    /// The folder chooser closed: the chosen folder, or none when cancelled.
    DownloadDestination(u64, Option<PathBuf>),
    /// WebKit stopped a download; whether the user cancelled it.
    DownloadStopped(u64, bool),
    /// WebKit wrote the whole file.
    DownloadDone(u64),
    /// The file was moved into its folder, or could not be.
    DownloadFinished(u64, Result<PathBuf, String>),
    /// A download was refused: not HTTP(S), already active, or too many.
    DownloadRejected,
    /// What checking the chosen folder for abandoned staging found.
    DownloadRecovery(String),
}

#[derive(Debug)]
pub enum PageEvent {
    Started(String),
    Finished(String),
    Title(String),
    Fullscreen(bool),
    Blocked,
    /// The page was pressed with the mouse.
    Pressed,
    /// How many matches the page has for the find bar's text.
    FindCount(u32),
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

/// The part of the shared toolbar document a trusted view shows (its URL
/// fragment), as on Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Toolbar,
    Tabs,
    Library,
}

impl Role {
    fn fragment(self) -> &'static str {
        match self {
            Role::Toolbar => "",
            Role::Tabs => "#tabs",
            Role::Library => "#library",
        }
    }
}

/// Rectangles (x, y, width, height) in whole pixels.
type Rects = Vec<(i32, i32, i32, i32)>;

/// The rectangles, in whole logical pixels, that make up `areas`; rounded
/// corners are approximated one pixel row at a time, as an X11 shape needs.
fn clip_rects(areas: &[ClipArea]) -> Rects {
    let mut rects = Vec::new();
    for area in areas.iter().filter(|a| a.width > 0.0 && a.height > 0.0) {
        let (left, top) = (area.x.floor() as i32, area.y.floor() as i32);
        let right = (area.x + area.width).ceil() as i32;
        let bottom = (area.y + area.height).ceil() as i32;
        let (width, height) = (right - left, bottom - top);
        if width <= 0 || height <= 0 {
            continue;
        }
        let radius = (area.radius.round() as i32).clamp(0, width.min(height) / 2);
        let r = f64::from(radius);
        for row in 0..radius {
            // How far in the curve is, at the middle of this row, from each end.
            let dy = r - f64::from(row) - 0.5;
            let inset = (r - (r * r - dy * dy).max(0.0).sqrt()).round() as i32;
            let w = width - 2 * inset;
            rects.push((left + inset, top + row, w, 1));
            rects.push((left + inset, bottom - 1 - row, w, 1));
        }
        rects.push((left, top + radius, width, height - 2 * radius));
    }
    rects.retain(|r| r.2 > 0 && r.3 > 0);
    rects
}

/// Sets the shape of a view's X11 window, so that outside it the page below
/// shows and receives the pointer, as a window region does on Windows. GDK
/// cannot shape the container window wry hosts each view in (it is foreign to
/// GDK), so this uses XFixes on its own connection; shapes live on the server.
/// Without libX11 or libXfixes views stay unclipped.
struct Shaper {
    xlib: x11_dl::xlib::Xlib,
    // x11-dl names its XFixes bindings `Xlib` too.
    xfixes: x11_dl::xfixes::Xlib,
    display: *mut x11_dl::xlib::Display,
    /// What each window was last set to, so unchanged layouts cost nothing.
    applied: RefCell<Vec<(u64, Option<Rects>)>>,
}

impl Shaper {
    fn open() -> Option<Self> {
        let xlib = x11_dl::xlib::Xlib::open().ok()?;
        let xfixes = x11_dl::xfixes::Xlib::open().ok()?;
        // SAFETY: opens a new connection to the default display; checked for null.
        let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
        (!display.is_null()).then(|| Self {
            xlib,
            xfixes,
            display,
            applied: RefCell::new(Vec::new()),
        })
    }

    /// The X11 window wry hosts `view` in.
    fn window_of(view: &WebView) -> Option<u64> {
        use gtk::{glib::Cast, prelude::WidgetExt};
        view.webview()
            .toplevel()
            .and_then(|top| top.window())
            .and_then(|window| window.downcast::<gdkx11::X11Window>().ok())
            .map(|window| window.xid())
    }

    /// Give `view` the keyboard: GTK's focus moves only within a view, and each
    /// view is its own X11 window, so the X input focus moves to it as well. It
    /// goes to the web view's own window, not wry's container: GDK never selects
    /// key events on the container, so keys sent there would be lost.
    fn focus(&self, view: &WebView) {
        use gtk::{glib::Cast, prelude::WidgetExt};
        let _ = view.focus();
        let Some(xid) = view
            .webview()
            .window()
            .and_then(|window| window.downcast::<gdkx11::X11Window>().ok())
            .map(|window| window.xid())
        else {
            return;
        };
        const REVERT_TO_PARENT: i32 = 2;
        // SAFETY: this struct's own open connection and a live, mapped window of
        // this process; CurrentTime (0) as the timestamp.
        unsafe {
            (self.xlib.XSetInputFocus)(self.display, xid, REVERT_TO_PARENT, 0);
            (self.xlib.XFlush)(self.display);
        }
    }

    /// Clip `view` to `areas` (in its logical pixels, `scale` physical pixels
    /// each) and raise it above the page, or remove its clip.
    fn clip(&self, view: &WebView, areas: Option<&[ClipArea]>, scale: f64) {
        let Some(xid) = Self::window_of(view) else {
            return;
        };
        let rects = areas.map(|areas| {
            let scaled: Vec<_> = areas
                .iter()
                .map(|a| ClipArea {
                    x: a.x * scale,
                    y: a.y * scale,
                    width: a.width * scale,
                    height: a.height * scale,
                    radius: a.radius * scale,
                })
                .collect();
            clip_rects(&scaled)
        });
        {
            let mut applied = self.applied.borrow_mut();
            match applied.iter_mut().find(|(id, _)| *id == xid) {
                Some((_, last)) if *last == rects => return,
                Some((_, last)) => last.clone_from(&rects),
                None => applied.push((xid, rects.clone())),
            }
        }
        const SHAPE_BOUNDING: i32 = 0;
        // SAFETY: `display` is this struct's own open connection and `xid` a live
        // window of this process; the region is created and destroyed here.
        unsafe {
            match rects {
                None => (self.xfixes.XFixesSetWindowShapeRegion)(
                    self.display,
                    xid,
                    SHAPE_BOUNDING,
                    0,
                    0,
                    0,
                ),
                Some(rects) => {
                    let mut rects: Vec<_> = rects
                        .into_iter()
                        .map(|(x, y, w, h)| x11_dl::xlib::XRectangle {
                            x: x.clamp(0, i32::from(i16::MAX)) as i16,
                            y: y.clamp(0, i32::from(i16::MAX)) as i16,
                            width: w.clamp(0, i32::from(u16::MAX)) as u16,
                            height: h.clamp(0, i32::from(u16::MAX)) as u16,
                        })
                        .collect();
                    let region = (self.xfixes.XFixesCreateRegion)(
                        self.display,
                        rects.as_mut_ptr(),
                        rects.len() as i32,
                    );
                    (self.xfixes.XFixesSetWindowShapeRegion)(
                        self.display,
                        xid,
                        SHAPE_BOUNDING,
                        0,
                        0,
                        region,
                    );
                    (self.xfixes.XFixesDestroyRegion)(self.display, region);
                    (self.xlib.XRaiseWindow)(self.display, xid);
                }
            }
            (self.xlib.XFlush)(self.display);
        }
    }
}

impl Drop for Shaper {
    fn drop(&mut self) {
        // SAFETY: closes the connection `open` made; nothing uses it afterwards.
        unsafe { (self.xlib.XCloseDisplay)(self.display) };
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
    /// The vertical tab column and the library column, built when first needed
    /// and hidden, not destroyed, when not shown; each with its last render.
    tabs_view: Option<(WebView, RefCell<String>)>,
    library_view: Option<(WebView, RefCell<String>)>,
    toolbar_context: WebContext,
    shaper: Option<Shaper>,
    tabs: Tabs<Page>,
    closed: ClosedTabs,
    content: WebContext,
    /// Downloads from website views; hooked to their WebKit context with the
    /// first view.
    downloads: linux_downloads::Shared,
    downloads_hooked: bool,
    /// When download progress was last read.
    downloads_polled: Instant,
    proxy: EventLoopProxy<Event>,
    settings: Settings,
    store: Option<Store>,
    library: Worker,
    data: Data,
    filter: Filter,
    revision: u64,
    private: bool,
    panel: Option<Panel>,
    /// What the library column shows, while it is open.
    library_panel: Option<Panel>,
    library_width: f64,
    menu_open: bool,
    /// Where the open dropdown is drawn in the toolbar view.
    menu_area: Option<ClipArea>,
    /// The bookmarks-bar folder the open dropdown lists.
    menu_folder: Option<i64>,
    fullscreen: bool,
    /// The tab and text of the find bar's last search.
    find: Option<(TabId, String)>,
    /// Until when the page that entered fullscreen is named, and its host.
    fullscreen_notice: Option<(Instant, String)>,
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
        let toolbar = toolbar_view(
            &window,
            &mut toolbar_context,
            proxy.clone(),
            private,
            Role::Toolbar,
        )?;
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
            tabs_view: None,
            library_view: None,
            toolbar_context,
            shaper: Shaper::open(),
            tabs: Tabs::default(),
            closed: ClosedTabs::default(),
            content: WebContext::new((!private).then(|| profile.join("content"))),
            downloads: Rc::default(),
            downloads_hooked: false,
            downloads_polled: Instant::now(),
            proxy,
            settings: snapshot.settings.clone(),
            store,
            library,
            data: Data::default(),
            filter: Filter::default(),
            revision: 0,
            private,
            panel: None,
            library_panel: None,
            library_width: LIBRARY_WIDTH,
            menu_open: false,
            menu_area: None,
            menu_folder: None,
            fullscreen: false,
            find: None,
            fullscreen_notice: None,
            chrome: DEFAULT_CHROME,
            notice: String::new(),
            last_render: RefCell::new(String::new()),
            run_id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
            loads: 0,
        };
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
        if browser.settings.vertical_tabs {
            browser.ensure_side(Role::Tabs)?;
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
        // wry keeps the WebKit context to itself; every website view shares it.
        // Each view wry builds also adds its default handler, which saves to the
        // Downloads folder unasked; `linux_downloads` overrides it per download.
        if !self.downloads_hooked
            && let Some(context) = view.webview().context()
        {
            linux_downloads::install(&context, self.downloads.clone(), self.proxy.clone());
            self.downloads_hooked = true;
        }
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

    /// The full panel, or the new-tab page when the active tab shows it.
    fn effective_panel(&self) -> Option<Panel> {
        self.panel.or_else(|| {
            self.tabs
                .active()
                .filter(|t| t.url == navigation::HOME)
                .map(|_| Panel::NewTab)
        })
    }

    /// Where each view goes now, by the table Windows uses.
    fn current_layout(&self) -> Layout {
        let size = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor());
        let panel = self.effective_panel();
        let full = if self.panel.is_some() || (self.menu_open && panel.is_some()) {
            Full::Cover
        } else if self.menu_open {
            Full::Dropdown
        } else if panel == Some(Panel::NewTab) {
            Full::NewTab
        } else if self.fullscreen {
            Full::Bare
        } else {
            Full::None
        };
        layout(
            size.width,
            size.height,
            self.chrome,
            self.settings.vertical_tabs && self.tabs_view.is_some() && !self.fullscreen,
            self.library_panel
                .filter(|_| self.library_view.is_some() && !self.fullscreen)
                .map(|_| self.library_width),
            full,
        )
    }

    /// The fullscreen notice's area in the toolbar view, while a fullscreen page
    /// (the toolbar otherwise hidden) is being named.
    fn fullscreen_notice_shown(&self, plan: &Layout) -> Option<ClipArea> {
        (plan.toolbar.3 == 0.0
            && self.fullscreen
            && self
                .fullscreen_notice
                .as_ref()
                .is_some_and(|(until, _)| *until > Instant::now()))
        .then(|| fullscreen_notice_area(plan.toolbar.2))
    }

    /// Place and show the views for the current state.
    fn layout(&self) {
        let plan = self.current_layout();
        let notice = self.fullscreen_notice_shown(&plan);
        // A hidden wry view on X11 has no window to resize, so it is shown before
        // its bounds are set; bounds set while hidden are lost on the next show.
        let shown = plan.toolbar.3 > 0.0;
        let _ = self.toolbar.set_visible(shown || notice.is_some());
        if shown {
            let _ = self.toolbar.set_bounds(rect(plan.toolbar));
        } else if let Some(area) = notice {
            let (x, _, w, _) = plan.toolbar;
            let _ = self
                .toolbar
                .set_bounds(rect((x, 0.0, w, area.y + area.height)));
        }
        let active = self.tabs.active().map(|t| t.id);
        for tab in &self.tabs.items {
            let page = plan.page.filter(|_| Some(tab.id) == active);
            let _ = tab.content.view.set_visible(page.is_some());
            if let Some(page) = page {
                let _ = tab.content.view.set_bounds(rect(page));
            }
        }
        for (side, place) in [
            (&self.tabs_view, plan.tabs),
            (&self.library_view, plan.library),
        ] {
            if let Some((view, _)) = side {
                let _ = view.set_visible(place.is_some());
                if let Some(place) = place {
                    let _ = view.set_bounds(rect(place));
                }
            }
        }
        // Over a webpage, an open dropdown's toolbar shows only the chrome and the
        // dropdown; anywhere else it is unclipped.
        let dropdown = self.menu_open && plan.page.is_some() && plan.toolbar.3 > self.chrome;
        let areas = notice.map(|area| vec![area]).or_else(|| {
            dropdown.then(|| {
                let chrome = ClipArea {
                    x: 0.0,
                    y: 0.0,
                    width: plan.toolbar.2,
                    height: self.chrome,
                    radius: 0.0,
                };
                [Some(chrome), self.menu_area]
                    .into_iter()
                    .flatten()
                    .collect()
            })
        });
        if let Some(shaper) = &self.shaper {
            shaper.clip(&self.toolbar, areas.as_deref(), self.window.scale_factor());
        }
    }

    /// Create the side view for `role`, hidden, if it is missing.
    fn ensure_side(&mut self, role: Role) -> Result<(), Box<dyn Error>> {
        let present = match role {
            Role::Tabs => self.tabs_view.is_some(),
            Role::Library => self.library_view.is_some(),
            Role::Toolbar => true,
        };
        if present {
            return Ok(());
        }
        let view = toolbar_view(
            &self.window,
            &mut self.toolbar_context,
            self.proxy.clone(),
            self.private,
            role,
        )?;
        view.set_visible(false)?;
        let side = Some((view, RefCell::new(String::new())));
        match role {
            Role::Tabs => self.tabs_view = side,
            Role::Library => self.library_view = side,
            Role::Toolbar => {}
        }
        Ok(())
    }

    /// Open the library column beside the page, showing `panel`, and move keyboard
    /// focus into it.
    fn open_library(&mut self, panel: Panel) -> Result<(), Box<dyn Error>> {
        self.panel = None;
        self.ensure_side(Role::Library)?;
        self.library_panel = Some(panel);
        self.filter = Filter::default();
        self.refresh_library();
        self.layout();
        self.render();
        if let Some((view, _)) = &self.library_view {
            self.focus(view);
        }
        Ok(())
    }

    /// Give `view` the keyboard (see `Shaper::focus`).
    fn focus(&self, view: &WebView) {
        match &self.shaper {
            Some(x11) => x11.focus(view),
            None => {
                let _ = view.focus();
            }
        }
    }

    fn persist(&self) {
        let Some(store) = &self.store else {
            return;
        };
        let restore = self.settings.restore_session;
        let tabs: Vec<_> = self.tabs.items.iter().filter(|_| restore).collect();
        let snapshot = Snapshot {
            tabs: tabs.iter().map(|t| t.url.clone()).collect(),
            active: if restore { self.tabs.active_index() } else { 0 },
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
        filter.menu_folder = self.menu_folder;
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
        let plan = self.current_layout();
        let state = ToolbarState {
            zoom_percent: ((zoom - 1.0).abs() > 0.001).then(|| (zoom * 100.0).round() as u32),
            page_tools: self.effective_panel().is_none() && navigation::allowed(&active.url),
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
            panel: self.effective_panel(),
            menu_open: self.menu_open,
            library_panel: self.library_panel,
            vertical_tabs: plan.tabs.is_some(),
            maximized: self.window.is_maximized(),
            fullscreen: self.fullscreen,
            fullscreen_notice: self
                .fullscreen_notice_shown(&plan)
                .zip(self.fullscreen_notice.as_ref())
                .map(|(area, (_, host))| ui::FullscreenNotice {
                    host,
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: area.height,
                }),
            controls_in_library: !self.fullscreen
                && plan.library.is_some_and(|(_, y, _, _)| y == 0.0),
            blocked: None,
            count_blocked: false,
            extensions: Vec::new(),
            extension_popup: None,
            library: &self.data,
            library_folder: self.filter.folder,
            history_enabled: self.settings.history_enabled,
            downloads: self.downloads.borrow().records(),
            profiles: Vec::new(),
            profile_colors: &rust_browser::profiles::COLORS,
            can_add_profile: false,
        };
        if let Ok(json) = serde_json::to_string(&state) {
            let sides = [&self.tabs_view, &self.library_view]
                .into_iter()
                .flatten()
                .map(|(view, last)| (view, last));
            for (view, last) in [(&self.toolbar, &self.last_render)]
                .into_iter()
                .chain(sides)
            {
                if *last.borrow() != json
                    && view
                        .evaluate_script(&format!("window.render && window.render({json})"))
                        .is_ok()
                {
                    last.borrow_mut().clone_from(&json);
                }
            }
        }
        let title = if active.title.is_empty() {
            "Rust Browser".to_string()
        } else {
            format!("{} - Rust Browser", active.title)
        };
        self.window.set_title(&title);
    }

    /// Stop highlighting the matches of the last find, in whichever tab it ran.
    fn finish_find(&mut self) {
        if let Some((id, _)) = self.find.take()
            && let Some(finder) = self
                .tabs
                .items
                .iter()
                .find(|t| t.id == id)
                .and_then(|t| t.content.view.webview().find_controller())
        {
            finder.search_finish();
        }
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
            Command::SetMenuOpen(open) => {
                if !open {
                    self.menu_folder = None;
                    self.menu_area = None;
                }
                self.menu_open = open;
                // A dropdown opened from a shortcut takes the keyboard, as its
                // focused field or item expects.
                if open {
                    self.focus(&self.toolbar);
                }
            }
            Command::SetMenuArea {
                x,
                y,
                width,
                height,
                radius,
            } => {
                if self.menu_open
                    && [x, y, width, height, radius].iter().all(|v| v.is_finite())
                    && width > 0.0
                    && height > 0.0
                {
                    self.menu_area = Some(ClipArea {
                        x,
                        y,
                        width,
                        height,
                        radius: radius.clamp(0.0, 32.0),
                    });
                }
            }
            Command::OpenBookmarkFolder(id) => {
                if self.private {
                    return Err("Bookmarks are unavailable in private windows.".into());
                }
                if !self.data.folders.iter().any(|f| f.id == id) {
                    return Err("That bookmark folder no longer exists.".into());
                }
                self.menu_folder = Some(id);
                self.refresh_library();
            }
            Command::SetVerticalTabs(on) => {
                if self.private {
                    return Err(
                        "Private windows follow this setting from normal windows; change it there."
                            .into(),
                    );
                }
                self.settings.vertical_tabs = on;
                if on {
                    self.ensure_side(Role::Tabs)?;
                }
                self.persist();
            }
            Command::SetLibraryWidth(width) => {
                if !width.is_finite() {
                    return Err("Invalid library width.".into());
                }
                self.library_width = width.clamp(LIBRARY_MIN, LIBRARY_MAX);
            }
            Command::AddFavorite { title, url } => {
                let mut settings = self.settings.clone();
                let url = navigation::address(&url)?;
                // An open tab on the same site lends its icon at once.
                let icon = self
                    .tabs
                    .items
                    .iter()
                    .find(|t| {
                        rust_browser::settings::same_site(&t.url, &url)
                            && rust_browser::settings::valid_icon(&t.content.state.favicon)
                    })
                    .map(|t| t.content.state.favicon.clone())
                    .unwrap_or_default();
                settings.favorites.push(rust_browser::settings::Favorite {
                    title: title.trim().into(),
                    url,
                    icon,
                });
                settings.validate()?;
                self.settings = settings;
                self.persist();
            }
            Command::RemoveFavorite(index) => {
                if index >= self.settings.favorites.len() {
                    return Err("Favorite no longer exists.".into());
                }
                self.settings.favorites.remove(index);
                self.persist();
            }
            Command::RefreshDiagnostics | Command::RefreshProfiles => {}
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
            // As on Windows, find, zoom and print act on a website page only.
            Command::FindInPage
            | Command::PrintPage
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::ResetZoom
                if self.effective_panel().is_some() =>
            {
                return Err("Find, zoom and print apply to the visible website page. Return to the page first (F7).".into());
            }
            Command::FindInPage
            | Command::PrintPage
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::ResetZoom
                if !self
                    .tabs
                    .active()
                    .is_some_and(|t| navigation::allowed(&t.url)) =>
            {
                return Err("Find, zoom and print require a website page.".into());
            }
            Command::FindInPage => {
                self.focus(&self.toolbar);
                self.toolbar
                    .evaluate_script("typeof openFind==='function'&&openFind()")?;
            }
            Command::FindText { text, forward } => {
                if text.is_empty() || text.chars().count() > 1000 {
                    return Err("Find needs 1 to 1000 characters.".into());
                }
                let tab = self.tabs.active().ok_or("No tab is selected.")?;
                let id = tab.id;
                let finder = tab
                    .content
                    .view
                    .webview()
                    .find_controller()
                    .ok_or("Find in page is unavailable.")?;
                if self
                    .find
                    .as_ref()
                    .is_some_and(|(t, last)| *t == id && *last == text)
                {
                    if forward {
                        finder.search_next();
                    } else {
                        finder.search_previous();
                    }
                } else {
                    self.finish_find();
                    let options = (webkit2gtk::FindOptions::CASE_INSENSITIVE
                        | webkit2gtk::FindOptions::WRAP_AROUND)
                        .bits();
                    finder.search(&text, options, FIND_LIMIT);
                    finder.count_matches(&text, options, FIND_LIMIT);
                    self.find = Some((id, text));
                }
            }
            Command::CloseFind => self.finish_find(),
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
                if let Some(pinned) = self
                    .tabs
                    .items
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| !t.pinned)
                {
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
                    let url =
                        rust_browser::library::bookmark_url(&url).map_err(|e| e.to_string())?;
                    self.open_tab(url)?;
                }
                self.persist();
            }
            Command::ShowPanel(Panel::NewTab) => return self.command(el, Command::Home),
            // Bookmarks, history and downloads open beside the page; Settings stays
            // a full panel.
            Command::ShowPanel(panel @ (Panel::Bookmarks | Panel::History | Panel::Downloads)) => {
                return self.open_library(panel);
            }
            Command::ShowPanel(panel) => {
                self.panel = Some(panel);
                self.filter = Filter::default();
                self.refresh_library();
            }
            Command::ToggleLibrary => {
                if self.library_panel.is_some() {
                    self.library_panel = None;
                } else {
                    return self.open_library(Panel::Bookmarks);
                }
            }
            Command::ClosePanel => self.panel = None,
            Command::CloseLibrary => self.library_panel = None,
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
            // F7 and "Back to page": leave a full panel, then focus the page (or
            // the new-tab page's search field).
            Command::FocusPage => {
                self.panel = None;
                if self
                    .tabs
                    .active()
                    .is_some_and(|tab| tab.url == navigation::HOME)
                {
                    self.focus(&self.toolbar);
                    self.toolbar
                        .evaluate_script("document.getElementById('home-query').focus()")?;
                } else if let Some(tab) = self.tabs.active() {
                    self.focus(&tab.content.view);
                }
            }
            // The address field's menu. Only its selection is copied, so it is
            // bounded like an address; Paste reads the clipboard without waiting.
            Command::CopyText(text) => {
                if text.chars().count() > navigation::MAX_ADDRESS {
                    return Err("The selection is too long to copy.".into());
                }
                gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).set_text(&text);
            }
            Command::PasteAddress => {
                let proxy = self.proxy.clone();
                gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).request_text(move |_, text| {
                    if let Some(text) = text {
                        let _ = proxy.send_event(Event::Paste(text.to_string()));
                    }
                });
            }
            Command::FocusAddress => {
                self.focus(&self.toolbar);
                self.toolbar.evaluate_script("window.focusAddress()")?;
            }
            Command::FocusTabs => {
                let column = self
                    .tabs_view
                    .as_ref()
                    .filter(|_| self.current_layout().tabs.is_some())
                    .map(|(view, _)| view);
                let view = column.unwrap_or(&self.toolbar);
                self.focus(view);
                view.evaluate_script("window.focusActiveTab && window.focusActiveTab()")?;
            }
            Command::CancelDownload(id) => self.downloads.borrow_mut().cancel(id),
            Command::OpenDownloadFolder(id) => {
                let folder = self.downloads.borrow().folder(id);
                if let Some(folder) = folder {
                    linux_downloads::open_folder(&folder)?;
                }
            }
            _ => return Err(UNSUPPORTED.into()),
        }
        Ok(())
    }

    fn page_event(&mut self, id: TabId, event: PageEvent) {
        // A press on the page closes a dropdown open over it, as a press anywhere
        // else in the toolbar would.
        if let PageEvent::FindCount(count) = event {
            if self.find.as_ref().is_some_and(|(t, _)| *t == id) {
                let _ = self
                    .toolbar
                    .evaluate_script(&format!("window.findResult&&findResult({count})"));
            }
            return;
        }
        if matches!(event, PageEvent::Pressed) {
            if self.menu_open && self.tabs.active().is_some_and(|t| t.id == id) {
                let _ = self
                    .toolbar
                    .evaluate_script("typeof closeDropdown==='function'&&closeDropdown(false)");
            }
            return;
        }
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
                tab.title = title
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(512)
                    .collect();
                self.persist();
            }
            PageEvent::Fullscreen(on) => {
                let host = fullscreen_host(&tab.url);
                if self.tabs.active().is_some_and(|t| t.id == id) {
                    self.fullscreen = on;
                    self.fullscreen_notice = on.then(|| (Instant::now() + FULLSCREEN_NOTICE, host));
                    self.window
                        .set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
                }
            }
            PageEvent::Pressed | PageEvent::FindCount(_) => {}
            PageEvent::Blocked => {
                self.notice = "A link to an app or unsupported address was blocked. Only http:// and https:// addresses open here; the page is unchanged.".into();
            }
        }
    }

    /// A download is ready for its folder: say so in its tab, list it, and ask.
    fn download_requested(&mut self, id: u64, suggested: &str) {
        let view = self.downloads.borrow().view(id);
        if let Some(tab) = view.and_then(|view| {
            self.tabs
                .items
                .iter_mut()
                .find(|t| t.content.view.webview() == view)
        }) {
            tab.content.state.loading = false;
            tab.content.state.failed = false;
            tab.content.state.message = "Download handed to manager.".into();
        }
        if self
            .downloads
            .borrow_mut()
            .requested(id, suggested, &self.proxy)
            && let Err(error) = self.open_library(Panel::Downloads)
        {
            self.notice = error.to_string();
        }
    }

    /// The folder chooser closed. A chosen folder is first checked for staging
    /// folders an earlier run abandoned, off the UI thread.
    fn download_destination(&mut self, id: u64, folder: Option<PathBuf>) {
        if let Some(folder) = folder.clone() {
            let proxy = self.proxy.clone();
            let _ = std::thread::Builder::new()
                .name("download-staging-check".into())
                .spawn(move || {
                    use rust_browser::download_recovery::{clean_empty_abandoned, staging_notice};
                    if let Some(notice) = staging_notice(&clean_empty_abandoned(&folder)) {
                        let _ = proxy.send_event(Event::DownloadRecovery(notice));
                    }
                });
        }
        self.downloads.borrow_mut().destination(id, folder);
    }

    /// Refresh download progress twice a second while any transfer runs.
    fn poll_downloads(&mut self) {
        if self.downloads_polled.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.downloads_polled = Instant::now();
        let changed = {
            let mut downloads = self.downloads.borrow_mut();
            downloads.active_count() > 0 && downloads.poll()
        };
        if changed {
            self.render();
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
    role: Role,
) -> Result<WebView, Box<dyn Error>> {
    // Every trusted view loads the same local document; the fragment names the
    // part it shows. Navigation stays limited to this exact URL.
    let url = format!("{TOOLBAR_URL}{}", role.fragment());
    let allowed = url.clone();
    // The scheme belongs to the trusted views' own context, so website views
    // (another context) cannot load it; the first view registers it for all.
    let register = !context.is_custom_protocol_registered(TOOLBAR_SCHEME);
    let mut builder = WebViewBuilder::new_with_web_context(context);
    if register {
        builder = builder.with_custom_protocol(TOOLBAR_SCHEME.into(), |_, request| {
            toolbar_document(request.uri().path())
        });
    }
    let view = builder
        .with_url(&url)
        .with_incognito(private)
        .with_devtools(false)
        .with_clipboard(true)
        .with_hotkeys_zoom(false)
        .with_permission_handler(|_| wry::PermissionResponse::Deny)
        .with_download_started_handler(|_, _| false)
        .with_navigation_handler(move |to| to == allowed)
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_ipc_handler({
            let proxy = proxy.clone();
            move |request| {
                if request.body().len() <= 16384
                    && let Ok(command) = serde_json::from_str::<Command>(request.body())
                {
                    let _ = proxy.send_event(Event::Command(command));
                }
            }
        })
        .build_as_child(window)?;
    shortcuts(&view.webview(), proxy);
    Ok(view)
}

/// The Windows virtual-key code `ui::shortcut` reads for an X keysym (a GDK
/// keyval), or 0 for keys no shortcut uses. Letters match in either case; `+`
/// and `_` are the shifted `=` and `-`, as Windows reports them.
fn virtual_key(keyval: u32) -> u32 {
    match keyval {
        0x61..=0x7a => keyval - 0x20,
        0x30..=0x39 | 0x41..=0x5a => keyval,
        0x3d | 0x2b => 0xbb,
        0x2d | 0x5f => 0xbd,
        0xffab => 0x6b,
        0xffad => 0x6d,
        0xffb0 => 0x60,
        0xff09 | 0xfe20 => 0x09,
        0xff1b => 0x1b,
        0xff50 => 0x24,
        0xff51 => 0x25,
        0xff53 => 0x27,
        0xff55 => 0x21,
        0xff56 => 0x22,
        0xffc1..=0xffc4 => keyval - 0xffc1 + 0x73,
        _ => 0,
    }
}

/// Whether a press of `keycode` repeats the key already held: GDK 3 has no repeat
/// flag, so the held key is remembered until any key is released.
fn repeated(held: &Cell<Option<u16>>, keycode: u16) -> bool {
    held.replace(Some(keycode)) == Some(keycode)
}

/// The browser's shortcuts on a view, as Windows registers them on every
/// WebView2 controller: they run before the page sees the key. Escape also
/// reaches the page (fullscreen, selects and dialogs), and Ctrl+wheel zooms.
fn shortcuts(widget: &webkit2gtk::WebView, proxy: EventLoopProxy<Event>) {
    use gtk::{gdk::ModifierType, glib::Propagation, prelude::WidgetExt};
    let held = Rc::new(Cell::new(None));
    let released = Rc::clone(&held);
    let keys = proxy.clone();
    widget.connect_key_press_event(move |_, key| {
        let state = key.state();
        let code = virtual_key(*key.keyval());
        let Some(command) = ui::shortcut(
            code,
            state.contains(ModifierType::CONTROL_MASK),
            state.contains(ModifierType::MOD1_MASK),
            state.contains(ModifierType::SHIFT_MASK),
        ) else {
            return Propagation::Proceed;
        };
        if !repeated(&held, key.hardware_keycode()) || ui::repeatable(&command) {
            let _ = keys.send_event(Event::Command(command));
        }
        if code == 0x1b {
            Propagation::Proceed
        } else {
            Propagation::Stop
        }
    });
    widget.connect_key_release_event(move |_, _| {
        released.set(None);
        Propagation::Proceed
    });
    widget.connect_scroll_event(move |_, scroll| {
        if !scroll.state().contains(ModifierType::CONTROL_MASK) {
            return Propagation::Proceed;
        }
        let up = match scroll.direction() {
            gtk::gdk::ScrollDirection::Up => Some(true),
            gtk::gdk::ScrollDirection::Down => Some(false),
            gtk::gdk::ScrollDirection::Smooth => {
                let dy = scroll.delta().1;
                (dy != 0.0).then_some(dy < 0.0)
            }
            _ => None,
        };
        if let Some(up) = up {
            let command = if up {
                Command::ZoomIn
            } else {
                Command::ZoomOut
            };
            let _ = proxy.send_event(Event::Command(command));
        }
        Propagation::Stop
    });
}

/// The trusted views' private address. WebKitGTK cannot use a `data:` URL as on
/// Windows: wry drops every IPC message from a page whose URL is longer than an
/// HTTP URI may be (64 KiB), and the encoded toolbar document is longer.
const TOOLBAR_SCHEME: &str = "rust-browser";
const TOOLBAR_URL: &str = "rust-browser://toolbar/";

/// What the trusted scheme serves: the toolbar document at `/`, nothing else.
fn toolbar_document(path: &str) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    let (status, body): (u16, &'static [u8]) = if path == "/" {
        (200, ui::HTML.as_bytes())
    } else {
        (404, b"")
    };
    wry::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Cache-Control", "no-store")
        .body(body.into())
        .unwrap_or_else(|_| wry::http::Response::new((&[] as &[u8]).into()))
}

/// A website view: HTTP(S) and `about:blank` only, no command bridge, every
/// permission refused, new windows opened as tabs. Its downloads go to
/// `linux_downloads`, hooked on the shared WebKit context, not through wry,
/// whose handler cannot wait for a folder or report progress.
fn content_view(
    window: &Window,
    context: &mut WebContext,
    proxy: EventLoopProxy<Event>,
    id: TabId,
    private: bool,
) -> Result<WebView, Box<dyn Error>> {
    let (title, load, popup, blocked) =
        (proxy.clone(), proxy.clone(), proxy.clone(), proxy.clone());
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
    shortcuts(&engine, proxy.clone());
    // Every permission request (camera, microphone, location, notifications) is
    // refused until Linux has the prompt Windows has.
    engine.connect_permission_request(|_, request| {
        request.deny();
        true
    });
    if let Some(finder) = engine.find_controller() {
        let (counted, failed) = (proxy.clone(), proxy.clone());
        finder.connect_counted_matches(move |_, count| {
            let _ = counted.send_event(Event::Page(id, PageEvent::FindCount(count)));
        });
        finder.connect_failed_to_find_text(move |_| {
            let _ = failed.send_event(Event::Page(id, PageEvent::FindCount(0)));
        });
    }
    let pressed = proxy.clone();
    engine.connect_button_press_event(move |_, _| {
        let _ = pressed.send_event(Event::Page(id, PageEvent::Pressed));
        gtk::glib::Propagation::Proceed
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
            Event::Paste(text) => {
                let text = navigation::paste_text(&text);
                serde_json::to_string(&text)
                    .map_err(Into::into)
                    .and_then(|json| {
                        browser
                            .toolbar
                            .evaluate_script(&format!(
                                "window.pasteAddress&&window.pasteAddress({json})"
                            ))
                            .map_err(Into::into)
                    })
            }
            Event::StorageError => {
                browser.notice =
                    "Error: Session could not be saved. Check profile access and disk space."
                        .into();
                Ok(())
            }
            Event::DownloadRequested(id, suggested) => {
                browser.download_requested(id, &suggested);
                Ok(())
            }
            Event::DownloadDestination(id, folder) => {
                browser.download_destination(id, folder);
                Ok(())
            }
            Event::DownloadStopped(id, cancelled) => {
                browser.downloads.borrow_mut().stopped(id, cancelled);
                Ok(())
            }
            Event::DownloadDone(id) => {
                browser.downloads.borrow_mut().done(id, &browser.proxy);
                Ok(())
            }
            Event::DownloadFinished(id, result) => {
                browser.downloads.borrow_mut().finished(id, result);
                Ok(())
            }
            Event::DownloadRejected => {
                browser.notice = "A download from this address is already active, or the download limit was reached.".into();
                Ok(())
            }
            Event::DownloadRecovery(notice) => {
                browser.notice = notice;
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
                browser.downloads.borrow_mut().shutdown();
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
        if let Some(browser) = &mut self.browser {
            browser.poll_downloads();
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
    fn the_trusted_scheme_serves_only_the_toolbar_document() {
        let page = toolbar_document("/");
        assert_eq!(page.status(), 200);
        assert_eq!(page.body().as_ref(), ui::HTML.as_bytes());
        assert_eq!(toolbar_document("/other").status(), 404);
        assert_eq!(toolbar_document("/../etc/passwd").status(), 404);
        // Short enough for wry to deliver the toolbar's IPC messages.
        let longest = format!("{TOOLBAR_URL}{}", Role::Library.fragment());
        assert!(longest.parse::<wry::http::Uri>().is_ok());
    }

    #[test]
    fn a_rounded_dropdown_is_clipped_to_its_shape() {
        let square = ClipArea {
            x: 0.0,
            y: 0.0,
            width: 1200.0,
            height: 119.0,
            radius: 0.0,
        };
        assert_eq!(clip_rects(&[square]), [(0, 0, 1200, 119)]);
        let menu = ClipArea {
            x: 900.5,
            y: 110.0,
            width: 280.0,
            height: 300.0,
            radius: 8.0,
        };
        let rects = clip_rects(&[menu]);
        let area: i32 = rects.iter().map(|r| r.2 * r.3).sum();
        // Inside the box it covers (900 to 1181, 110 to 410), a little less than
        // the whole box because of the corners, and never outside it.
        assert!(area < 281 * 300 && area > 281 * 300 - 4 * 64, "{area}");
        for (x, y, w, h) in &rects {
            assert!(*x >= 900 && x + w <= 1181 && *y >= 110 && y + h <= 410);
        }
        // The top row is inset at the corners; the middle spans the full width.
        assert!(rects.iter().any(|r| r.1 == 110 && r.0 > 900));
        assert!(rects.iter().any(|r| r.0 == 900 && r.2 == 281 && r.3 > 200));
        // Empty areas add nothing.
        let empty = ClipArea { width: 0.0, ..menu };
        assert!(clip_rects(&[empty]).is_empty());
    }

    #[test]
    fn keys_map_to_the_codes_the_shortcut_table_reads() {
        // Ctrl+T, Ctrl+Shift+T (an upper-case keyval), Ctrl+5 and Ctrl+9.
        assert!(matches!(
            ui::shortcut(virtual_key(0x74), true, false, false),
            Some(Command::NewTab)
        ));
        assert!(matches!(
            ui::shortcut(virtual_key(0x54), true, false, true),
            Some(Command::ReopenClosedTab)
        ));
        assert!(matches!(
            ui::shortcut(virtual_key(0x35), true, false, false),
            Some(Command::SelectIndex(4))
        ));
        assert!(matches!(
            ui::shortcut(virtual_key(0x39), true, false, false),
            Some(Command::SelectIndex(usize::MAX))
        ));
        // Ctrl+= and Ctrl++ (Shift held), Ctrl+- and the keypad keys zoom.
        for keyval in [0x3d, 0x2b, 0xffab] {
            assert!(matches!(
                ui::shortcut(virtual_key(keyval), true, false, keyval == 0x2b),
                Some(Command::ZoomIn)
            ));
        }
        for keyval in [0x2d, 0xffad] {
            assert!(matches!(
                ui::shortcut(virtual_key(keyval), true, false, false),
                Some(Command::ZoomOut)
            ));
        }
        // Alt+Left and Alt+Right, F5, F6, Escape, Ctrl+Shift+Tab (ISO_Left_Tab).
        assert!(matches!(
            ui::shortcut(virtual_key(0xff51), false, true, false),
            Some(Command::Back)
        ));
        assert!(matches!(
            ui::shortcut(virtual_key(0xff53), false, true, false),
            Some(Command::Forward)
        ));
        assert!(matches!(
            ui::shortcut(virtual_key(0xffc2), false, false, false),
            Some(Command::Reload)
        ));
        assert!(matches!(
            ui::shortcut(virtual_key(0xffc3), false, false, false),
            Some(Command::FocusAddress)
        ));
        assert!(matches!(
            ui::shortcut(virtual_key(0xff1b), false, false, false),
            Some(Command::Stop)
        ));
        assert!(matches!(
            ui::shortcut(virtual_key(0xfe20), true, false, true),
            Some(Command::CycleTab(true))
        ));
        // Ctrl+Shift+1 arrives as `!`: no shortcut, and typing is left alone.
        assert_eq!(virtual_key(0x21), 0);
        assert!(ui::shortcut(virtual_key(0x61), false, false, false).is_none());
    }

    #[test]
    fn a_held_key_repeats_until_a_key_is_released() {
        let held = Cell::new(None);
        assert!(!repeated(&held, 28));
        assert!(repeated(&held, 28));
        assert!(!repeated(&held, 29));
        held.set(None);
        assert!(!repeated(&held, 29));
    }

    #[test]
    fn zoom_moves_between_steps_and_stops_at_the_ends() {
        assert_eq!(next_zoom(1.0, true), 1.1);
        assert_eq!(next_zoom(1.0, false), 0.9);
        assert_eq!(next_zoom(2.0, true), 2.0);
        assert_eq!(next_zoom(0.25, false), 0.25);
    }
}
