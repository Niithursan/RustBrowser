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
            return;
        }
        if self.started.elapsed() > self.limit {
            self.failure = Some(format!("Windows fixture timed out at phase {}", self.phase));
            el.exit();
            return;
        }
        let ready =
            self.app.session.as_ref().is_some_and(|s| s.ready) || self.app.closing.is_some();
        if ready {
            let steps = self.steps;
            if let Err(error) = steps(self, el) {
                self.failure = Some(error);
                el.exit();
                return;
            }
        }
        if !self.done {
            el.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(25),
            ));
        }
    }
}

fn run(
    label: &str,
    steps: fn(&mut Probe, &ActiveEventLoop) -> Result<(), String>,
    prepare: impl FnOnce(&Path, &str),
) -> (PathBuf, Vec<String>) {
    let server = super::compat_validation::Server::start();
    let root = root(label);
    prepare(&root, &server.url);
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
        private: false,
    };
    if label.starts_with("private") {
        probe.private = true;
    }
    let result = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let done = probe.done;
    let log = std::mem::take(&mut probe.log);
    drop(probe);
    println!("{}", log.join("\n"));
    assert!(result.is_ok());
    assert!(failure.is_none(), "{failure:?}\n{}", log.join("\n"));
    assert!(done, "Fixture did not finish");
    (root, log)
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
            ..Snapshot::default()
        };
        storage::save(&root.join("session.json"), &snapshot).unwrap();
    });
    let saved = storage::load(&root.join("session.json")).unwrap();
    assert_eq!(saved.other_windows.len(), 2);
    assert_eq!(saved.tabs.len(), 2);
    assert_eq!(saved.pinned, vec![true, false]);
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
