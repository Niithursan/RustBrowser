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
            profile_id: rust_browser::profiles::DEFAULT_ID.into(),
            profiles_root: self.root.clone(),
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

/// The library's folder choices and bookmark rows (their folder paths), as JSON.
const LIBRARY_PATHS: &str = "JSON.stringify({folders:[...document.getElementById('folder-filter').options].map(o=>o.textContent),parents:[...document.getElementById('folder-parent').options].map(o=>o.textContent),rows:[...document.querySelectorAll('#library-results .row .secondary')].map(e=>e.textContent)})";

/// A chosen file's bytes reach the library in one import with a summary; a file
/// that is not an export is refused and changes nothing.
fn import_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let _ = el;
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            p.s()?
                .open_library(Panel::Bookmarks)
                .map_err(|_| "Could not open the bookmarks library")?;
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
                let nested = s
                    .library_data
                    .folders
                    .iter()
                    .find(|folder| folder.name == "Work")
                    .is_some_and(|folder| {
                        folder.path.iter().map(String::as_str).collect::<Vec<_>>()
                            == ["Bookmarks bar", "Work"]
                    });
                let guide = s
                    .library_data
                    .bookmarks
                    .iter()
                    .find(|item| item.title == "Guide");
                if !nested
                    || guide.is_none_or(|item| {
                        s.library_data
                            .folders
                            .iter()
                            .find(|folder| Some(folder.id) == item.folder)
                            .is_none_or(|folder| {
                                folder.path.iter().map(String::as_str).collect::<Vec<_>>()
                                    != ["Bookmarks bar", "Work"]
                            })
                    })
                {
                    return Err("The import did not preserve the nested folder path.".into());
                }
                eval_library(p, LIBRARY_PATHS)?;
                p.next();
                p.marks = vec![0];
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err(format!("The import never finished: {:?}", s.notice));
            }
        }
        // The library view was created in phase 0 and may still be loading, or not
        // yet showing the import, when first asked: ask again until it shows the
        // nested path, and fail if it still does not after 10s.
        2 => {
            let waited = p.at.elapsed();
            let value = p.result.lock().map_err(|_| "Result lock failed")?.take();
            let report = value
                .and_then(|value| serde_json::from_str::<String>(&value).ok())
                .and_then(|inner| serde_json::from_str::<serde_json::Value>(&inner).ok());
            let has = |key: &str, wanted: &dyn Fn(&str) -> bool| {
                report.as_ref().is_some_and(|r| {
                    r[key]
                        .as_array()
                        .is_some_and(|items| items.iter().any(|i| i.as_str().is_some_and(wanted)))
                })
            };
            let work = |option: &str| option == "Bookmarks bar / Work";
            if has("folders", &work)
                && has("parents", &work)
                && has("rows", &|row| {
                    row.starts_with("Bookmarks bar / Work · ")
                        && row.contains("docs.example.com/guide")
                })
            {
                p.marks.clear();
                p.app
                    .proxy
                    .send_event(Event::BookmarkFile(Ok(Some(
                        b"<html>not bookmarks</html>".to_vec(),
                    ))))
                    .map_err(|_| "proxy")?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!(
                    "The nested bookmark UI did not render its path: {report:?}"
                ));
            } else {
                // At most one question every 300ms; a lost answer is simply asked again.
                let asked = Duration::from_millis(p.marks.first().copied().unwrap_or(0) as u64);
                if waited.saturating_sub(asked) > Duration::from_millis(300) {
                    eval_library(p, LIBRARY_PATHS)?;
                    p.marks = vec![waited.as_millis() as usize];
                }
            }
        }
        3 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.notice.contains("not an exported bookmarks file") {
                if s.library_data.bookmarks.len() != 3 {
                    return Err("A refused file changed bookmarks".into());
                }
                p.log.push(
                    "import: nested path visible in library; refused file changes nothing".into(),
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
/// A private window runs the extensions added in a normal window (marker A,
/// on) but not those turned off (marker B), plus the built-in uBlock Origin,
/// and still refuses to add, remove or change extensions itself.
fn private_extension_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if !s.private {
                return Err("The fixture did not start a private window".into());
            }
            if !s.extensions.iter().any(|e| e.name.contains('A')) {
                if p.at.elapsed() > Duration::from_secs(20) {
                    return Err(format!(
                        "The user's extension was not added to the private window: {:?}",
                        s.extensions
                    ));
                }
                return Ok(());
            }
            if s.command(Command::AddExtension).is_ok()
                || s.command(Command::RemoveExtension("a".repeat(32))).is_ok()
                || s.command(Command::SetBuiltInBlocker(false)).is_ok()
            {
                return Err("A private window accepted an extension command".into());
            }
            // A folder chosen in a private window is ignored.
            add_marker(p, 'b')?;
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
                if found != r#""[\"on\",\"\"]""# {
                    return Err(format!(
                        "A private window ran the wrong extensions: {found}, {:?}",
                        s.extensions
                    ));
                }
                // The shield never counts in a private window.
                if s.counts_blocked() || s.tabs.items[0].content.engine.blocked.count().is_some() {
                    return Err("A private window counts blocked requests".into());
                }
                p.next();
            }
        }
        // Only the built-in uBlock Origin runs, and it blocks.
        3 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.built_in_private.is_some() {
                p.command(el, 0, Command::Navigate(format!("{url}article.html")))?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(40) {
                return Err(format!(
                    "The built-in ad blocker never started in a private window: {:?}",
                    s.built_in_error
                ));
            }
        }
        4.. => {
            if !ad_blocked(p, el, 4)? {
                return Ok(());
            }
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.settings.extensions.len() != 1 {
                return Err(format!(
                    "A private window kept an extension it was given: {:?}",
                    s.settings.extensions
                ));
            }
            p.log.push(
                "extensions: private windows run the user's extensions and the built-in uBlock Origin"
                    .into(),
            );
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

/// Ask the first tab whether the ad fixture's script was blocked; repeat the
/// load while uBlock Origin may still be reading its filter lists (the first
/// minute after it is added). `step` is the phase that loaded `/article.html`.
fn ad_blocked(p: &mut Probe, el: &ActiveEventLoop, step: u32) -> Result<bool, String> {
    let s = p.app.session.as_mut().ok_or("No session")?;
    if p.phase == step {
        if !s.windows.iter().all(loaded) || !s.tabs.items[0].url.contains("/article.html") {
            return Ok(false);
        }
        *p.result.lock().unwrap() = None;
        let result = p.result.clone();
        s.tabs.items[0]
            .content
            .engine
            .view
            .evaluate_script_with_callback(
                "document.documentElement.dataset.ad||''",
                move |value| *result.lock().unwrap() = Some(value),
            )
            .map_err(|_| "Page script failed".to_string())?;
        p.phase += 1;
        return Ok(false);
    }
    let Some(answer) = p.result.lock().unwrap().clone() else {
        return Ok(false);
    };
    if answer == "\"blocked\"" {
        return Ok(true);
    }
    if p.at.elapsed() > Duration::from_secs(60) {
        let s = p.app.session.as_ref().ok_or("No session")?;
        return Err(format!(
            "The ad fixture's script was not blocked: {answer:?} at {} (built-in: {:?}, private: {:?}, listed: {:?})",
            s.tabs.items[0].url,
            s.built_in_state(),
            s.built_in_private,
            s.extensions
        ));
    }
    p.phase = step;
    p.command(el, 0, Command::Reload)?;
    Ok(false)
}

/// The built-in uBlock Origin is added on first start, recorded as built in,
/// blocks what EasyList blocks, and turning it off removes it for good.
fn built_in_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    match p.phase {
        0 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let state = s.built_in_state();
            if let Some(saved) = s.built_in_saved().cloned()
                && state.active
            {
                let folder = s.built_in_folder.join("ublock-origin");
                if saved.version != rust_browser::bundled::VERSION
                    || saved.folder != folder.display().to_string()
                    || saved.name != "uBlock Origin"
                    || !folder.join("manifest.json").is_file()
                {
                    return Err(format!("Unexpected built-in entry: {saved:?}"));
                }
                p.command(el, 0, Command::Navigate(format!("{url}article.html")))?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(40) {
                return Err(format!(
                    "The built-in ad blocker never started: {:?}, {state:?}",
                    s.built_in_error
                ));
            }
        }
        1 | 2 => {
            if ad_blocked(p, el, 1)? {
                p.phase = 10;
                p.at = Instant::now();
            }
        }
        // uBlock Origin alone starts no counting: WebView2 cannot tell its blocks
        // from other cancelled requests.
        10 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.counts_blocked() || s.tabs.items[0].content.engine.blocked.count().is_some() {
                return Err("The shield counts for the built-in uBlock Origin alone".into());
            }
            p.command(el, 0, Command::SetBuiltInBlocker(false))?;
            p.phase = 3;
            p.at = Instant::now();
        }
        3 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if !s.settings.built_in_blocker && s.built_in_saved().is_none() {
                if s.built_in_state().active {
                    return Err("The built-in ad blocker still shows as on".into());
                }
                // Listing again must not add it back.
                s.refresh_extensions();
                p.command(el, 0, Command::Navigate(format!("{url}article.html?off")))?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(15) {
                return Err("Turning the built-in ad blocker off did not remove it".into());
            }
        }
        4 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows.iter().all(loaded) && s.tabs.items[0].url.contains("?off") {
                eval_page(p, "document.documentElement.dataset.ad||''")?;
                p.next();
            }
        }
        5 => {
            let Some(answer) = p.result.lock().unwrap().clone() else {
                return Ok(());
            };
            let s = p.app.session.as_mut().ok_or("No session")?;
            if answer != "\"loaded\"" || s.built_in_saved().is_some() || s.built_in_adding {
                return Err(format!("Turned off, the blocker still acted: {answer}"));
            }
            p.log.push(
                "built-in: uBlock Origin added, blocks, and stays off when turned off".into(),
            );
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window and unpacks uBlock Origin"]
fn native_built_in_ublock_origin_blocks_and_turns_off() {
    TEST_BUILT_IN.store(true, std::sync::atomic::Ordering::SeqCst);
    let (root, _) = run("built-in", built_in_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real private window"]
fn native_private_windows_run_extensions_and_the_built_in_blocker() {
    TEST_BUILT_IN.store(true, std::sync::atomic::Ordering::SeqCst);
    let (root, _) = run("private-extensions", private_extension_steps, |root, _| {
        // The normal profile: marker A added and on, marker B added and off.
        let saved = |letter: char, disabled: bool| {
            serde_json::json!({
                "id": letter.to_string().repeat(32),
                "name": format!("Marker {}", letter.to_ascii_uppercase()),
                "version": "1",
                "folder": marker(letter).display().to_string(),
                "disabled": disabled,
            })
        };
        let file = serde_json::json!({
            "version": 1,
            "tabs": [navigation::HOME],
            "active": 0,
            "settings": {"extensions": [saved('a', false), saved('b', true)]},
        });
        std::fs::write(root.join("session.json"), file.to_string()).unwrap();
    });
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
    const PAGES: [&str; 10] = [
        "newtab",
        "website",
        "menu",
        "permission",
        "settings",
        "notice-info",
        "notice-error",
        "clear-confirm",
        "settings-privacy",
        "settings-clear",
    ];
    /// A request shown with made-up values: the toolbar answers a real one the same way.
    // Every render passes through, so the request stays open across the theme changes.
    const FAKE_PROMPT: &str = "if(!window.realRender){window.realRender=window.render;window.render=s=>window.realRender({...s,permission_prompt:{id:999999,tab:0,origin:'https://meet.example.com',capability:'camera'},permission_seconds:52})}";
    const REAL_RENDER: &str =
        "if(window.realRender){window.render=window.realRender;delete window.realRender}";
    let out = PathBuf::from(std::env::var_os("RUST_BROWSER_SHOTS").ok_or("RUST_BROWSER_SHOTS")?);
    let label = std::env::var("RUST_BROWSER_SHOTS_LABEL").unwrap_or_else(|_| "shot".into());
    let url = p.url.clone();
    // Phases: 0 settle; then per page (new tab, website) and per shot: set, capture.
    let index = p.phase as usize;
    if p.at.elapsed() < Duration::from_millis(if index == 0 { 2000 } else { 900 }) {
        return Ok(());
    }
    if index > 2 * SHOTS.len() * PAGES.len() {
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
        if page == 3 && shot == 0 {
            // Close the menu; the request opens its own dropdown.
            let s = p.app.session.as_mut().ok_or("No session")?;
            let _ = s.windows[0]
                .toolbar
                .evaluate_script("document.getElementById('more').click()");
            let _ = s.windows[0].toolbar.evaluate_script(FAKE_PROMPT);
        }
        if page == 4 && shot == 0 {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let _ = s.windows[0].toolbar.evaluate_script(REAL_RENDER);
            p.command(el, 0, Command::ShowPanel(Panel::Settings))?;
        }
        if page == 5 && shot == 0 {
            let s = p.app.session.as_mut().ok_or("No session")?;
            s.panel = None;
            let _ = s.activate();
            s.inform("Extension added: uBlock Origin Lite. It can read and change every website you open in normal windows.".into());
        }
        if page == 6 && shot == 0 {
            let s = p.app.session.as_mut().ok_or("No session")?;
            s.notice = "Library database is unavailable. Browsing remains available; check profile access or use the browser version that created this library. Bookmarks, history and remembered permissions cannot be shown until it opens again.".into();
        }
        if page == 7 && shot == 0 {
            // The confirmation for two chosen categories; nothing is cleared.
            p.command(el, 0, Command::ShowPanel(Panel::Settings))?;
            let s = p.app.session.as_mut().ok_or("No session")?;
            s.notice.clear();
            let _ = s.windows[0].toolbar.evaluate_script("setTimeout(()=>{for(const id of ['clear-app-history','clear-cookies'])document.getElementById(id).checked=true;document.getElementById('clear-data').click()},300)");
        }
        if (page == 8 || page == 9) && shot == 0 {
            let section = if page == 8 {
                "settings-privacy"
            } else {
                "settings-clear"
            };
            let s = p.app.session.as_mut().ok_or("No session")?;
            let _ = s.windows[0].toolbar.evaluate_script(&format!(
                "document.getElementById('confirm-dialog').close();document.getElementById('{section}').scrollIntoView({{block:'start'}})"
            ));
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
            PAGES[page],
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

/// The address field, its selection and the suggestion list, as JSON.
const SUGGEST_REPORT: &str = "(()=>{const a=document.getElementById('address'),p=document.getElementById('suggest-panel'),rows=[...p.querySelectorAll('[role=option]')];return JSON.stringify({value:a.value,start:a.selectionStart,end:a.selectionEnd,open:!p.hidden,expanded:a.getAttribute('aria-expanded'),rows:rows.map(r=>r.dataset.target),titles:rows.map(r=>r.textContent),focused:document.activeElement===a})})()";

/// One key through Chromium's own keyboard input, as the user presses it.
fn press_toolbar_key(s: &Session, key: &str, code: i32, text: &str) -> Result<(), String> {
    for kind in ["rawKeyDown", "char", "keyUp"] {
        if kind == "char" && text.is_empty() {
            continue;
        }
        toolbar_cdp(
            s,
            "Input.dispatchKeyEvent",
            serde_json::json!({ "type": kind, "key": key, "code": key,
                "text": if kind == "char" { text } else { "" },
                "windowsVirtualKeyCode": code }),
        )?;
    }
    Ok(())
}

/// Typing the start of a visited site completes it inline, with the added part
/// selected, and lists the matching pages from history under the field; the
/// arrow keys and Enter open a listed page.
fn suggest_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let origin = url::Url::parse(&p.url)
        .map_err(|e| e.to_string())?
        .origin()
        .ascii_serialization();
    let (one, two) = (
        format!("{origin}/suggest/one"),
        format!("{origin}/suggest/two"),
    );
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let now = rust_browser::library::now();
            // "one" twice, so it lists before "two".
            for (key, url, title, at) in [
                ("s1", &one, "Suggestion one", now - 300),
                ("s2", &one, "Suggestion one", now - 200),
                ("s3", &two, "Suggestion two", now - 100),
            ] {
                p.command(
                    el,
                    0,
                    Command::Library(Action::Record {
                        key: key.into(),
                        url: url.clone(),
                        title: title.into(),
                        timestamp: at,
                    }),
                )?;
            }
            // As Ctrl+L: the toolbar takes the keyboard and selects the address.
            p.command(el, 0, Command::FocusAddress)?;
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_millis(500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            for text in ["1", "2", "7"] {
                toolbar_cdp(s, "Input.insertText", serde_json::json!({ "text": text }))?;
            }
            p.next();
        }
        2 if p.at.elapsed() > Duration::from_millis(300) => {
            eval_toolbar(p, SUGGEST_REPORT)?;
            p.next();
        }
        3 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            let host = origin.trim_start_matches("http://");
            let ready = r["open"] == true
                && r["value"] == host
                && r["rows"].as_array().is_some_and(|rows| rows.len() >= 3);
            if !ready {
                if p.at.elapsed() > Duration::from_secs(10) {
                    return Err(format!("No completion or list for 127: {r}"));
                }
                eval_toolbar(p, SUGGEST_REPORT)?;
                return Ok(());
            }
            let rows: Vec<&str> = r["rows"]
                .as_array()
                .ok_or("No rows")?
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            if r["start"] != 3
                || r["end"].as_u64() != Some(host.len() as u64)
                || r["expanded"] != "true"
                || r["focused"] != true
                || rows[0] != format!("{origin}/")
                || rows[1] != one
                || rows[2] != two
            {
                return Err(format!("Unexpected completion or list: {r}"));
            }
            p.log
                .push("suggestions: 127 completes to the visited site and lists its pages".into());
            let s = p.app.session.as_mut().ok_or("No session")?;
            press_toolbar_key(s, "ArrowDown", 40, "")?;
            press_toolbar_key(s, "ArrowDown", 40, "")?;
            press_toolbar_key(s, "Enter", 13, "\r")?;
            p.next();
        }
        4 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.tabs.items[0].url == one {
                p.log
                    .push("suggestions: arrow keys and Enter open a listed page".into());
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err(format!(
                    "Enter did not open the picked page: {}",
                    s.tabs.items[0].url
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_address_suggestions_complete_and_list() {
    let (root, _) = run("suggest", suggest_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// The centre of the first tab's close icon, and whether that point hits the icon
/// itself (not the button's padding around it).
const CLOSE_ICON_POINT: &str = r#"(()=>{const icon=document.querySelector('#tabs .tab .close svg');if(!icon)return JSON.stringify(null);const r=icon.getBoundingClientRect(),x=r.left+r.width/2,y=r.top+r.height/2;const hit=document.elementFromPoint(x,y);return JSON.stringify({x,y,tabs:document.querySelectorAll('#tabs .tab').length,onIcon:!!hit&&icon.contains(hit)})})()"#;

/// One left click through Chromium's own mouse input, so pointer capture and click
/// targeting behave as they do for the user.
fn click_toolbar(s: &Session, x: f64, y: f64) -> Result<(), String> {
    for (kind, buttons) in [("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 0)] {
        toolbar_cdp(
            s,
            "Input.dispatchMouseEvent",
            serde_json::json!({ "type": kind, "x": x, "y": y, "button": "left",
                "buttons": buttons, "clickCount": 1 }),
        )?;
    }
    Ok(())
}

/// Regression: a click on the close icon (rather than the padding around it) let
/// the tab capture the pointer for dragging, so the click selected the tab and the
/// user had to click several times before it closed.
fn close_icon_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    match p.phase {
        0 => {
            let first = p.s()?.tabs.items[0].id;
            p.marks = vec![usize::try_from(first).map_err(|_| "Tab ID too large")?];
            p.command(el, 0, Command::NewTab)?;
            p.next();
        }
        1 => {
            if p.s()?.tabs.items.len() == 2 {
                eval_toolbar(p, CLOSE_ICON_POINT)?;
                p.next();
            }
        }
        2 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            // Ask again until the toolbar has drawn both tabs.
            if r.is_null() || r["tabs"] != 2 {
                eval_toolbar(p, CLOSE_ICON_POINT)?;
                return Ok(());
            }
            if r["onIcon"] != true {
                return Err(format!("The point is not on the close icon: {r}"));
            }
            let (x, y) = (
                r["x"].as_f64().ok_or("No x")?,
                r["y"].as_f64().ok_or("No y")?,
            );
            click_toolbar(p.s()?, x, y)?;
            p.next();
        }
        3 => {
            let first = TabId::try_from(p.marks[0]).map_err(|_| "Tab ID too large")?;
            let waited = p.at.elapsed();
            let s = p.s()?;
            if s.tabs.items.len() == 1 && s.tabs.items[0].id != first {
                p.log
                    .push("tabs: one click on the close icon closes the tab".into());
                p.done = true;
                el.exit();
            } else if waited > Duration::from_secs(10) {
                return Err(format!(
                    "One click on the close icon did not close the tab ({} tabs, first still open: {})",
                    s.tabs.items.len(),
                    s.tabs.items.iter().any(|t| t.id == first)
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_one_click_on_the_close_icon_closes_the_tab() {
    let (root, _) = run("close-icon", close_icon_steps, |_, _| {});
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
const POLISH_REPORT: &str = r#"(()=>{const b=id=>document.getElementById(id);const shown=e=>!!e&&!e.hidden&&e.getClientRects().length>0;const tab=document.querySelector('.tab.active .select');const wc=b('window-controls'),wcr=wc?wc.getBoundingClientRect():{top:-99,right:-99,height:0};return JSON.stringify({reload:shown(b('reload')),stop:shown(b('stop')),status:!!b('status'),address:b('address').title,picker:b('tab-picker').tagName+'|'+b('tab-picker').getAttribute('aria-label'),pickerShown:shown(b('tab-picker')),shortcuts:shown(b('show-shortcuts'))||!!b('show-shortcuts'),captionFlush:shown(wc)&&Math.abs(wcr.top)<=1&&Math.abs(wcr.right-innerWidth)<=1&&wcr.height>=36&&wcr.height<=48&&shown(b('win-minimize'))&&shown(b('win-maximize'))&&shown(b('win-close')),tabTitle:tab?tab.title:'',tabLabel:tab?tab.getAttribute('aria-label'):''})})()"#;

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
                    (r["status"] == false, "there is no status row"),
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
                    (
                        r["captionFlush"] == true,
                        "window caption controls sit flush at top-right",
                    ),
                ];
                if let Some((_, what)) = checks.iter().find(|(ok, _)| !ok) {
                    return Err(format!("Idle toolbar: {what}: {r}"));
                }
                {
                    let win = &p.s()?.windows[0].window;
                    if win.is_decorated() {
                        return Err("Browser window still has a separate OS title bar".into());
                    }
                    if win.title().contains(env!("CARGO_PKG_VERSION")) {
                        return Err(format!(
                            "Window title still carries version suffix: {}",
                            win.title()
                        ));
                    }
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
                "document.getElementById('show-shortcuts').getAttribute('aria-expanded')+'|'+!document.getElementById('shortcut-list').hidden+'|'+document.querySelectorAll('#shortcut-list li').length+'|'+Math.round(document.getElementById('shortcut-list').getBoundingClientRect().height)",
            )?;
            p.next();
        }
        6 => {
            if let Some(text) = p.result.lock().unwrap().clone() {
                let inner: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                let parts: Vec<&str> = inner.split('|').collect();
                if parts.len() != 4
                    || parts[0] != "true"
                    || parts[1] != "true"
                    || parts[2].parse::<usize>().unwrap_or(0) < 15
                    || parts[3].parse::<usize>().unwrap_or(0) < 120
                {
                    return Err(format!(
                        "Keyboard shortcuts list did not open cleanly: {inner}"
                    ));
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

/// Run `script` in the active tab's page; the answer arrives in `p.result`.
fn eval_page(p: &mut Probe, script: &str) -> Result<(), String> {
    let s = p.app.session.as_mut().ok_or("No session")?;
    *p.result.lock().unwrap() = None;
    let result = p.result.clone();
    s.tabs
        .active()
        .ok_or("No active tab")?
        .content
        .engine
        .view
        .evaluate_script_with_callback(script, move |value| *result.lock().unwrap() = Some(value))
        .map_err(|_| "Page script failed".to_string())
}

/// Ask for notifications from the page; the answer lands in a data attribute.
const ASK_NOTIFICATIONS: &str = "delete document.documentElement.dataset.rbPermission;Notification.requestPermission().then(v=>document.documentElement.dataset.rbPermission=v);'asked'";
/// What the toolbar shows for a pending request, as JSON.
const PROMPT_REPORT: &str = r#"(()=>{const b=id=>document.getElementById(id);return JSON.stringify({open:!b('site-panel').hidden,prompt:!b('permission-prompt').hidden,isolated:b('site-connection-info').hidden,title:b('permission-title').textContent,expiry:b('permission-expiry').textContent,focus:document.activeElement&&document.activeElement.id,hasFocus:document.hasFocus(),remember:b('remember-permission-label').textContent,labelled:b('site-panel').getAttribute('aria-labelledby'),manager:b('manager').hidden})})()"#;
/// A click on Allow the moment the request opens (the second click of a double-click).
const EARLY_ALLOW: &str = "document.getElementById('allow-permission').click()";

/// A website's permission request opens under the site-information badge over the
/// page, never as a full panel; focus starts on "Don't allow" and the toolbar holds
/// the window's keyboard focus; an Allow click in the first half second is ignored;
/// Escape answers "Don't allow"; and Allow reaches the page. A private window says
/// that a remembered choice lasts for that window. WebView2 does not always treat a
/// script's request as made by the user and then denies it without asking, so a
/// request that never reaches a prompt is asked again (a few times at most).
fn permission_prompt_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let pending = |p: &mut Probe| -> Result<bool, String> {
        Ok(p.s()?.permissions.borrow().prompt().is_some())
    };
    let loaded = |p: &Probe| {
        p.app.session.as_ref().is_some_and(|s| {
            s.tabs
                .active()
                .is_some_and(|t| t.url != navigation::HOME && !t.content.state.loading)
        })
    };
    // Two rounds: phases 1-4 answer with Escape, 5-8 with Allow.
    let round = |phase: u32| if phase < 5 { 0 } else { 4 };
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let url = p.url.clone();
            p.command(el, 0, Command::Navigate(url))?;
            p.next();
        }
        1 | 5 if p.at.elapsed() > Duration::from_millis(800) && loaded(p) => {
            if p.marks.iter().filter(|m| **m != 99).count() > 6 {
                return Err("No notification request reached a prompt".into());
            }
            p.marks.push(p.phase as usize);
            eval_page(p, ASK_NOTIFICATIONS)?;
            p.next();
        }
        2 | 6 => {
            if pending(p)? {
                if p.phase == 6 && !p.marks.contains(&99) {
                    // Too early to count: the request must still be waiting afterwards.
                    p.marks.push(99);
                    let s = p.s()?;
                    s.windows[0]
                        .toolbar
                        .evaluate_script(EARLY_ALLOW)
                        .map_err(|_| "Toolbar script failed")?;
                }
                if p.at.elapsed() > Duration::from_millis(700) {
                    eval_toolbar(p, PROMPT_REPORT)?;
                    p.next();
                }
            } else if p.at.elapsed() > Duration::from_secs(4) {
                // Denied without asking (not user-initiated): reload and ask again.
                let url = p.url.clone();
                p.command(el, 0, Command::Navigate(url))?;
                p.phase = 1 + round(p.phase);
                p.at = Instant::now();
            }
        }
        3 | 7 => {
            let Some(r) = polish_report(p)? else {
                return Ok(());
            };
            let shown = r["open"] == true
                && r["prompt"] == true
                && r["isolated"] == true
                && r["labelled"] == "permission-title"
                && r["manager"] == true
                && r["title"]
                    .as_str()
                    .is_some_and(|t| t.ends_with(" wants to show notifications"))
                && r["expiry"].as_str().is_some_and(|t| t.contains("seconds"))
                && r["focus"] == "deny-permission";
            if !shown || p.s()?.panel.is_some() || !pending(p)? {
                return Err(format!("The request was not shown over the page: {r}"));
            }
            // The page is hidden under the dropdown, so the toolbar must hold keyboard
            // focus for Escape to reach it (checked when the window itself is focused).
            if p.s()?.is_focused() && r["hasFocus"] != true {
                return Err(format!("The toolbar did not take keyboard focus: {r}"));
            }
            let remembered = if p.private {
                "Remember my choice for this site in this window"
            } else {
                "Remember my choice for this site"
            };
            if r["remember"] != remembered {
                return Err(format!("Wrong remember label: {r}"));
            }
            let script = if p.phase == 3 {
                "document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))"
            } else {
                "document.getElementById('allow-permission').click()"
            };
            *p.result.lock().unwrap() = None;
            let s = p.s()?;
            s.windows[0]
                .toolbar
                .evaluate_script(script)
                .map_err(|_| "Toolbar script failed")?;
            p.next();
        }
        4 | 8 if p.at.elapsed() > Duration::from_millis(300) => {
            if pending(p)? {
                if p.at.elapsed() > Duration::from_secs(5) {
                    return Err("The answer did not reach the request".into());
                }
                return Ok(());
            }
            let answer = p.result.lock().unwrap().clone();
            match answer {
                None => eval_page(
                    p,
                    "document.documentElement.dataset.rbPermission||'pending'",
                )?,
                Some(text) => {
                    let value: String = serde_json::from_str(&text).unwrap_or_default();
                    let expected = if p.phase == 4 { "denied" } else { "granted" };
                    if value == "pending" || value.is_empty() {
                        *p.result.lock().unwrap() = None;
                        return Ok(());
                    }
                    if value != expected || p.s()?.panel.is_some() {
                        return Err(format!("Expected the page to be {expected}, got {value}"));
                    }
                    if p.phase == 4 {
                        // A fresh page for the second request.
                        let url = p.url.clone();
                        p.command(el, 0, Command::Navigate(format!("{url}?again")))?;
                        p.next();
                    } else {
                        p.log.push(
                            "permission prompt: over the page, Escape denies, Allow allows".into(),
                        );
                        p.done = true;
                        el.exit();
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_permission_prompt_over_the_page() {
    let (root, _) = run("permission-prompt", permission_prompt_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_private_permission_prompt_over_the_page() {
    let (root, _) = run(
        "private-permission-prompt",
        permission_prompt_steps,
        |_, _| {},
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Whether a view is shown, as WebView2 reports it.
fn view_visible(view: &wry::WebView) -> bool {
    let mut visible = windows::core::BOOL::default();
    // SAFETY: a read-only query on this fixture's own controller.
    unsafe { view.controller().IsVisible(&mut visible) }.is_ok() && visible.as_bool()
}

/// The menu opens as a dropdown under its button. The toolbar spans the window but is
/// clipped to the chrome and the dropdown, so the live page stays shown around it;
/// focusing the page closes the dropdown and removes the clip.
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
                if !view_visible(&s.tabs.items[0].content.engine.view) {
                    return Err("The live page was hidden under the dropdown".into());
                }
                check_live_under_dropdown(s)?;
                eval_toolbar(
                    p,
                    "(()=>{const m=document.getElementById('tab-actions').getBoundingClientRect(),b=document.getElementById('more').getBoundingClientRect(),a=JSON.parse(menuArea||'null');return JSON.stringify({under:m.top>=b.bottom&&m.top-b.bottom<12,aligned:Math.abs(m.right-b.right)<=1,inside:m.bottom<=innerHeight&&m.left>=0,full:m.top<=0||m.bottom>=innerHeight-1&&m.height>innerHeight*0.95,narrow:m.width<innerWidth/2,shown:!document.getElementById('tab-actions').hidden,expanded:document.getElementById('more').getAttribute('aria-expanded'),area:!!a&&Math.abs(a.x-m.left)<1&&Math.abs(a.y-m.top)<1&&Math.abs(a.height-m.height)<1})})()",
                )?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("The menu sidebar never opened".into());
            }
        }
        // A press comes after the menu has been open a moment; focus within
        // `MENU_FOCUS_GRACE` of opening is the browser's own hand-back, not a press.
        3 if p.at.elapsed() > super::MENU_FOCUS_GRACE + Duration::from_millis(200) => {
            if let Some(r) = polish_report(p)? {
                if r["under"] != true
                    || r["aligned"] != true
                    || r["inside"] != true
                    || r["full"] != false
                    || r["narrow"] != true
                    || r["shown"] != true
                    || r["expanded"] != "true"
                {
                    return Err(format!("The menu is not a dropdown under its button: {r}"));
                }
                if r["area"] != true {
                    return Err(format!("The browser was not told where the menu is: {r}"));
                }
                let s = p.app.session.as_mut().ok_or("No session")?;
                let menu = s.windows[0].menu_area.ok_or("No menu area")?;
                let clip = toolbar_clip(s, 0).ok_or("The toolbar is not clipped")?;
                let scale = s.window.scale_factor();
                if (clip.bottom as f64) < (menu.y + menu.height) * scale - 1.0 {
                    return Err(format!("The clip cuts off the menu: {clip:?} {menu:?}"));
                }
                // A press on the page moves focus there, which closes the dropdown.
                s.tabs.items[0]
                    .content
                    .engine
                    .view
                    .focus()
                    .map_err(|e| e.to_string())?;
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
                if toolbar_clip(s, 0).is_some() {
                    return Err("The toolbar kept its clip after the menu closed".into());
                }
                p.log.push(
                    "menu: a dropdown under its button over the live page; focusing the page closes it"
                        .into(),
                );
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("Focusing the page did not close the dropdown".into());
            }
        }
        _ => {}
    }
    Ok(())
}

/// Read the JSON a toolbar or library script returned, once it has arrived.
fn script_json(p: &Probe) -> Result<Option<serde_json::Value>, String> {
    polish_report(p)
}

const MENU_KEYS: &str = r#"(()=>{const m=document.getElementById('menu-items');const key=k=>document.activeElement.dispatchEvent(new KeyboardEvent('keydown',{key:k,bubbles:true}));const items=[...m.querySelectorAll('[role^=menuitem]:not(:disabled)')];items[0].focus();const id=()=>document.activeElement.id;key('ArrowDown');const down=id();key('End');const end=id();key('Home');const home=id();key('ArrowUp');const up=id();const awake=document.getElementById('keep-awake-state').textContent;const draft=input.value;input.value='https://login.sub.evil.example:8443/?q=1';updateAddressView();const v=document.getElementById('address-view'),originOk=v.firstChild.firstChild.textContent==='login.sub.'&&v.firstChild.lastChild.textContent==='evil.example:8443'&&v.lastChild.textContent==='/?q=1';input.value=draft;updateAddressView();return JSON.stringify({down,end,home,up,first:items[0].id,second:items[1].id,last:items.at(-1).id,awake,originOk})})()"#;
const SECTION_LINK: &str = r#"(()=>{const before=document.activeElement.id;document.querySelector('#settings-index [data-section=settings-clear]').click();const current=document.querySelector('#settings-index [aria-current=true]')?.dataset.section;const headTop=Math.round(document.querySelector('.manager-head').getBoundingClientRect().top-document.getElementById('manager').getBoundingClientRect().top);return JSON.stringify({before,after:document.activeElement.id,current,headTop})})()"#;
const BOOKMARK_DISCLOSURE: &str = r#"(()=>{const editorHidden=document.getElementById('bookmark-editor').hidden,foldersHidden=document.getElementById('folder-tools').hidden;document.getElementById('toggle-bookmark-form').click();document.getElementById('bookmark-title').value='Kept title';document.getElementById('bookmark-url').value='javascript:alert(1)';document.getElementById('bookmark-form').dispatchEvent(new Event('submit',{cancelable:true}));const retained=!document.getElementById('bookmark-editor').hidden&&document.getElementById('bookmark-title').value==='Kept title'&&document.getElementById('bookmark-url').getAttribute('aria-invalid')==='true'&&document.getElementById('bookmark-form-error').textContent.includes('http');document.getElementById('reset-bookmark').click();document.getElementById('toggle-folder-tools').click();const opened=!document.getElementById('folder-tools').hidden,disabledNew=document.getElementById('remove-folder').disabled;renderLibrary({folders:[{id:7,name:'Work',parent_id:null,path:['Work']}],bookmarks:[],history:[]},'bookmarks');const sel=document.getElementById('edit-folder');sel.value='7';sel.dispatchEvent(new Event('change'));const enabledExisting=!document.getElementById('remove-folder').disabled;document.getElementById('remove-folder').click();const dialog=document.getElementById('confirm-dialog'),confirmOpen=dialog.open,focus=document.activeElement?.id;document.getElementById('confirm-cancel').click();return JSON.stringify({editorHidden,foldersHidden,retained,opened,disabledNew,enabledExisting,confirmOpen,focus,closed:!dialog.open})})()"#;
const CONFIRM_CANCEL: &str = r#"(()=>{const r=document.getElementById('restore-session'),d=document.getElementById('confirm-dialog');if(!r.checked)return JSON.stringify({start:false});r.click();const open=d.open,title=document.getElementById('confirm-title').textContent,focus=document.activeElement.id;document.getElementById('confirm-cancel').click();return JSON.stringify({start:true,open,title,focus,checked:r.checked,closed:!d.open})})()"#;
const CONFIRM_OK: &str = r#"(()=>{const r=document.getElementById('restore-session'),d=document.getElementById('confirm-dialog');const rechecked=r.checked;r.click();const open=d.open,action=document.getElementById('confirm-ok').textContent;document.getElementById('confirm-ok').click();return JSON.stringify({rechecked,open,action,closed:!d.open,checked:r.checked})})()"#;
const CONFIRM_STRANDED: &str = r#"(()=>{document.getElementById('clear-cache').checked=true;document.getElementById('clear-data').click();return JSON.stringify({open:document.getElementById('confirm-dialog').open})})()"#;
const DIALOG_OPEN: &str =
    r#"JSON.stringify({open:document.getElementById('confirm-dialog').open})"#;
const DATE_FILTER: &str = r#"(()=>{const b=document.getElementById('history-dates'),f=document.getElementById('date-fields'),from=document.getElementById('history-from');const before=f.hidden;b.click();const shown=!f.hidden&&b.getAttribute('aria-expanded')==='true';from.value='2026-01-01';from.dispatchEvent(new Event('change'));return JSON.stringify({before,shown,label:b.textContent})})()"#;

/// The interactions the chrome design pass added: the menu's arrow keys, Settings
/// opened at a section from the site-information dropdown and from its own section
/// links, the browser's confirmation (Cancel sends nothing and re-checks the box; the
/// named action runs it; a panel change cancels it), and the history date filter.
fn chrome_interaction_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let wait = |p: &Probe, ms: u64| p.at.elapsed() > Duration::from_millis(ms);
    match p.phase {
        0 if wait(p, 1000) => {
            p.s()?.windows[0]
                .toolbar
                .evaluate_script("document.getElementById('more').click()")
                .map_err(|_| "Toolbar script failed")?;
            p.next();
        }
        1 if wait(p, 400) => {
            eval_toolbar(p, MENU_KEYS)?;
            p.next();
        }
        2 => {
            let Some(r) = script_json(p)? else {
                return Ok(());
            };
            if r["down"] != r["second"]
                || r["end"] != r["last"]
                || r["home"] != r["first"]
                || r["up"] != r["last"]
                || r["last"] != "exit-browser"
                || r["awake"] != "Off"
                || r["originOk"] != true
            {
                return Err(format!("Menu keys or origin split: {r}"));
            }
            // The site-information dropdown's button opens Settings at Privacy & security.
            p.s()?.windows[0]
                .toolbar
                .evaluate_script("document.getElementById('site-settings').click()")
                .map_err(|_| "Toolbar script failed")?;
            p.next();
        }
        3 if wait(p, 600)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.panel == Some(Panel::Settings)) =>
        {
            eval_toolbar(p, SECTION_LINK)?;
            p.next();
        }
        4 => {
            let Some(r) = script_json(p)? else {
                return Ok(());
            };
            if r["before"] != "settings-privacy"
                || r["after"] != "settings-clear"
                || r["current"] != "settings-clear"
                || r["headTop"] != 0
            {
                return Err(format!("Settings sections: {r}"));
            }
            eval_toolbar(p, CONFIRM_CANCEL)?;
            p.next();
        }
        5 => {
            let Some(r) = script_json(p)? else {
                return Ok(());
            };
            if r["start"] != true
                || r["open"] != true
                || r["focus"] != "confirm-cancel"
                || r["closed"] != true
                || r["title"] != "Stop restoring tabs?"
            {
                return Err(format!("Confirmation cancel: {r}"));
            }
            p.next();
        }
        6 if wait(p, 400) => {
            if !p.s()?.settings.restore_session {
                return Err("Cancel changed the setting".into());
            }
            eval_toolbar(p, CONFIRM_OK)?;
            p.next();
        }
        7 => {
            let Some(r) = script_json(p)? else {
                return Ok(());
            };
            // Cancel re-checks the box once the dialog's close event has run.
            if r["rechecked"] != true
                || r["open"] != true
                || r["action"] != "Stop restoring"
                || r["closed"] != true
                || r["checked"] != false
            {
                return Err(format!("Confirmation action: {r}"));
            }
            p.next();
        }
        8 if wait(p, 400) => {
            if p.s()?.settings.restore_session {
                return Err("The confirmed action did not reach the browser".into());
            }
            p.command(el, 0, Command::SetRestoreSession(true))?;
            eval_toolbar(p, CONFIRM_STRANDED)?;
            p.next();
        }
        9 => {
            let Some(r) = script_json(p)? else {
                return Ok(());
            };
            if r["open"] != true {
                return Err(format!("Clear data did not ask first: {r}"));
            }
            p.command(el, 0, Command::ClosePanel)?;
            p.next();
        }
        10 if wait(p, 600) => {
            eval_toolbar(p, DIALOG_OPEN)?;
            p.next();
        }
        11 => {
            let Some(r) = script_json(p)? else {
                return Ok(());
            };
            if r["open"] != false {
                return Err(format!("A confirmation outlived its panel: {r}"));
            }
            p.command(el, 0, Command::ShowPanel(Panel::History))?;
            p.next();
        }
        12 if wait(p, 1500) => {
            eval_library(p, DATE_FILTER)?;
            p.next();
        }
        13 => {
            let Some(r) = script_json(p)? else {
                return Ok(());
            };
            if r["before"] != true || r["shown"] != true || r["label"] != "Filter by date: on" {
                return Err(format!("Date filter: {r}"));
            }
            p.command(el, 0, Command::ShowPanel(Panel::Bookmarks))?;
            p.next();
        }
        14 if wait(p, 800) => {
            eval_library(p, BOOKMARK_DISCLOSURE)?;
            p.next();
        }
        15 => {
            let Some(r) = script_json(p)? else {
                return Ok(());
            };
            if r["editorHidden"] != true
                || r["foldersHidden"] != true
                || r["retained"] != true
                || r["opened"] != true
                || r["disabledNew"] != true
                || r["enabledExisting"] != true
                || r["confirmOpen"] != true
                || r["focus"] != "confirm-cancel"
                || r["closed"] != true
            {
                return Err(format!("Bookmark disclosure and folder confirmation: {r}"));
            }
            p.log.push(
                "chrome interactions: menu keys, settings sections, confirmations, date filter"
                    .into(),
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
fn native_chrome_interactions() {
    let (root, _) = run("chrome-interactions", chrome_interaction_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_menu_opens_as_a_dropdown() {
    let (root, _) = run("sidebar", sidebar_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// The menu over the new-tab page (a full panel, so there is no page still) leaves
/// the new-tab page visible and dimmed behind the sidebar instead of a blank area.
fn newtab_menu_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let waited = p.at.elapsed();
    match p.phase {
        0 if waited > Duration::from_secs(1) => {
            let s = p.s()?;
            if s.effective_panel() != Some(Panel::NewTab) {
                return Err("The window did not start on the new-tab page".into());
            }
            if s.windows[0]
                .toolbar
                .evaluate_script("document.getElementById('more').click()")
                .is_err()
            {
                return Err("Menu script failed".into());
            }
            p.next();
        }
        1 => {
            if p.s()?.windows[0].menu_open {
                eval_toolbar(
                    p,
                    "(()=>{const b=document.getElementById('menu-backdrop'),h=document.getElementById('newtab-body'),r=h.getBoundingClientRect(),m=document.getElementById('tab-actions').getBoundingClientRect(),x=Math.min(r.left+r.width/2,m.left-20),y=r.top+Math.min(r.height/2,40);return JSON.stringify({backdrop:!b.hidden,background:getComputedStyle(b).backgroundColor,still:!!document.getElementById('menu-snapshot'),home:!h.hidden&&r.width>0&&r.height>0,under:document.elementsFromPoint(x,y).some(e=>h.contains(e)),scrim:getComputedStyle(b,'::after').backgroundColor})})()",
                )?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err("The menu sidebar never opened".into());
            }
        }
        2 => {
            if let Some(r) = polish_report(p)? {
                if r["backdrop"] != true
                    || r["still"] != false
                    || r["home"] != true
                    || r["under"] != true
                    || r["background"] != "rgba(0, 0, 0, 0)"
                    || r["scrim"] == "rgba(0, 0, 0, 0)"
                {
                    return Err(format!("The new-tab page is hidden behind the menu: {r}"));
                }
                p.log.push(
                    "menu: the new-tab page stays visible, dimmed, behind the sidebar".into(),
                );
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
fn native_menu_keeps_the_new_tab_page_visible() {
    let (root, _) = run("newtab-menu", newtab_menu_steps, |_, _| {});
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
    // An open dropdown's toolbar spans the page on purpose, clipped to the chrome and
    // the dropdown and raised above the page; only its clip must stay off the page.
    let clipped = toolbar_clip(s, index);
    if s.windows[index].menu_open && s.effective_panel().is_none() && clipped.is_none() {
        return Err(format!("{step}: the dropdown's toolbar is not clipped"));
    }
    for (i, (a, ra)) in views.iter().enumerate() {
        for (b, rb) in &views[i + 1..] {
            if clipped.is_some() && (*a == "toolbar" || *b == "toolbar") {
                continue;
            }
            let x = ra.right.min(rb.right) - ra.left.max(rb.left);
            let y = ra.bottom.min(rb.bottom) - ra.top.max(rb.top);
            if x > 1 && y > 1 {
                return Err(format!("{step}: {a} {ra:?} overlaps {b} {rb:?}"));
            }
        }
    }
    Ok(())
}

/// The bounding box of the toolbar host's window region (relative to the host), or
/// `None` when the toolbar is not clipped.
fn toolbar_clip(s: &Session, index: usize) -> Option<windows::Win32::Foundation::RECT> {
    use windows::Win32::Graphics::Gdi::{GDI_REGION_TYPE, GetWindowRgnBox};
    let mut host = windows::Win32::Foundation::HWND::default();
    let mut r = windows::Win32::Foundation::RECT::default();
    // SAFETY: read-only queries on this fixture's own controller and its host.
    let kind = unsafe {
        s.windows[index]
            .toolbar
            .controller()
            .ParentWindow(&mut host)
            .ok()?;
        GetWindowRgnBox(host, &mut r)
    };
    (kind != GDI_REGION_TYPE(0)).then_some(r)
}

/// While a dropdown is open over a webpage, the page is still shown and running, and
/// the clipped toolbar leaves a point of the page far from the menu (its lower left
/// corner; menus open under the toolbar) uncovered.
fn check_live_under_dropdown(s: &Session) -> Result<(), String> {
    use windows::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject, GetWindowRgn, PtInRegion};
    let w = &s.windows[0];
    if !w.menu_open || !view_visible(&w.tabs.items[0].content.engine.view) {
        return Err("The live page was hidden under the dropdown".into());
    }
    let mut host = windows::Win32::Foundation::HWND::default();
    let mut r = windows::Win32::Foundation::RECT::default();
    // SAFETY: read-only queries on this fixture's own windows; the copied region
    // is created and deleted here.
    unsafe {
        w.toolbar
            .controller()
            .ParentWindow(&mut host)
            .map_err(|e| e.to_string())?;
        windows::Win32::UI::WindowsAndMessaging::GetClientRect(host, &mut r)
            .map_err(|e| e.to_string())?;
        let region = CreateRectRgn(0, 0, 0, 0);
        let kind = GetWindowRgn(host, region);
        let covered = PtInRegion(region, 8, r.bottom - 8).as_bool();
        let _ = DeleteObject(region.into());
        if kind.0 == 0 {
            return Err("The dropdown's toolbar is not clipped".into());
        }
        if covered {
            return Err(format!("The toolbar clip covers the page: {r:?}"));
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
            p.s()?.windows[0]
                .toolbar
                .evaluate_script("document.getElementById('home-bookmarks').click()")
                .map_err(|_| "New-tab Open bookmarks click failed")?;
            p.phase = 8;
            p.next();
        }
        9 if p.at.elapsed() > Duration::from_millis(800) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            check_no_overlap(s, "library beside new tab")?;
            let views = shown_views(s);
            let (Some(toolbar), Some(library)) =
                (named(&views, "toolbar"), named(&views, "library"))
            else {
                return Err(format!(
                    "New tab and library should both show side by side: {views:?}"
                ));
            };
            if named(&views, "page").is_some()
                || (toolbar.right - library.left).abs() > 2
                || s.windows[0].library_panel != Some(Panel::Bookmarks)
            {
                return Err(format!("Library beside new tab is misplaced: {views:?}"));
            }
            p.command(el, 0, Command::CloseLibrary)?;
            p.command(el, 0, Command::Navigate(format!("{url}?library")))?;
            p.phase = 0;
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
/// that count at the end of the address line and in its dropdown (over the live
/// page), says what it cannot do, closes with Escape, and starts again at zero
/// on the next page.
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
            check_no_overlap(s, "shield dropdown open")?;
            if !s.windows[0].menu_open || !view_visible(&s.tabs.items[0].content.engine.view) {
                return Err("The shield dropdown should show over the live page".into());
            }
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
                || text("focus") != "shield-title"
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
            if s.windows[0].menu_open || !view_visible(&s.tabs.items[0].content.engine.view) {
                return Err("Closing the dropdown should show the live page again".into());
            }
            check_no_overlap(s, "shield dropdown closed")?;
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
                .push("shield: dropdown over the live page, Escape, reset, off".into());
            // The switch in Privacy & security: off removes the listener.
            p.command(el, 0, Command::SetCountBlocked(false))?;
            p.phase = 19;
            p.next();
        }
        20 if p.at.elapsed() > Duration::from_millis(500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let engine = &s.tabs.items[0].content.engine;
            if engine.blocked.listening()
                || engine.blocked.count().is_some()
                || s.settings.count_blocked
            {
                return Err("Counting off should remove the listener".into());
            }
            p.command(
                el,
                0,
                Command::Navigate(format!("{url}shield.html?counting-off")),
            )?;
            p.next();
        }
        21 if p.at.elapsed() > Duration::from_secs(2)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.tabs.items[0].content.engine.blocked.listening() {
                return Err("A page listened with counting off".into());
            }
            eval_toolbar(p, SHIELD_REPORT)?;
            p.next();
        }
        22 => {
            let Some(r) = shield_report(p)? else {
                return Ok(());
            };
            if r["badge"] != ""
                || !r["count"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("Counting blocked requests is off")
            {
                return Err(format!("The shield should say counting is off: {r}"));
            }
            p.command(el, 0, Command::SetCountBlocked(true))?;
            p.next();
        }
        23 if p.at.elapsed() > Duration::from_millis(800) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let engine = &s.tabs.items[0].content.engine;
            if !engine.blocked.listening() || engine.blocked.count().is_none() {
                return Err("Counting on again should listen again".into());
            }
            // Removing every extension stops the counting in open pages.
            for id in s
                .settings
                .extensions
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>()
            {
                p.command(el, 0, Command::RemoveExtension(id))?;
            }
            p.phase = 11;
            p.next();
        }
        12 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.settings.extensions.is_empty() {
                let count = s.tabs.items[0].content.engine.blocked.count();
                if count.is_some() || s.tabs.items[0].content.engine.blocked.listening() {
                    return Err(format!(
                        "Counting went on after the last extension: {count:?}"
                    ));
                }
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err(format!("Removing extensions: {:?}", names(s)));
            }
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

const POPUP_BLOCKER: &str = "Rust Browser test popup blocker";

/// Run `script` in the first window's extension popup; the answer lands in `p.result`.
fn eval_extension_popup(p: &mut Probe, script: &str) -> Result<(), String> {
    let s = p.app.session.as_mut().ok_or("No session")?;
    *p.result.lock().unwrap() = None;
    let result = p.result.clone();
    s.windows[0]
        .extension_popup
        .as_ref()
        .and_then(|popup| popup.view.as_ref())
        .ok_or("No popup view")?
        .evaluate_script_with_callback(script, move |value| *result.lock().unwrap() = Some(value))
        .map_err(|_| "Popup script failed".to_string())
}

/// The popup is open, shown, and fits under the shield inside the window.
fn popup_shown(s: &Session) -> Result<bool, String> {
    let window = &s.windows[0];
    let Some(view) = window
        .extension_popup
        .as_ref()
        .filter(|popup| popup.shown)
        .and_then(|popup| popup.view.as_ref())
    else {
        return Ok(false);
    };
    if !view_visible(view) {
        return Err("A shown popup is not visible".into());
    }
    let bounds = view.bounds().map_err(|e| e.to_string())?;
    let scale = window.window.scale_factor();
    let (at, size) = (
        bounds.position.to_logical::<f64>(scale),
        bounds.size.to_logical::<f64>(scale),
    );
    let room = window.window.inner_size().to_logical::<f64>(scale);
    if at.x < -0.5
        || at.y < window.chrome_height - 60.0
        || at.x + size.width > room.width + 0.5
        || at.y + size.height > room.height + 0.5
    {
        return Err(format!(
            "The popup should hang under the shield inside the window: {at:?} {size:?} in {room:?}"
        ));
    }
    // The fixture popup's body is 300 + 2 * 16 pixels wide and taller than the
    // first height: the view fits its content.
    if (size.width - 332.0).abs() > 2.0 || size.height <= 150.0 {
        return Err(format!("The popup should fit its content: {size:?}"));
    }
    Ok(true)
}

/// The shield opens an extension's own popup over the live page, and that popup
/// acts on the page (not on itself). Escape, a press on the page, a second press
/// on the shield and a tab switch close it; a right-click still shows the details.
fn extension_popup_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    let page = format!("{url}shield.html");
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            // WebView2 writes indexed rules into the folder: use a copy.
            let folder = p.root.join("popup-blocker");
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/extensions/popup-blocker");
            for name in ["manifest.json", "rules.json", "popup.html", "popup.js"] {
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
            if names(s) == [(POPUP_BLOCKER.to_string(), "on")]
                && s.extension_rows().iter().all(|row| row.popup)
            {
                p.command(el, 0, Command::Navigate(page))?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(20) {
                return Err(format!(
                    "Adding the popup blocker: {:?}, notice {:?}",
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
            eval_toolbar(p, "document.getElementById('shield').click()")?;
            p.next();
        }
        3 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if popup_shown(s)? {
                if s.windows[0].menu_open || !view_visible(&s.tabs.items[0].content.engine.view) {
                    return Err("The popup should open over the live page, with no dropdown".into());
                }
                eval_extension_popup(p, "document.getElementById('tab').textContent")?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("The extension popup never showed".into());
            }
        }
        4 => {
            let Some(text) = p.result.lock().unwrap().clone() else {
                return Ok(());
            };
            let seen: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if seen != page {
                return Err(format!(
                    "The popup should act on the page ({page}), not on {seen:?}"
                ));
            }
            p.log.push(format!("extension popup: acts on {seen}"));
            eval_toolbar(
                p,
                "document.getElementById('shield').getAttribute('aria-expanded')",
            )?;
            p.next();
        }
        5 => {
            let Some(expanded) = p.result.lock().unwrap().clone() else {
                return Ok(());
            };
            if expanded != "\"true\"" {
                return Err(format!("The shield should read as expanded: {expanded}"));
            }
            // Escape in the popup closes it.
            eval_extension_popup(
                p,
                "document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));true",
            )?;
            p.next();
        }
        6 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows[0].extension_popup.is_none() {
                if s.windows[0].retired_popups.is_empty() {
                    return Err("A closed popup should be retired, not destroyed".into());
                }
                eval_toolbar(p, "document.getElementById('shield').click()")?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(5) {
                return Err("Escape should close the popup".into());
            }
        }
        7 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if popup_shown(s)? {
                // A press on the page takes the focus from the popup.
                s.tabs.items[0]
                    .content
                    .engine
                    .view
                    .focus()
                    .map_err(|e| e.to_string())?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("The popup did not open again".into());
            }
        }
        8 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows[0].extension_popup.is_none() {
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(5) {
                return Err("Focusing the page should close the popup".into());
            }
        }
        // Past the grace period a press on the shield opens it again; a second
        // press while it is open closes it.
        9 if p.at.elapsed() > Duration::from_millis(600) => {
            eval_toolbar(p, "document.getElementById('shield').click()")?;
            p.next();
        }
        10 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if popup_shown(s)? {
                eval_toolbar(p, "document.getElementById('shield').click()")?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("The popup did not open a third time".into());
            }
        }
        11 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.windows[0].extension_popup.is_none() {
                // A right-click shows the shield's details instead.
                eval_toolbar(
                    p,
                    "document.getElementById('shield').dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true}));true",
                )?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(5) {
                return Err("A second press on the shield should close the popup".into());
            }
        }
        12 if p.at.elapsed() > Duration::from_millis(500) => {
            eval_toolbar(p, SHIELD_REPORT)?;
            p.next();
        }
        13 => {
            let Some(r) = shield_report(p)? else {
                return Ok(());
            };
            let limits = r["limits"].as_str().unwrap_or("");
            if r["open"] != true || !limits.contains(POPUP_BLOCKER) {
                return Err(format!("A right-click should show the details: {r}"));
            }
            eval_toolbar(
                p,
                "closeDropdown(false);document.getElementById('shield').click()",
            )?;
            p.next();
        }
        14 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if popup_shown(s)? {
                // A website the popup opens (`chrome.tabs.create`) opens in a new
                // tab, which closes the popup: it acts on one tab.
                eval_extension_popup(
                    p,
                    &format!("chrome.tabs.create({{url:'{url}shield.html?from-popup'}});true"),
                )?;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err("The popup did not open after the details".into());
            }
        }
        15 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let active = s.tabs.active().map(|t| t.url.clone()).unwrap_or_default();
            if s.windows[0].extension_popup.is_none()
                && s.tabs.items.len() == 2
                && active == format!("{url}shield.html?from-popup")
            {
                p.log.push(
                    "extension popup: Escape, page press, shield, new tab close it; links open in a tab"
                        .into(),
                );
                p.done = true;
                el.exit();
            } else if p.at.elapsed() > Duration::from_secs(10) {
                return Err(format!(
                    "A website opened from the popup should be a new active tab: {} tabs, active {active}, popup open {}",
                    s.tabs.items.len(),
                    s.windows[0].extension_popup.is_some()
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_shield_opens_the_extension_popup() {
    let (root, _) = run("extension-popup", extension_popup_steps, |_, _| {});
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
                "JSON.stringify(getComputedStyle(document.getElementById('tabrow')).display+' '+Math.abs(Math.ceil(document.getElementById('chrome').getBoundingClientRect().height)-innerHeight))",
            )?;
            p.next();
        }
        4 => {
            let Some(text) = p.result.lock().unwrap().clone() else {
                return Ok(());
            };
            // No tab strip, and the toolbar view is as tall as its content (no
            // empty band above the page).
            // The script's JSON string, inside the callback's JSON.
            let outer: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let inner: String = serde_json::from_str(&outer).map_err(|e| e.to_string())?;
            let (display, gap) = inner.split_once(' ').unwrap_or((&inner, "?"));
            if display != "none" || gap.parse::<f64>().map_or(true, |g| g > 2.0) {
                return Err(format!("The toolbar beside the column is wrong: {inner}"));
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
            // Page fullscreen with the column and the library open.
            p.command(el, 0, Command::ShowPanel(Panel::History))?;
            p.phase = 19;
            p.next();
        }
        // From a maximized window, as most people browse: full screen must still
        // cover the whole monitor, taskbar included, not just its work area.
        20 if p.at.elapsed() > Duration::from_millis(800) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            s.windows[0].window.set_maximized(true);
            p.phase = 92;
            p.at = Instant::now();
        }
        92 if p.at.elapsed() > Duration::from_millis(800) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if !s.windows[0].window.is_maximized() {
                return Err("The window did not maximize".into());
            }
            let id = s.windows[0].tabs.active().map(|t| t.id).ok_or("No tab")?;
            p.app
                .proxy
                .send_event(Event::Fullscreen(id, true))
                .map_err(|_| "proxy")?;
            p.phase = 21;
            p.at = Instant::now();
        }
        // A fullscreen page (a video) fills the window: the tab column and the
        // library are hidden, and for its first seconds the toolbar shows only a
        // notice naming the site, centred at the top and clipped to it.
        21 if p.at.elapsed() > Duration::from_millis(1500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            check_no_overlap(s, "fullscreen")?;
            let views = shown_views(s);
            let host = s.windows[0]
                .window
                .inner_position()
                .map_err(|e| e.to_string())?;
            let size = s.windows[0].window.inner_size();
            let (width, height) = (size.width as i32, size.height as i32);
            let scale = s.windows[0].window.scale_factor();
            let page = named(&views, "page").ok_or("No page")?;
            let monitor = s.windows[0].window.current_monitor().ok_or("No monitor")?;
            let (screen_at, screen) = (monitor.position(), monitor.size());
            let (window_at, window) = (
                s.windows[0]
                    .window
                    .outer_position()
                    .map_err(|e| e.to_string())?,
                s.windows[0].window.outer_size(),
            );
            if window_at != screen_at
                || window != screen
                || (width, height) != (screen.width as i32, screen.height as i32)
            {
                return Err(format!(
                    "Full screen should cover the whole monitor {screen_at:?} {screen:?}: window {window_at:?} {window:?}, page area {width}x{height}"
                ));
            }
            if s.windows[0].fullscreen.is_none()
                || named(&views, "tabs").is_some()
                || named(&views, "library").is_some()
                || (page.left - host.x).abs() > 2
                || (page.top - host.y).abs() > 2
                || ((page.right - page.left) - width).abs() > 2
                || ((page.bottom - page.top) - height).abs() > 2
            {
                return Err(format!(
                    "A fullscreen page should fill the window: {views:?}"
                ));
            }
            let clip = toolbar_clip(s, 0);
            let notice_ok = named(&views, "toolbar").is_some()
                && clip.is_some_and(|r| {
                    let (w, h) = (f64::from(r.right - r.left), f64::from(r.bottom - r.top));
                    let centre = f64::from(r.left + r.right) / 2.0;
                    w <= 442.0 * scale
                        && (h - 44.0 * scale).abs() <= 3.0
                        && (f64::from(r.top) - 16.0 * scale).abs() <= 2.0
                        && (centre - f64::from(width) / 2.0).abs() <= 2.0
                });
            if !notice_ok {
                return Err(format!(
                    "The toolbar should show only the centred fullscreen notice: {views:?}, clip {clip:?}"
                ));
            }
            eval_toolbar(
                p,
                "JSON.stringify({hidden:document.getElementById('fullscreen-notice').hidden,text:document.getElementById('fullscreen-notice').textContent})",
            )?;
            p.phase = 90;
            p.at = Instant::now();
        }
        90 => {
            let Some(report) = answer(p)? else {
                return Ok(());
            };
            let text = report["text"].as_str().unwrap_or("");
            let s = p.app.session.as_ref().ok_or("No session")?;
            let named_host = s.windows[0]
                .fullscreen_notice
                .as_ref()
                .map(|(_, host)| host.clone())
                .unwrap_or_default();
            let tab_url = s.windows[0]
                .tabs
                .active()
                .map(|t| t.url.clone())
                .unwrap_or_default();
            if report["hidden"] != false
                || named_host != fullscreen_host(&tab_url)
                || !text.starts_with(&named_host)
                || !text.contains("is now full screen")
                || !text.contains("Press Esc to exit")
            {
                return Err(format!(
                    "The fullscreen notice should name the site: {report}"
                ));
            }
            p.next();
        }
        // Once the notice's time is up, the page is all there is.
        91 if p.at.elapsed() > Duration::from_millis(3500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            check_no_overlap(s, "fullscreen after the notice")?;
            let views = shown_views(s);
            if named(&views, "toolbar").is_some() || s.windows[0].fullscreen.is_none() {
                return Err(format!(
                    "After its notice a fullscreen page should be alone: {views:?}"
                ));
            }
            p.log
                .push("fullscreen: page alone, site named for 4 s".into());
            let id = s.windows[0].tabs.active().map(|t| t.id).ok_or("No tab")?;
            p.app
                .proxy
                .send_event(Event::Fullscreen(id, false))
                .map_err(|_| "proxy")?;
            p.phase = 22;
            p.at = Instant::now();
        }
        22 if p.at.elapsed() > Duration::from_millis(1500) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            check_no_overlap(s, "after fullscreen")?;
            // Back to the maximized window it was, its work area clear of the taskbar.
            let monitor = s.windows[0].window.current_monitor().ok_or("No monitor")?;
            if s.windows[0].fullscreen.is_some()
                || s.windows[0].window.fullscreen().is_some()
                || !s.windows[0].window.is_maximized()
                || s.windows[0].window.inner_size().height > monitor.size().height
            {
                return Err("Leaving full screen should restore the maximized window".into());
            }
            s.windows[0].window.set_maximized(false);
            let views = shown_views(s);
            if named(&views, "toolbar").is_none()
                || named(&views, "tabs").is_none()
                || named(&views, "library").is_none()
            {
                return Err(format!(
                    "The toolbar, column and library should come back: {views:?}"
                ));
            }
            p.command(el, 0, Command::CloseLibrary)?;
            p.command(el, 1, Command::SetVerticalTabs(false))?;
            p.phase = 12;
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
        // Start from a settled window, not after a fixed time.
        0 if p.at.elapsed() > Duration::from_secs(1)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
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
                // Another page of the same site, without an icon of its own, must
                // not replace the favourite's icon with the engine's generic one.
                *p.result.lock().unwrap() = Some(favorites[0].icon.clone());
                p.command(el, 0, Command::Navigate(format!("{url}?other-page")))?;
                p.phase = 9;
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(15) {
                // Say whether the engine gave the tab an icon at all, or the icon
                // arrived and was not matched to the favourite.
                let tab = &s.windows[0].tabs.items[0];
                return Err(format!(
                    "The favourite never got its site's icon (tab at {}, loading {}, tab icon {} bytes; favourites: {:?})",
                    tab.url,
                    tab.content.state.loading,
                    tab.content.state.favicon.len(),
                    favorites
                        .iter()
                        .map(|f| (f.url.as_str(), f.icon.len()))
                        .collect::<Vec<_>>()
                ));
            }
        }
        10 if p.at.elapsed() > Duration::from_secs(3)
            && p.app
                .session
                .as_ref()
                .is_some_and(|s| s.windows.iter().all(loaded)) =>
        {
            let saved = p.result.lock().unwrap().clone().unwrap_or_default();
            let s = p.app.session.as_mut().ok_or("No session")?;
            if s.settings.favorites[0].icon != saved {
                return Err("Another page of the site replaced the favourite's icon".into());
            }
            p.command(el, 0, Command::Home)?;
            p.command(
                el,
                0,
                Command::SetNewtabBackground(rust_browser::settings::Background::Dusk),
            )?;
            p.phase = 1;
            p.next();
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
            check_no_overlap(s, "narrow shield dropdown")?;
            if !s.windows[0].menu_open {
                return Err("The shield dropdown did not open".into());
            }
            eval_toolbar(
                p,
                "(()=>{const panel=document.getElementById('shield-panel').getBoundingClientRect(),chrome=document.getElementById('chrome').getBoundingClientRect(),list=document.getElementById('shield-panel');return JSON.stringify({buttonsBottom:list.querySelector('.dropdown-actions').getBoundingClientRect().bottom,open:!list.hidden,lines:document.getElementById('shield-state').textContent.length+document.getElementById('shield-limits').textContent.length,last:lastChromeHeight,width:innerWidth,height:innerHeight,panelBottom:panel.bottom,chromeBottom:chrome.bottom,scrolls:list.scrollHeight>list.clientHeight,overflow:getComputedStyle(list).overflowY})})()",
            )?;
            p.next();
        }
        4 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            let num = |k: &str| r[k].as_f64().unwrap_or(f64::MAX);
            // Everything of the toolbar, panel included, fits in the toolbar view;
            // a panel taller than its room scrolls within itself, so its buttons
            // are inside the view or reachable by scrolling it.
            let scrollable = r["scrolls"] == true && r["overflow"] == "auto";
            if r["open"] != true
                || (num("buttonsBottom") > num("height") + 1.0 && !scrollable)
                || num("lines") < 200.0
                || num("width") > 700.0
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

/// The profile folder a view's WebView2 profile lives in.
fn profile_path(view: &wry::WebView) -> Result<String, String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_13;
    let mut path = windows::core::PWSTR::null();
    // SAFETY: read-only queries on a live view of this fixture, on its own thread.
    unsafe {
        view.webview()
            .cast::<ICoreWebView2_13>()
            .and_then(|v| v.Profile())
            .and_then(|p| p.ProfilePath(&mut path))
            .map_err(|e| e.to_string())?;
    }
    Ok(webview2_com::take_pwstr(path))
}

fn side<'a>(s: &'a Session, role: &str) -> Result<&'a wry::WebView, String> {
    let window = &s.windows[0];
    let found = if role == "tabs" {
        &window.tabs_view
    } else {
        &window.library_view
    };
    found
        .as_ref()
        .map(|v| &v.view)
        .ok_or_else(|| format!("No {role} view"))
}

/// The tab and library columns are built like the toolbar: its profile and
/// engine process, no saved passwords or form data, no way to navigate away from
/// their own document. When the toolbar's engine ends, both come back with the
/// toolbar and show the browser's state again.
fn side_restriction_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    use super::toolbar_validation::{browser_process, terminate};
    let url = p.url.clone();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            p.command(el, 0, Command::Navigate(format!("{url}?side")))?;
            p.command(el, 0, Command::SetVerticalTabs(true))?;
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
        2 if p.at.elapsed() > Duration::from_secs(1) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let page = &s.windows[0].tabs.items[0].content.engine.view;
            let toolbar = &s.windows[0].toolbar;
            for role in ["tabs", "library"] {
                let view = side(s, role)?;
                let forms = native_security::forms(view).map_err(|e| e.to_string())?;
                if forms != Forms::default() {
                    return Err(format!(
                        "The {role} view saves passwords or forms: {forms:?}"
                    ));
                }
                if profile_path(view)? != profile_path(toolbar)?
                    || profile_path(view)? == profile_path(page)?
                {
                    return Err(format!("The {role} view is not in the toolbar profile"));
                }
                if browser_process(view)? != browser_process(toolbar)?
                    || browser_process(view)? == browser_process(page)?
                {
                    return Err(format!(
                        "The {role} view does not share the toolbar's engine"
                    ));
                }
                // Try to leave the trusted document.
                view.evaluate_script(&format!(
                    "location.href={}",
                    serde_json::to_string(&format!("{url}?escape")).map_err(|e| e.to_string())?
                ))
                .map_err(|_| "Script failed")?;
            }
            p.next();
        }
        3 if p.at.elapsed() > Duration::from_millis(1500) => {
            eval_tabs_column(
                p,
                0,
                "JSON.stringify([location.protocol,location.hash,document.documentElement.dataset.role,typeof window.render])",
            )?;
            p.next();
        }
        4 => {
            let Some(text) = p.result.lock().unwrap().clone() else {
                return Ok(());
            };
            let inner: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if inner != r##"["data:","#tabs","tabs","function"]"## {
                return Err(format!("The tab column navigated away: {inner}"));
            }
            eval_library(
                p,
                "JSON.stringify([location.protocol,location.hash,document.documentElement.dataset.role,typeof window.render])",
            )?;
            p.next();
        }
        5 => {
            let Some(text) = p.result.lock().unwrap().clone() else {
                return Ok(());
            };
            let inner: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if inner != r##"["data:","#library","library","function"]"## {
                return Err(format!("The library column navigated away: {inner}"));
            }
            let s = p.app.session.as_mut().ok_or("No session")?;
            // End the toolbar's engine: the toolbar and both columns go with it.
            // Each trusted view's source number names that build of it (a new view
            // can reuse a dropped view's COM address).
            let w = &s.windows[0];
            p.marks = vec![
                w.toolbar_source as usize,
                w.tabs_view.as_ref().map_or(0, |v| v.source as usize),
                w.library_view.as_ref().map_or(0, |v| v.source as usize),
            ];
            terminate(browser_process(&s.windows[0].toolbar)?)?;
            p.next();
        }
        6 => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let w = &s.windows[0];
            let now = [
                Some(w.toolbar_source as usize),
                w.tabs_view.as_ref().map(|v| v.source as usize),
                w.library_view.as_ref().map(|v| v.source as usize),
            ];
            if now
                .iter()
                .zip(&p.marks)
                .all(|(n, old)| n.is_some_and(|n| n != *old))
            {
                p.next();
            } else if p.at.elapsed() > Duration::from_secs(30) {
                return Err(format!(
                    "Not every trusted view was replaced: {now:?} was {:?}",
                    p.marks
                ));
            }
        }
        7 if p.at.elapsed() > Duration::from_secs(2) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            check_no_overlap(s, "after recovery")?;
            let views = shown_views(s);
            if named(&views, "tabs").is_none() || named(&views, "library").is_none() {
                return Err(format!("The columns did not come back: {views:?}"));
            }
            eval_tabs_column(p, 0, COLUMN_REPORT)?;
            p.next();
        }
        8 => {
            let Some(r) = answer(p)? else { return Ok(()) };
            let s = p.app.session.as_mut().ok_or("No session")?;
            if r["role"] != "tabs" || r["count"] != s.windows[0].tabs.items.len() {
                return Err(format!(
                    "The rebuilt tab column does not show the tabs: {r}"
                ));
            }
            eval_library(
                p,
                "JSON.stringify(document.getElementById('manager').hidden?'hidden':document.getElementById('manager-title').textContent)",
            )?;
            p.next();
        }
        9 => {
            let Some(text) = p.result.lock().unwrap().clone() else {
                return Ok(());
            };
            let inner: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if inner != r#""Browsing history""# {
                return Err(format!(
                    "The rebuilt library does not show history: {inner}"
                ));
            }
            p.log
                .push("side views: toolbar restrictions, recovered with the toolbar".into());
            p.marks.clear();
            p.next();
        }
        // A tab column that keeps failing is rebuilt a bounded number of times;
        // then the window shows the tab strip again.
        10 if p.at.elapsed() > Duration::from_millis(400) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if let Some(source) = s.windows[0].tabs_view.as_ref().map(|v| v.source) {
                if p.marks.len() > 8 {
                    return Err("The tab column was rebuilt without limit".into());
                }
                p.marks.push(source as usize);
                p.app
                    .proxy
                    .send_event(Event::ToolbarProcess(
                        source,
                        crate::native_activity::Failure::Renderer,
                    ))
                    .map_err(|_| "proxy")?;
                p.at = Instant::now();
            } else {
                p.next();
            }
        }
        11 if p.at.elapsed() > Duration::from_millis(600) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            check_no_overlap(s, "column given up")?;
            let views = shown_views(s);
            let (toolbar, page) = (named(&views, "toolbar"), named(&views, "page"));
            let host = s.windows[0]
                .window
                .inner_position()
                .map_err(|e| e.to_string())?;
            if named(&views, "tabs").is_some()
                || toolbar.is_none_or(|t| (t.left - host.x).abs() > 2)
                || page.is_none_or(|t| (t.left - host.x).abs() > 2)
                || !s.notice.contains("keeps stopping")
            {
                return Err(format!(
                    "After the last rebuild the strip should return: {views:?}, {:?}",
                    s.notice
                ));
            }
            p.log.push(format!(
                "side views: tab column given up after {} more failures",
                p.marks.len()
            ));
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; ends only the fixture's own toolbar processes"]
fn native_side_views_have_the_toolbar_restrictions_and_recover() {
    let (root, _) = run("side-views", side_restriction_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Whole-window screenshots for the v0.21 review (not a gate). Each shown view is
/// captured by its own engine at the emulated scale and the captures are placed at
/// their layout positions with System.Drawing, so 150% needs no display setting.
/// Writes `<label>-<scene>-<theme>-<scale>.png` to `RUST_BROWSER_SHOTS`.
fn window_shot_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    use rust_browser::settings::Theme;
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
    // The horizontal new-tab page fills the toolbar view alone; `layout_screenshots`
    // captures it (a whole-window capture of it came out blank).
    const SCENES: [&str; 6] = [
        "website",
        "shield",
        "library",
        "vertical-website",
        "vertical-library",
        "vertical-newtab",
    ];
    const SHOTS: [(&str, Theme, f64); 4] = [
        ("light", Theme::Light, 1.0),
        ("light", Theme::Light, 1.5),
        ("dark", Theme::Dark, 1.0),
        ("dark", Theme::Dark, 1.5),
    ];
    const PER_SCENE: u32 = 1 + 3 * SHOTS.len() as u32;
    // Seven scenes of four shots take longer than a test's usual limit.
    p.limit = Duration::from_secs(400);
    let out = PathBuf::from(std::env::var_os("RUST_BROWSER_SHOTS").ok_or("RUST_BROWSER_SHOTS")?);
    let label = std::env::var("RUST_BROWSER_SHOTS_LABEL").unwrap_or_else(|_| "window".into());
    let url = p.url.clone();
    let wait = if p.phase.is_multiple_of(PER_SCENE) {
        2500
    } else {
        1500
    };
    if p.at.elapsed() < Duration::from_millis(wait) {
        return Ok(());
    }
    let (scene, k) = ((p.phase / PER_SCENE) as usize, p.phase % PER_SCENE);
    if scene >= SCENES.len() {
        p.done = true;
        el.exit();
        return Ok(());
    }
    let name = SCENES[scene];
    type Part = (&'static str, (f64, f64, f64, f64));
    fn parts(s: &Session) -> Vec<Part> {
        let plan = s.current_layout();
        let mut found = vec![("toolbar", plan.toolbar)];
        found.extend(plan.page.map(|r| ("page", r)));
        found.extend(plan.tabs.map(|r| ("tabs", r)));
        found.extend(plan.library.map(|r| ("library", r)));
        found
    }
    fn view(s: &Session, part: &str) -> Option<ICoreWebView2> {
        let w = &s.windows[0];
        match part {
            "toolbar" => Some(w.toolbar.webview()),
            "page" => w.tabs.active().map(|t| t.content.engine.view.webview()),
            "tabs" => w.tabs_view.as_ref().map(|v| v.view.webview()),
            _ => w.library_view.as_ref().map(|v| v.view.webview()),
        }
    }
    fn cdp(
        core: &ICoreWebView2,
        method: &str,
        params: String,
        path: Option<PathBuf>,
    ) -> Result<(), String> {
        // SAFETY: UI-thread call on one of this fixture's own views.
        unsafe {
            core.CallDevToolsProtocolMethod(
                &windows::core::HSTRING::from(method),
                &windows::core::HSTRING::from(params),
                &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |_, json| {
                    if let Some(path) = &path
                        && let Ok(value) =
                            serde_json::from_str::<serde_json::Value>(&json.to_string())
                        && let Some(data) = value["data"].as_str()
                        && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data)
                    {
                        let _ = std::fs::write(path, bytes);
                    }
                    Ok(())
                })),
            )
        }
        .map_err(|e| format!("{method}: {e}"))
    }
    if k == 0 {
        // Set the scene up.
        match name {
            "website" => {
                let s = p.app.session.as_mut().ok_or("No session")?;
                s.window
                    .set_window_level(winit::window::WindowLevel::AlwaysOnTop);
                let _ = s
                    .window
                    .request_inner_size(winit::dpi::LogicalSize::new(1280.0, 760.0));
                s.command(Command::AddFavorite {
                    title: "Tile fixture".into(),
                    url: format!("{url}tile.html"),
                })
                .map_err(|e| e.to_string())?;
                s.command(Command::AddFavorite {
                    title: "Library".into(),
                    url: format!("{}?letter", url.replace("127.0.0.1", "localhost")),
                })
                .map_err(|e| e.to_string())?;
                p.command(el, 0, Command::Navigate(format!("{url}tile.html")))?;
                let folder = p.root.join("blocker");
                std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/extensions/blocker");
                for file in ["manifest.json", "rules.json"] {
                    std::fs::copy(source.join(file), folder.join(file))
                        .map_err(|e| e.to_string())?;
                }
                p.app
                    .proxy
                    .send_event(Event::Extension(native_extensions::Reply::Folder(Ok(
                        Some(folder),
                    ))))
                    .map_err(|_| "proxy")?;
            }
            "shield" => {
                p.command(
                    el,
                    0,
                    Command::SetNewtabBackground(rust_browser::settings::Background::Glow),
                )?;
                p.command(el, 0, Command::Navigate(format!("{url}shield.html")))?;
                let s = p.app.session.as_mut().ok_or("No session")?;
                let _ = s.windows[0]
                    .toolbar
                    .evaluate_script("document.getElementById('shield').click()");
            }
            "library" => {
                let s = p.app.session.as_mut().ok_or("No session")?;
                let _ = s.windows[0]
                    .toolbar
                    .evaluate_script("document.getElementById('shield-close').click()");
                p.command(el, 0, Command::ShowPanel(Panel::History))?;
            }
            "vertical-website" => {
                p.command(el, 0, Command::CloseLibrary)?;
                p.command(el, 0, Command::SetVerticalTabs(true))?;
            }
            "vertical-library" => p.command(el, 0, Command::ShowPanel(Panel::History))?,
            _ => {
                p.command(el, 0, Command::CloseLibrary)?;
                p.command(el, 0, Command::NewTab)?;
            }
        }
        p.next();
        return Ok(());
    }
    let (shot, stage) = (((k - 1) / 3) as usize, (k - 1) % 3);
    let (theme_name, theme, scale) = SHOTS[shot];
    let dir = p.root.join("window-shot");
    let s = p.app.session.as_mut().ok_or("No session")?;
    match stage {
        0 => {
            s.settings.theme = theme;
            s.last_render.borrow_mut().clear();
            for side in [&s.windows[0].tabs_view, &s.windows[0].library_view]
                .into_iter()
                .flatten()
            {
                side.last_render.borrow_mut().clear();
            }
            s.render();
            for (part, (_, _, w, h)) in parts(s) {
                if let Some(core) = view(s, part) {
                    let params = serde_json::json!({ "width": w.round() as u32,
                        "height": h.round() as u32, "deviceScaleFactor": scale, "mobile": false });
                    cdp(
                        &core,
                        "Emulation.setDeviceMetricsOverride",
                        params.to_string(),
                        None,
                    )?;
                }
            }
        }
        1 => {
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            for (part, _) in parts(s) {
                if let Some(core) = view(s, part) {
                    cdp(
                        &core,
                        "Page.captureScreenshot",
                        r#"{"format":"png"}"#.into(),
                        Some(dir.join(format!("{part}.png"))),
                    )?;
                }
            }
        }
        _ => {
            let size = s
                .window
                .inner_size()
                .to_logical::<f64>(s.window.scale_factor());
            if size.width < 100.0 || size.height < 100.0 {
                return Err(format!(
                    "{name}: window is {}x{} (minimized: {:?})",
                    size.width,
                    size.height,
                    s.window.is_minimized()
                ));
            }
            let mut draws = Vec::new();
            for (part, (x, y, w, h)) in parts(s) {
                let file = dir.join(format!("{part}.png"));
                if !file.exists() {
                    return Err(format!("{name}: no capture of {part}"));
                }
                draws.push(format!(
                    "@('{}',{},{},{},{})",
                    file.display(),
                    (x * scale).round(),
                    (y * scale).round(),
                    (w * scale).round(),
                    (h * scale).round()
                ));
            }
            let target = out.join(format!(
                "{label}-{name}-{theme_name}-{}.png",
                (scale * 100.0) as u32
            ));
            let script = format!(
                "Add-Type -AssemblyName System.Drawing; $b=New-Object System.Drawing.Bitmap({},{}); $g=[System.Drawing.Graphics]::FromImage($b); foreach($d in @({})){{$i=[System.Drawing.Image]::FromFile($d[0]); $g.DrawImage($i,[int]$d[1],[int]$d[2],[int]$d[3],[int]$d[4]); $i.Dispose()}}; $b.Save('{}',[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $b.Dispose()",
                (size.width * scale).round(),
                (size.height * scale).round(),
                draws.join(","),
                target.display()
            );
            // A capture may still be being written: try a few times.
            let mut error = String::new();
            for _ in 0..4 {
                let output = std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .output()
                    .map_err(|e| e.to_string())?;
                if output.status.success() {
                    error.clear();
                    break;
                }
                error = String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(400)
                    .collect();
                std::thread::sleep(Duration::from_millis(700));
            }
            if !error.is_empty() {
                return Err(format!("{name}: composing failed: {error}"));
            }
            for (part, _) in parts(s) {
                if let Some(core) = view(s, part) {
                    cdp(
                        &core,
                        "Emulation.clearDeviceMetricsOverride",
                        "{}".into(),
                        None,
                    )?;
                }
            }
            p.log.push(format!("wrote {}", target.display()));
        }
    }
    p.next();
    Ok(())
}

#[test]
#[ignore = "Layout review tool: writes whole-window screenshots to RUST_BROWSER_SHOTS"]
fn window_screenshots() {
    let (root, _) = run("window-shots", window_shot_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// A private window takes one setting from the normal profile, read-only: tabs
/// down the side. Nothing else is read (favourites, theme), the switch is refused
/// there, and the normal session file is never written.
fn private_vertical_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            if !s.private {
                return Err("The fixture did not start a private window".into());
            }
            if !s.settings.vertical_tabs
                || !s.settings.favorites.is_empty()
                || s.settings.theme != rust_browser::settings::Settings::default().theme
            {
                return Err(format!(
                    "A private window should take only vertical tabs: {:?}",
                    s.settings
                ));
            }
            if s.command(Command::SetVerticalTabs(false)).is_ok()
                || s.command(Command::SetCountBlocked(false)).is_ok()
            {
                return Err("A private window changed a setting it only reads".into());
            }
            p.next();
        }
        1 if p.at.elapsed() > Duration::from_secs(1) => {
            let s = p.app.session.as_mut().ok_or("No session")?;
            let views = shown_views(s);
            if named(&views, "tabs").is_none() {
                return Err(format!(
                    "The private window should show the tab column: {views:?}"
                ));
            }
            check_no_overlap(s, "private vertical")?;
            p.log
                .push("private: vertical tabs read from the normal profile only".into());
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real private window"]
fn native_private_windows_follow_vertical_tabs_read_only() {
    let (root, _) = run("private-vertical", private_vertical_steps, |root, _| {
        let snapshot = Snapshot {
            settings: rust_browser::settings::Settings {
                vertical_tabs: true,
                theme: rust_browser::settings::Theme::Plum,
                favorites: vec![rust_browser::settings::Favorite {
                    title: "Normal favourite".into(),
                    url: "https://example.com/".into(),
                    icon: String::new(),
                }],
                ..Default::default()
            },
            ..Snapshot::default()
        };
        storage::save(&root.join("session.json"), &snapshot).unwrap();
        std::fs::copy(root.join("session.json"), root.join("session-before.json")).unwrap();
    });
    let before = std::fs::read(root.join("session-before.json")).unwrap();
    let saved = storage::load(&root.join("session.json")).unwrap();
    assert!(saved.settings.vertical_tabs && saved.settings.favorites.len() == 1);
    // Nothing was written to the normal session.
    let after = std::fs::read(root.join("session.json")).unwrap();
    assert_eq!(before, after);
    let _ = std::fs::remove_dir_all(root);
}

/// Going back from a page to the start of a new tab (as the mouse's back button
/// does, inside the page) shows the new-tab page again, not a blank `about:blank`;
/// Forward returns to the page.
fn back_to_new_tab_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    let waited = p.at.elapsed();
    match p.phase {
        0 if p.at.elapsed() > Duration::from_secs(1) => {
            let s = p.s()?;
            if s.tabs.items.len() != 1 || s.tabs.items[0].url != navigation::HOME {
                return Err(format!("Expected one new tab: {:?}", urls(&s.windows[0])));
            }
            // The back entry is the new tab's blank document, so it must have finished
            // loading first: a navigation that starts while it is still loading replaces
            // it instead of adding to the history. A slow machine is not done at 1s.
            if s.tabs.items[0].content.state.loading {
                if p.started.elapsed() > Duration::from_secs(20) {
                    return Err("The new tab's blank document did not finish loading".into());
                }
                return Ok(());
            }
            p.command(el, 0, Command::Navigate(format!("{url}?back-home")))?;
            p.next();
        }
        1 => {
            let s = p.s()?;
            let tab = &s.tabs.items[0];
            let loaded = tab.url.ends_with("?back-home") && !tab.content.state.loading;
            // WebView2 reports history changes in an event of their own, separate from
            // the load: wait for the back entry rather than reading it once.
            if loaded && tab.content.state.back {
                // Engine-initiated, like the mouse's back button: no toolbar command.
                tab.content
                    .engine
                    .view
                    .evaluate_script("history.back()")
                    .map_err(|_| "Back script failed".to_string())?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(if loaded {
                    "The page has no back entry to the new tab".into()
                } else {
                    format!("The page did not load: {}", tab.url)
                });
            }
        }
        2 => {
            let s = p.s()?;
            let tab = &s.tabs.items[0];
            if tab.url == navigation::HOME && !tab.content.state.loading {
                if s.effective_panel() != Some(Panel::NewTab) {
                    return Err("The new-tab page is not shown".into());
                }
                if view_visible(&tab.content.engine.view) {
                    return Err("The blank page stayed visible over the new-tab page".into());
                }
                if tab.title != "New tab" || !tab.content.state.forward {
                    return Err(format!(
                        "Wrong title or no way forward: {:?} {}",
                        tab.title, tab.content.state.forward
                    ));
                }
                p.command(el, 0, Command::Forward)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!(
                    "Back did not return to the new-tab page: {} ({:?})",
                    tab.url,
                    tab.content.engine.view.url()
                ));
            }
        }
        3 => {
            let s = p.s()?;
            let tab = &s.tabs.items[0];
            if tab.url.ends_with("?back-home") && !tab.content.state.loading {
                if s.effective_panel().is_some() || !view_visible(&tab.content.engine.view) {
                    return Err("Forward did not show the page again".into());
                }
                p.log.push(
                    "back: the start of a new tab is the new-tab page; Forward returns".into(),
                );
                p.done = true;
                el.exit();
            } else if waited > Duration::from_secs(10) {
                return Err(format!("Forward did not return to the page: {}", tab.url));
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_back_to_the_start_of_a_new_tab_shows_the_new_tab_page() {
    let (root, _) = run("back-home", back_to_new_tab_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Records which tab cards the toolbar animates, so the check does not depend on
/// catching a 180 ms animation while it runs.
const RECORD_GROWTH: &str = "window.__grown=[];const animate=Element.prototype.animate;Element.prototype.animate=function(...a){if(this.dataset.tabId)window.__grown.push(this.dataset.tabId);return animate.apply(this,a)}";

/// A tab opened with the + button grows into the strip; tabs already shown do not
/// animate again on later renders, and nothing animates with reduced motion.
fn tab_growth_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let waited = p.at.elapsed();
    match p.phase {
        0 if waited > Duration::from_secs(1) => {
            let s = p.s()?;
            s.windows[0]
                .toolbar
                .evaluate_script(RECORD_GROWTH)
                .map_err(|_| "Record script failed".to_string())?;
            // Windows with animations turned off (CI runners) ask for reduced
            // motion; this part checks the animation itself.
            toolbar_cdp(
                s,
                "Emulation.setEmulatedMedia",
                serde_json::json!({ "features": [{ "name": "prefers-reduced-motion", "value": "no-preference" }] }),
            )?;
            p.next();
        }
        1 if waited > Duration::from_millis(300) => {
            p.command(el, 0, Command::NewTab)?;
            p.next();
        }
        2 if waited > Duration::from_millis(700) => {
            eval_toolbar(p, "JSON.stringify(window.__grown)")?;
            p.next();
        }
        3 => {
            if let Some(grown) = polish_report(p)? {
                let s = p.s()?;
                let new = s.tabs.items.last().ok_or("No tab")?.id;
                if s.tabs.items.len() != 2 || grown != serde_json::json!([new.to_string()]) {
                    return Err(format!("Only the new tab {new:?} should grow: {grown}"));
                }
                toolbar_cdp(
                    s,
                    "Emulation.setEmulatedMedia",
                    serde_json::json!({ "features": [{ "name": "prefers-reduced-motion", "value": "reduce" }] }),
                )?;
                p.next();
            }
        }
        4 if waited > Duration::from_millis(300) => {
            p.command(el, 0, Command::NewTab)?;
            p.next();
        }
        5 if waited > Duration::from_millis(700) => {
            eval_toolbar(p, "JSON.stringify(window.__grown)")?;
            p.next();
        }
        6 => {
            if let Some(grown) = polish_report(p)? {
                let s = p.s()?;
                if s.tabs.items.len() != 3 || grown.as_array().map(Vec::len) != Some(1) {
                    return Err(format!("A tab grew with reduced motion: {grown}"));
                }
                p.log
                    .push("tabs: a new tab grows into place; not with reduced motion".into());
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
fn native_new_tabs_grow_into_the_tab_strip() {
    let (root, _) = run("tab-growth", tab_growth_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Right-click the first tab: its menu opens with focus on its first row and acts on
/// that tab, not the selected one. Duplicate opens a copy beside it; Pin pins it;
/// Close other tabs keeps it (and pinned tabs); middle-click closes a tab, a pinned one
/// included. Script results come back as JSON through `polish_report`.
const TAB_MENU_OPEN: &str = "(()=>{const item=document.querySelectorAll('#tabs .tab')[0];item.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:40,clientY:20}));const m=document.getElementById('tab-menu');return JSON.stringify({open:!m.hidden,focus:document.activeElement.id,pin:document.getElementById('tab-menu-pin').textContent,others:!document.getElementById('tab-menu-others').disabled})})()";
fn tab_menu_click(button: &str) -> String {
    format!(
        "(()=>{{const item=document.querySelectorAll('#tabs .tab')[0];item.dispatchEvent(new MouseEvent('contextmenu',{{bubbles:true,cancelable:true,clientX:40,clientY:20}}));document.getElementById('{button}').click();return JSON.stringify({{closed:document.getElementById('tab-menu').hidden}})}})()"
    )
}
fn middle_click(index: usize) -> String {
    format!(
        "(()=>{{const item=document.querySelectorAll('#tabs .tab')[{index}];item.dispatchEvent(new MouseEvent('auxclick',{{bubbles:true,cancelable:true,button:1}}));return JSON.stringify({{sent:true}})}})()"
    )
}

fn tab_menu_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let waited = p.at.elapsed();
    let url = p.url.clone();
    let tabs = |p: &mut Probe| -> Result<Vec<(String, bool, bool)>, String> {
        let s = p.s()?;
        let active = s.tabs.active().map(|t| t.id);
        Ok(s.tabs
            .items
            .iter()
            .map(|t| (t.url.clone(), t.pinned, Some(t.id) == active))
            .collect())
    };
    match p.phase {
        0 if waited > Duration::from_secs(1) => {
            p.command(el, 0, Command::Navigate(url))?;
            p.command(el, 0, Command::NewTab)?;
            p.command(el, 0, Command::NewTab)?;
            p.next();
        }
        1 if waited > Duration::from_millis(1500) => {
            eval_toolbar(p, TAB_MENU_OPEN)?;
            p.next();
        }
        2 => {
            if let Some(r) = polish_report(p)? {
                if r["open"] != true
                    || r["focus"] != "tab-menu-duplicate"
                    || r["pin"] != "Pin tab"
                    || r["others"] != true
                {
                    return Err(format!("The tab menu opened wrongly: {r}"));
                }
                eval_toolbar(p, &tab_menu_click("tab-menu-duplicate"))?;
                p.next();
            }
        }
        3 if waited > Duration::from_millis(1200) => {
            let t = tabs(p)?;
            // The copy sits right after its source and is selected.
            if t.len() != 4 || t[1].0 != t[0].0 || !t[1].2 {
                return Err(format!("Duplicate went wrong: {t:?}"));
            }
            eval_toolbar(p, &middle_click(3))?;
            p.next();
        }
        4 if waited > Duration::from_millis(800) => {
            let t = tabs(p)?;
            if t.len() != 3 {
                return Err(format!("Middle-click did not close the tab: {t:?}"));
            }
            eval_toolbar(p, &tab_menu_click("tab-menu-pin"))?;
            p.next();
        }
        5 if waited > Duration::from_millis(800) => {
            let t = tabs(p)?;
            // The first tab is pinned; the selected tab stays the copy.
            if !t[0].1 || !t[1].2 {
                return Err(format!("Pin from the tab menu went wrong: {t:?}"));
            }
            eval_toolbar(p, &tab_menu_click("tab-menu-others"))?;
            p.next();
        }
        6 if waited > Duration::from_millis(1200) => {
            let t = tabs(p)?;
            if t.len() != 1 || !t[0].1 || !t[0].2 {
                return Err(format!("Close other tabs went wrong: {t:?}"));
            }
            eval_toolbar(p, &middle_click(0))?;
            p.next();
        }
        7 if waited > Duration::from_millis(1200) => {
            let t = tabs(p)?;
            // Closing the last tab leaves a new, unpinned tab behind.
            if t.len() != 1 || t[0].1 {
                return Err(format!("Middle-click on the pinned tab went wrong: {t:?}"));
            }
            p.log
                .push("tab menu: duplicate, pin, close others; middle-click closes".into());
            p.done = true;
            el.exit();
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_tab_menu_and_middle_click() {
    let (root, _) = run("tab-menu", tab_menu_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

type Rect = windows::Win32::Foundation::RECT;
/// Hit-test answers by edge name, the client area and the window, in screen pixels.
type EdgeHits = (Vec<(&'static str, isize)>, Rect, Rect);

/// What Windows answers `WM_NCHITTEST` with just outside each edge of the client
/// area, inside it, and the client and window rectangles (screen pixels).
fn edge_hits(window: &Window) -> Result<EdgeHits, String> {
    use windows::Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT, WPARAM},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, SendMessageW, WM_NCHITTEST},
    };
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let hwnd = match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => HWND(handle.hwnd.get() as *mut _),
        _ => return Err("No window handle".into()),
    };
    let mut client = RECT::default();
    let mut outer = RECT::default();
    let mut origin = POINT::default();
    // SAFETY: the fixture's own live window, on its own thread.
    unsafe {
        GetClientRect(hwnd, &mut client).map_err(|e| e.to_string())?;
        GetWindowRect(hwnd, &mut outer).map_err(|e| e.to_string())?;
        let _ = ClientToScreen(hwnd, &mut origin);
    }
    let (left, top) = (origin.x, origin.y);
    let (right, bottom) = (left + client.right, top + client.bottom);
    let (mid_x, mid_y) = ((left + right) / 2, (top + bottom) / 2);
    let at = |x: i32, y: i32| {
        let packed = ((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF);
        // SAFETY: as above; WM_NCHITTEST only reads the point.
        unsafe {
            SendMessageW(
                hwnd,
                WM_NCHITTEST,
                Some(WPARAM(0)),
                Some(LPARAM(packed as isize)),
            )
            .0
        }
    };
    let screen = RECT {
        left,
        top,
        right,
        bottom,
    };
    Ok((
        vec![
            ("left", at(left - 2, mid_y)),
            ("right", at(right + 1, mid_y)),
            ("bottom", at(mid_x, bottom + 1)),
            ("bottom-right", at(right + 1, bottom + 1)),
            ("inside", at(mid_x, mid_y)),
        ],
        screen,
        outer,
    ))
}

/// A restored window resizes from its left, right and bottom edges and bottom corner
/// (Windows' invisible borders, outside the page), while the page fills the client
/// area; a maximized window has no borders and its client area is the whole window.
fn window_edges_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    const HTCLIENT: isize = 1;
    const HTLEFT: isize = 10;
    const HTRIGHT: isize = 11;
    const HTBOTTOM: isize = 15;
    const HTBOTTOMRIGHT: isize = 17;
    let waited = p.at.elapsed();
    match p.phase {
        0 if waited > Duration::from_secs(1) => {
            let s = p.s()?;
            let (hits, client, outer) = edge_hits(&s.windows[0].window)?;
            let want = [
                ("left", HTLEFT),
                ("right", HTRIGHT),
                ("bottom", HTBOTTOM),
                ("bottom-right", HTBOTTOMRIGHT),
                ("inside", HTCLIENT),
            ];
            if hits != want {
                return Err(format!("Edges do not resize: {hits:?}"));
            }
            // The caption stays the window's top edge; the borders sit outside the
            // client area on the other three sides.
            if client.top != outer.top
                || client.left <= outer.left
                || client.right >= outer.right
                || client.bottom >= outer.bottom
            {
                return Err(format!(
                    "Unexpected frame: client {client:?}, window {outer:?}"
                ));
            }
            s.windows[0].window.set_maximized(true);
            p.next();
        }
        1 => {
            let s = p.s()?;
            if s.windows[0].window.is_maximized() {
                let (hits, _, _) = edge_hits(&s.windows[0].window)?;
                let size = s.windows[0].window.inner_size();
                let monitor = s.windows[0]
                    .window
                    .current_monitor()
                    .ok_or("No monitor")?
                    .size();
                // Maximized: the page reaches the screen's edges (minus the taskbar),
                // with no resize border left over at the sides.
                if size.width != monitor.width || hits[4].1 != HTCLIENT {
                    return Err(format!(
                        "Maximized window has borders: {size:?} on {monitor:?}, {hits:?}"
                    ));
                }
                p.log
                    .push("window edges: left, right, bottom resize; maximized fills".into());
                p.done = true;
                el.set_control_flow(winit::event_loop::ControlFlow::Poll);
                el.exit();
            } else if waited > Duration::from_secs(10) {
                return Err("The window did not maximize".into());
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_window_edges_resize() {
    let (root, _) = run("window-edges", window_edges_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// Right-click a bookmarks-bar button (by its label): its menu opens at the pointer
/// with focus on its first row. Reports the menu's shown rows as JSON.
fn bookmark_menu_open(label: &str) -> String {
    format!(
        "(()=>{{const b=[...document.querySelectorAll('#bar-items .bar-item')].find(b=>b.textContent==='{label}');if(!b)return JSON.stringify({{missing:true}});const r=b.getBoundingClientRect();b.dispatchEvent(new MouseEvent('contextmenu',{{bubbles:true,cancelable:true,clientX:r.left+4,clientY:r.bottom}}));const m=document.getElementById('bm-menu'),q=m.getBoundingClientRect();return JSON.stringify({{open:!m.hidden,focus:document.activeElement.id,atPointer:Math.abs(q.left-(r.left+4))<=1&&Math.abs(q.top-r.bottom)<=1,items:[...m.querySelectorAll('button')].filter(x=>!x.hidden).map(x=>x.textContent)}})}})()"
    )
}
/// Click a row of the open bookmark menu.
fn bookmark_menu_click(id: &str) -> String {
    format!(
        "(()=>{{document.getElementById('{id}').click();return JSON.stringify({{menu:!document.getElementById('bm-menu').hidden,editor:!document.getElementById('bm-editor').hidden,title:document.getElementById('bm-editor-title').textContent,name:document.getElementById('bm-name').value,url:document.getElementById('bm-url').value}})}})()"
    )
}
const BOOKMARK_OPEN_ALL: &str = "JSON.stringify({label:document.getElementById('bm-open-all').textContent,off:document.getElementById('bm-open-all').disabled})";
fn bookmark_editor_submit(name: &str) -> String {
    format!(
        "(()=>{{document.getElementById('bm-name').value='{name}';document.getElementById('bm-editor-form').requestSubmit();return JSON.stringify({{closed:document.getElementById('bm-editor').hidden}})}})()"
    )
}

/// The bookmarks bar's right-click menus. A bookmark's menu opens it in a new tab,
/// window or private window, edits and deletes it; a folder's menu opens all its
/// bookmarks in tabs and adds a folder inside it.
fn bookmark_menu_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let waited = p.at.elapsed();
    let url = p.url.clone();
    let link_rows = serde_json::json!([
        "Open in new tab",
        "Open in new window",
        "Open in new private window",
        "Edit bookmark…",
        "Delete bookmark",
        "Copy link",
        "Add bookmark…",
        "Add folder…",
        "Manage bookmarks"
    ]);
    let bar_labels = |p: &mut Probe| -> Result<Vec<String>, String> {
        Ok(p.s()?
            .library_data
            .bar
            .iter()
            .map(|item| match item {
                rust_browser::library::BarItem::Link { title, .. } => title.clone(),
                rust_browser::library::BarItem::Folder { name, .. } => format!("[{name}]"),
            })
            .collect())
    };
    match p.phase {
        0 if waited > Duration::from_secs(1) => {
            let file = format!(
                "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<DL><p>\n<DT><H3 PERSONAL_TOOLBAR_FOLDER=\"true\">Bookmarks bar</H3>\n<DL><p><DT><A HREF=\"{url}?bar-link\">Bar link</A>\n<DT><H3>Work</H3><DL><p><DT><A HREF=\"{url}?work-1\">Work one</A><DT><A HREF=\"{url}?work-2\">Work two</A></DL><p></DL><p>\n</DL><p>\n"
            );
            p.app
                .proxy
                .send_event(Event::BookmarkFile(Ok(Some(file.into_bytes()))))
                .map_err(|_| "proxy")?;
            p.next();
        }
        1 => {
            let s = p.s()?;
            if s.notice.starts_with("Bookmarks imported") && !s.library_data.bar.is_empty() {
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!("The bar did not appear: {:?}", s.notice));
            }
        }
        2 if waited > Duration::from_millis(500) => {
            eval_toolbar(p, &bookmark_menu_open("Bar link"))?;
            p.next();
        }
        3 => {
            if let Some(r) = polish_report(p)? {
                if r["open"] != true
                    || r["focus"] != "bm-open-tab"
                    || r["atPointer"] != true
                    || r["items"] != link_rows
                {
                    return Err(format!("The bookmark menu opened wrongly: {r}"));
                }
                eval_toolbar(p, &bookmark_menu_click("bm-open-private"))?;
                p.next();
            }
        }
        4 if waited > Duration::from_millis(500) => {
            let links = super::OPENED_LINKS.with(|o| o.borrow().clone());
            let opened = super::OPENED_PROFILES.with(|o| o.borrow().clone());
            if links != [format!("{url}?bar-link")] || !opened.last().is_some_and(|o| o.1) {
                return Err(format!(
                    "Open in new private window went wrong: {links:?} {opened:?}"
                ));
            }
            eval_toolbar(p, &bookmark_menu_open("Work"))?;
            p.next();
        }
        5 => {
            if let Some(r) = polish_report(p)? {
                let rows = r["items"].as_array().cloned().unwrap_or_default();
                if r["open"] != true
                    || r["focus"] != "bm-edit"
                    || rows.first() != Some(&serde_json::json!("Open all in tabs"))
                    || !rows.contains(&serde_json::json!("Rename folder…"))
                    || !rows.contains(&serde_json::json!("Delete folder…"))
                    || rows.contains(&serde_json::json!("Copy link"))
                {
                    return Err(format!("The folder menu opened wrongly: {r}"));
                }
                eval_toolbar(p, BOOKMARK_OPEN_ALL)?;
                p.next();
            }
        }
        // Open all counts the folder's bookmarks once the browser has read them.
        6 => {
            if let Some(r) = polish_report(p)? {
                if r["label"] == "Open all in tabs (2)" && r["off"] == false {
                    eval_toolbar(p, &bookmark_menu_click("bm-open-all"))?;
                    p.next();
                } else if waited > Duration::from_secs(10) {
                    return Err(format!("Open all never counted the folder: {r}"));
                } else {
                    eval_toolbar(p, BOOKMARK_OPEN_ALL)?;
                }
            }
        }
        7 if waited > Duration::from_millis(1200) => {
            let s = p.s()?;
            let urls: Vec<&str> = s.tabs.items.iter().map(|t| t.url.as_str()).collect();
            let active = s.tabs.active().map(|t| t.url.as_str()).unwrap_or_default();
            if urls.len() != 3
                || !urls[1].ends_with("?work-1")
                || !urls[2].ends_with("?work-2")
                || !active.ends_with("?work-1")
            {
                return Err(format!("Open all went wrong: {urls:?}, selected {active}"));
            }
            eval_toolbar(p, &bookmark_menu_open("Bar link"))?;
            p.next();
        }
        8 => {
            if polish_report(p)?.is_some() {
                eval_toolbar(p, &bookmark_menu_click("bm-edit"))?;
                p.next();
            }
        }
        9 => {
            if let Some(r) = polish_report(p)? {
                if r["menu"] != false
                    || r["editor"] != true
                    || r["title"] != "Edit bookmark"
                    || r["name"] != "Bar link"
                    || r["url"] != format!("{url}?bar-link")
                {
                    return Err(format!("The bookmark editor opened wrongly: {r}"));
                }
                eval_toolbar(p, &bookmark_editor_submit("Renamed"))?;
                p.next();
            }
        }
        10 => {
            let labels = bar_labels(p)?;
            if labels.first().map(String::as_str) == Some("Renamed") {
                eval_toolbar(p, &bookmark_menu_open("Renamed"))?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!("Edit bookmark did not rename it: {labels:?}"));
            }
        }
        11 => {
            if polish_report(p)?.is_some() {
                eval_toolbar(p, &bookmark_menu_click("bm-add-folder"))?;
                p.next();
            }
        }
        12 => {
            if let Some(r) = polish_report(p)? {
                if r["editor"] != true || r["title"] != "Add folder" || r["name"] != "" {
                    return Err(format!("Add folder opened wrongly: {r}"));
                }
                eval_toolbar(p, &bookmark_editor_submit("Reading"))?;
                p.next();
            }
        }
        // The bookmark is in the bar folder, so the new folder goes there too and
        // shows on the bar after Work.
        13 => {
            let labels = bar_labels(p)?;
            if labels == ["Renamed", "[Work]", "[Reading]"] {
                eval_toolbar(p, &bookmark_menu_open("Renamed"))?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!("Add folder went wrong: {labels:?}"));
            }
        }
        14 => {
            if polish_report(p)?.is_some() {
                eval_toolbar(p, &bookmark_menu_click("bm-open-window"))?;
                p.next();
            }
        }
        15 => {
            let s = p.s()?;
            if s.windows.len() == 2 {
                let tab = s
                    .tabs
                    .items
                    .iter()
                    .map(|t| t.url.clone())
                    .collect::<Vec<_>>();
                if tab.len() != 1 || !tab[0].ends_with("?bar-link") {
                    return Err(format!("The new window opened the wrong page: {tab:?}"));
                }
                p.log
                    .push("bookmark menus: private, open all, edit, add folder, new window".into());
                p.done = true;
                el.set_control_flow(winit::event_loop::ControlFlow::Poll);
                el.exit();
            } else if waited > Duration::from_secs(10) {
                return Err(format!(
                    "Open in new window did not open one: {}",
                    s.windows.len()
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_bookmark_menus() {
    let (root, _) = run("bookmark-menus", bookmark_menu_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// The open folder dropdown: its rows, whether it sits under its button, and
/// whether its first row is the way back.
const FOLDER_REPORT: &str = "(()=>{const p=document.getElementById('folder-panel'),rows=[...p.querySelectorAll('button')],a=[...document.querySelectorAll('#bar-items .bar-item')].find(b=>b.getAttribute('aria-expanded')==='true'),r=p.getBoundingClientRect(),q=a?a.getBoundingClientRect():{bottom:-99,left:-99};return JSON.stringify({rows:rows.map(b=>b.textContent),icons:rows.map(b=>b.querySelector('img')?'img':b.querySelector('.bm-letter')?'letter:'+b.querySelector('.bm-letter').dataset.letter:'svg'),back:!!rows[0]&&rows[0].classList.contains('folder-back'),under:!p.hidden&&r.top>=q.bottom&&r.top-q.bottom<12&&Math.abs(r.left-q.left)<=1,expanded:a?a.getAttribute('aria-expanded'):''})})()";

/// The bookmarks bar's buttons as `{labels, hidden}` JSON (only the shown buttons).
const BAR_REPORT: &str = r#"JSON.stringify({hidden:document.getElementById('bookmarks-bar').hidden,labels:[...document.querySelectorAll('#bar-items .bar-item')].filter(b=>!b.hidden).map(b=>b.textContent),barLinkImg:!!document.querySelector('#bar-items .bar-item img[src^="data:image/png;base64,"]'),noGlobe:!document.querySelector('#bar-items .icon-favicon'),setting:document.getElementById('bookmarks-bar-setting').checked})"#;

/// The toolbar's own measure of its height (as it reports it to the browser), and
/// whether the bookmarks bar is hidden and the notice empty, as JSON.
const SETTLED_REPORT: &str = "JSON.stringify({height:Math.min(320,Math.ceil(document.getElementById('chrome').getBoundingClientRect().height)),bar:document.getElementById('bookmarks-bar').hidden,notice:document.getElementById('notice').textContent})";

/// Imported bookmarks appear on a bar under the address line: the exporting
/// browser's bar folder opened, then the other folders. A link opens in the tab,
/// a folder opens the library column showing it, and the setting hides the bar;
/// the page moves down and back up and nothing overlaps.
fn bar_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    let waited = p.at.elapsed();
    let first = p.marks.first().copied().unwrap_or_default();
    let click = |p: &mut Probe, label: &str| -> Result<(), String> {
        let script = format!(
            "[...document.querySelectorAll('#bar-items .bar-item')].find(b=>b.textContent==={})?.click()",
            serde_json::to_string(label).unwrap()
        );
        p.s()?.windows[0]
            .toolbar
            .evaluate_script(&script)
            .map_err(|_| "Bar click failed".to_string())
    };
    match p.phase {
        // The height before the import is the baseline that the bar must add to and
        // that hiding the bar must return to. Take it only once the toolbar has
        // settled: two measurements of its own height in a row that agree with the
        // height the browser holds, with no bar and no notice. A slow runner can
        // still be mid-layout at a fixed delay.
        0 if waited > Duration::from_millis(500) => {
            let Some(r) = polish_report(p)? else {
                eval_toolbar(p, SETTLED_REPORT)?;
                p.at = Instant::now();
                return Ok(());
            };
            let held = p.s()?.chrome_height as usize;
            let agrees =
                r["height"].as_u64() == Some(held as u64) && r["bar"] == true && r["notice"] == "";
            if !agrees || p.marks.last() != Some(&held) {
                // Not settled yet: remember this height and measure again.
                p.marks = if agrees { vec![held] } else { Vec::new() };
                // A measurement that agrees always gets its confirming second one;
                // only a toolbar that still disagrees runs out of time.
                if !agrees && p.started.elapsed() > Duration::from_secs(30) {
                    return Err(format!("The toolbar did not settle: {r} {held}"));
                }
                eval_toolbar(p, SETTLED_REPORT)?;
                p.at = Instant::now();
                return Ok(());
            }
            let file = format!(
                "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<DL><p>\n<DT><H3 PERSONAL_TOOLBAR_FOLDER=\"true\">Barre de favoris</H3>\n<DL><p><DT><A HREF=\"{url}?bar-link\" ICON=\"data:image/png;base64,iVBORw0KGgo=\">Bar link</A>\n<DT><H3>Work</H3><DL><p><DT><H3>Deep</H3><DL><p><DT><A HREF=\"{url}?deep\" ICON=\"data:image/png;base64,iVBORw0KGgo=\">Deep link</A></DL><p><DT><A HREF=\"{url}?work\">Work link</A></DL><p></DL><p>\n<DT><H3>Other</H3><DL><p><DT><A HREF=\"{url}?other\">Other link</A></DL><p>\n</DL><p>\n"
            );
            p.app
                .proxy
                .send_event(Event::BookmarkFile(Ok(Some(file.into_bytes()))))
                .map_err(|_| "proxy")?;
            p.next();
        }
        1 => {
            let s = p.s()?;
            if s.notice.starts_with("Bookmarks imported") && s.chrome_height as usize > first {
                eval_toolbar(p, BAR_REPORT)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!(
                    "The bar did not appear: {:?} {} (before: {first})",
                    s.notice, s.chrome_height
                ));
            }
        }
        2 => {
            if let Some(r) = polish_report(p)? {
                if r["hidden"] != false
                    || r["labels"] != serde_json::json!(["Bar link", "Work", "Other"])
                    || r["barLinkImg"] != true
                    || r["noGlobe"] != true
                    || r["setting"] != true
                {
                    return Err(format!("Wrong bookmarks bar: {r}"));
                }
                check_no_overlap(p.s()?, "bar shown")?;
                click(p, "Bar link")?;
                p.next();
            }
        }
        3 => {
            let s = p.s()?;
            if s.tabs.items[0].url.ends_with("?bar-link") && s.effective_panel().is_none() {
                click(p, "Work")?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!(
                    "The bar link did not open: {}",
                    s.tabs.items[0].url
                ));
            }
        }
        4 => {
            let s = p.s()?;
            let open = s.windows[0].menu_open && s.windows[0].library_panel.is_none();
            if open
                && s.library_data
                    .menu
                    .as_ref()
                    .is_some_and(|m| m.name == "Work")
            {
                eval_toolbar(p, FOLDER_REPORT)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err("The bar folder did not open as a dropdown".into());
            }
        }
        5 => {
            if let Some(r) = polish_report(p)? {
                if r["rows"] != serde_json::json!(["Deep", "Work link"])
                    || r["icons"] != serde_json::json!(["svg", "letter:W"])
                    || r["under"] != true
                    || r["expanded"] != "true"
                {
                    return Err(format!("Wrong folder dropdown: {r}"));
                }
                check_live_under_dropdown(p.s()?)?;
                eval_toolbar(p, "document.querySelector('#folder-panel button').click()")?;
                p.next();
            }
        }
        6 => {
            let s = p.s()?;
            if s.library_data
                .menu
                .as_ref()
                .is_some_and(|m| m.name == "Deep")
            {
                eval_toolbar(p, FOLDER_REPORT)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err("The subfolder did not open in the dropdown".into());
            }
        }
        7 => {
            if let Some(r) = polish_report(p)? {
                // A way back (named after the open folder), then its bookmark.
                if r["rows"] != serde_json::json!(["Deep", "Deep link"])
                    || r["icons"] != serde_json::json!(["svg", "img"])
                    || r["back"] != true
                {
                    return Err(format!("Wrong subfolder dropdown: {r}"));
                }
                eval_toolbar(
                    p,
                    "[...document.querySelectorAll('#folder-panel button')].at(-1).click()",
                )?;
                p.next();
            }
        }
        8 => {
            let s = p.s()?;
            if s.tabs.items[0].url.ends_with("?deep")
                && !s.windows[0].menu_open
                && view_visible(&s.tabs.items[0].content.engine.view)
            {
                p.command(el, 0, Command::SetBookmarksBar(false))?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!(
                    "The dropdown bookmark did not open: {}",
                    s.tabs.items[0].url
                ));
            }
        }
        9 => {
            let s = p.s()?;
            if s.chrome_height as usize == first {
                check_no_overlap(s, "bar hidden")?;
                eval_toolbar(p, BAR_REPORT)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!(
                    "The toolbar did not shrink: {} (before: {first})",
                    s.chrome_height
                ));
            }
        }
        10 => {
            if let Some(r) = polish_report(p)? {
                if r["hidden"] != true || r["setting"] != false || p.s()?.settings.bookmarks_bar {
                    return Err(format!("The setting did not hide the bar: {r}"));
                }
                p.log.push(
                    "bar: imported bookmarks on a bar; links, folder dropdowns and the setting work".into(),
                );
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
fn native_imported_bookmarks_show_on_the_bookmarks_bar() {
    let (root, _) = run("bookmarks-bar", bar_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// The star and site-information dropdowns as JSON.
const STAR_REPORT: &str = "(()=>{const b=id=>document.getElementById(id),p=b('star-panel'),s=b('site-panel'),r=p.getBoundingClientRect(),a=b('bookmark-star').getBoundingClientRect(),t=s.getBoundingClientRect(),c=b('security').getBoundingClientRect();return JSON.stringify({star:!p.hidden,starUnder:r.top>=a.bottom&&r.top-a.bottom<12&&r.right<=innerWidth,marked:b('bookmark-star').dataset.bookmarked,title:b('star-title').textContent,name:b('star-name').value,enabled:!b('star-name').disabled,focus:document.activeElement&&document.activeElement.id,site:!s.hidden,siteUnder:t.top>=c.bottom&&t.top-c.bottom<12&&Math.abs(t.left-c.left)<=1,siteTitle:b('site-title').textContent,siteOrigin:b('site-origin').textContent})})()";

/// Ctrl+D bookmarks the page and opens the star's dropdown to rename it, move it or
/// remove it; the site badge opens connection information under itself. Both show
/// over the live page and close with Escape.
fn star_site_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    let url = p.url.clone();
    let waited = p.at.elapsed();
    let page_bookmarks = |s: &Session| -> Vec<(i64, String)> {
        s.library_data
            .page_bookmark
            .iter()
            .map(|b| (b.id, b.title.clone()))
            .collect()
    };
    match p.phase {
        0 if waited > Duration::from_secs(1) => {
            if p.s()?.tabs.items[0].content.state.loading {
                if p.started.elapsed() > Duration::from_secs(20) {
                    return Err("The new tab's blank document did not finish loading".into());
                }
                return Ok(());
            }
            p.command(el, 0, Command::Navigate(format!("{url}?star")))?;
            p.next();
        }
        1 => {
            let s = p.s()?;
            let tab = &s.tabs.items[0];
            if tab.url.ends_with("?star") && !tab.content.state.loading {
                // Ctrl+D reaches the browser as this command.
                p.command(el, 0, Command::BookmarkCurrent)?;
                p.next();
            } else if waited > Duration::from_secs(20) {
                return Err(format!(
                    "The page did not load: {} ({})",
                    tab.url, tab.content.state.message
                ));
            }
        }
        2 => {
            let s = p.s()?;
            if s.windows[0].menu_open
                && page_bookmarks(s).len() == 1
                && waited > Duration::from_millis(500)
            {
                if s.windows[0].library_panel.is_some() {
                    return Err("The star opened the library column".into());
                }
                eval_toolbar(p, STAR_REPORT)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err("The star's dropdown did not open with the new bookmark".into());
            }
        }
        3 => {
            if let Some(r) = polish_report(p)? {
                if r["star"] != true
                    || r["starUnder"] != true
                    || r["marked"] != "true"
                    || r["title"] != "Bookmarked"
                    || r["enabled"] != true
                    || r["focus"] != "star-name"
                    || r["name"].as_str().is_none_or(str::is_empty)
                {
                    return Err(format!("Wrong star dropdown: {r}"));
                }
                check_live_under_dropdown(p.s()?)?;
                eval_toolbar(
                    p,
                    "document.getElementById('star-name').value='Renamed star';document.getElementById('star-done').click()",
                )?;
                p.next();
            }
        }
        4 => {
            let s = p.s()?;
            if page_bookmarks(s)
                .first()
                .is_some_and(|(_, t)| t == "Renamed star")
                && !s.windows[0].menu_open
            {
                // Again: the page is already bookmarked; Remove deletes it.
                p.command(el, 0, Command::BookmarkCurrent)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!(
                    "Done did not rename the bookmark: {:?}",
                    page_bookmarks(s)
                ));
            }
        }
        5 if waited > Duration::from_millis(800) => {
            // The toolbar reports the open dropdown back; on a busy machine that can
            // take longer than the first look. Still exactly one bookmark: no copy.
            let s = p.s()?;
            if !s.windows[0].menu_open || page_bookmarks(s).len() != 1 {
                if waited > Duration::from_secs(10) {
                    return Err(format!(
                        "The star's dropdown did not open again: open {}, bookmarks {:?}",
                        s.windows[0].menu_open,
                        page_bookmarks(s)
                    ));
                }
                return Ok(());
            }
            eval_toolbar(p, "document.getElementById('star-remove').click()")?;
            p.next();
        }
        6 => {
            let s = p.s()?;
            if page_bookmarks(s).is_empty() && !s.windows[0].menu_open {
                eval_toolbar(p, "document.getElementById('security').click()")?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err("Remove did not delete the bookmark".into());
            }
        }
        7 if waited > Duration::from_millis(800) => {
            if !p.s()?.windows[0].menu_open {
                return Err("The site dropdown did not open".into());
            }
            eval_toolbar(p, STAR_REPORT)?;
            p.next();
        }
        8 => {
            if let Some(r) = polish_report(p)? {
                if r["site"] != true
                    || r["siteUnder"] != true
                    || r["star"] != false
                    || r["marked"] != "false"
                    || r["siteTitle"] != "Not encrypted"
                    || !r["siteOrigin"].as_str().is_some_and(|o| url.starts_with(o))
                {
                    return Err(format!("Wrong site dropdown: {r}"));
                }
                eval_toolbar(
                    p,
                    "document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))",
                )?;
                p.next();
            }
        }
        9 => {
            let s = p.s()?;
            if !s.windows[0].menu_open
                && s.effective_panel().is_none()
                && view_visible(&s.tabs.items[0].content.engine.view)
            {
                p.log
                    .push("dropdowns: star (save, rename, remove) and site information".into());
                p.done = true;
                el.exit();
            } else if waited > Duration::from_secs(10) {
                return Err("Escape did not close the site dropdown".into());
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
#[ignore = "Requires Windows/WebView2; opens a real window"]
fn native_star_and_site_information_open_as_dropdowns() {
    let (root, _) = run("star-site", star_site_steps, |_, _| {});
    let _ = std::fs::remove_dir_all(root);
}

/// The profile button, its dropdown and Settings > Profiles as JSON.
const PROFILE_REPORT: &str = "(()=>{const b=id=>document.getElementById(id),btn=b('profile-button'),r=btn.getBoundingClientRect(),m=b('more').getBoundingClientRect(),a=btn.querySelector('.profile-avatar'),panel=b('profile-panel');return JSON.stringify({shown:!btn.hidden&&r.width>0,beforeMenu:r.right<=m.left+1,initial:a?a.textContent:'',color:a?getComputedStyle(a).backgroundColor:'',label:btn.getAttribute('aria-label'),open:!panel.hidden,expanded:btn.getAttribute('aria-expanded'),title:b('profile-title').textContent,others:[...b('profile-others').querySelectorAll('button')].map(x=>x.textContent),rows:[...b('profile-list').querySelectorAll('.row .title')].map(x=>x.textContent)})})()";

/// Profiles from a normal window: the avatar button shows this profile's initial
/// and colour left of the menu; its dropdown lists the other profiles and opens
/// one in its own process; adding, renaming and deleting change the shared list;
/// this window's profile and Default cannot be deleted, and the window title names
/// the profile once there is more than one.
fn profile_steps(p: &mut Probe, el: &ActiveEventLoop) -> Result<(), String> {
    use rust_browser::profiles::{self, COLORS};
    let waited = p.at.elapsed();
    let root = p.root.clone();
    match p.phase {
        // The window learns about the second profile from the shared list after
        // it opens: wait for the title and the button to show it, not a fixed time.
        0 => {
            let s = p.s()?;
            let title = s.last_title.borrow().clone();
            if title.ends_with("Rust Browser - Default") {
                eval_toolbar(p, PROFILE_REPORT)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err(format!("The title does not name the profile: {title}"));
            }
        }
        1 => {
            if let Some(r) = polish_report(p)? {
                if r["shown"] != true
                    || r["beforeMenu"] != true
                    || r["initial"] != "D"
                    || r["color"] != "rgb(14, 111, 124)"
                    || r["label"] != "Profile: Default"
                {
                    if waited > Duration::from_secs(10) {
                        return Err(format!("Wrong profile button: {r}"));
                    }
                    eval_toolbar(p, PROFILE_REPORT)?;
                    return Ok(());
                }
                eval_toolbar(p, "document.getElementById('profile-button').click()")?;
                p.next();
            }
        }
        2 => {
            if p.s()?.windows[0].menu_open {
                eval_toolbar(p, PROFILE_REPORT)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err("The profile dropdown did not open".into());
            }
        }
        3 => {
            if let Some(r) = polish_report(p)? {
                if r["open"] != true
                    || r["expanded"] != "true"
                    || r["title"] != "Default"
                    || r["others"] != serde_json::json!(["WWork"])
                {
                    return Err(format!("Wrong profile dropdown: {r}"));
                }
                eval_toolbar(
                    p,
                    "document.querySelector('#profile-others button').click()",
                )?;
                p.next();
            }
        }
        4 => {
            let opened = super::OPENED_PROFILES.with(|o| o.borrow().clone());
            if opened.is_empty() || p.s()?.windows[0].menu_open {
                if waited > Duration::from_secs(10) {
                    return Err(format!("Choosing Work did not open it: {opened:?}"));
                }
                return Ok(());
            }
            if opened != [("p2".to_string(), false)] {
                return Err(format!("Choosing Work opened something else: {opened:?}"));
            }
            p.command(
                el,
                0,
                Command::AddProfile {
                    name: Some("  Home ".into()),
                    color: Some(COLORS[2].into()),
                },
            )?;
            p.command(
                el,
                0,
                Command::EditProfile {
                    id: "p2".into(),
                    name: "Office".into(),
                    color: COLORS[5].into(),
                },
            )?;
            // A duplicate name, this window's profile and Default are refused.
            p.command(
                el,
                0,
                Command::AddProfile {
                    name: Some("office".into()),
                    color: None,
                },
            )?;
            let duplicate = p.s()?.notice.clone();
            p.command(el, 0, Command::DeleteProfile("default".into()))?;
            let own = p.s()?.notice.clone();
            if !duplicate.contains("already has that name")
                || !own.contains("Switch to another profile")
            {
                return Err(format!("Refusals were not reported: {duplicate} | {own}"));
            }
            let registry = profiles::load(&root).map_err(|e| e.to_string())?;
            let names: Vec<_> = registry.profiles.iter().map(|p| p.name.as_str()).collect();
            let opened = super::OPENED_PROFILES.with(|o| o.borrow().clone());
            if names != ["Default", "Office", "Home"]
                || registry.get("p3").map(|p| p.color.as_str()) != Some(COLORS[2])
                || opened.last() != Some(&("p3".to_string(), false))
            {
                return Err(format!("Add or edit went wrong: {names:?} {opened:?}"));
            }
            std::fs::create_dir_all(root.join("profiles").join("p2").join("content"))
                .map_err(|e| e.to_string())?;
            p.command(el, 0, Command::DeleteProfile("p2".into()))?;
            p.command(el, 0, Command::ShowPanel(Panel::Settings))?;
            p.next();
        }
        5 => {
            let registry = profiles::load(&root).map_err(|e| e.to_string())?;
            let gone = registry.get("p2").is_none() && !root.join("profiles").join("p2").exists();
            if gone && p.s()?.effective_panel() == Some(Panel::Settings) {
                eval_toolbar(p, PROFILE_REPORT)?;
                p.next();
            } else if waited > Duration::from_secs(10) {
                return Err("Deleting Office kept its profile or folder".into());
            }
        }
        6 => {
            if let Some(r) = polish_report(p)? {
                if r["rows"] != serde_json::json!(["Default", "Home"]) {
                    return Err(format!("Settings > Profiles is out of date: {r}"));
                }
                p.log.push(
                    "profiles: button, dropdown, open, add, edit, delete and refusals".into(),
                );
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
fn native_profiles_switch_add_edit_and_delete() {
    let (root, _) = run("profiles", profile_steps, |root, _| {
        rust_browser::profiles::update(root, |r| {
            r.create("Work", rust_browser::profiles::COLORS[1])
        })
        .unwrap();
    });
    let _ = std::fs::remove_dir_all(root);
}
