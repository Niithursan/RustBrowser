//! The Linux front end: browser windows drawn by winit on X11 (XWayland in a
//! Wayland session), with WebKitGTK views from wry. Normal windows of a profile
//! share one process, as on Windows; a private window runs in its own process.
//!
//! It reuses everything that is not tied to WebView2: the trusted toolbar page and
//! its command protocol (`ui`), tabs, navigation, sessions, settings and the
//! bookmarks and history library (`library_worker`), the download staging rules
//! (`linux_downloads`) and the permission rules (`linux_permissions`). What WebView2
//! alone provides on Windows (extensions and the shield, profiles, password
//! autofill) is not offered here yet: such commands answer with a notice
//! instead. Clearing browsing data uses WebKit's own data manager. WebKitGTK
//! cannot freeze a page as WebView2 does, so a sleeping tab's view is closed
//! and its back/forward list kept; selecting it loads the page again.
//!
//! A later launch of a normal window hands its links (or a request for a window)
//! to the browser already holding the profile through `linux_handoff`, and exits.
//!
//! Security model, as on Windows: website views only navigate to HTTP(S) and
//! `about:blank`, have no command bridge, get a permission only when the user allows
//! it (or a remembered rule does), and download only into a folder the user picks
//! each time; the toolbar is a separate WebKit
//! context that may only show its own local document and never downloads; a link a
//! website opens in a new window becomes a tab.
use crate::{
    library_worker::{self, Worker},
    linux_downloads, linux_handoff, linux_permissions,
    ui::{self, Command, Panel, ResizeEdge, State, TabState, ToolbarState},
};
use gtk::prelude::WidgetExt as _;
use rust_browser::{
    background::{Lifecycle, Phase, Protection},
    handoff::{Request, Status},
    layout::{
        ClipArea, FULLSCREEN_NOTICE, Full, LIBRARY_MAX, LIBRARY_MIN, LIBRARY_WIDTH, Layout,
        fullscreen_host, fullscreen_notice_area, layout,
    },
    library::{Action, Data, Filter},
    navigation,
    privacy::ClearRequest,
    security::{self, Capability, Decision, Rule},
    settings::Settings,
    storage::{self, MAX_WINDOWS, Snapshot, Store, WindowSnapshot},
    tabs::{ClosedTabs, MAX_TABS, TabId, Tabs},
};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};
use webkit2gtk::{BackForwardListExt, FindControllerExt, MediaCaptureState, WebViewExt};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Fullscreen, ResizeDirection, Window, WindowId},
};
use wry::{
    PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix,
};

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

/// Where a command came from: a window's trusted views, or a key pressed in a
/// tab's website view.
#[derive(Clone, Copy, Debug)]
pub enum Source {
    Window(WindowId),
    Tab(TabId),
}

/// What reaches the event loop from views, GTK handlers and worker threads.
#[derive(Debug)]
pub enum Event {
    /// From a trusted view, or a shortcut in a website view.
    Command(Source, Command),
    Page(TabId, PageEvent),
    /// A website in tab `.0` asked for a new window: the address opens as a tab.
    OpenTab(TabId, String),
    Library(library_worker::Reply),
    /// The clipboard's text arrived for that window's address field.
    Paste(WindowId, String),
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
    /// A website's permission request waits for the user.
    PermissionPrompt,
    /// WebKit finished clearing website data, or could not.
    EngineCleared(bool),
    /// A later launch's request, with where to send the answer.
    HandOff(Request, std::sync::mpsc::SyncSender<Status>),
    /// The bookmarks file chooser closed: the chosen file, or none.
    BookmarkFile(Option<PathBuf>),
    /// The chosen bookmarks file, read and parsed, or why it was refused.
    BookmarksRead(Result<rust_browser::bookmark_import::Parsed, String>),
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
    /// The website view; `None` while the tab sleeps.
    view: Option<WebView>,
    /// A sleeping tab's back/forward list as WebKit saved it, restored when the
    /// tab wakes.
    saved: Option<webkit2gtk::WebViewSessionState>,
    state: State,
    zoom: f64,
    lifecycle: Lifecycle,
}

impl Page {
    /// The view of an awake tab. The selected tab is always awake.
    fn awake(&self) -> Result<&WebView, &'static str> {
        self.view
            .as_ref()
            .ok_or("This tab is asleep. Select it, or use Wake tab, to wake it.")
    }

    fn hide(&self) {
        if let Some(view) = &self.view {
            let _ = view.set_visible(false);
        }
    }
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
    let mut links_only = false;
    for arg in args {
        match arg.as_str() {
            "--" if !links_only => links_only = true,
            "--private" if !links_only => launch.private = true,
            "--version" if !links_only => launch.version = true,
            other if !links_only && other.starts_with("--") => {
                return Err(format!("Unknown option {other}."));
            }
            link => launch
                .links
                .push(navigation::external(link).map_err(|e| e.to_string())?),
        }
        if launch.links.len() > rust_browser::launch::MAX_LINKS {
            return Err(format!(
                "At most {} links can be opened at once.",
                rust_browser::launch::MAX_LINKS
            ));
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

/// One browser window: its views and what it shows. Fields drop in order, so the
/// views go before the window that holds them.
struct Win {
    tabs: Tabs<Page>,
    /// The vertical tab column and the library column, built when first needed
    /// and hidden, not destroyed, when not shown; each with its last render.
    tabs_view: Option<(WebView, RefCell<String>)>,
    library_view: Option<(WebView, RefCell<String>)>,
    toolbar: WebView,
    /// Tabs closed in this window, for "Reopen closed tab".
    closed: ClosedTabs,
    panel: Option<Panel>,
    /// What the library column shows, while it is open.
    library_panel: Option<Panel>,
    library_width: f64,
    menu_open: bool,
    /// Where the open dropdown is drawn in the toolbar view.
    menu_area: Option<ClipArea>,
    fullscreen: bool,
    /// The tab and text of the find bar's last search.
    find: Option<(TabId, String)>,
    /// Until when the page that entered fullscreen is named, and its host.
    fullscreen_notice: Option<(Instant, String)>,
    chrome: f64,
    notice: String,
    /// The notice, while it is information rather than a problem.
    info: String,
    last_render: RefCell<String>,
    window: Window,
}

/// The browser: its windows and what they share (one profile's store, library,
/// settings, WebKit contexts, downloads and permissions). It dereferences to the
/// window an event is being handled in (`current`), as on Windows.
struct Browser {
    /// A private window's hidden anchor view: every website view is related to
    /// it, so they share its one in-memory WebKit context (sign-ins, storage,
    /// downloads) and nothing is written to disk.
    private_anchor: Option<WebView>,
    // Windows and their views drop before the contexts below.
    windows: Vec<Win>,
    current: usize,
    /// One allocator for every window, so a tab ID names one tab in one window.
    tab_ids: Rc<Cell<TabId>>,
    /// Window IDs, most recently focused first.
    focus_order: Vec<WindowId>,
    /// Tabs (address, pinned) of windows closed this run, newest first, for
    /// "Reopen closed window". In memory only, like closed tabs.
    closed_windows: Vec<Vec<(String, bool)>>,
    /// Windows the last save left out for its size, already announced.
    unsaved_windows: Vec<WindowId>,
    toolbar_context: WebContext,
    shaper: Option<Shaper>,
    content: WebContext,
    /// Downloads from website views; hooked to their WebKit context with the
    /// first view.
    downloads: linux_downloads::Shared,
    /// Website permission requests and remembered answers.
    permissions: linux_permissions::Shared,
    downloads_hooked: bool,
    /// When download progress was last read.
    downloads_polled: Instant,
    proxy: EventLoopProxy<Event>,
    settings: Settings,
    store: Option<Store>,
    library: Worker,
    data: Data,
    filter: Filter,
    /// The bookmarks-bar folder an open dropdown lists.
    menu_folder: Option<i64>,
    revision: u64,
    private: bool,
    run_id: u128,
    loads: u64,
    profile: PathBuf,
    /// WebKit's context of the website views, once the first one exists.
    content_context: Option<webkit2gtk::WebContext>,
    /// A clear of browsing data in progress.
    clearing: Option<Clearing>,
    /// Listens for later launches of this profile (normal windows only).
    handoff: Option<linux_handoff::Server>,
    /// When the browser started: the clock of the tabs' sleep timers.
    started: Instant,
    /// When background tabs were last checked.
    background_at: Instant,
    /// The file chooser of Bookmarks > Import, while it is open.
    import_chooser: Option<gtk::FileChooserNative>,
}

impl std::ops::Deref for Browser {
    type Target = Win;
    fn deref(&self) -> &Win {
        &self.windows[self.current]
    }
}

impl std::ops::DerefMut for Browser {
    fn deref_mut(&mut self) -> &mut Win {
        &mut self.windows[self.current]
    }
}

/// The order windows are saved in: `main` first, then the others most recently
/// focused first, then any never focused.
fn save_order<T: PartialEq>(windows: &[T], focus: &[T], main: usize) -> Vec<usize> {
    let mut order = vec![main];
    for index in focus
        .iter()
        .filter_map(|id| windows.iter().position(|w| w == id))
        .chain(0..windows.len())
    {
        if !order.contains(&index) {
            order.push(index);
        }
    }
    order
}

/// A window's tabs as saved: the new-tab page and valid addresses only.
fn window_snapshot(tabs: &Tabs<Page>) -> WindowSnapshot {
    WindowSnapshot {
        tabs: tabs
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
        active: tabs.active_index(),
        pinned: tabs.items.iter().map(|tab| tab.pinned).collect(),
        titles: tabs
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
        keep_awake: tabs.items.iter().map(|tab| tab.keep_awake).collect(),
    }
}

/// How a window is named in a notice: its selected tab's title or address.
fn window_label(tabs: &Tabs<Page>) -> String {
    let name = tabs
        .active()
        .map(|tab| {
            if tab.title.trim().is_empty() {
                tab.url.as_str()
            } else {
                tab.title.as_str()
            }
        })
        .unwrap_or("New tab");
    let mut label: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    if name.chars().count() > 40 {
        label.push('\u{2026}');
    }
    label
}

/// The arguments that open a private window, showing `link` if given.
fn private_args(link: Option<&str>) -> Vec<String> {
    let mut args = vec!["--private".to_string()];
    if let Some(link) = link {
        // After `--`, so an address can never be read as an option.
        args.extend(["--".to_string(), link.to_string()]);
    }
    args
}

/// Start a private window: another process of this program, with nothing saved.
fn open_private(link: Option<&str>) -> std::io::Result<()> {
    std::process::Command::new(std::env::current_exe()?)
        .args(private_args(link))
        .spawn()
        .map(|_| ())
}

/// A clear of browsing data: its library half (history, permissions) and its
/// WebKit half (cookies, site storage, caches) each report back.
struct Clearing {
    /// The library job whose reply ends the library half.
    revision: u64,
    library_done: bool,
    engine_done: bool,
    failed: bool,
    results: Vec<String>,
}

/// The WebKit data a clear removes. HSTS is never cleared: forgetting which sites
/// must use HTTPS would weaken later visits.
fn engine_kinds(request: &ClearRequest) -> webkit2gtk::WebsiteDataTypes {
    use webkit2gtk::WebsiteDataTypes as Kind;
    let mut kinds = Kind::empty();
    if request.cookies {
        kinds |= Kind::COOKIES
            | Kind::SESSION_STORAGE
            | Kind::LOCAL_STORAGE
            | Kind::INDEXEDDB_DATABASES
            | Kind::WEBSQL_DATABASES
            | Kind::OFFLINE_APPLICATION_CACHE
            | Kind::SERVICE_WORKER_REGISTRATIONS
            | Kind::DOM_CACHE
            | Kind::DEVICE_ID_HASH_SALT
            | Kind::ITP;
    }
    if request.cache {
        kinds |= Kind::MEMORY_CACHE | Kind::DISK_CACHE | Kind::DOM_CACHE;
    }
    kinds
}

impl Browser {
    fn open(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
        profile: &Path,
        lock: Option<std::fs::File>,
        links: Vec<String>,
    ) -> Result<Self, Box<dyn Error>> {
        // Only a normal window holds the profile's lock.
        let private = lock.is_none();
        let (store, snapshot) = match lock {
            None => (None, Snapshot::default()),
            Some(lock) => {
                let errors = proxy.clone();
                let (store, snapshot, _) = Store::open_locked(profile, lock, move || {
                    let _ = errors.send_event(Event::StorageError);
                })?;
                (Some(store), snapshot)
            }
        };
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
            private_anchor: None,
            windows: Vec::new(),
            current: 0,
            tab_ids: Rc::new(Cell::new(1)),
            focus_order: Vec::new(),
            closed_windows: Vec::new(),
            unsaved_windows: Vec::new(),
            toolbar_context: WebContext::new((!private).then(|| profile.join("toolbar"))),
            shaper: Shaper::open(),
            content: WebContext::new((!private).then(|| profile.join("content"))),
            downloads: Rc::default(),
            permissions: Rc::default(),
            downloads_hooked: false,
            downloads_polled: Instant::now(),
            proxy,
            settings: snapshot.settings.clone(),
            store,
            library,
            data: Data::default(),
            filter: Filter::default(),
            menu_folder: None,
            revision: 0,
            private,
            run_id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
            loads: 0,
            profile: profile.to_path_buf(),
            content_context: None,
            clearing: None,
            handoff: None,
            started: Instant::now(),
            background_at: Instant::now(),
            import_chooser: None,
        };
        let restore = !private && browser.settings.restore_session;
        let main = restore.then(|| WindowSnapshot {
            tabs: snapshot.tabs.clone(),
            active: snapshot.active,
            pinned: snapshot.pinned.clone(),
            titles: snapshot.titles.clone(),
            keep_awake: snapshot.keep_awake.clone(),
        });
        // The other saved windows open first, behind; the main one, last focused,
        // opens last so it is in front and gets the keyboard.
        let mut failed = None;
        if restore {
            for other in snapshot.other_windows.iter().take(MAX_WINDOWS - 1) {
                if let Err(error) =
                    browser.open_window(event_loop, Some(other.clone()), Vec::new(), false)
                {
                    failed.get_or_insert(format!("A saved window could not be reopened: {error}"));
                }
            }
        }
        browser.open_window(event_loop, main, links, true)?;
        if let Some(notice) = failed {
            browser.notice = notice;
        }
        if !private {
            browser.start_handoff();
        }
        browser.claim_permissions();
        browser.layout_all();
        browser.refresh_library();
        Ok(browser)
    }

    /// Listen for later launches of this profile. If that is not possible, keep
    /// browsing and say that a later launch cannot reach this browser.
    fn start_handoff(&mut self) {
        let proxy = self.proxy.clone();
        let dispatch: linux_handoff::Dispatch = std::sync::Arc::new(move |request| {
            let (reply, answer) = std::sync::mpsc::sync_channel(1);
            if proxy.send_event(Event::HandOff(request, reply)).is_err() {
                return Status::Closing;
            }
            answer
                .recv_timeout(linux_handoff::UI_TIMEOUT)
                .unwrap_or(Status::NotResponding)
        });
        match linux_handoff::endpoint(&self.profile)
            .ok_or_else(|| std::io::Error::other("no private folder"))
            .and_then(|path| linux_handoff::Server::start(&path, dispatch))
        {
            Ok(server) => self.handoff = Some(server),
            Err(_) => {
                self.notice = "Links from other apps cannot reach this browser while it runs, because its link channel could not be created. Restart Rust Browser to try again.".into();
            }
        }
    }

    /// Open what a later launch handed over in the focused window (or a new
    /// window), then bring it forward.
    fn hand_off(&mut self, el: &ActiveEventLoop, request: Request) -> Status {
        if self.clearing.is_some() {
            return Status::Busy;
        }
        self.current = self.focused_index();
        match request {
            Request::Open(links) => {
                let mut opened = 0;
                for link in links {
                    // Checked again with the command-line policy.
                    match navigation::external(&link)
                        .map_err(|e| e.to_string())
                        .and_then(|url| self.open_tab(url).map_err(|e| e.to_string()))
                    {
                        Ok(()) => opened += 1,
                        Err(error) => self.notice = error,
                    }
                }
                if opened == 0 {
                    return Status::Refused;
                }
                self.panel = None;
            }
            Request::NewWindow => {
                if let Err(error) = self.open_window(el, None, Vec::new(), true) {
                    self.notice = error.to_string();
                    return Status::Refused;
                }
            }
        }
        self.persist();
        self.window.set_minimized(false);
        self.window.focus_window();
        Status::Accepted
    }

    /// Open a window with `restore`'s tabs, then `links`, or else the new-tab
    /// page, and make it the current one. A `foreground` window is focused; one
    /// restored behind the first goes last in the focus order.
    fn open_window(
        &mut self,
        el: &ActiveEventLoop,
        restore: Option<WindowSnapshot>,
        links: Vec<String>,
        foreground: bool,
    ) -> Result<(), Box<dyn Error>> {
        if self.private && !self.windows.is_empty() {
            return Err("New windows open from a normal window. Use Ctrl+Shift+N for another private window.".into());
        }
        if self.windows.len() >= MAX_WINDOWS {
            return Err(format!(
                "At most {MAX_WINDOWS} windows can be open. Close a window to open another."
            )
            .into());
        }
        let window = el.create_window(
            Window::default_attributes()
                .with_title("Rust Browser")
                .with_decorations(false)
                .with_inner_size(LogicalSize::new(1200.0, 820.0))
                .with_min_inner_size(LogicalSize::new(480.0, 360.0)),
        )?;
        let toolbar = toolbar_view(
            &window,
            &mut self.toolbar_context,
            self.proxy.clone(),
            self.private,
            Role::Toolbar,
        )?;
        let id = window.id();
        self.windows.push(Win {
            tabs: Tabs::with_ids(self.tab_ids.clone()),
            tabs_view: None,
            library_view: None,
            toolbar,
            closed: ClosedTabs::default(),
            panel: None,
            library_panel: None,
            library_width: LIBRARY_WIDTH,
            menu_open: false,
            menu_area: None,
            fullscreen: false,
            find: None,
            fullscreen_notice: None,
            chrome: DEFAULT_CHROME,
            notice: String::new(),
            info: String::new(),
            last_render: RefCell::new(String::new()),
            window,
        });
        self.current = self.windows.len() - 1;
        if foreground {
            self.focus_order.insert(0, id);
        } else {
            self.focus_order.push(id);
        }
        if let Err(error) = self.fill_window(restore, links) {
            self.remove_window(self.current, false);
            self.current = self.focused_index();
            return Err(error);
        }
        if foreground {
            self.window.focus_window();
        }
        Ok(())
    }

    /// Open the current (new) window's tabs and its tab column.
    fn fill_window(
        &mut self,
        restore: Option<WindowSnapshot>,
        links: Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(restore) = restore {
            for (index, url) in restore.tabs.iter().enumerate().take(MAX_TABS) {
                if self.open_tab(url.clone()).is_ok()
                    && let Some(tab) = self.tabs.active_mut()
                {
                    tab.pinned = restore.pinned.get(index).copied().unwrap_or(false);
                    tab.keep_awake = restore.keep_awake.get(index).copied().unwrap_or(false);
                    if let Some(title) = restore.titles.get(index) {
                        tab.title.clone_from(title);
                    }
                }
            }
            if let Some(id) = self.tabs.items.get(restore.active).map(|t| t.id) {
                self.tabs.select(id);
            }
        }
        for link in links {
            let _ = self.open_tab(link);
        }
        if self.tabs.items.is_empty() {
            self.open_tab(navigation::HOME.into())?;
        }
        if self.settings.vertical_tabs {
            self.ensure_side(Role::Tabs)?;
        }
        Ok(())
    }

    /// Take window `index` away: hide it, forget its tabs' waiting requests,
    /// and keep its tabs for "Reopen closed window" when `remember` is set.
    fn remove_window(&mut self, index: usize, remember: bool) {
        let win = self.windows.remove(index);
        if remember {
            let tabs: Vec<(String, bool)> = win
                .tabs
                .items
                .iter()
                .filter(|t| t.url == navigation::HOME || navigation::allowed(&t.url))
                .map(|t| (t.url.clone(), t.pinned))
                .collect();
            self.closed_windows.insert(0, tabs);
            self.closed_windows.truncate(MAX_WINDOWS);
        }
        {
            let mut permissions = self.permissions.borrow_mut();
            for tab in &win.tabs.items {
                permissions.cancel_tab(tab.id);
                tab.content.hide();
            }
        }
        let _ = win.toolbar.set_visible(false);
        win.window.set_visible(false);
        let id = win.window.id();
        self.focus_order.retain(|w| *w != id);
        self.unsaved_windows.retain(|w| *w != id);
        self.current = self.current.min(self.windows.len().saturating_sub(1));
    }

    /// Close the window being handled; closing the last one ends the browser.
    fn close_window(&mut self, el: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        if self.clearing.is_some() {
            return Err("Wait for the clearing result before closing the browser.".into());
        }
        if self.windows.len() == 1 {
            self.shutdown(el);
            return Ok(());
        }
        let tabs = self
            .tabs
            .items
            .iter()
            .filter(|t| t.url != navigation::HOME)
            .count();
        self.remove_window(self.current, true);
        self.current = self.focused_index();
        if tabs > 0 {
            self.inform(format!(
                "A window with {tabs} tab{} was closed. Menu (\u{22ee}) > Reopen closed window brings it back until Rust Browser exits.",
                if tabs == 1 { "" } else { "s" }
            ));
        }
        self.persist();
        self.window.focus_window();
        Ok(())
    }

    /// Reopen the most recently closed window of this run.
    fn reopen_window(&mut self, el: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let tabs = self
            .closed_windows
            .first()
            .cloned()
            .ok_or("No window was closed in this session.")?;
        let restore = WindowSnapshot {
            pinned: tabs.iter().map(|(_, pinned)| *pinned).collect(),
            tabs: tabs.into_iter().map(|(url, _)| url).collect(),
            ..WindowSnapshot::default()
        };
        self.open_window(el, Some(restore), Vec::new(), true)?;
        self.closed_windows.remove(0);
        self.persist();
        Ok(())
    }

    /// Save every window and end the browser.
    fn shutdown(&mut self, el: &ActiveEventLoop) {
        // Later launches then wait for the profile instead of being answered.
        self.handoff = None;
        self.persist();
        self.downloads.borrow_mut().shutdown();
        el.exit();
    }

    /// The most recently focused window that is still open.
    fn focused_index(&self) -> usize {
        self.focus_order
            .iter()
            .find_map(|id| self.windows.iter().position(|w| w.window.id() == *id))
            .unwrap_or(0)
    }

    /// Window `id` was focused.
    fn focused(&mut self, id: WindowId) {
        self.focus_order.retain(|w| *w != id);
        self.focus_order.insert(0, id);
    }

    /// Handle what follows in window `id`; false when it has closed.
    fn select_window(&mut self, id: WindowId) -> bool {
        match self.windows.iter().position(|w| w.window.id() == id) {
            Some(index) => {
                self.current = index;
                true
            }
            None => false,
        }
    }

    /// Handle what follows in the window holding tab `id`; false when it closed.
    fn select_tab_window(&mut self, id: TabId) -> bool {
        match self
            .windows
            .iter()
            .position(|w| w.tabs.items.iter().any(|t| t.id == id))
        {
            Some(index) => {
                self.current = index;
                true
            }
            None => false,
        }
    }

    /// Run `each` in every window in turn, then return to the current one.
    fn each_window(&mut self, mut each: impl FnMut(&mut Self)) {
        let current = self.current;
        for index in 0..self.windows.len() {
            self.current = index;
            each(self);
        }
        self.current = current.min(self.windows.len().saturating_sub(1));
    }

    /// Bookmarks > Import: ask for an exported bookmarks file.
    fn choose_bookmarks_file(&mut self) -> Result<(), Box<dyn Error>> {
        use gtk::prelude::{FileChooserExt, NativeDialogExt};
        if self.private {
            return Err(
                "Import bookmarks from a normal window; private windows do not write bookmarks."
                    .into(),
            );
        }
        if self.import_chooser.is_some() {
            return Err("Choose the bookmarks file in the window that is already open.".into());
        }
        let chooser = gtk::FileChooserNative::new(
            Some("Import bookmarks"),
            None::<&gtk::Window>,
            gtk::FileChooserAction::Open,
            Some("Import"),
            Some("Cancel"),
        );
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Exported bookmarks (*.html, *.htm)"));
        for pattern in ["*.html", "*.htm", "*.HTML", "*.HTM"] {
            filter.add_pattern(pattern);
        }
        chooser.add_filter(filter);
        let proxy = self.proxy.clone();
        chooser.connect_response(move |chooser, response| {
            let file = (response == gtk::ResponseType::Accept)
                .then(|| chooser.filename())
                .flatten();
            let _ = proxy.send_event(Event::BookmarkFile(file));
        });
        chooser.show();
        self.import_chooser = Some(chooser);
        Ok(())
    }

    /// The bookmarks file chooser closed. A chosen file is read and parsed off
    /// the UI thread; the library worker then imports it.
    fn bookmark_file(&mut self, file: Option<PathBuf>) {
        use gtk::prelude::NativeDialogExt;
        if let Some(chooser) = self.import_chooser.take() {
            chooser.destroy();
        }
        let Some(file) = file else {
            return;
        };
        let proxy = self.proxy.clone();
        let started = std::thread::Builder::new()
            .name("bookmark-file".into())
            .spawn(move || {
                let result = rust_browser::bookmark_import::read_file(&file).and_then(|bytes| {
                    rust_browser::bookmark_import::parse(&bytes).map_err(|e| e.message())
                });
                let _ = proxy.send_event(Event::BookmarksRead(result));
            });
        if started.is_err() {
            self.notice = "The bookmarks file could not be read.".into();
        }
    }

    /// Wake each window's selected tab if it sleeps. A tab whose waking failed
    /// waits for Wake tab rather than being retried after every event.
    fn wake_selected(&mut self) {
        self.each_window(|browser| {
            if let Err(error) = browser.wake_active(false) {
                browser.notice = error.to_string();
            }
        });
    }

    /// Build the selected tab's view again and restore its back/forward list, so
    /// WebKit loads its page anew where it was. With `retry`, also after a
    /// failed wake.
    fn wake_active(&mut self, retry: bool) -> Result<(), Box<dyn Error>> {
        let Some(tab) = self.tabs.active() else {
            return Ok(());
        };
        let phase = tab.content.lifecycle.phase;
        if tab.content.view.is_some() || !(phase == Phase::Sleeping || retry) {
            return Ok(());
        }
        let (id, url) = (tab.id, tab.url.clone());
        let built = content_view(
            &self.windows[self.current].window,
            &mut self.content,
            self.proxy.clone(),
            self.permissions.clone(),
            id,
            self.private_anchor.as_ref().map(|anchor| anchor.webview()),
        );
        let now = self.started.elapsed();
        let Some(tab) = self.windows[self.current].tabs.get_mut(id) else {
            return Ok(());
        };
        let view = match built {
            Ok(view) => view,
            Err(error) => {
                tab.content.lifecycle.resumed(false);
                return Err(format!(
                    "This tab could not wake ({error}). Use Wake tab to try again."
                )
                .into());
            }
        };
        if url != navigation::HOME {
            let engine = view.webview();
            let restored = tab.content.saved.take().is_some_and(|state| {
                engine.restore_session_state(&state);
                engine
                    .back_forward_list()
                    .and_then(|list| list.current_item())
                    .map(|item| engine.go_to_back_forward_list_item(&item))
                    .is_some()
            });
            if !restored && let Err(error) = view.load_url(&url) {
                tab.content.lifecycle.resumed(false);
                return Err(format!(
                    "This tab could not wake ({error}). Use Wake tab to try again."
                )
                .into());
            }
            tab.content.state.loading = true;
        }
        if (tab.content.zoom - 1.0).abs() > 0.001 {
            let _ = view.zoom(tab.content.zoom);
        }
        tab.content.saved = None;
        tab.content.view = Some(view);
        tab.content.lifecycle.resumed(true);
        tab.content.lifecycle.visible(true, now);
        Ok(())
    }

    /// Once a second, let hidden tabs sleep as Settings says. Returns whether a
    /// tab fell asleep.
    fn background_tick(&mut self) -> bool {
        if self.background_at.elapsed() < Duration::from_secs(1) {
            return false;
        }
        self.background_at = Instant::now();
        let mut changed = false;
        self.each_window(|browser| changed |= browser.background_window());
        changed
    }

    /// Sleep decisions for the tabs of the window being handled. A sleeping tab
    /// keeps its address, title and icon; its view goes, and with it the web
    /// process WebKit ran it in.
    fn background_window(&mut self) -> bool {
        let now = self.started.elapsed();
        let delay = Duration::from_secs(self.settings.suspend_after_seconds);
        let enabled = self.settings.automatic_suspension;
        let shared = Protection {
            downloads: self.downloads.borrow().active_count() > 0,
            permissions: self.permissions.borrow().prompt().is_some(),
            busy: self.clearing.is_some(),
            ..Protection::default()
        };
        let active = self.tabs.active().map(|t| t.id);
        let mut changed = false;
        for tab in &mut self.windows[self.current].tabs.items {
            tab.content.lifecycle.visible(Some(tab.id) == active, now);
            let Some(view) = &tab.content.view else {
                continue;
            };
            let engine = view.webview();
            let protection = Protection {
                active: Some(tab.id) == active,
                pinned: tab.pinned,
                keep_awake: tab.keep_awake,
                loading: tab.content.state.loading,
                capture: [
                    engine.camera_capture_state(),
                    engine.microphone_capture_state(),
                    engine.display_capture_state(),
                ]
                .iter()
                .any(|state| *state != MediaCaptureState::None),
                audio: engine.is_playing_audio(),
                ..shared
            };
            let Some(ticket) = tab
                .content
                .lifecycle
                .request(now, delay, enabled, protection)
            else {
                continue;
            };
            tab.content.saved = engine.session_state();
            tab.content.hide();
            tab.content.view = None;
            tab.content.lifecycle.complete(ticket, true, now);
            changed = true;
        }
        changed
    }

    fn layout_all(&mut self) {
        self.each_window(|browser| browser.layout());
    }

    fn render_all(&mut self) {
        self.each_window(|browser| browser.render());
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
        if self.private && self.private_anchor.is_none() {
            self.private_anchor = Some(private_anchor(&self.windows[self.current].window)?);
        }
        let view = content_view(
            &self.windows[self.current].window,
            &mut self.content,
            self.proxy.clone(),
            self.permissions.clone(),
            id,
            self.private_anchor.as_ref().map(|anchor| anchor.webview()),
        )?;
        // wry keeps the WebKit context to itself; every website view shares it.
        // Each view wry builds also adds its default handler, which saves to the
        // Downloads folder unasked; `linux_downloads` overrides it per download.
        if !self.downloads_hooked
            && let Some(context) = view.webview().context()
        {
            linux_downloads::install(&context, self.downloads.clone(), self.proxy.clone());
            self.downloads_hooked = true;
            self.content_context = Some(context);
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
                view: Some(view),
                saved: None,
                state,
                zoom: 1.0,
                lifecycle: Lifecycle::default(),
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
            let Some(view) = &tab.content.view else {
                continue;
            };
            let _ = view.set_visible(page.is_some());
            if let Some(page) = page {
                let _ = view.set_bounds(rect(page));
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
            &self.windows[self.current].window,
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

    /// Save every window: the most recently focused one in the format 1 fields,
    /// the others in `other_windows`, which is left out if it would make the file
    /// larger than every version accepts.
    fn persist(&mut self) {
        if self.store.is_none() || self.windows.is_empty() {
            return;
        }
        let main = self.focused_index();
        let first = window_snapshot(&self.windows[main].tabs);
        let ids: Vec<WindowId> = self.windows.iter().map(|w| w.window.id()).collect();
        let others: Vec<usize> = save_order(&ids, &self.focus_order, main)
            .into_iter()
            .skip(1)
            .collect();
        let mut snapshot = Snapshot {
            version: 1,
            tabs: first.tabs,
            active: first.active,
            settings: self.settings.clone(),
            pinned: first.pinned,
            titles: first.titles,
            keep_awake: first.keep_awake,
            other_windows: others
                .iter()
                .take(MAX_WINDOWS - 1)
                .map(|index| window_snapshot(&self.windows[*index].tabs))
                .collect(),
        };
        // Least recently focused windows are left out first; say once which.
        let dropped = storage::trim_to_fit(&mut snapshot);
        let kept = snapshot.other_windows.len();
        let unsaved: Vec<usize> = others.iter().copied().skip(kept).take(dropped).collect();
        let unsaved_ids: Vec<WindowId> = unsaved.iter().map(|i| ids[*i]).collect();
        if unsaved_ids
            .iter()
            .any(|id| !self.unsaved_windows.contains(id))
        {
            let labels: Vec<String> = unsaved
                .iter()
                .map(|i| window_label(&self.windows[*i].tabs))
                .collect();
            let (count, them) = if labels.len() == 1 {
                ("1 window".to_string(), "It stays")
            } else {
                (format!("{} windows", labels.len()), "They stay")
            };
            self.windows[main].notice = format!(
                "{count} will not be restored next time because the saved session would be larger than 1 MiB: {}. {them} open now; close some tabs to save every window.",
                labels.join(", ")
            );
        }
        self.unsaved_windows = unsaved_ids;
        if let (Some(store), Some(snapshot)) = (
            &self.store,
            rust_browser::privacy::persistent_snapshot(snapshot, self.private),
        ) {
            store.queue(snapshot);
        }
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
        let (prompt, prompt_seconds) = {
            let permissions = self.permissions.borrow();
            match permissions
                .prompt()
                .filter(|p| self.tabs.items.iter().any(|t| t.id == p.tab))
            {
                Some(prompt) => (Some(prompt), permissions.prompt_seconds_left()),
                None => (None, None),
            }
        };
        let state = ToolbarState {
            zoom_percent: ((zoom - 1.0).abs() > 0.001).then(|| (zoom * 100.0).round() as u32),
            page_tools: self.effective_panel().is_none() && navigation::allowed(&active.url),
            diagnostics: None,
            settings: &self.settings,
            private: self.private,
            restore_session: self.settings.restore_session,
            security,
            permission_prompt: prompt,
            permission_seconds: prompt_seconds,
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
                    keep_awake: tab.keep_awake,
                    phase: tab.content.lifecycle.phase,
                    loading: tab.content.state.loading,
                    favicon: &tab.content.state.favicon,
                })
                .collect(),
            active_id: active.id,
            search_engine: self.settings.search_engine,
            notice: &self.notice,
            notice_kind: ui::notice_kind(&self.notice, &self.info),
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
                .and_then(|t| t.content.view.as_ref()?.webview().find_controller())
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
        self.permissions.borrow_mut().cancel_tab(id);
        if let Some(tab) = self.tabs.close(id) {
            tab.content.hide();
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
            Command::CloseWindow => return self.close_window(el),
            Command::ExitBrowser => {
                if self.clearing.is_some() {
                    return Err("Wait for the clearing result before closing the browser.".into());
                }
                self.shutdown(el);
            }
            Command::NewWindow => {
                self.open_window(el, None, Vec::new(), true)?;
                self.persist();
            }
            Command::OpenSavedWindow(url) => {
                let url = rust_browser::library::bookmark_url(&url).map_err(|e| e.to_string())?;
                let restore = WindowSnapshot {
                    tabs: vec![url],
                    ..WindowSnapshot::default()
                };
                self.open_window(el, Some(restore), Vec::new(), true)?;
                self.persist();
            }
            Command::ReopenClosedWindow => return self.reopen_window(el),
            Command::NewPrivateWindow => open_private(None)?,
            Command::OpenSavedPrivate(url) => {
                let url = rust_browser::library::bookmark_url(&url).map_err(|e| e.to_string())?;
                open_private(Some(&url))?;
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
                    let mut failed = None;
                    self.each_window(|browser| {
                        if let Err(error) = browser.ensure_side(Role::Tabs) {
                            failed.get_or_insert(error.to_string());
                        }
                    });
                    if let Some(error) = failed {
                        return Err(error.into());
                    }
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
            Command::ImportBookmarks => self.choose_bookmarks_file()?,
            Command::KeepAwake => {
                if let Some(tab) = self.tabs.active_mut() {
                    tab.keep_awake = !tab.keep_awake;
                }
                self.persist();
            }
            Command::WakeTab => self.wake_active(true)?,
            Command::SetAutomaticSuspension(on) => {
                self.settings.automatic_suspension = on;
                self.persist();
            }
            Command::SetSuspendDelay(seconds) => {
                if !(15..=3600).contains(&seconds) {
                    return Err("Choose a sleep delay between 15 and 3600 seconds.".into());
                }
                self.settings.suspend_after_seconds = seconds;
                self.persist();
            }
            Command::Navigate(input) => {
                let url = navigation::resolve(&input, self.settings.search_engine)?;
                self.panel = None;
                let tab = self.tabs.active_mut().ok_or("No tab is selected.")?;
                tab.url.clone_from(&url);
                tab.content.state.url.clone_from(&url);
                tab.content.state.loading = true;
                tab.content.state.failed = false;
                tab.content.awake()?.load_url(&url)?;
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
                    if let Ok(view) = tab.content.awake() {
                        let _ = view.load_url("about:blank");
                    }
                }
                self.persist();
            }
            Command::Back | Command::Forward | Command::Reload | Command::Stop => {
                let tab = self.tabs.active().ok_or("No tab is selected.")?;
                let engine = tab.content.awake()?.webview();
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
                    .awake()?
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
                tab.content.awake()?.zoom(zoom)?;
                tab.content.zoom = zoom;
            }
            Command::PrintPage => {
                self.tabs
                    .active()
                    .ok_or("No tab is selected.")?
                    .content
                    .awake()?
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
                // As on Windows: the reopened page is shown, not a panel over it,
                // and an address that cannot open again stays reopenable.
                self.panel = None;
                if let Err(error) = self.open_tab(url.clone()) {
                    self.closed.push(url);
                    return Err(error);
                }
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
                } else if let Some(view) = self.tabs.active().and_then(|t| t.content.view.as_ref())
                {
                    self.focus(view);
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
                let window = self.window.id();
                gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).request_text(move |_, text| {
                    if let Some(text) = text {
                        let _ = proxy.send_event(Event::Paste(window, text.to_string()));
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
            Command::PermissionDecision {
                id,
                decision,
                remember,
            } => return self.permission_decision(id, decision, remember),
            Command::RevokePermission {
                origin,
                capability,
                confirmed,
            } => return self.revoke_permission(origin, capability, confirmed),
            Command::ClearData(request) => return self.begin_clear(request),
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
        let current = self.current;
        let Some(tab) = self.windows[current].tabs.get_mut(id) else {
            return;
        };
        match event {
            PageEvent::Started(url) => {
                // A new page cannot answer the old page's requests.
                self.permissions.borrow_mut().cancel_tab(id);
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
                    if let Some(view) = &tab.content.view {
                        let engine = view.webview();
                        tab.content.state.back = engine.can_go_back();
                        tab.content.state.forward = engine.can_go_forward();
                    }
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
        let tab = view.and_then(|view| {
            self.windows.iter().enumerate().find_map(|(index, w)| {
                w.tabs
                    .items
                    .iter()
                    .find(|t| t.content.view.as_ref().is_some_and(|v| v.webview() == view))
                    .map(|t| (index, t.id))
            })
        });
        if let Some((index, _)) = tab {
            self.current = index;
        }
        if let Some(tab) = tab.and_then(|(_, tab)| self.tabs.get_mut(tab)) {
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

    /// Only the selected tab may ask for a permission; selecting another tab
    /// denies what was waiting.
    fn claim_permissions(&mut self) {
        let next = self
            .windows
            .get(self.focused_index())
            .and_then(|w| w.tabs.active())
            .map(|tab| tab.id);
        let mut permissions = self.permissions.borrow_mut();
        if permissions.active != next {
            permissions.cancel_all();
        }
        permissions.active = next;
    }

    /// The user answered the request shown in the toolbar.
    fn permission_decision(
        &mut self,
        id: u64,
        decision: Decision,
        remember: bool,
    ) -> Result<(), Box<dyn Error>> {
        let prompt = self.permissions.borrow().prompt();
        let Some(prompt) = prompt.filter(|p| p.id == id) else {
            return Ok(());
        };
        let tab = self.tabs.active().filter(|t| t.id == prompt.tab);
        let current = tab
            .and_then(|t| t.content.view.as_ref()?.url().ok())
            .and_then(|url| security::origin(&url));
        if current.as_deref() != Some(prompt.origin.as_str()) {
            self.permissions.borrow_mut().resolve(id, Decision::Deny);
            return Err("Permission request expired after navigation.".into());
        }
        if remember {
            // A private window remembers only until it closes.
            if !self.private {
                self.submit(Action::SetPermission {
                    origin: prompt.origin.clone(),
                    capability: prompt.capability,
                    decision: Some(decision),
                })?;
            }
            self.permissions.borrow_mut().remember(Rule {
                origin: prompt.origin,
                capability: prompt.capability,
                decision,
            });
        }
        self.permissions.borrow_mut().resolve(id, decision);
        // The answer was given over the page; give it the keyboard back.
        if let Some(view) = self.tabs.active().and_then(|t| t.content.view.as_ref()) {
            self.focus(view);
        }
        Ok(())
    }

    /// Forget a remembered answer. The site's tabs close, so a camera or
    /// microphone it was given stops at once, as Settings says.
    fn revoke_permission(
        &mut self,
        origin: String,
        capability: Capability,
        confirmed: bool,
    ) -> Result<(), Box<dyn Error>> {
        if !confirmed || security::permission_origin(&origin).as_deref() != Some(origin.as_str()) {
            return Err("Invalid or unconfirmed permission revocation.".into());
        }
        if !self.private {
            self.submit(Action::SetPermission {
                origin: origin.clone(),
                capability,
                decision: None,
            })?;
        }
        self.permissions
            .borrow_mut()
            .rules
            .retain(|r| !(r.origin == origin && r.capability == capability));
        let mut failed = None;
        self.each_window(|browser| {
            let closing: Vec<_> = browser
                .tabs
                .items
                .iter()
                .filter(|t| security::origin(&t.url).as_deref() == Some(origin.as_str()))
                .map(|t| t.id)
                .collect();
            for id in closing {
                if let Err(error) = browser.close_tab(id) {
                    failed.get_or_insert(error.to_string());
                }
            }
        });
        self.persist();
        if let Some(error) = failed {
            return Err(error.into());
        }
        Ok(())
    }

    /// Clear the chosen browsing data, as on Windows: every tab closes first so no
    /// open page keeps or rewrites it, closed tabs are forgotten, and Settings
    /// stays open over a new tab until both halves report back.
    fn begin_clear(&mut self, request: ClearRequest) -> Result<(), Box<dyn Error>> {
        request.validate()?;
        if self.clearing.is_some() {
            return Err("Browsing data is already being cleared.".into());
        }
        if self.downloads.borrow().active_count() > 0 {
            return Err("Cancel or finish all downloads before clearing browsing data.".into());
        }
        // No other window may keep a page open, and none can be reopened.
        self.closed_windows.clear();
        let keep = self.current;
        for index in (0..self.windows.len()).rev() {
            if index != keep {
                self.remove_window(index, false);
            }
        }
        self.current = 0;
        self.permissions.borrow_mut().cancel_all();
        self.finish_find();
        self.closed.clear();
        for tab in std::mem::take(&mut self.tabs.items) {
            tab.content.hide();
        }
        self.open_tab(navigation::HOME.into())?;
        self.panel = Some(Panel::Settings);
        let mut clearing = Clearing {
            revision: 0,
            library_done: true,
            engine_done: true,
            failed: false,
            results: Vec::new(),
        };
        if request.passwords || request.autofill {
            clearing.results.push(
                "Rust Browser on Linux does not save passwords or form data, so there were none to clear."
                    .into(),
            );
        }
        if request.session {
            self.settings.restore_session = false;
            let result = self
                .store
                .as_ref()
                .map(|store| store.forget_session(self.settings.clone()))
                .unwrap_or(Ok(()))
                .and_then(|()| {
                    rust_browser::privacy::clear_session_backups(&self.profile).map(|_| ())
                });
            clearing.failed |= result.is_err();
            clearing.results.push(
                if result.is_ok() {
                    "Saved session URLs and session recovery files cleared; restoration is now disabled."
                } else {
                    "Session clearing failed or was partial; recovery files may remain."
                }
                .into(),
            );
        }
        if request.history || request.permissions {
            if self.private {
                // A private window's library lives in memory and goes with it.
                if request.permissions {
                    self.permissions.borrow_mut().rules.clear();
                }
                clearing.results.push(
                    "This private window keeps no history or permissions after it closes.".into(),
                );
            } else {
                match self.submit(Action::ClearLibrary {
                    history: request.history,
                    permissions: request.permissions,
                    confirmed: true,
                }) {
                    Ok(()) => {
                        clearing.revision = self.revision;
                        clearing.library_done = false;
                        if request.permissions {
                            self.permissions.borrow_mut().rules.clear();
                        }
                    }
                    Err(_) => {
                        clearing.failed = true;
                        clearing
                            .results
                            .push("Application database clearing failed.".into());
                    }
                }
            }
        }
        let kinds = engine_kinds(&request);
        if !kinds.is_empty() {
            match self
                .content_context
                .as_ref()
                .and_then(webkit2gtk::WebContextExt::website_data_manager)
            {
                Some(manager) => {
                    use webkit2gtk::WebsiteDataManagerExtManual;
                    let proxy = self.proxy.clone();
                    clearing.engine_done = false;
                    manager.clear(
                        kinds,
                        gtk::glib::TimeSpan(0),
                        None::<&gtk::gio::Cancellable>,
                        move |result| {
                            let _ = proxy.send_event(Event::EngineCleared(result.is_ok()));
                        },
                    );
                }
                None => {
                    clearing.failed = true;
                    clearing
                        .results
                        .push("Website data clearing could not start; its data may remain.".into());
                }
            }
        }
        self.clearing = Some(clearing);
        self.inform("Clearing selected data. Tabs were closed.".into());
        self.finish_clear();
        self.persist();
        Ok(())
    }

    /// Show `text` as information rather than a problem.
    fn inform(&mut self, text: String) {
        self.info.clone_from(&text);
        self.notice = text;
    }

    /// Report the clear once both halves are done.
    fn finish_clear(&mut self) {
        if !self
            .clearing
            .as_ref()
            .is_some_and(|c| c.library_done && c.engine_done)
        {
            return;
        }
        let Some(clear) = self.clearing.take() else {
            return;
        };
        let text = format!(
            "{} Bookmarks and downloaded files were kept. Library recovery backups were retained and may contain old history or permissions. New browsing can create new data; this is not secure disk erasure.",
            clear.results.join(" ")
        )
        .trim_start()
        .to_string();
        // A clear that did not fully succeed is reported as a problem.
        if clear.failed {
            self.notice = text;
        } else {
            self.inform(text);
        }
        self.persist();
    }

    /// Deny permission requests whose minute ran out, and keep the toolbar's
    /// countdown current.
    fn expire_permissions(&mut self) {
        let (expired, waiting) = {
            let mut permissions = self.permissions.borrow_mut();
            (permissions.expire(), permissions.prompt().is_some())
        };
        if expired || waiting {
            self.render_all();
        }
    }

    /// Refresh download progress twice a second while any transfer runs.
    fn poll_downloads(&mut self) {
        if self.downloads_polled.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.downloads_polled = Instant::now();
        self.expire_permissions();
        let changed = {
            let mut downloads = self.downloads.borrow_mut();
            downloads.active_count() > 0 && downloads.poll()
        };
        if changed {
            self.render_all();
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
            let source = Source::Window(window.id());
            move |request| {
                if request.body().len() <= 16384
                    && let Ok(command) = serde_json::from_str::<Command>(request.body())
                {
                    let _ = proxy.send_event(Event::Command(source, command));
                }
            }
        })
        .build_as_child(window)?;
    shortcuts(&view.webview(), proxy, Source::Window(window.id()));
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
fn shortcuts(widget: &webkit2gtk::WebView, proxy: EventLoopProxy<Event>, source: Source) {
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
            let _ = keys.send_event(Event::Command(source, command));
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
            let _ = proxy.send_event(Event::Command(source, command));
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
    let (status, body): (u16, std::borrow::Cow<'static, [u8]>) = if path == "/" {
        (200, linux_document().into_bytes().into())
    } else {
        (404, (&[] as &[u8]).into())
    };
    wry::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap_or_else(|_| wry::http::Response::new((&[] as &[u8]).into()))
}

/// What differs in the shared toolbar document on Linux: each Windows text, and
/// what Linux shows instead.
const LINUX_TEXT: [[&str; 2]; 2] = [
    // What Settings says sleeping does.
    [
        "Sleeping keeps inactive pages intact while freeing background activity.",
        "Sleeping closes an inactive page to free its memory. Selecting the tab loads it again where it was, usually with what you typed in its forms; anything else the page held, such as a playing video or an unsent chat message, may be lost.",
    ],
    // Linux keeps no diagnostics, so a problem notice offers none.
    [
        "byId('notice-diagnostics').hidden=!s.notice||",
        "byId('notice-diagnostics').hidden=true||",
    ],
];

/// The shared toolbar document, with what differs on Linux.
fn linux_document() -> String {
    LINUX_TEXT
        .iter()
        .fold(ui::HTML.to_string(), |page, [windows, linux]| {
            page.replacen(windows, linux, 1)
        })
}

/// A private window's anchor: a hidden, empty view in a new in-memory WebKit
/// context, which its website views are related to. wry would otherwise give
/// each private view a context of its own, so tabs would not share a sign-in
/// and only the first tab's downloads would reach `linux_downloads`.
fn private_anchor(window: &Window) -> Result<WebView, Box<dyn Error>> {
    Ok(WebViewBuilder::new()
        .with_incognito(true)
        .with_devtools(false)
        .with_clipboard(false)
        .with_autoplay(false)
        .with_visible(false)
        .with_permission_handler(|_| wry::PermissionResponse::Deny)
        .with_download_started_handler(|_, _| false)
        .with_navigation_handler(|url| url == "about:blank")
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .build_as_child(window)?)
}

/// A website view: HTTP(S) and `about:blank` only, no command bridge, permission
/// requests to `linux_permissions`, new windows opened as tabs. Its downloads go to
/// `linux_downloads`, hooked on the shared WebKit context, not through wry,
/// whose handler cannot wait for a folder or report progress.
fn content_view(
    window: &Window,
    context: &mut WebContext,
    proxy: EventLoopProxy<Event>,
    permissions: linux_permissions::Shared,
    id: TabId,
    related: Option<webkit2gtk::WebView>,
) -> Result<WebView, Box<dyn Error>> {
    let (title, load, popup, blocked) =
        (proxy.clone(), proxy.clone(), proxy.clone(), proxy.clone());
    let mut builder = WebViewBuilder::new_with_web_context(context);
    if let Some(related) = related {
        builder = builder.with_related_view(related);
    }
    let view = builder
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
                let _ = popup.send_event(Event::OpenTab(id, url));
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
    shortcuts(&engine, proxy.clone(), Source::Tab(id));
    // Every request is answered (now, or after the prompt), never by WebKit.
    let asked = proxy.clone();
    engine.connect_permission_request(move |engine, request| {
        let url = engine.uri();
        if permissions
            .borrow_mut()
            .request(id, url.as_deref(), request)
        {
            let _ = asked.send_event(Event::PermissionPrompt);
        }
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
    /// The profile's lock, for a normal window, until the browser opens.
    lock: Option<std::fs::File>,
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
        if el.exiting() {
            return;
        }
        browser.wake_selected();
        browser.claim_permissions();
        browser.layout_all();
        browser.render_all();
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
            self.lock.take(),
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
        // Events not tied to a window are shown in the focused one.
        browser.current = browser.focused_index();
        let result = match event {
            Event::BookmarkFile(file) => {
                browser.bookmark_file(file);
                Ok(())
            }
            Event::BookmarksRead(result) => match result {
                // The library's reply says what was imported.
                Ok(parsed) => browser.submit(Action::ImportBookmarks(parsed)),
                Err(message) => Err(message.into()),
            },
            Event::HandOff(request, reply) => {
                let status = if el.exiting() {
                    Status::Closing
                } else {
                    browser.hand_off(el, request)
                };
                let _ = reply.send(status);
                Ok(())
            }
            Event::Command(source, command) => {
                let found = match source {
                    Source::Window(id) => browser.select_window(id),
                    Source::Tab(id) => browser.select_tab_window(id),
                };
                if !found {
                    return;
                }
                browser.command(el, command)
            }
            Event::Page(id, event) => {
                if browser.select_tab_window(id) {
                    browser.page_event(id, event);
                }
                Ok(())
            }
            Event::OpenTab(id, url) => {
                browser.select_tab_window(id);
                browser.open_tab(url).map(|()| browser.persist())
            }
            Event::Library(reply) => {
                if let Some(clear) = &mut browser.clearing
                    && clear.revision == reply.revision
                    && !clear.library_done
                {
                    clear.library_done = true;
                    clear.failed |= reply.message.is_some();
                    clear.results.push(
                        if reply.message.is_none() {
                            "Selected application history/permission records cleared."
                        } else {
                            "Application database clearing failed; records may remain."
                        }
                        .into(),
                    );
                }
                if let Some(message) = reply.message {
                    browser.notice = message;
                }
                // What an import did is information, as on Windows.
                if let Some(summary) = reply.summary {
                    browser.inform(summary);
                }
                if let Some(data) = reply.data
                    && reply.revision >= browser.revision.saturating_sub(64)
                {
                    browser.data = *data;
                    // A private window's library is empty: its rules live only
                    // in the permission manager.
                    if !browser.private {
                        browser.permissions.borrow_mut().rules = browser.data.permissions.clone();
                    }
                }
                browser.finish_clear();
                Ok(())
            }
            Event::Paste(window, text) => {
                if !browser.select_window(window) {
                    return;
                }
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
            // The toolbar opens the request under the site badge on its own.
            Event::PermissionPrompt => Ok(()),
            Event::EngineCleared(ok) => {
                if let Some(clear) = &mut browser.clearing {
                    clear.engine_done = true;
                    clear.failed |= !ok;
                    clear.results.push(
                        if ok {
                            "Cookies, site storage and cached web content as selected were cleared."
                        } else {
                            "Website data clearing failed; some may remain."
                        }
                        .into(),
                    );
                }
                browser.finish_clear();
                Ok(())
            }
        };
        self.after(el, result);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(browser) = &mut self.browser else {
            return;
        };
        if !browser.select_window(id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                let result = browser.close_window(el);
                self.after(el, result);
            }
            WindowEvent::Focused(true) => {
                browser.focused(id);
                browser.persist();
                self.after(el, Ok(()));
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
            if browser.background_tick() {
                browser.render_all();
            }
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + GTK_TICK));
    }
}

/// Take the profile's lock, or hand `links` (or a request for a window) to the
/// browser holding it: `None` when that browser accepted. While it is starting or
/// closing nothing answers, so keep trying for a few seconds, taking the profile
/// if it becomes free. Never open a second browser on the same profile.
fn lock_or_hand_off(profile: &Path, links: &[String]) -> Result<Option<std::fs::File>, String> {
    use linux_handoff::Outcome;
    const PATIENCE: Duration = Duration::from_secs(6);
    let started = Instant::now();
    let request = if links.is_empty() {
        Request::NewWindow
    } else {
        Request::Open(links.to_vec())
    };
    loop {
        match storage::lock(profile) {
            Ok(lock) => return Ok(Some(lock)),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.to_string()),
        }
        let message = match linux_handoff::send(profile, &request) {
            Outcome::Answered(Status::Accepted) => return Ok(None),
            Outcome::NoServer | Outcome::Answered(Status::Closing)
                if started.elapsed() < PATIENCE =>
            {
                std::thread::sleep(Duration::from_millis(300));
                continue;
            }
            Outcome::NoServer | Outcome::Answered(Status::Closing) => {
                "Rust Browser is already running with this profile but did not accept the link. It may be starting, closing or not responding. Wait a moment and try again."
            }
            Outcome::Answered(Status::Refused) => {
                "The running Rust Browser refused the request: the link is not one it can open, or no more windows can be opened."
            }
            Outcome::Answered(Status::Busy) => {
                "The running Rust Browser is busy (clearing browsing data, or receiving many links at once). Try again in a few seconds."
            }
            Outcome::Answered(Status::NotResponding) | Outcome::NotResponding => {
                "The running Rust Browser is not responding, so the link was not opened. Wait, or close it and try again."
            }
            Outcome::Untrusted => {
                "Something other than Rust Browser is in the place of its link channel, so nothing was sent to it."
            }
            Outcome::Failed => "Rust Browser could not hand the link to its running window.",
        };
        return Err(format!(
            "{message} No second window was opened and nothing was changed."
        ));
    }
}

/// winit's Xlib error handler is process-wide and records every X error as one
/// of its own, whichever connection it came from; its next call then panics.
/// WebKit's GL code makes harmless errors on GDK's connection when a window
/// manager handles the window (a `GLXBadWindow` under Openbox, Mutter or
/// WSLg's Weston), which ended the browser at its first title change. Errors
/// on other connections are now marked handled, so winit sees only its own.
fn keep_other_x_errors_from_winit(event_loop: &EventLoop<Event>) {
    use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    let Some(own) = event_loop
        .display_handle()
        .ok()
        .and_then(|handle| match handle.as_raw() {
            RawDisplayHandle::Xlib(xlib) => xlib.display.map(|d| d.as_ptr() as usize),
            _ => None,
        })
    else {
        return;
    };
    winit::platform::x11::register_xlib_error_hook(Box::new(move |display, _| {
        display as usize != own
    }));
}

/// Start the Linux browser. Returns a message for the user when it cannot.
pub fn run(launch: Launch) -> Result<(), String> {
    use winit::platform::x11::EventLoopBuilderExtX11;
    let profile = storage::profile_dir().map_err(|e| e.to_string())?;
    let lock = if launch.private {
        None
    } else {
        match lock_or_hand_off(&profile, &launch.links)? {
            Some(lock) => Some(lock),
            // The running browser opened it.
            None => return Ok(()),
        }
    };
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
    keep_other_x_errors_from_winit(&event_loop);
    let mut app = App {
        proxy: event_loop.create_proxy(),
        profile,
        lock,
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
    fn clearing_asks_webkit_for_exactly_the_chosen_kinds() {
        use webkit2gtk::WebsiteDataTypes as Kind;
        let request = |cookies, cache| ClearRequest {
            history: true,
            cookies,
            cache,
            permissions: true,
            confirmed: true,
            ..ClearRequest::default()
        };
        // History and permissions live in the library, not in WebKit.
        assert!(engine_kinds(&request(false, false)).is_empty());
        let cookies = engine_kinds(&request(true, false));
        assert!(cookies.contains(Kind::COOKIES | Kind::LOCAL_STORAGE | Kind::INDEXEDDB_DATABASES));
        assert!(!cookies.contains(Kind::DISK_CACHE));
        let cache = engine_kinds(&request(false, true));
        assert!(cache.contains(Kind::MEMORY_CACHE | Kind::DISK_CACHE));
        assert!(!cache.contains(Kind::COOKIES));
        // Which sites must use HTTPS is never forgotten.
        assert!(!engine_kinds(&request(true, true)).contains(Kind::HSTS_CACHE));
    }

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
        // After `--`, everything is a link, even what looks like an option.
        let launch = args(&["--", "https://example.com/"]).unwrap();
        assert!(!launch.private);
        assert_eq!(launch.links, ["https://example.com/"]);
        assert!(args(&["--", "--private"]).is_err());
    }

    #[test]
    fn a_private_window_starts_with_its_link_after_the_options() {
        let launch = parse_args(private_args(None)).unwrap();
        assert!(launch.private && launch.links.is_empty());
        let launch = parse_args(private_args(Some("https://example.com/a"))).unwrap();
        assert!(launch.private);
        assert_eq!(launch.links, ["https://example.com/a"]);
    }

    #[test]
    fn windows_are_saved_main_first_then_by_recent_focus() {
        // Windows 10, 20, 30, 40; 30 then 10 were focused, 20 and 40 never.
        let windows = [10, 20, 30, 40];
        assert_eq!(save_order(&windows, &[30, 10], 2), [2, 0, 1, 3]);
        // The main window is first even when not first in the focus order, and
        // closed windows still in the focus order are skipped.
        assert_eq!(save_order(&windows, &[99, 10, 30], 3), [3, 0, 2, 1]);
        assert_eq!(save_order(&[7], &[], 0), [0]);
    }

    #[test]
    fn the_trusted_scheme_serves_only_the_toolbar_document() {
        let page = toolbar_document("/");
        assert_eq!(page.status(), 200);
        assert_eq!(page.body().as_ref(), linux_document().as_bytes());
        // Each Windows text Linux replaces must still be in the shared document,
        // exactly once.
        for [windows, linux] in LINUX_TEXT {
            assert_eq!(ui::HTML.matches(windows).count(), 1, "{windows}");
            assert!(linux_document().contains(linux));
            assert!(!linux_document().contains(windows));
        }
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
