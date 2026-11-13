//! Several normal windows in one process: routing, limits, closing and reopening,
//! hand-off, engine restart, toolbar recovery, clearing and session restore; and
//! the password saving and autofill switches across windows and restarts.
use super::*;
use rust_browser::handoff::{Request, Status};
use std::path::Path;

/// Per window: tab IDs, addresses, pins and the selected index.
type WindowShape = (Vec<TabId>, Vec<String>, Vec<bool>, usize);
use std::sync::{Arc, Mutex};
use windows::Win32::{
    Foundation::CloseHandle,
    System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
};
use windows::core::Interface;
use winit::platform::windows::EventLoopBuilderExtWindows;
use wry::WebViewExtWindows;

fn root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "rust-browser-windows-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    root
}

fn new_app(event_loop: &EventLoop<Event>) -> App {
    App {
        session: None,
        proxy: event_loop.create_proxy(),
        failure: None,
        modifiers: ModifiersState::default(),
        closing: None,
        start: None,
    }
}

fn loaded(window: &WindowState) -> bool {
    window
        .tabs
        .items
        .iter()
        .all(|t| t.url == navigation::HOME || !t.content.state.loading)
}

fn urls(window: &WindowState) -> Vec<String> {
    window.tabs.items.iter().map(|t| t.url.clone()).collect()
}

fn view(page: &Page) -> usize {
    page.engine.view.webview().as_raw() as usize
}

/// End this fixture's own content browser process.
fn terminate_engine(session: &Session) -> Result<(), String> {
    let tab = session
        .windows
        .iter()
        .flat_map(|w| w.tabs.items.iter())
        .find(|t| t.url != navigation::HOME)
        .ok_or("No web tab to identify the engine")?;
    // SAFETY: the process ID comes from this fixture's live view and names its own
    // disposable WebView2 browser process; the handle is closed before returning.
    unsafe {
        let mut process = 0;
        tab.content
            .engine
            .view
            .webview()
            .BrowserProcessId(&mut process)
            .map_err(|_| "Engine process is unknown")?;
        let handle = OpenProcess(PROCESS_TERMINATE, false, process)
            .map_err(|_| "Engine process could not be opened")?;
        let ended = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        ended.map_err(|_| "Engine process could not be ended".to_string())
    }
}

/// Shared driver: each test supplies its steps as a function of the phase.
struct Probe {
    app: App,
    root: PathBuf,
    url: String,
    phase: u32,
    started: Instant,
    at: Instant,
    limit: Duration,
    failure: Option<String>,
    done: bool,
    log: Vec<String>,
    result: Arc<Mutex<Option<String>>>,
    marks: Vec<usize>,
    snapshot: Vec<WindowShape>,
    /// Diagnostics while a phase waits: last trace and longest idle-callback gap.
    /// The answer to a hand-off sent through the event loop.
    reply: Option<std::sync::mpsc::Receiver<Status>>,
    steps: fn(&mut Probe, &ActiveEventLoop) -> Result<(), String>,
    /// Start a private window instead of a normal one.
    private: bool,
}

impl Probe {
    fn s(&mut self) -> Result<&mut Session, String> {
        self.app
            .session
            .as_mut()
            .ok_or_else(|| "No session".to_string())
    }
    fn next(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    fn command(
        &mut self,
        el: &ActiveEventLoop,
        window: usize,
        command: Command,
    ) -> Result<(), String> {
        let id = self.s()?.windows[window].window.id();
        self.app.user_event(el, Event::WindowCommand(id, command));
        Ok(())
    }
    fn window_event(
        &mut self,
        el: &ActiveEventLoop,
        window: usize,
        event: WindowEvent,
    ) -> Result<(), String> {
        let id = self.s()?.windows[window].window.id();
        self.app.window_event(el, id, event);
        Ok(())
    }
}

impl ApplicationHandler<Event> for Probe {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.app.session.is_some() {
            return;
        }
        let start = Start {
            profile: self.root.clone(),
            private: self.private,
            links: Vec::new(),
            lock: None,
        };
        match Session::create(el, self.app.proxy.clone(), start) {
            Ok(mut s) => {
                s.render_all();
                self.app.session = Some(s)
            }
            Err(_) => {
                self.failure = Some("Windows fixture startup failed".into());
                el.exit();
            }
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(el, id, event)
    }
    fn user_event(&mut self, el: &ActiveEventLoop, event: Event) {
        self.app.user_event(el, event)
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.app.about_to_wait(el);
        if self.done || self.failure.is_some() {
            // Exit now instead of when the browser's next timer wakes the loop.
            el.set_control_flow(ControlFlow::Poll);
            return;
        }
        if self.started.elapsed() > self.limit {
            self.failure = Some(format!("Windows fixture timed out at phase {}", self.phase));
            el.exit();
            el.set_control_flow(ControlFlow::Poll);
            return;
        }
        let ready =
            self.app.session.as_ref().is_some_and(|s| s.ready) || self.app.closing.is_some();
        if ready {
            let steps = self.steps;
            if let Err(error) = steps(self, el) {
                self.failure = Some(error);
                el.exit();
                el.set_control_flow(ControlFlow::Poll);
                return;
            }
        }
        if self.done {
            el.set_control_flow(ControlFlow::Poll);
        } else {
            el.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(25),
            ));
        }
    }
}

/// Run `steps` against an existing profile (a restart of an earlier run).
fn run_in(root: PathBuf, steps: fn(&mut Probe, &ActiveEventLoop) -> Result<(), String>) {
    let _ = drive(root, steps, false);
}

fn run(
    label: &str,
    steps: fn(&mut Probe, &ActiveEventLoop) -> Result<(), String>,
    prepare: impl FnOnce(&Path, &str),
) -> (PathBuf, Vec<String>) {
    let server = super::compat_validation::Server::start();
    let root = root(label);
    prepare(&root, &server.url);
    let private = label.starts_with("private");
    let log = drive_with(root.clone(), steps, private, server);
    (root, log)
}

fn drive(
    root: PathBuf,
    steps: fn(&mut Probe, &ActiveEventLoop) -> Result<(), String>,
    private: bool,
) -> Vec<String> {
    drive_with(
        root,
        steps,
        private,
        super::compat_validation::Server::start(),
    )
}

fn drive_with(
    root: PathBuf,
    steps: fn(&mut Probe, &ActiveEventLoop) -> Result<(), String>,
    private: bool,
    server: super::compat_validation::Server,
) -> Vec<String> {
    let event_loop = EventLoop::<Event>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut probe = Probe {
        app: new_app(&event_loop),
        root: root.clone(),
        url: server.url.clone(),
        phase: 0,
        started: Instant::now(),
        at: Instant::now(),
        limit: Duration::from_secs(150),
        failure: None,
        done: false,
        log: Vec::new(),
        result: Arc::new(Mutex::new(None)),
        marks: Vec::new(),
        snapshot: Vec::new(),
        reply: None,
        steps,
        private,
    };
    let result = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let done = probe.done;
    let log = std::mem::take(&mut probe.log);
    drop(probe);
    println!("{}", log.join("\n"));
    assert!(result.is_ok());
    assert!(failure.is_none(), "{failure:?}\n{}", log.join("\n"));
    assert!(done, "Fixture did not finish");
    drop(server);
    log
}

/// Send a hand-off as a second launch would: through the event loop.
fn hand_off(p: &Probe, request: Request) -> Result<std::sync::mpsc::Receiver<Status>, String> {
    let (reply, answer) = std::sync::mpsc::sync_channel(1);
    p.app
        .proxy
        .send_event(Event::HandOff(request, reply))
        .map_err(|_| "Hand-off could not be sent")?;
    Ok(answer)
}
/// The hand-off answer once the browser has given it.
fn answered(p: &mut Probe) -> Result<Option<Status>, String> {
    match p.reply.as_ref().map(|r| r.try_recv()) {
        Some(Ok(status)) => {
            p.reply = None;
            Ok(Some(status))
        }
        Some(Err(std::sync::mpsc::TryRecvError::Empty))
            if p.at.elapsed() < Duration::from_secs(10) =>
        {
            Ok(None)
        }
        _ => Err("The browser never answered a hand-off".into()),
    }
}

/// Routing, toolbar IPC per window, the 8-window limit, closing one window,
/// reopening it, focus-based link hand-off and a "new window" hand-off.
fn routing_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 => {
            // As in the browser: window requests arrive as events, never from
            // the idle callback.
            let proxy = p.app.proxy.clone();
            proxy
                .send_event(Event::Command(Command::NewWindow))
                .map_err(|_| "proxy")?;
            proxy
                .send_event(Event::Command(Command::NewWindow))
                .map_err(|_| "proxy")?;
            p.phase = 20;
            p.at = Instant::now();
        }
        20 if p.app.session.as_ref().is_some_and(|s| s.windows.len() == 3) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows.len() != 3 {
                return Err(format!("Expected 3 windows, found {}", s.windows.len()));
            }
            let ids: Vec<TabId> = s
                .windows
                .iter()
                .flat_map(|w| w.tabs.items.iter().map(|t| t.id))
                .collect();
            let mut unique = ids.clone();
            unique.sort();
            unique.dedup();
            if unique.len() != ids.len() {
                return Err(format!("Tab IDs repeat across windows: {ids:?}"));
            }
            let toolbars: Vec<usize> = s
                .windows
                .iter()
                .map(|w| w.toolbar.webview().as_raw() as usize)
                .collect();
            if toolbars[0] == toolbars[1]
                || toolbars[1] == toolbars[2]
                || toolbars[0] == toolbars[2]
            {
                return Err("Windows share a toolbar view".into());
            }
            p.phase = 1;
            p.at = Instant::now();
        }
        // Let each new window's first tab finish its initial blank load first.
        1 if p.at.elapsed() > Duration::from_secs(2) => {
            for window in 0..3 {
                p.command(el, window, Command::Navigate(format!("{url}?w{window}")))?;
            }
            p.phase = 10;
            p.at = Instant::now();
        }
        10 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows.iter().all(loaded)
                && (0..3).all(|i| {
                    s.windows[i]
                        .tabs
                        .active()
                        .is_some_and(|t| t.url.ends_with(&format!("?w{i}")))
                })
            {
                // A real IPC message from the second window's own toolbar.
                s.windows[1]
                    .toolbar
                    .evaluate_script("window.ipc.postMessage(JSON.stringify({action:'new_tab'}))")
                    .map_err(|_| "Toolbar IPC failed")?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(20) {
                let found: Vec<Vec<String>> = s.windows.iter().map(urls).collect();
                return Err(format!("Navigation did not stay in its window: {found:?}"));
            }
        }
        11 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let counts: Vec<usize> = s.windows.iter().map(|w| w.tabs.items.len()).collect();
            if counts == [1, 2, 1] {
                p.window_event(el, 2, WindowEvent::Focused(true))?;
                p.reply = Some(hand_off(p, Request::Open(vec![format!("{url}?handed")]))?);
                p.phase = 12;
                p.at = Instant::now();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err(format!(
                    "Toolbar IPC did not reach only its window: {counts:?}"
                ));
            }
        }
        12 => {
            if let Some(status) = answered(p)? {
                if status != Status::Accepted {
                    return Err(format!("Hand-off answered {status:?}"));
                }
                let s = p.app.session.as_mut().ok_or("No session")?;
                let found: Vec<Vec<String>> = s.windows.iter().map(urls).collect();
                if !found[2].iter().any(|u| u.ends_with("?handed"))
                    || found[0]
                        .iter()
                        .chain(&found[1])
                        .any(|u| u.ends_with("?handed"))
                {
                    return Err(format!(
                        "A link did not land in the focused window: {found:?}"
                    ));
                }
                p.reply = Some(hand_off(p, Request::NewWindow)?);
                p.phase = 13;
                p.at = Instant::now();
            }
        }
        13 => {
            if let Some(status) = answered(p)? {
                let windows = p.app.session.as_ref().map_or(0, |s| s.windows.len());
                if status != Status::Accepted || windows != 4 {
                    return Err(format!(
                        "New-window hand-off: {status:?}, {windows} windows"
                    ));
                }
                // Four more windows reach the limit; the fifth request is refused.
                for _ in 0..5 {
                    p.app
                        .proxy
                        .send_event(Event::Command(Command::NewWindow))
                        .map_err(|_| "proxy")?;
                }
                p.phase = 3;
                p.at = Instant::now();
            }
        }
        3 if p.at.elapsed() > Duration::from_secs(3) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows.len() != storage::MAX_WINDOWS {
                return Err(format!(
                    "Expected {} windows, found {}",
                    storage::MAX_WINDOWS,
                    s.windows.len()
                ));
            }
            let focused = s.focused_index();
            if !s.windows[focused].notice.contains("At most 8 windows") {
                return Err(format!(
                    "Ninth window was not refused with a notice: {:?}",
                    s.windows[focused].notice
                ));
            }
            // Close the second window, which has two tabs.
            p.window_event(el, 1, WindowEvent::CloseRequested)?;
            let closing = p.app.closing.is_some();
            let s = p.app.session.as_mut().ok_or("No session")?;
            if closing || s.windows.len() != 7 || s.closing_windows.len() != 1 {
                return Err(format!(
                    "Closing one window: closing={closing} windows={} retiring={}",
                    s.windows.len(),
                    s.closing_windows.len()
                ));
            }
            if !s
                .closed_windows
                .first()
                .is_some_and(|tabs| tabs.iter().any(|(u, _)| u.ends_with("?w1")))
            {
                return Err("The closed window's tabs were not kept for reopening".into());
            }
            if !s.notice.contains("Reopen closed window") {
                return Err(format!(
                    "No notice after closing a window with tabs: {:?}",
                    s.notice
                ));
            }
            p.next();
        }
        4 if p
            .app
            .session
            .as_ref()
            .is_some_and(|s| s.closing_windows.is_empty()) =>
        {
            if !(p.at.elapsed() > RETIRE_DELAY) {
                return Err("A closed window was destroyed before the retirement delay".into());
            }
            p.app
                .proxy
                .send_event(Event::Command(Command::ReopenClosedWindow))
                .map_err(|_| "proxy")?;
            p.phase = 30;
            p.at = Instant::now();
        }
        30 if p.app.session.as_ref().is_some_and(|s| s.windows.len() == 8) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let last = s.windows.last().map(urls).unwrap_or_default();
            if s.windows.len() != 8 || !last.iter().any(|u| u.ends_with("?w1")) || last.len() != 2 {
                return Err(format!(
                    "Reopened window: {} windows, tabs {last:?}",
                    s.windows.len()
                ));
            }
            p.phase = 5;
            p.at = Instant::now();
        }
        4 if p.at.elapsed() > RETIRE_DELAY + Duration::from_secs(5) => {
            return Err("A closed window was never destroyed".into());
        }
        5 => {
            // The saved session holds every window, the focused one first.
            let s = p.app.session.as_mut().ok_or("No session")?;
            s.persist();
            let saved = s.last_snapshot.borrow().clone().ok_or("Nothing saved")?;
            let focused = s.focused_index();
            if saved.other_windows.len() != 7 || saved.tabs != urls(&s.windows[focused]) {
                return Err(format!(
                    "Saved {} other windows; main {:?}, focused {:?}",
                    saved.other_windows.len(),
                    saved.tabs,
                    urls(&s.windows[focused])
                ));
            }
            p.app.user_event(el, Event::Command(Command::ExitBrowser));
            p.next();
        }
        6 if p.app.session.is_none() => {
            p.log
                .push("windows: routing, limit, close, reopen, hand-off and exit passed".into());
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens several real windows"]
fn native_multiple_windows_routing_limit_close_reopen_and_handoff() {
    let (root, _) = run("routing", routing_steps, |_, _| {});
    // More > Exit kept every window for the next start.
    let snapshot = storage::load(&root.join("session.json")).unwrap();
    assert_eq!(snapshot.other_windows.len(), 7);
    let _ = std::fs::remove_dir_all(root);
}

/// Engine restart keeps every window's tabs; toolbar recovery stays in its window;
/// clearing waits for retired views in two windows.
fn engine_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 => {
            p.app
                .proxy
                .send_event(Event::Command(Command::NewWindow))
                .map_err(|_| "proxy")?;
            p.phase = 20;
            p.at = Instant::now();
        }
        20 if p.app.session.as_ref().is_some_and(|s| s.windows.len() == 2)
            && p.at.elapsed() > Duration::from_secs(2) =>
        {
            p.command(el, 0, Command::Navigate(format!("{url}?a")))?;
            p.command(el, 0, Command::NewTab)?;
            p.command(el, 0, Command::Navigate(format!("{url}?b")))?;
            p.command(el, 1, Command::Navigate(format!("{url}?c")))?;
            p.command(el, 1, Command::NewTab)?;
            p.command(el, 1, Command::Navigate(format!("{url}?d")))?;
            p.command(el, 1, Command::NewTab)?;
            p.command(el, 1, Command::Navigate(format!("{url}?e")))?;
            p.phase = 1;
            p.at = Instant::now();
        }
        1 if p
            .app
            .session
            .as_ref()
            .is_some_and(|s| s.windows.iter().all(loaded))
            && p.at.elapsed() > Duration::from_secs(1) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let first = s.windows[0].tabs.items[0].id;
            s.current = 0;
            s.tabs.pin(first, true);
            let second = s.windows[1].tabs.items[1].id;
            s.current = 1;
            s.tabs.select(second);
            let _ = s.activate();
            p.snapshot = s
                .windows
                .iter()
                .map(|w| {
                    (
                        w.tabs.items.iter().map(|t| t.id).collect(),
                        urls(w),
                        w.tabs.items.iter().map(|t| t.pinned).collect(),
                        w.tabs.active_index(),
                    )
                })
                .collect();
            p.marks = s
                .windows
                .iter()
                .flat_map(|w| w.tabs.items.iter().map(|t| view(&t.content)))
                .collect();
            terminate_engine(s)?;
            p.next();
        }
        2 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let restarted = s.windows.iter().all(|w| w.notice.contains("was restarted"));
            if restarted && s.windows.iter().all(loaded) {
                let now: Vec<WindowShape> = s
                    .windows
                    .iter()
                    .map(|w| {
                        (
                            w.tabs.items.iter().map(|t| t.id).collect(),
                            urls(w),
                            w.tabs.items.iter().map(|t| t.pinned).collect(),
                            w.tabs.active_index(),
                        )
                    })
                    .collect();
                if now != p.snapshot {
                    return Err(format!(
                        "Engine restart changed windows: {:?} vs {:?}",
                        now, p.snapshot
                    ));
                }
                let views: Vec<usize> = s
                    .windows
                    .iter()
                    .flat_map(|w| w.tabs.items.iter().map(|t| view(&t.content)))
                    .collect();
                // Per tab, as in the single-window restart suite: the allocator may
                // give a new view the address another tab's old view had.
                if views.len() != p.marks.len()
                    || views.iter().zip(&p.marks).any(|(new, old)| new == old)
                {
                    return Err(format!(
                        "A window's views were not rebuilt: {views:x?} was {:x?}",
                        p.marks
                    ));
                }
                // A failure reported for the toolbar of the window without focus,
                // so handling it in the focused window would be visible.
                let target = 1 - s.focused_index();
                let marks = [
                    s.windows[0].toolbar.webview().as_raw() as usize,
                    s.windows[1].toolbar.webview().as_raw() as usize,
                    target,
                ];
                let source = s.windows[target].toolbar_source;
                p.marks = marks.to_vec();
                p.app.user_event(
                    el,
                    Event::ToolbarProcess(source, crate::native_activity::Failure::Renderer),
                );
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(30) {
                return Err("The engine restart did not reach every window".into());
            }
        }
        3 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let (target, other) = (p.marks[2], 1 - p.marks[2]);
            let rebuilt = s.windows[target].toolbar.webview().as_raw() as usize;
            let kept = s.windows[other].toolbar.webview().as_raw() as usize;
            if rebuilt == p.marks[target]
                || kept != p.marks[other]
                || s.windows[other].retired_toolbar.is_some()
            {
                return Err("Toolbar recovery did not stay in its own window".into());
            }
            // Close a tab in the first window now and one in the second window
            // half a retirement delay later, so the closed second window outlives
            // the first window's retired view.
            let a = s.windows[0].tabs.items[1].id;
            p.command(el, 0, Command::CloseTab(a))?;
            p.phase = 31;
            p.at = Instant::now();
        }
        31 if p.at.elapsed() > RETIRE_DELAY / 2 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let b = s.windows[1].tabs.items[2].id;
            p.command(el, 1, Command::CloseTab(b))?;
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows.iter().any(|w| w.retired.is_empty()) {
                return Err("Closed tabs were not retired in both windows".into());
            }
            p.command(
                el,
                0,
                Command::ClearData(rust_browser::privacy::ClearRequest {
                    cookies: true,
                    confirmed: true,
                    ..Default::default()
                }),
            )?;
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.clearing.is_none() || s.windows.len() != 1 || s.closing_windows.len() != 1 {
                return Err(format!(
                    "Clearing: started={} windows={} closing={}",
                    s.clearing.is_some(),
                    s.windows.len(),
                    s.closing_windows.len()
                ));
            }
            // Clearing retires this window's tabs at the same moment it closes the
            // other window, so their delays end together. Keep the closed window
            // longer, as a slower teardown would: clearing must still wait for it.
            s.closing_windows[0].0 = Instant::now() + RETIRE_DELAY * 2;
            p.phase = 4;
            p.at = Instant::now();
        }
        4 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let retiring =
                s.windows.iter().any(|w| !w.retired.is_empty()) || !s.closing_windows.is_empty();
            let native = s.clearing.as_ref().is_some_and(|c| c.started_native);
            if native && retiring {
                return Err(
                    "Engine clearing started while a window still had retired views".into(),
                );
            }
            if s.clearing.is_none() || native {
                if p.at.elapsed() > RETIRE_DELAY * 2 || s.clearing.is_none() {
                    p.log.push(
                        "windows: engine restart, toolbar isolation and clearing passed".into(),
                    );
                    p.done = true;
                    el.exit();
                }
            } else if p.at.elapsed() > Duration::from_secs(30) {
                return Err("Engine clearing never started".into());
            }
        }
        _ => {}
    }
    let _ = &p.result;
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; ends only the fixture's own engine process"]
fn native_multiple_windows_engine_restart_toolbar_and_clearing() {
    let (root, _) = run("engine", engine_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Startup restores every saved window; More > Exit saves them again.
fn restore_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let found: Vec<(Vec<String>, usize, Vec<bool>)> = s
                .windows
                .iter()
                .map(|w| {
                    (
                        urls(w),
                        w.tabs.active_index(),
                        w.tabs.items.iter().map(|t| t.pinned).collect(),
                    )
                })
                .collect();
            let expected = vec![
                (
                    vec![format!("{url}?m1"), format!("{url}?m2")],
                    1,
                    vec![true, false],
                ),
                (vec![format!("{url}?o1")], 0, vec![false]),
                (
                    vec![format!("{url}?o2a"), format!("{url}?o2b")],
                    0,
                    vec![false, false],
                ),
            ];
            if found != expected {
                return Err(format!("Restored windows {found:?}, expected {expected:?}"));
            }
            if s.focused_index() != 0 {
                return Err(format!(
                    "The main window was not the focused window after restore: {:?}",
                    s.focus_order
                        .iter()
                        .map(|id| s.windows.iter().position(|w| w.window.id() == *id))
                        .collect::<Vec<_>>()
                ));
            }
            // Every restored window follows the saved vertical-tabs setting (v0.21),
            // with its column beside the toolbar and page, never over them.
            for index in 0..s.windows.len() {
                let views = shown_views_in(s, index);
                if s.windows[index].tabs_view.is_none() || named(&views, "tabs").is_none() {
                    return Err(format!(
                        "Restored window {index} has no tab column: {views:?}"
                    ));
                }
                check_no_overlap_in(s, index, &format!("restored window {index}"))?;
            }
            p.app.user_event(el, Event::Command(Command::ExitBrowser));
            // Hiding the windows lets Windows activate another one, as it does on
            // a foreground desktop. That must not change what Exit saved.
            p.window_event(el, 2, WindowEvent::Focused(true))?;
            p.next();
        }
        1 if p.app.session.is_none() => {
            p.log.push("windows: restore and exit passed".into());
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens several real windows"]
fn native_multiple_windows_restore_and_exit() {
    let (root, _) = run("restore", restore_steps, |root, url| {
        let snapshot = Snapshot {
            tabs: vec![format!("{url}?m1"), format!("{url}?m2")],
            active: 1,
            pinned: vec![true, false],
            other_windows: vec![
                storage::WindowSnapshot {
                    tabs: vec![format!("{url}?o1")],
                    ..Default::default()
                },
                storage::WindowSnapshot {
                    tabs: vec![format!("{url}?o2a"), format!("{url}?o2b")],
                    ..Default::default()
                },
            ],
            settings: rust_browser::settings::Settings {
                vertical_tabs: true,
                ..Default::default()
            },
            ..Snapshot::default()
        };
        storage::save(&root.join("session.json"), &snapshot).unwrap();
    });
    let saved = storage::load(&root.join("session.json")).unwrap();
    assert_eq!(saved.other_windows.len(), 2);
    assert_eq!(saved.tabs.len(), 2);
    assert_eq!(saved.pinned, vec![true, false]);
    assert!(saved.settings.vertical_tabs);
    let _ = std::fs::remove_dir_all(root);
}

/// Every content view's password and autofill settings, then every toolbar's.
fn form_settings(s: &Session) -> Result<(Vec<Forms>, Vec<Forms>), String> {
    let read = |view: &wry::WebView| {
        native_security::forms(view).map_err(|e| format!("Settings unreadable: {e}"))
    };
    let mut pages = Vec::new();
    let mut toolbars = Vec::new();
    for window in &s.windows {
        for tab in &window.tabs.items {
            pages.push(read(&tab.content.engine.view)?);
        }
        toolbars.push(read(&window.toolbar)?);
    }
    Ok((pages, toolbars))
}

/// Every content view has `pages` and every toolbar has both off.
fn expect_forms(s: &Session, pages: Forms, step: &str) -> Result<(), String> {
    let (found, toolbars) = form_settings(s)?;
    if found.is_empty() || found.iter().any(|f| *f != pages) {
        return Err(format!("{step}: pages {found:?}, expected {pages:?}"));
    }
    if toolbars.iter().any(|f| *f != Forms::default()) {
        return Err(format!(
            "{step}: a toolbar may save passwords or fill forms: {toolbars:?}"
        ));
    }
    Ok(())
}

use native_security::Forms;
const BOTH: Forms = Forms {
    passwords: true,
    autofill: true,
};

/// The switches start off (WebView2's own default turns autofill on), apply at
/// once to open views, reach new tabs, new windows and views rebuilt after an
/// engine restart, and never reach a toolbar.
fn forms_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.settings.save_passwords || s.settings.autofill {
                return Err("A new profile has a switch on".into());
            }
            expect_forms(s, Forms::default(), "new profile")?;
            p.command(el, 0, Command::Navigate(format!("{url}?form")))?;
            p.command(el, 0, Command::SetSavePasswords(true))?;
            p.command(el, 0, Command::SetAutofill(true))?;
            let s = p.app.session.as_mut().ok_or("No session")?;
            if !s.settings.save_passwords || !s.settings.autofill {
                return Err("The switches were not saved in settings".into());
            }
            expect_forms(s, BOTH, "switched on")?;
            p.command(el, 0, Command::NewTab)?;
            let s = p.app.session.as_mut().ok_or("No session")?;
            expect_forms(s, BOTH, "new tab")?;
            p.app
                .proxy
                .send_event(Event::Command(Command::NewWindow))
                .map_err(|_| "proxy")?;
            p.next();
        }
        1 if p.app.session.as_ref().is_some_and(|s| s.windows.len() == 2) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            expect_forms(s, BOTH, "new window")?;
            p.next();
        }
        2 if p.at.elapsed() > Duration::from_secs(1)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            p.marks = s
                .windows
                .iter()
                .flat_map(|w| w.tabs.items.iter().map(|t| view(&t.content)))
                .collect();
            terminate_engine(s)?;
            p.next();
        }
        3 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let restarted = s.windows.iter().all(|w| w.notice.contains("was restarted"));
            if restarted && s.windows.iter().all(loaded) {
                let views: Vec<usize> = s
                    .windows
                    .iter()
                    .flat_map(|w| w.tabs.items.iter().map(|t| view(&t.content)))
                    .collect();
                if views.len() != p.marks.len()
                    || views.iter().zip(&p.marks).any(|(new, old)| new == old)
                {
                    return Err("The engine restart did not rebuild every view".into());
                }
                expect_forms(s, BOTH, "after engine restart")?;
                p.command(el, 1, Command::SetAutofill(false))?;
                let s = p.app.session.as_mut().ok_or("No session")?;
                expect_forms(
                    s,
                    Forms {
                        passwords: true,
                        autofill: false,
                    },
                    "autofill switched off",
                )?;
                p.log
                    .push("forms: switches apply live, to new tabs, windows and restarts".into());
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(30) {
                return Err("The engine restart did not finish".into());
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; ends only the fixture's own engine process"]
fn native_passwords_and_autofill_follow_the_switches() {
    let (root, _) = run("forms", forms_steps, |_, _| {});
    // The switches are saved with the other settings.
    let saved = storage::load(&root.join("session.json")).unwrap();
    assert!(saved.settings.save_passwords);
    assert!(!saved.settings.autofill);
    let _ = std::fs::remove_dir_all(root);
}

/// A private window never saves passwords or fills forms, even when its
/// in-memory settings say so, and refuses to change the switches.
fn private_forms_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    if p.at.elapsed() < Duration::from_secs(1) {
        return Ok(());
    }
    let s = p.app.session.as_mut().ok_or("No session")?;
    if !s.private {
        return Err("The fixture did not start a private window".into());
    }
    expect_forms(s, Forms::default(), "private window")?;
    if s.command(Command::SetSavePasswords(true)).is_ok()
        || s.command(Command::SetAutofill(true)).is_ok()
    {
        return Err("A private window accepted a password or autofill switch".into());
    }
    // Private windows never import bookmarks.
    if s.command(Command::ImportBookmarks).is_ok()
        || s.library_action(Action::ImportBookmarks(Default::default()))
            .is_ok()
    {
        return Err("A private window accepted a bookmark import".into());
    }
    if !s.toolbar_autofill_clears.is_empty() {
        return Err("A private window ran the toolbar autofill cleanup".into());
    }
    s.settings.save_passwords = true;
    s.settings.autofill = true;
    s.open_tab(navigation::HOME.into())
        .map_err(|e| e.to_string())?;
    expect_forms(s, Forms::default(), "private new tab")?;
    p.log
        .push("forms: private windows never save passwords or forms".into());
    p.done = true;
    el.exit();
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real private window"]
fn native_private_windows_never_save_passwords_or_forms() {
    let (root, _) = run("private-forms", private_forms_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

fn bookmark_fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/bookmarks")
            .join(name),
    )
    .unwrap()
}

/// A chosen file's bytes reach the library in one import with a summary; a file
/// that is not an export is refused and changes nothing.
fn import_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let _ = el;
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            p.app
                .proxy
                .send_event(Event::BookmarkFile(Ok(Some(bookmark_fixture(
                    "chrome.html",
                )))))
                .map_err(|_| "proxy")?;
            p.next();
        }
        1 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.notice.starts_with("Bookmarks imported") {
                let found: Vec<&str> = s
                    .library_data
                    .bookmarks
                    .iter()
                    .map(|b| b.url.as_str())
                    .collect();
                if found.len() != 3
                    || !s
                        .notice
                        .contains("3 added, 1 skipped as duplicates, 1 refused")
                {
                    return Err(format!("Import: {found:?}, notice {:?}", s.notice));
                }
                p.app
                    .proxy
                    .send_event(Event::BookmarkFile(Ok(Some(
                        b"<html>not bookmarks</html>".to_vec(),
                    ))))
                    .map_err(|_| "proxy")?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err(format!("The import never finished: {:?}", s.notice));
            }
        }
        2 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.notice.contains("not an exported bookmarks file") {
                if s.library_data.bookmarks.len() != 3 {
                    return Err("A refused file changed bookmarks".into());
                }
                p.log.push(
                    "import: file to library with summary; refused file changes nothing".into(),
                );
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err(format!("A refused file was not explained: {:?}", s.notice));
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_bookmark_import_reaches_the_library() {
    let (root, _) = run("import", import_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Write a 0.19-shaped session file: settings without `toolbar_autofill_cleared`
/// (or with it, when `cleared`), and website password saving on.
fn write_019_session(root: &Path, cleared: bool) {
    let mut settings = serde_json::to_value(rust_browser::settings::Settings {
        save_passwords: true,
        toolbar_autofill_cleared: cleared,
        ..Default::default()
    })
    .unwrap();
    if !cleared {
        settings
            .as_object_mut()
            .unwrap()
            .remove("toolbar_autofill_cleared");
    }
    let file = serde_json::json!({
        "version": 1,
        "tabs": [navigation::HOME],
        "active": 0,
        "settings": settings,
    });
    std::fs::write(root.join("session.json"), file.to_string()).unwrap();
}

/// Upgrading from 0.19 clears the toolbar profile's form data once, on the
/// toolbar view only, and leaves website password saving as it was.
fn toolbar_cleanup_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let s = p.app.session.as_mut().ok_or("No session")?;
    if s.settings.toolbar_autofill_cleared {
        let toolbar = s.windows[0].toolbar.webview().as_raw() as usize;
        if s.toolbar_autofill_clears != [toolbar] {
            return Err(format!(
                "Cleanup ran {} times, or not on the toolbar view",
                s.toolbar_autofill_clears.len()
            ));
        }
        if !s.settings.save_passwords {
            return Err("The cleanup changed the website password switch".into());
        }
        let page = native_security::forms(&s.tabs.items[0].content.engine.view)
            .map_err(|e| e.to_string())?;
        if !page.passwords {
            return Err("A website view lost password saving".into());
        }
        p.log.push("toolbar: leftover autofill cleared once".into());
        p.done = true;
        el.exit();
    } else if p.at.elapsed() > Duration::from_secs(15) {
        return Err(format!(
            "The toolbar cleanup never finished ({} requests)",
            s.toolbar_autofill_clears.len()
        ));
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_toolbar_autofill_is_cleared_once_on_upgrade() {
    let (root, _) = run("toolbar-cleanup", toolbar_cleanup_steps, |root, _| {
        write_019_session(root, false)
    });
    let saved = storage::load(&root.join("session.json")).unwrap();
    assert!(saved.settings.toolbar_autofill_cleared);
    assert!(saved.settings.save_passwords);
    let _ = std::fs::remove_dir_all(root);
}

fn toolbar_cleanup_done_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    if p.at.elapsed() < Duration::from_secs(3) {
        return Ok(());
    }
    let s = p.app.session.as_mut().ok_or("No session")?;
    if !s.toolbar_autofill_clears.is_empty() {
        return Err("The toolbar cleanup ran again after it had finished".into());
    }
    p.done = true;
    el.exit();
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_toolbar_autofill_cleanup_does_not_repeat() {
    let (root, _) = run(
        "toolbar-cleanup-done",
        toolbar_cleanup_done_steps,
        |root, _| write_019_session(root, true),
    );
    let _ = std::fs::remove_dir_all(root);
}

fn marker(letter: char) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/extensions")
        .join(format!("marker-{}", letter.to_ascii_lowercase()))
}

/// Ask the first tab which test markers ran on its page; the answer arrives in
/// `p.result`.
fn read_markers(p: &mut Probe) -> Result<(), String> {
    let s = p.app.session.as_mut().ok_or("No session")?;
    *p.result.lock().unwrap() = None;
    let result = p.result.clone();
    s.tabs.items[0]
        .content
        .engine
        .view
        .evaluate_script_with_callback(
            "JSON.stringify([document.documentElement.dataset.rbMarkerA||'',document.documentElement.dataset.rbMarkerB||''])",
            move |value| *result.lock().unwrap() = Some(value),
        )
        .map_err(|_| "Page script failed".to_string())
}

fn markers(p: &Probe) -> Option<String> {
    p.result.lock().unwrap().clone()
}

/// Add a test extension as if its folder had been chosen in the dialog.
fn add_marker(p: &mut Probe, letter: char) -> Result<(), String> {
    p.app
        .proxy
        .send_event(Event::Extension(native_extensions::Reply::Folder(Ok(
            Some(marker(letter)),
        ))))
        .map_err(|_| "proxy".into())
}

fn names(s: &Session) -> Vec<(String, &'static str)> {
    s.extension_rows()
        .into_iter()
        .map(|row| (row.name, row.state))
        .collect()
}

const MARKER_A: &str = "Rust Browser test marker A";
const MARKER_B: &str = "Rust Browser test marker B";

/// Add two test extensions from their folders, list them, check both run on a
/// page, turn one off and remove the other; the toolbar profile has neither.
/// Then a second browser process on the same profile must find the same state.
fn extension_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            add_marker(p, 'a')?;
            p.next();
        }
        1 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if names(s) == [(MARKER_A.to_string(), "on")] {
                add_marker(p, 'b')?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err(format!("Adding A: {:?}, notice {:?}", names(s), s.notice));
            }
        }
        2 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if names(s) == [(MARKER_A.to_string(), "on"), (MARKER_B.to_string(), "on")] {
                p.command(el, 0, Command::Navigate(format!("{url}?extensions")))?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err(format!("Adding B: {:?}, notice {:?}", names(s), s.notice));
            }
        }
        3 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            read_markers(p)?;
            p.next();
        }
        4 => {
            if let Some(found) = markers(p) {
                if found != r#""[\"on\",\"on\"]""# {
                    return Err(format!("Both extensions should run on the page: {found}"));
                }
                let s = p.app.session.as_mut().ok_or("No session")?;
                // WebView2's built-in PDF viewer is not the user's extension.
                let builtin = "mhjfbmdgcfjbbpaeojofohoefgiehjai".to_string();
                if s.command(Command::RemoveExtension(builtin.clone())).is_ok()
                    || s.command(Command::SetExtensionEnabled {
                        id: builtin,
                        enabled: false,
                    })
                    .is_ok()
                {
                    return Err("A built-in extension could be removed or turned off".into());
                }
                let a = s.settings.extensions[0].id.clone();
                let b = s.settings.extensions[1].id.clone();
                p.command(
                    el,
                    0,
                    Command::SetExtensionEnabled {
                        id: a,
                        enabled: false,
                    },
                )?;
                p.command(el, 0, Command::RemoveExtension(b))?;
                p.next();
            }
        }
        5 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if names(s) == [(MARKER_A.to_string(), "off")] {
                p.command(el, 0, Command::Reload)?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err(format!(
                    "Turning off and removing: {:?}, notice {:?}",
                    names(s),
                    s.notice
                ));
            }
        }
        6 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            read_markers(p)?;
            p.next();
        }
        7 => {
            if let Some(found) = markers(p) {
                if found != r#""[\"\",\"\"]""# {
                    return Err(format!(
                        "A turned-off or removed extension still ran: {found}"
                    ));
                }
                // The toolbar profile never has the user's extensions.
                let s = p.app.session.as_mut().ok_or("No session")?;
                s.extension_probe = None;
                native_extensions::installed(&s.windows[0].toolbar, s.proxy.clone())?;
                p.next();
            }
        }
        8 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if let Some(found) = s.extension_probe.clone() {
                if let Ok(list) = &found
                    && list
                        .iter()
                        .any(|e| e.name.starts_with("Rust Browser test marker"))
                {
                    return Err(format!(
                        "The toolbar profile has a user extension: {list:?}"
                    ));
                }
                p.log
                    .push(format!("extensions: toolbar profile reported {found:?}"));
                s.extension_added = None;
                // Refused either at once or in WebView2's answer.
                if let Err(message) =
                    native_extensions::add(&s.windows[0].toolbar, &marker('b'), s.proxy.clone())
                {
                    s.extension_added = Some(Err(message));
                }
                p.phase = 80;
                p.at = Instant::now();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("The toolbar profile was never listed".into());
            }
        }
        80 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if let Some(added) = s.extension_added.clone() {
                if added.is_ok() {
                    return Err("WebView2 accepted an extension in the toolbar profile".into());
                }
                p.app.user_event(el, Event::Command(Command::ExitBrowser));
                p.phase = 9;
                p.at = Instant::now();
            } else if p.at.elapsed() > Duration::from_secs(15) {
                return Err("Adding to the toolbar profile never answered".into());
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("The toolbar profile was never listed".into());
            }
        }
        9 if p.app.session.is_none() => {
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

/// After a restart on the same profile: A is still off, B is still gone, and
/// neither runs on a page.
fn extension_restart_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.extensions_listed {
                if names(s) != [(MARKER_A.to_string(), "off")] {
                    return Err(format!("After restart: {:?}", names(s)));
                }
                p.command(el, 0, Command::Navigate(format!("{url}?restart")))?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err("Extensions were never listed after restart".into());
            }
        }
        1 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            read_markers(p)?;
            p.next();
        }
        2 => {
            if let Some(found) = markers(p) {
                if found != r#""[\"\",\"\"]""# {
                    return Err(format!("After restart an extension ran: {found}"));
                }
                // B is not merely hidden: WebView2 no longer has it installed.
                let s = p.app.session.as_mut().ok_or("No session")?;
                s.extension_probe = None;
                native_extensions::installed(
                    &s.tabs.items[0].content.engine.view,
                    s.proxy.clone(),
                )?;
                p.next();
            }
        }
        3 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if let Some(found) = s.extension_probe.clone() {
                let list = found.map_err(|e| format!("Listing after restart failed: {e}"))?;
                let a = list.iter().find(|e| e.name == MARKER_A);
                if !a.is_some_and(|a| !a.enabled) || list.iter().any(|e| e.name == MARKER_B) {
                    return Err(format!("WebView2 after restart: {list:?}"));
                }
                p.log.push("extensions: state kept after restart".into());
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("The profile was never listed after restart".into());
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; starts a second test process for the restart"]
fn native_extensions_add_list_disable_remove_and_restart() {
    let (root, _) = run("extensions", extension_steps, |_, _| {});
    let saved = storage::load(&root.join("session.json")).unwrap();
    assert_eq!(saved.settings.extensions.len(), 1);
    assert_eq!(saved.settings.extensions[0].name, MARKER_A);
    // A real restart: a new process on the same profile.
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "app::windows_validation::extension_restart_child",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("RUST_BROWSER_EXTENSION_ROOT", &root)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "the restarted browser did not keep the extension state"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Second half of the restart check; run only by the test above.
#[test]
#[ignore = "Started by native_extensions_add_list_disable_remove_and_restart"]
fn extension_restart_child() {
    let root = PathBuf::from(
        std::env::var_os("RUST_BROWSER_EXTENSION_ROOT")
            .expect("run by native_extensions_add_list_disable_remove_and_restart"),
    );
    run_in(root, extension_restart_steps);
}

/// A private window never runs extensions and refuses to add or change them.
fn private_extension_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if !s.private {
                return Err("The fixture did not start a private window".into());
            }
            if s.command(Command::AddExtension).is_ok()
                || s.command(Command::RemoveExtension("a".repeat(32))).is_ok()
            {
                return Err("A private window accepted an extension command".into());
            }
            add_marker(p, 'a')?;
            p.command(el, 0, Command::Navigate(format!("{url}?private")))?;
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_secs(3)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            read_markers(p)?;
            p.next();
        }
        2 => {
            if let Some(found) = markers(p) {
                let s = p.app.session.as_mut().ok_or("No session")?;
                if found != r#""[\"\",\"\"]""# || !s.settings.extensions.is_empty() {
                    return Err(format!(
                        "A private window ran or kept an extension: {found}, {:?}",
                        s.settings.extensions
                    ));
                }
                // The shield never counts in a private window.
                if s.counts_blocked() || s.tabs.items[0].content.engine.blocked.count().is_some() {
                    return Err("A private window counts blocked requests".into());
                }
                s.extension_added = None;
                // Refused either at once or in WebView2's answer.
                if let Err(message) = native_extensions::add(
                    &s.tabs.items[0].content.engine.view,
                    &marker('a'),
                    s.proxy.clone(),
                ) {
                    s.extension_added = Some(Err(message));
                }
                p.next();
            }
        }
        3 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if let Some(added) = s.extension_added.clone() {
                if added.is_ok() {
                    return Err("WebView2 accepted an extension in a private window".into());
                }
                p.log
                    .push("extensions: private windows never run them".into());
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(15) {
                return Err("Adding in a private window never answered".into());
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real private window"]
fn native_private_windows_never_run_extensions() {
    let (root, _) = run("private-extensions", private_extension_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Screenshot tool for the layout review (not a gate): the toolbar view in the
/// light and dark themes at 100% and 150%, on the new-tab page (the toolbar view
/// fills the window there) and above a website. Writes PNG files to
/// `RUST_BROWSER_SHOTS` named `<label>-<page>-<theme>-<scale>.png`.
fn screenshot_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    const SHOTS: [(&str, rust_browser::settings::Theme, f64); 4] = [
        ("light", rust_browser::settings::Theme::Light, 1.0),
        ("light", rust_browser::settings::Theme::Light, 1.5),
        ("dark", rust_browser::settings::Theme::Dark, 1.0),
        ("dark", rust_browser::settings::Theme::Dark, 1.5),
    ];
    let out = PathBuf::from(std::env::var_os("RUST_BROWSER_SHOTS").ok_or("RUST_BROWSER_SHOTS")?);
    let label = std::env::var("RUST_BROWSER_SHOTS_LABEL").unwrap_or_else(|_| "shot".into());
    let url = p.url.clone();
    // Phases: 0 settle; then per page (new tab, website) and per shot: set, capture.
    let index = p.phase as usize;
    if p.at.elapsed() < Duration::from_millis(if index == 0 { 2000 } else { 900 }) {
        return Ok(());
    }
    if index > 2 * SHOTS.len() * 3 {
        p.done = true;
        el.exit();
        return Ok(());
    }
    if index == 0 {
        p.next();
        return Ok(());
    }
    let step = index - 1;
    let (page, shot, capture) = (
        step / (SHOTS.len() * 2),
        (step / 2) % SHOTS.len(),
        step % 2 == 1,
    );
    let (theme_name, theme, scale) = SHOTS[shot];
    let s = p.app.session.as_mut().ok_or("No session")?;
    let toolbar = s.windows[0].toolbar.webview();
    if !capture {
        if page == 1 && shot == 0 {
            p.command(el, 0, Command::Navigate(format!("{url}?layout")))?;
        }
        if page == 2 && shot == 0 {
            // The menu sidebar over the website.
            let s = p.app.session.as_mut().ok_or("No session")?;
            let _ = s.windows[0]
                .toolbar
                .evaluate_script("document.getElementById('more').click()");
        }
        let s = p.app.session.as_mut().ok_or("No session")?;
        s.settings.theme = theme;
        s.last_render.borrow_mut().clear();
        s.render();
        let size = s
            .window
            .inner_size()
            .to_logical::<f64>(s.window.scale_factor());
        let height = if page == 1 {
            s.chrome_height
        } else {
            size.height
        };
        let params = serde_json::json!({ "width": size.width.round() as u32,
            "height": height.round() as u32, "deviceScaleFactor": scale, "mobile": false });
        let toolbar = s.windows[0].toolbar.webview();
        // SAFETY: UI-thread call on this fixture's own toolbar view.
        unsafe {
            toolbar.CallDevToolsProtocolMethod(
                &windows::core::HSTRING::from("Emulation.setDeviceMetricsOverride"),
                &windows::core::HSTRING::from(params.to_string()),
                &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(()))),
            )
        }
        .map_err(|_| "Metrics override failed")?;
    } else {
        let name = format!(
            "{label}-{}-{theme_name}-{}.png",
            ["newtab", "website", "menu"][page],
            (scale * 100.0) as u32
        );
        let path = out.join(name);
        // SAFETY: UI-thread call on this fixture's own toolbar view.
        unsafe {
            toolbar.CallDevToolsProtocolMethod(
                &windows::core::HSTRING::from("Page.captureScreenshot"),
                &windows::core::HSTRING::from(r#"{"format":"png"}"#),
                &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |_, json| {
                    let text = json.to_string();
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
                        && let Some(data) = value["data"].as_str()
                        && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data)
                    {
                        let _ = std::fs::write(&path, bytes);
                    }
                    Ok(())
                })),
            )
        }
        .map_err(|_| "Screenshot failed")?;
    }
    p.next();
    Ok(())
}

#[test]
#[ignore = "Layout review tool: writes screenshots to RUST_BROWSER_SHOTS"]
fn layout_screenshots() {
    let (root, _) = run("shots", screenshot_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Send one DevTools protocol call to the first window's toolbar.
fn toolbar_cdp(s: &Session, method: &str, params: serde_json::Value) -> Result<(), String> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    // SAFETY: UI-thread call on this fixture's own toolbar view.
    unsafe {
        s.windows[0].toolbar.webview().CallDevToolsProtocolMethod(
            &windows::core::HSTRING::from(method),
            &windows::core::HSTRING::from(params.to_string()),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(()))),
        )
    }
    .map_err(|_| format!("DevTools call {method} failed"))
}

/// The layout has no Go button: pressing Enter in the address field still
/// navigates (the form's implicit submission, through Chromium's own key input).
fn enter_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows[0].toolbar.evaluate_script(&format!(
                "(()=>{{if(document.querySelector('#nav button[type=submit]'))throw 'Go button present';const a=document.getElementById('address');a.focus();a.value={};}})()",
                serde_json::to_string(&format!("{url}?entered")).unwrap()
            )).is_err() {
                return Err("Address field script failed".into());
            }
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_millis(300) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            for kind in ["keyDown", "keyUp"] {
                toolbar_cdp(
                    s,
                    "Input.dispatchKeyEvent",
                    serde_json::json!({ "type": kind, "key": "Enter", "code": "Enter",
                        "text": if kind == "keyDown" { "\r" } else { "" },
                        "windowsVirtualKeyCode": 13 }),
                )?;
            }
            p.next();
        }
        2 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.tabs.items[0].url.ends_with("?entered") {
                p.log
                    .push("layout: Enter in the address field navigates".into());
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err(format!("Enter did not navigate: {}", s.tabs.items[0].url));
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_address_enter_navigates_without_a_go_button() {
    let (root, _) = run("enter", enter_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Run `script` in the first window's toolbar; the answer lands in `p.result`.
fn eval_toolbar(p: &mut Probe, script: &str) -> Result<(), String> {
    let s = p.app.session.as_mut().ok_or("No session")?;
    *p.result.lock().unwrap() = None;
    let result = p.result.clone();
    s.windows[0]
        .toolbar
        .evaluate_script_with_callback(script, move |value| *result.lock().unwrap() = Some(value))
        .map_err(|_| "Toolbar script failed".to_string())
}

/// The toolbar state the polish defines, as JSON.
const POLISH_REPORT: &str = r#"(()=>{const b=id=>document.getElementById(id);const shown=e=>!!e&&!e.hidden&&e.getClientRects().length>0;const tab=document.querySelector('.tab.active .select');return JSON.stringify({reload:shown(b('reload')),stop:shown(b('stop')),status:b('status').textContent.trim(),role:b('status').getAttribute('role'),address:b('address').title,picker:b('tab-picker').tagName+'|'+b('tab-picker').getAttribute('aria-label'),pickerShown:shown(b('tab-picker')),shortcuts:shown(b('show-shortcuts'))||!!b('show-shortcuts'),tabTitle:tab?tab.title:'',tabLabel:tab?tab.getAttribute('aria-label'):''})})()"#;

fn polish_report(p: &Probe) -> Result<Option<serde_json::Value>, String> {
    let Some(text) = p.result.lock().unwrap().clone() else {
        return Ok(None);
    };
    let inner: String = serde_json::from_str(&text).map_err(|e| format!("{e}: {text}"))?;
    serde_json::from_str(&inner)
        .map(Some)
        .map_err(|e| format!("{e}: {inner}"))
}

/// The polished toolbar: reload and stop swap places while loading, the status line
/// carries only the message, the shortcuts live in the address tooltip and a menu
/// list, the all-tabs list stays a named native list, and tab tooltips put the
/// state on its own line.
fn polish_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            p.command(el, 0, Command::Navigate(format!("{url}?polish")))?;
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            eval_toolbar(p, POLISH_REPORT)?;
            p.next();
        }
        2 => {
            if let Some(r) = polish_report(p)? {
                let checks = [
                    (
                        r["reload"] == true && r["stop"] == false,
                        "idle shows reload, not stop",
                    ),
                    (r["status"] == "Ready", "the status line says only Ready"),
                    (r["role"] == "status", "the status row keeps role=status"),
                    (
                        r["address"]
                            .as_str()
                            .is_some_and(|t| t.contains("Ctrl+L") && t.contains("F7")),
                        "the address tooltip lists shortcuts",
                    ),
                    (
                        r["picker"] == "SELECT|All browser tabs" && r["pickerShown"] == true,
                        "the all-tabs list keeps its name",
                    ),
                    (r["shortcuts"] == true, "the Keyboard shortcuts item exists"),
                ];
                if let Some((_, what)) = checks.iter().find(|(ok, _)| !ok) {
                    return Err(format!("Idle toolbar: {what}: {r}"));
                }
                let s = p.app.session.as_mut().ok_or("No session")?;
                if let Some(tab) = s.tabs.active_mut() {
                    tab.content.state.loading = true;
                    tab.keep_awake = true;
                }
                s.last_render.borrow_mut().clear();
                s.render();
                p.next();
            }
        }
        3 if p.at.elapsed() > Duration::from_millis(300) => {
            eval_toolbar(p, POLISH_REPORT)?;
            p.next();
        }
        4 => {
            if let Some(r) = polish_report(p)? {
                let title = r["tabTitle"].as_str().unwrap_or("");
                if r["reload"] != false || r["stop"] != true {
                    return Err(format!("Loading toolbar should show stop, not reload: {r}"));
                }
                if !title.ends_with("\nKeep awake") || title.contains(" - ") {
                    return Err(format!(
                        "Tab tooltip should put the state on its own line: {title:?}"
                    ));
                }
                if !r["tabLabel"]
                    .as_str()
                    .is_some_and(|l| l.ends_with(", loading"))
                {
                    return Err(format!("A loading tab should say so: {r}"));
                }
                let s = p.app.session.as_mut().ok_or("No session")?;
                if let Some(tab) = s.tabs.active_mut() {
                    tab.content.state.loading = false;
                    tab.keep_awake = false;
                }
                // The shortcuts item opens its list inside the menu.
                if s.windows[0]
                    .toolbar
                    .evaluate_script("document.getElementById('more').click();document.getElementById('show-shortcuts').click()")
                    .is_err()
                {
                    return Err("Menu script failed".into());
                }
                p.next();
            }
        }
        5 if p.at.elapsed() > Duration::from_millis(300) => {
            eval_toolbar(
                p,
                "document.getElementById('show-shortcuts').getAttribute('aria-expanded')+'|'+!document.getElementById('shortcut-list').hidden+'|'+document.querySelectorAll('#shortcut-list li').length",
            )?;
            p.next();
        }
        6 => {
            if let Some(text) = p.result.lock().unwrap().clone() {
                let inner: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                let parts: Vec<&str> = inner.split('|').collect();
                if parts.len() != 3
                    || parts[0] != "true"
                    || parts[1] != "true"
                    || parts[2].parse::<usize>().unwrap_or(0) < 15
                {
                    return Err(format!("Keyboard shortcuts list did not open: {inner}"));
                }
                p.log
                    .push("toolbar polish: states, status, shortcuts and tooltips".into());
                p.done = true;
                el.exit();
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_toolbar_polish_states() {
    let (root, _) = run("polish", polish_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Whether a view is shown, as WebView2 reports it.
fn view_visible(view: &wry::WebView) -> bool {
    let mut visible = windows::core::BOOL::default();
    // SAFETY: a read-only query on this fixture's own controller.
    unsafe { view.controller().IsVisible(&mut visible) }.is_ok() && visible.as_bool()
}

/// The menu opens as a vertical sidebar: the toolbar fills the window over a still
/// of the page, the live page is hidden, and Escape restores the page.
fn sidebar_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            p.command(el, 0, Command::Navigate(format!("{url}?sidebar")))?;
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows[0]
                .toolbar
                .evaluate_script("document.getElementById('more').click()")
                .is_err()
            {
                return Err("Menu script failed".into());
            }
            p.next();
        }
        2 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows[0].menu_open {
                let size = s.window.inner_size();
                let mut bounds = windows::Win32::Foundation::RECT::default();
                // SAFETY: a read-only query on this fixture's own controller.
                unsafe { s.windows[0].toolbar.controller().Bounds(&mut bounds) }
                    .map_err(|e| e.to_string())?;
                if (bounds.bottom - bounds.top) < size.height as i32 - 2 {
                    return Err(format!(
                        "The toolbar does not fill the window: {bounds:?} in {size:?}"
                    ));
                }
                if view_visible(&s.tabs.items[0].content.engine.view) {
                    return Err("The live page stayed visible under the sidebar".into());
                }
                eval_toolbar(
                    p,
                    "(()=>{const m=document.getElementById('tab-actions').getBoundingClientRect(),i=document.getElementById('menu-snapshot');return JSON.stringify({right:Math.round(innerWidth-m.right),tall:m.height>innerHeight/2,narrow:m.width<innerWidth/2,shown:!document.getElementById('tab-actions').hidden,still:!!i&&i.src.startsWith('data:image/jpeg;base64,')})})()",
                )?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("The menu sidebar never opened".into());
            }
        }
        3 => {
            if let Some(r) = polish_report(p)? {
                if r["right"] != 0 || r["tall"] != true || r["narrow"] != true || r["shown"] != true
                {
                    return Err(format!(
                        "The menu is not a right-hand vertical sidebar: {r}"
                    ));
                }
                if r["still"] != true {
                    return Err(format!("No page still behind the sidebar: {r}"));
                }
                let s = p.app.session.as_mut().ok_or("No session")?;
                if s.windows[0]
                    .toolbar
                    .evaluate_script(
                        "document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))",
                    )
                    .is_err()
                {
                    return Err("Escape script failed".into());
                }
                p.next();
            }
        }
        4 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if !s.windows[0].menu_open && view_visible(&s.tabs.items[0].content.engine.view) {
                let mut bounds = windows::Win32::Foundation::RECT::default();
                // SAFETY: a read-only query on this fixture's own controller.
                unsafe { s.windows[0].toolbar.controller().Bounds(&mut bounds) }
                    .map_err(|e| e.to_string())?;
                if (bounds.bottom - bounds.top) as f64
                    > s.chrome_height * s.window.scale_factor() + 2.0
                {
                    return Err(format!("The toolbar did not shrink back: {bounds:?}"));
                }
                p.log.push(
                    "menu: right-hand sidebar over a page still; Escape restores the page".into(),
                );
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("Escape did not close the sidebar and restore the page".into());
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_menu_opens_as_a_sidebar() {
    let (root, _) = run("sidebar", sidebar_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Where every shown view of the first window is on screen (physical pixels):
/// toolbar, active page and the side views. Each view sits in its own host window
/// (its controller's parent), whose screen rectangle is the view's real place.
fn shown_views(s: &Session) -> Vec<(&'static str, windows::Win32::Foundation::RECT)> {
    shown_views_in(s, 0)
}

/// Bounds of every shown view of window `index` (see `shown_views`).
fn shown_views_in(
    s: &Session,
    index: usize,
) -> Vec<(&'static str, windows::Win32::Foundation::RECT)> {
    let w = &s.windows[index];
    let mut views: Vec<(&'static str, &wry::WebView)> = vec![("toolbar", &w.toolbar)];
    if let Some(tab) = w.tabs.active() {
        views.push(("page", &tab.content.engine.view));
    }
    if let Some(side) = &w.tabs_view {
        views.push(("tabs", &side.view));
    }
    if let Some(side) = &w.library_view {
        views.push(("library", &side.view));
    }
    views
        .into_iter()
        .filter(|(_, v)| view_visible(v))
        .filter_map(|(name, v)| {
            let mut host = windows::Win32::Foundation::HWND::default();
            let mut r = windows::Win32::Foundation::RECT::default();
            // SAFETY: read-only queries on this fixture's own controller and its host.
            unsafe {
                v.controller().ParentWindow(&mut host).ok()?;
                windows::Win32::UI::WindowsAndMessaging::GetWindowRect(host, &mut r).ok()?;
            }
            Some((name, r))
        })
        .collect()
}

/// No two shown views overlap (the stacking order of the toolbar and page windows
/// is not kept by Windows, so an overlap could hide either).
fn check_no_overlap(s: &Session, step: &str) -> Result<(), String> {
    check_no_overlap_in(s, 0, step)
}

fn check_no_overlap_in(s: &Session, index: usize, step: &str) -> Result<(), String> {
    let views = shown_views_in(s, index);
    for (i, (a, ra)) in views.iter().enumerate() {
        for (b, rb) in &views[i + 1..] {
            let x = ra.right.min(rb.right) - ra.left.max(rb.left);
            let y = ra.bottom.min(rb.bottom) - ra.top.max(rb.top);
            if x > 1 && y > 1 {
                return Err(format!("{step}: {a} {ra:?} overlaps {b} {rb:?}"));
            }
        }
    }
    Ok(())
}

fn named(
    views: &[(&'static str, windows::Win32::Foundation::RECT)],
    name: &str,
) -> Option<windows::Win32::Foundation::RECT> {
    views.iter().find(|(n, _)| *n == name).map(|(_, r)| *r)
}

/// Run `script` in the first window's library view; the answer lands in `p.result`.
fn eval_library(p: &mut Probe, script: &str) -> Result<(), String> {
    let s = p.app.session.as_mut().ok_or("No session")?;
    *p.result.lock().unwrap() = None;
    let result = p.result.clone();
    s.windows[0]
        .library_view
        .as_ref()
        .ok_or("No library view")?
        .view
        .evaluate_script_with_callback(script, move |value| *result.lock().unwrap() = Some(value))
        .map_err(|_| "Library script failed".to_string())
}

/// Bookmarks, history and downloads open beside a live page: the page shrinks,
/// stays visible and still navigates; the column resizes, toggles and gives way to
/// a full panel; nothing overlaps.
fn library_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            p.command(el, 0, Command::Navigate(format!("{url}?library")))?;
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            p.command(el, 0, Command::ShowPanel(Panel::History))?;
            p.next();
        }
        2 if p.at.elapsed() > Duration::from_millis(800) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            check_no_overlap(s, "library open")?;
            let views = shown_views(s);
            let (page, library) = (named(&views, "page"), named(&views, "library"));
            let (Some(page), Some(library)) = (page, library) else {
                return Err(format!("Page and library should both show: {views:?}"));
            };
            let toolbar = named(&views, "toolbar").ok_or("No toolbar")?;
            if (library.right - toolbar.right).abs() > 2 || (page.right - library.left).abs() > 2 {
                return Err(format!(
                    "The library should sit at the right, beside the page: {views:?}"
                ));
            }
            if s.panel.is_some() || s.windows[0].library_panel != Some(Panel::History) {
                return Err("History did not open in the library column".into());
            }
            eval_library(
                p,
                "JSON.stringify([document.documentElement.dataset.role,document.getElementById('manager').hidden,document.getElementById('manager-title').textContent,getComputedStyle(document.getElementById('chrome')).display])",
            )?;
            p.next();
        }
        3 => {
            let answer = p.result.lock().unwrap().clone();
            if let Some(text) = answer {
                let inner: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                if inner != r#"["library",false,"Browsing history","none"]"# {
                    return Err(format!("The library view shows the wrong part: {inner}"));
                }
                // The page is usable while the library is open.
                p.command(el, 0, Command::Navigate(format!("{url}?beside")))?;
                p.next();
            }
        }
        4 if p.at.elapsed() > Duration::from_secs(1)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if !s.tabs.items[0].url.ends_with("?beside") || s.windows[0].library_panel.is_none() {
                return Err("The page did not navigate beside the library".into());
            }
            p.command(el, 0, Command::SetLibraryWidth(500.0))?;
            p.next();
        }
        5 if p.at.elapsed() > Duration::from_millis(500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let views = shown_views(s);
            let library = named(&views, "library").ok_or("No library")?;
            let want = (500.0 * s.window.scale_factor()).round() as i32;
            if ((library.right - library.left) - want).abs() > 2 {
                return Err(format!("The library did not resize to 500: {views:?}"));
            }
            check_no_overlap(s, "library resized")?;
            // A full panel takes the window; the library comes back after it.
            p.command(el, 0, Command::ShowPanel(Panel::Settings))?;
            p.next();
        }
        6 if p.at.elapsed() > Duration::from_millis(500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let views = shown_views(s);
            if named(&views, "library").is_some() || named(&views, "page").is_some() {
                return Err(format!(
                    "A full panel should hide the page and library: {views:?}"
                ));
            }
            check_no_overlap(s, "full panel")?;
            p.command(el, 0, Command::ClosePanel)?;
            p.next();
        }
        7 if p.at.elapsed() > Duration::from_millis(500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let views = shown_views(s);
            if named(&views, "library").is_none() || named(&views, "page").is_none() {
                return Err(format!(
                    "The library did not return after the full panel: {views:?}"
                ));
            }
            p.command(el, 0, Command::ToggleLibrary)?;
            p.next();
        }
        8 if p.at.elapsed() > Duration::from_millis(500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let views = shown_views(s);
            let page = named(&views, "page").ok_or("No page")?;
            let toolbar = named(&views, "toolbar").ok_or("No toolbar")?;
            if named(&views, "library").is_some() || (page.right - toolbar.right).abs() > 2 {
                return Err(format!(
                    "Closing the library should give the page the width back: {views:?}"
                ));
            }
            p.log.push(
                "library: beside a live page, resized, toggled, gives way to a full panel".into(),
            );
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_library_sidebar_beside_a_live_page() {
    let (root, _) = run("library", library_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

const BLOCKER: &str = "Rust Browser test blocker";

/// The shield panel's state in the first window's toolbar, as JSON.
const SHIELD_REPORT: &str = r#"(()=>{const b=id=>document.getElementById(id);const r=b('shield').getBoundingClientRect(),l=b('location').getBoundingClientRect();return JSON.stringify({open:!b('shield-panel').hidden,expanded:b('shield').getAttribute('aria-expanded'),on:b('shield').dataset.on,badge:b('shield-count').textContent,label:b('shield').getAttribute('aria-label'),state:b('shield-state').textContent,count:b('shield-count-text').hidden?'':b('shield-count-text').textContent,limits:b('shield-limits').textContent,inAddress:r.right<=l.right+1&&r.left>=l.left-1,focus:document.activeElement&&document.activeElement.id})})()"#;

fn shield_report(p: &Probe) -> Result<Option<serde_json::Value>, String> {
    let Some(text) = p.result.lock().unwrap().clone() else {
        return Ok(None);
    };
    let inner: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    serde_json::from_str(&inner)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The shield counts exactly the requests the test blocker stops on a page, shows
/// that count at the end of the address line and in its panel (which takes its own
/// space above the page), says what it cannot do, closes with Escape, and starts
/// again at zero on the next page.
fn shield_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            // WebView2 writes indexed rules into the folder: use a copy.
            let folder = p.root.join("blocker");
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let source =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions/blocker");
            for name in ["manifest.json", "rules.json"] {
                std::fs::copy(source.join(name), folder.join(name)).map_err(|e| e.to_string())?;
            }
            // Before any extension: nothing is counted and the panel says so.
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.tabs.items[0].content.engine.blocked.count().is_some() {
                return Err("Pages count before an extension is added".into());
            }
            p.app
                .proxy
                .send_event(Event::Extension(native_extensions::Reply::Folder(Ok(
                    Some(folder),
                ))))
                .map_err(|_| "proxy")?;
            p.next();
        }
        1 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let both = [(BLOCKER.to_string(), "on"), (MARKER_A.to_string(), "on")];
            if names(s) == both {
                p.marks.clear();
                p.command(el, 0, Command::Navigate(format!("{url}shield.html")))?;
                p.next();
            } else if names(s) == [(BLOCKER.to_string(), "on")] && p.marks.is_empty() {
                // A second extension that does not ask to block requests.
                p.marks.push(1);
                add_marker(p, 'a')?;
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err(format!(
                    "Adding the blocker: {:?}, notice {:?}",
                    names(s),
                    s.notice
                ));
            }
        }
        2 if p.at.elapsed() > Duration::from_secs(1)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let count = s.tabs.items[0].content.engine.blocked.count();
            if count == Some(2) {
                // The tab existed before the extension was added; it counts anyway.
                p.log.push("shield: 2 of 2 blocked requests counted".into());
                eval_toolbar(p, "document.getElementById('shield').click()")?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(15) {
                return Err(format!("The shield counted {count:?}, not 2"));
            }
        }
        3 if p.at.elapsed() > Duration::from_millis(800) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            check_no_overlap(s, "shield panel open")?;
            let views = shown_views(s);
            let (toolbar, page) = (named(&views, "toolbar"), named(&views, "page"));
            let (Some(toolbar), Some(page)) = (toolbar, page) else {
                return Err(format!(
                    "The page should stay shown under the panel: {views:?}"
                ));
            };
            if (page.top - toolbar.bottom).abs() > 2 {
                return Err(format!(
                    "The page should move down below the panel: {views:?}"
                ));
            }
            p.marks.push((toolbar.bottom - toolbar.top).max(0) as usize);
            eval_toolbar(p, SHIELD_REPORT)?;
            p.next();
        }
        4 => {
            let Some(r) = shield_report(p)? else {
                return Ok(());
            };
            let text = |key: &str| r[key].as_str().unwrap_or("").to_string();
            if r["open"] != true
                || text("expanded") != "true"
                || text("on") != "true"
                || text("badge") != "2"
                || r["inAddress"] != true
                || !text("label").contains("2 blocked")
                || !text("state").contains(&format!("Can block requests: {BLOCKER}."))
                || !text("state").contains(&format!(
                    "On, but not asking to block requests: {MARKER_A}."
                ))
                || !text("count").starts_with("2 requests blocked on this page")
                || !text("limits").contains("cannot turn blocking off for one site")
                || text("focus") != "shield-panel"
            {
                return Err(format!("The shield panel is wrong: {r}"));
            }
            eval_toolbar(
                p,
                "document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))",
            )?;
            p.next();
        }
        5 if p.at.elapsed() > Duration::from_millis(600) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let views = shown_views(s);
            let toolbar = named(&views, "toolbar").ok_or("No toolbar")?;
            let open = p.marks.first().copied().unwrap_or(0);
            if (toolbar.bottom - toolbar.top).max(0) as usize >= open {
                return Err(format!(
                    "Closing the panel should give the page its room back: {views:?}"
                ));
            }
            check_no_overlap(s, "shield panel closed")?;
            eval_toolbar(p, SHIELD_REPORT)?;
            p.next();
        }
        6 => {
            let Some(r) = shield_report(p)? else {
                return Ok(());
            };
            if r["open"] != false || r["focus"] != "shield" {
                return Err(format!(
                    "Escape should close the panel and return focus: {r}"
                ));
            }
            p.command(el, 0, Command::Navigate(format!("{url}?after-shield")))?;
            p.next();
        }
        7 if p.at.elapsed() > Duration::from_secs(1)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let count = s.tabs.items[0].content.engine.blocked.count();
            if count != Some(0) {
                return Err(format!(
                    "The count should start again on a new page: {count:?}"
                ));
            }
            eval_toolbar(p, SHIELD_REPORT)?;
            p.next();
        }
        8 => {
            let Some(r) = shield_report(p)? else {
                return Ok(());
            };
            // The toolbar shows the engine's count for this page: none.
            if r["badge"] != ""
                || r["label"].as_str().unwrap_or("").contains("blocked")
                || !r["count"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("0 requests blocked")
            {
                return Err(format!("The shield should show 0 on the new page: {r}"));
            }
            // Turned off, the extension blocks nothing and the shield says so.
            let s = p.app.session.as_mut().ok_or("No session")?;
            let id = s.settings.extensions[0].id.clone();
            p.command(el, 0, Command::SetExtensionEnabled { id, enabled: false })?;
            p.next();
        }
        9 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if names(s) == [(BLOCKER.to_string(), "off"), (MARKER_A.to_string(), "on")] {
                p.command(el, 0, Command::Navigate(format!("{url}shield.html?off")))?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err(format!("Turning the blocker off: {:?}", names(s)));
            }
        }
        10 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let count = s.tabs.items[0].content.engine.blocked.count();
            if count != Some(0) {
                return Err(format!("A turned-off extension blocked {count:?}"));
            }
            eval_toolbar(p, SHIELD_REPORT)?;
            p.next();
        }
        11 => {
            let Some(r) = shield_report(p)? else {
                return Ok(());
            };
            // Only an extension that does not ask to block requests is on.
            let state = r["state"].as_str().unwrap_or("");
            if r["on"] != "false"
                || r["badge"] != ""
                || !state.contains("No extension on in this window asks to block requests")
                || !state.contains(MARKER_A)
                || state.contains(BLOCKER)
            {
                return Err(format!("The shield should say no blocker is on: {r}"));
            }
            p.log
                .push("shield: panel beside the page, Escape, reset, off".into());
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_shield_counts_what_an_extension_blocks() {
    let (root, _) = run("shield", shield_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Load a page of 300 requests 12 times, alternating between a tab that counts
/// blocked requests and one that does not, and log both load times.
fn shield_cost_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    const ROUNDS: u32 = 12;
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            p.command(el, 0, Command::NewTab)?;
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_secs(1) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let engine = &s.tabs.items[1].content.engine;
            native_shield::watch(
                &engine.view,
                engine.blocked.clone(),
                s.proxy.clone(),
                s.tabs.items[1].id,
            )
            .map_err(|e| e.to_string())?;
            p.next();
        }
        n if (2..2 + 2 * ROUNDS).contains(&n)
            && n % 2 == 0
            && p.at.elapsed() > Duration::from_millis(500) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let index = ((n / 2) % 2) as usize;
            let id = s.tabs.items[index].id;
            p.command(el, 0, Command::SelectTab(id))?;
            p.command(el, 0, Command::Navigate(format!("{url}many.html?{n}")))?;
            p.next();
        }
        n if (2..2 + 2 * ROUNDS).contains(&n)
            && n % 2 == 1
            && p.at.elapsed() > Duration::from_millis(200)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let ms = p.at.elapsed().as_millis() as usize;
            p.log.push(format!(
                "{} {ms}",
                if (n / 2) % 2 == 1 {
                    "counting"
                } else {
                    "plain"
                }
            ));
            p.marks.push(ms);
            p.next();
        }
        n if n == 2 + 2 * ROUNDS => {
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Measurement only; run by hand with --nocapture"]
fn native_shield_cost_probe() {
    let (root, _) = run("shield-cost", shield_cost_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Run `script` in window `index`'s tab column; the answer lands in `p.result`.
fn eval_tabs_column(p: &mut Probe, index: usize, script: &str) -> Result<(), String> {
    let s = p.app.session.as_mut().ok_or("No session")?;
    *p.result.lock().unwrap() = None;
    let result = p.result.clone();
    s.windows[index]
        .tabs_view
        .as_ref()
        .ok_or("No tab column")?
        .view
        .evaluate_script_with_callback(script, move |value| *result.lock().unwrap() = Some(value))
        .map_err(|_| "Tab column script failed".to_string())
}

fn answer(p: &Probe) -> Result<Option<serde_json::Value>, String> {
    let Some(text) = p.result.lock().unwrap().clone() else {
        return Ok(None);
    };
    let inner: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    serde_json::from_str(&inner)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Drag with the left mouse button inside `view`, from and to points given in its
/// CSS pixels. The messages go only to this fixture's own render widget for the
/// view; no global input or cursor movement is used.
fn drag_in(
    view: &wry::WebView,
    scale: f64,
    from: (f64, f64),
    to: (f64, f64),
) -> Result<(), String> {
    use windows::Win32::{
        Foundation::{HWND, LPARAM, RECT, WPARAM},
        UI::WindowsAndMessaging::{
            EnumChildWindows, GetClassNameW, GetWindowRect, IsWindowVisible, SendMessageW,
            WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        },
    };
    unsafe extern "system" fn find(hwnd: HWND, state: LPARAM) -> windows::core::BOOL {
        let mut name = [0u16; 128];
        // SAFETY: synchronous enumeration with a live, stack-owned output list.
        unsafe {
            let len = GetClassNameW(hwnd, &mut name) as usize;
            if String::from_utf16_lossy(&name[..len]) == "Chrome_RenderWidgetHostHWND"
                && IsWindowVisible(hwnd).as_bool()
            {
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    (*(state.0 as *mut Vec<(HWND, RECT)>)).push((hwnd, rect));
                }
            }
        }
        windows::core::BOOL(1)
    }
    let point = |(x, y): (f64, f64)| {
        let (x, y) = ((x * scale).round() as i32, (y * scale).round() as i32);
        LPARAM(((x as u16 as u32) | ((y as u16 as u32) << 16)) as isize)
    };
    // SAFETY: the controller, its descendants and the synchronous messages belong
    // only to this disposable fixture; enumeration ends before the list is dropped.
    unsafe {
        let controller = view.controller();
        let mut parent = HWND::default();
        controller
            .ParentWindow(&mut parent)
            .map_err(|e| e.to_string())?;
        let mut place = RECT::default();
        GetWindowRect(parent, &mut place).map_err(|e| e.to_string())?;
        let mut widgets: Vec<(HWND, RECT)> = Vec::new();
        let _ = EnumChildWindows(
            Some(parent),
            Some(find),
            LPARAM(&mut widgets as *mut _ as isize),
        );
        // The column's own widget fills its host window.
        let (hwnd, _) = widgets
            .into_iter()
            .find(|(_, r)| (r.left - place.left).abs() <= 2 && (r.top - place.top).abs() <= 2)
            .ok_or("The tab column's render widget was not found")?;
        const MK_LBUTTON: usize = 1;
        let _ = SendMessageW(hwnd, WM_MOUSEMOVE, Some(WPARAM(0)), Some(point(from)));
        let _ = SendMessageW(
            hwnd,
            WM_LBUTTONDOWN,
            Some(WPARAM(MK_LBUTTON)),
            Some(point(from)),
        );
        for step in 1..=8 {
            let t = f64::from(step) / 8.0;
            let at = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            let _ = SendMessageW(
                hwnd,
                WM_MOUSEMOVE,
                Some(WPARAM(MK_LBUTTON)),
                Some(point(at)),
            );
        }
        let _ = SendMessageW(hwnd, WM_LBUTTONUP, Some(WPARAM(0)), Some(point(to)));
    }
    Ok(())
}

/// The tab column's state, as JSON.
const COLUMN_REPORT: &str = r#"(()=>{const tabs=[...document.querySelectorAll('#tabs .tab')];const rect=e=>{const r=e.getBoundingClientRect();return [r.left,r.top,r.width,r.height]};return JSON.stringify({role:document.documentElement.dataset.role,orientation:document.getElementById('tabs').getAttribute('aria-orientation'),count:tabs.length,titles:tabs.map(t=>t.querySelector('.select').textContent),labels:tabs.map(t=>t.querySelector('.select').getAttribute('aria-label')),rects:tabs.map(rect),nav:getComputedStyle(document.getElementById('nav')).display,focus:document.hasFocus()&&document.activeElement?document.activeElement.getAttribute('role'):null,selected:document.activeElement?document.activeElement.getAttribute('aria-selected'):null})})()"#;

/// The tab column in each window: placed at the left beside the toolbar and page,
/// never over them; the same tab actions, Up and Down keys, focus, pinned tabs
/// and drag reordering; switching it off gives both windows the strip back.
fn vertical_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            p.command(el, 0, Command::Navigate(format!("{url}?first")))?;
            p.command(el, 0, Command::NewTab)?;
            p.command(el, 0, Command::Navigate(format!("{url}?second")))?;
            p.app
                .proxy
                .send_event(Event::Command(Command::NewWindow))
                .map_err(|_| "proxy")?;
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.len() == 2 && s.windows.iter().all(loaded)) =>
        {
            p.command(el, 1, Command::Navigate(format!("{url}?window-two")))?;
            p.command(el, 0, Command::SetVerticalTabs(true))?;
            p.next();
        }
        2 if p.at.elapsed() > Duration::from_millis(1500)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if !s.settings.vertical_tabs {
                return Err("The setting did not change".into());
            }
            for index in 0..2 {
                check_no_overlap_in(s, index, &format!("vertical, window {index}"))?;
                let views = shown_views_in(s, index);
                let (Some(tabs), Some(toolbar), Some(page)) = (
                    named(&views, "tabs"),
                    named(&views, "toolbar"),
                    named(&views, "page"),
                ) else {
                    return Err(format!(
                        "Window {index} should show the column, toolbar and page: {views:?}"
                    ));
                };
                let want = (240.0 * s.windows[index].window.scale_factor()).round() as i32;
                let host = s.windows[index]
                    .window
                    .inner_position()
                    .map_err(|e| e.to_string())?;
                if (tabs.left - host.x).abs() > 2
                    || ((tabs.right - tabs.left) - want).abs() > 2
                    || (toolbar.left - tabs.right).abs() > 2
                    || (page.left - tabs.right).abs() > 2
                {
                    return Err(format!(
                        "Window {index}: the column should sit at the left, 240 wide, beside the toolbar and page: {views:?}"
                    ));
                }
            }
            eval_tabs_column(p, 0, COLUMN_REPORT)?;
            p.next();
        }
        3 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            let s = p.app.session.as_mut().ok_or("No session")?;
            if r["role"] != "tabs"
                || r["orientation"] != "vertical"
                || r["count"] != 2
                || r["nav"] != "none"
            {
                return Err(format!("The column shows the wrong part: {r}"));
            }
            // The strip is gone from the toolbar.
            let active = s.windows[0].tabs.active().map(|t| t.id).ok_or("No tab")?;
            p.marks = vec![active as usize];
            eval_toolbar(
                p,
                "JSON.stringify(getComputedStyle(document.getElementById('tabrow')).display)",
            )?;
            p.next();
        }
        4 => {
            let Some(text) = p.result.lock().unwrap().clone() else {
                return Ok(());
            };
            if text != r#""\"none\"""# {
                return Err(format!("The toolbar still shows the tab strip: {text}"));
            }
            // Keyboard: focus moves to the column, and Down selects the next tab.
            p.command(el, 0, Command::FocusTabs)?;
            p.next();
        }
        5 if p.at.elapsed() > Duration::from_millis(500) => {
            eval_tabs_column(p, 0, COLUMN_REPORT)?;
            p.next();
        }
        6 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            if r["focus"] != "tab" || r["selected"] != "true" {
                return Err(format!(
                    "Focus should be on the active tab in the column: {r}"
                ));
            }
            eval_tabs_column(
                p,
                0,
                "document.activeElement.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));JSON.stringify(0)",
            )?;
            p.next();
        }
        7 if p.at.elapsed() > Duration::from_millis(500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let before = p.marks[0];
            let now = s.windows[0].tabs.active().map(|t| t.id).ok_or("No tab")?;
            let expected = s
                .tabs
                .items
                .iter()
                .position(|t| t.id as usize == before)
                .map(|i| s.windows[0].tabs.items[(i + 1) % s.windows[0].tabs.items.len()].id);
            if Some(now) != expected {
                return Err(format!(
                    "Down should select the next tab: {before} -> {now}"
                ));
            }
            // Pinned tabs keep their full title in the column.
            p.command(el, 0, Command::PinActive)?;
            p.next();
        }
        8 if p.at.elapsed() > Duration::from_millis(500) => {
            eval_tabs_column(p, 0, COLUMN_REPORT)?;
            p.next();
        }
        9 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            let s = p.app.session.as_mut().ok_or("No session")?;
            let pinned = s
                .tabs
                .items
                .iter()
                .position(|t| t.pinned)
                .ok_or("Nothing pinned")?;
            let title = r["titles"][pinned].as_str().unwrap_or("");
            let label = r["labels"][pinned].as_str().unwrap_or("");
            if title != s.windows[0].tabs.items[pinned].title
                || title.chars().count() <= 5
                || !label.ends_with(", pinned")
            {
                return Err(format!("A pinned tab should show its whole title: {r}"));
            }
            // Unpin, then drag the second tab above the first.
            p.command(el, 0, Command::PinActive)?;
            p.next();
        }
        10 if p.at.elapsed() > Duration::from_millis(500) => {
            eval_tabs_column(p, 0, COLUMN_REPORT)?;
            p.next();
        }
        11 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            let s = p.app.session.as_mut().ok_or("No session")?;
            p.marks = s.windows[0]
                .tabs
                .items
                .iter()
                .map(|t| t.id as usize)
                .collect();
            let rect = |i: usize| -> Result<(f64, f64), String> {
                let a = &r["rects"][i];
                let (x, y, w, h) = (a[0].as_f64(), a[1].as_f64(), a[2].as_f64(), a[3].as_f64());
                match (x, y, w, h) {
                    (Some(x), Some(y), Some(w), Some(h)) => Ok((x + w / 3.0, y + h / 2.0)),
                    _ => Err(format!("No tab rectangle: {r}")),
                }
            };
            let (from, to) = (rect(1)?, rect(0)?);
            let scale = s.windows[0].window.scale_factor();
            let view = &s.windows[0].tabs_view.as_ref().ok_or("No column")?.view;
            drag_in(view, scale, from, to)?;
            p.next();
        }
        12 if p.at.elapsed() > Duration::from_millis(800) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let now: Vec<usize> = s.windows[0]
                .tabs
                .items
                .iter()
                .map(|t| t.id as usize)
                .collect();
            let want = vec![p.marks[1], p.marks[0]];
            if now != want {
                return Err(format!(
                    "Dragging should move the second tab first: {:?} -> {now:?}",
                    p.marks
                ));
            }
            p.command(el, 1, Command::SetVerticalTabs(false))?;
            p.next();
        }
        13 if p.at.elapsed() > Duration::from_millis(1500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            for index in 0..2 {
                check_no_overlap_in(s, index, &format!("horizontal, window {index}"))?;
                let views = shown_views_in(s, index);
                let toolbar = named(&views, "toolbar").ok_or("No toolbar")?;
                let page = named(&views, "page").ok_or("No page")?;
                if named(&views, "tabs").is_some() || (page.left - toolbar.left).abs() > 2 {
                    return Err(format!(
                        "Window {index} should give the page its width back: {views:?}"
                    ));
                }
            }
            if s.settings.vertical_tabs {
                return Err("The setting did not turn off".into());
            }
            eval_toolbar(
                p,
                "JSON.stringify(getComputedStyle(document.getElementById('tabrow')).display)",
            )?;
            p.next();
        }
        14 => {
            let Some(text) = p.result.lock().unwrap().clone() else {
                return Ok(());
            };
            if text == r#""\"none\"""# {
                return Err("The tab strip did not come back".into());
            }
            p.log
                .push("vertical tabs: two windows, keys, focus, pin, drag, off".into());
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_vertical_tabs_in_two_windows() {
    let (root, _) = run("vertical", vertical_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// The new-tab tiles, as JSON.
const TILES_REPORT: &str = r#"(()=>{const cards=[...document.querySelectorAll('#favorite-list .favorite-card')];const m=document.getElementById('manager');return JSON.stringify({shown:!document.getElementById('newtab-body').hidden,background:m.dataset.background,image:getComputedStyle(m).backgroundImage,picker:document.getElementById('home-background').value,tiles:cards.map(c=>{const t=c.querySelector('.tile'),i=t&&t.querySelector('img');return {icon:i?i.getAttribute('src').slice(0,22):'',letter:i?'':t.textContent,hidden:t.getAttribute('aria-hidden'),name:c.querySelector('.visit span:last-child').textContent}})})})()"#;

/// Favourite tiles show the icon saved from a visit to their site, or a letter;
/// the background comes from the theme's colours and is saved as a setting.
fn tiles_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let other = url.replace("127.0.0.1", "localhost");
            let s = p.app.session.as_mut().ok_or("No session")?;
            s.command(Command::AddFavorite {
                title: "Tile fixture".into(),
                url: format!("{url}tile.html"),
            })
            .map_err(|e| e.to_string())?;
            s.command(Command::AddFavorite {
                title: "letter fixture".into(),
                url: format!("{other}?letter"),
            })
            .map_err(|e| e.to_string())?;
            p.command(el, 0, Command::Navigate(format!("{url}tile.html")))?;
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_secs(1) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let favorites = &s.settings.favorites;
            if !favorites[0].icon.is_empty() {
                let favicon = &s.windows[0].tabs.items[0].content.state.favicon;
                if favorites[0].icon != *favicon || !favorites[1].icon.is_empty() {
                    return Err("Only the visited site's favourite should keep its icon".into());
                }
                s.settings.validate()?;
                p.command(el, 0, Command::Home)?;
                p.command(
                    el,
                    0,
                    Command::SetNewtabBackground(rust_browser::settings::Background::Dusk),
                )?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(15) {
                return Err("The favourite never got its site's icon".into());
            }
        }
        2 if p.at.elapsed() > Duration::from_millis(800) => {
            eval_toolbar(p, TILES_REPORT)?;
            p.next();
        }
        3 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            let tile = |i: usize, key: &str| r["tiles"][i][key].as_str().unwrap_or("").to_string();
            if r["shown"] != true
                || r["background"] != "dusk"
                || r["picker"] != "dusk"
                || !r["image"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("linear-gradient")
                || tile(0, "icon") != "data:image/png;base64,"
                || tile(0, "name") != "Tile fixture"
                || !tile(1, "icon").is_empty()
                || tile(1, "letter") != "L"
                || tile(1, "hidden") != "true"
            {
                return Err(format!("The new-tab tiles are wrong: {r}"));
            }
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.settings.newtab_background != rust_browser::settings::Background::Dusk {
                return Err("The background was not saved".into());
            }
            p.log
                .push("new tab: icon tile, letter tile, dusk background".into());
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_newtab_tiles_and_background() {
    let (root, _) = run("tiles", tiles_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// A request the blocker stops inside a frame from another site (localhost in a
/// 127.0.0.1 page) is not in the page's count: WebView2 reports such frames'
/// requests to their own DevTools target. The panel says so.
fn shield_frame_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let folder = p.root.join("blocker");
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let source =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions/blocker");
            for name in ["manifest.json", "rules.json"] {
                std::fs::copy(source.join(name), folder.join(name)).map_err(|e| e.to_string())?;
            }
            p.app
                .proxy
                .send_event(Event::Extension(native_extensions::Reply::Folder(Ok(
                    Some(folder),
                ))))
                .map_err(|_| "proxy")?;
            p.next();
        }
        1 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if names(s) == [(BLOCKER.to_string(), "on")] {
                p.command(el, 0, Command::Navigate(format!("{url}shield-frame.html")))?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err(format!("Adding the blocker: {:?}", names(s)));
            }
        }
        2 if p.at.elapsed() > Duration::from_secs(3)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            *p.result.lock().unwrap() = None;
            let result = p.result.clone();
            s.tabs.items[0]
                .content
                .engine
                .view
                .evaluate_script_with_callback(
                    "document.documentElement.dataset.frame||'none'",
                    move |v| *result.lock().unwrap() = Some(v),
                )
                .map_err(|_| "eval")?;
            p.next();
        }
        3 => {
            if let Some(v) = p.result.lock().unwrap().clone() {
                let s = p.app.session.as_mut().ok_or("No session")?;
                let count = s.tabs.items[0].content.engine.blocked.count();
                // The frame loaded and its script was stopped, yet nothing is counted.
                if v != r#""stopped:localhost""# || count != Some(0) {
                    return Err(format!(
                        "Frame from another site: count {count:?}, frame {v}"
                    ));
                }
                p.log
                    .push("shield: a cross-site frame's blocked request is not counted".into());
                p.done = true;
                el.exit();
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_shield_does_not_count_frames_from_other_sites() {
    let (root, _) = run("shield-frame", shield_frame_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// The open shield panel, with two extensions listed, in a narrow window: all of
/// it stays inside the toolbar view (scrolling within itself if needed), and the
/// page starts below it.
fn shield_narrow_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let folder = p.root.join("blocker");
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let source =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions/blocker");
            for name in ["manifest.json", "rules.json"] {
                std::fs::copy(source.join(name), folder.join(name)).map_err(|e| e.to_string())?;
            }
            p.app
                .proxy
                .send_event(Event::Extension(native_extensions::Reply::Folder(Ok(
                    Some(folder),
                ))))
                .map_err(|_| "proxy")?;
            p.next();
        }
        1 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if names(s) == [(BLOCKER.to_string(), "on"), (MARKER_A.to_string(), "on")] {
                let _ = s
                    .window
                    .request_inner_size(winit::dpi::LogicalSize::new(520.0, 700.0));
                p.command(el, 0, Command::Navigate(format!("{url}shield.html")))?;
                p.next();
            } else if names(s) == [(BLOCKER.to_string(), "on")] && p.marks.is_empty() {
                p.marks.push(1);
                add_marker(p, 'a')?;
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err(format!("Adding extensions: {:?}", names(s)));
            }
        }
        2 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            eval_toolbar(p, "document.getElementById('shield').click()")?;
            p.next();
        }
        3 if p.at.elapsed() > Duration::from_millis(1200) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            check_no_overlap(s, "narrow shield panel")?;
            let views = shown_views(s);
            let (Some(toolbar), Some(page)) = (named(&views, "toolbar"), named(&views, "page"))
            else {
                return Err(format!("Toolbar and page should show: {views:?}"));
            };
            if (page.top - toolbar.bottom).abs() > 2 {
                return Err(format!("The page should start below the panel: {views:?}"));
            }
            eval_toolbar(
                p,
                "(()=>{const panel=document.getElementById('shield-panel').getBoundingClientRect(),chrome=document.getElementById('chrome').getBoundingClientRect(),list=document.getElementById('shield-panel');return JSON.stringify({last:lastChromeHeight,width:innerWidth,height:innerHeight,panelBottom:panel.bottom,chromeBottom:chrome.bottom,scrolls:list.scrollHeight>list.clientHeight,overflow:getComputedStyle(list).overflowY})})()",
            )?;
            p.next();
        }
        4 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            let num = |k: &str| r[k].as_f64().unwrap_or(f64::MAX);
            // Everything of the toolbar, panel included, fits in the toolbar view;
            // a panel taller than its room scrolls within itself.
            if num("width") > 700.0
                || num("panelBottom") > num("height") + 1.0
                || num("chromeBottom") > num("height") + 1.0
                || (r["scrolls"] == true && r["overflow"] != "auto")
            {
                let s = p.app.session.as_ref().ok_or("No session")?;
                return Err(format!(
                    "The shield panel is cut off: {r} (chrome height {}, notice {:?})",
                    s.chrome_height, s.notice
                ));
            }
            p.log.push(format!("shield: narrow panel fits {r}"));
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_shield_panel_fits_a_narrow_window() {
    let (root, _) = run("shield-narrow", shield_narrow_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}
