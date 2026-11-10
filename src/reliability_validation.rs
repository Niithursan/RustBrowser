//! Controlled reliability checks: honest failure reporting, deferred destruction of
//! closed tabs and recovery after the shared WebView2 browser process ends. Only this
//! fixture's own disposable browser process is terminated.
use super::*;
use std::sync::{Arc, Mutex};
use windows::Win32::{
    Foundation::CloseHandle,
    System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
};
use windows::core::Interface;
use winit::platform::windows::EventLoopBuilderExtWindows;
use wry::WebViewExtWindows;

struct Probe {
    app: App,
    root: PathBuf,
    url: String,
    refused: String,
    phase: u8,
    started: Instant,
    step: Instant,
    result: Arc<Mutex<Option<String>>>,
    failure: Option<String>,
    failures: Vec<String>,
    loading_events: u32,
    first: TabId,
    second: TabId,
    views: Vec<usize>,
    message: String,
}

fn recorded(session: &Session, code: DiagnosticCode) -> u32 {
    session
        .diagnostics
        .snapshot()
        .entries
        .iter()
        .filter(|entry| entry.code == code)
        .map(|entry| entry.count)
        .sum()
}

fn view_identity(session: &Session) -> Vec<usize> {
    session
        .tabs
        .items
        .iter()
        .map(|tab| tab.content.engine.view.webview().as_raw() as usize)
        .collect()
}

/// End the content browser process that hosts this fixture's own views.
fn terminate_engine(session: &Session) -> Result<(), String> {
    let tab = session
        .tabs
        .active()
        .ok_or("No tab to identify the engine")?;
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

impl Probe {
    fn eval(&self, script: &str) -> Result<(), String> {
        self.eval_in(false, script)
    }
    fn eval_in(&self, toolbar: bool, script: &str) -> Result<(), String> {
        let session = self.app.session.as_ref().ok_or("No session")?;
        let result = self.result.clone();
        let view = if toolbar {
            &session.toolbar
        } else {
            &session.tabs.active().ok_or("No tab")?.content.engine.view
        };
        view.evaluate_script_with_callback(script, move |value| {
            if let Ok(mut slot) = result.lock() {
                *slot = Some(value)
            }
        })
        .map_err(|_| "Fixture evaluation failed".into())
    }
    fn poll(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        if self.phase == 100 {
            return Ok(());
        }
        if self.started.elapsed() > Duration::from_secs(90) {
            return Err(format!(
                "Reliability fixture timed out at phase {}",
                self.phase
            ));
        }
        if self.phase == 13 {
            // Shutdown hides the window at once and destroys views after the delay.
            match &self.app.session {
                Some(session) => {
                    if session.window.is_visible() != Some(false)
                        || self.step.elapsed() > RETIRE_DELAY + Duration::from_secs(5)
                    {
                        return Err("Closing window stayed visible or never closed".into());
                    }
                }
                None => {
                    if self.step.elapsed() < RETIRE_DELAY {
                        return Err("Views were destroyed before the shutdown delay".into());
                    }
                    self.phase = 100;
                }
            }
            return Ok(());
        }
        let value = self.result.lock().map_err(|_| "Result lock failed")?.take();
        let s = self.app.session.as_mut().ok_or("No application session")?;
        if !s.ready {
            return Ok(());
        }
        let settled = |s: &Session, title: &str| {
            s.tabs
                .active()
                .is_some_and(|t| !t.content.state.loading && t.title == title)
        };
        match self.phase {
            0 => {
                self.first = s.tabs.active().ok_or("No initial tab")?.id;
                s.command(Command::Navigate(self.url.clone()))
                    .map_err(|_| "Fixture navigation failed")?;
                self.phase = 1;
            }
            1 if settled(s, "Compatibility fixture") => {
                s.command(Command::PinActive).map_err(|_| "Pin failed")?;
                s.open_tab(format!("{}status-404", self.url))
                    .map_err(|_| "Second tab failed")?;
                s.activate().map_err(|_| "Second tab activation failed")?;
                self.second = s.tabs.active().ok_or("No second tab")?.id;
                self.phase = 2;
            }
            // A server error page is a loaded page, not a failed connection.
            2 if settled(s, "Missing fixture") => {
                let state = &s.tabs.active().ok_or("No 404 tab")?.content.state;
                if state.failed || !state.message.contains("HTTP 404") || !self.failures.is_empty()
                {
                    return Err(format!(
                        "HTTP 404 was reported as a failure: failed={} message={} events={:?}",
                        state.failed, state.message, self.failures
                    ));
                }
                if s.diagnostics
                    .snapshot()
                    .entries
                    .iter()
                    .any(|e| e.code == DiagnosticCode::NavigationFailed)
                {
                    return Err("HTTP 404 was recorded as a navigation failure".into());
                }
                // Announced politely as status text, not assertively as an error.
                s.render();
                self.eval_in(true, "document.getElementById('alert').textContent==='' && document.getElementById('status').className==='' && document.getElementById('status').getAttribute('role')==='status' && document.getElementById('status').textContent.includes('HTTP 404')")?;
                self.phase = 18;
            }
            18 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("HTTP 404 was announced as an error".into());
                }
                s.command(Command::Navigate(format!("{}status-503", self.url)))
                    .map_err(|_| "503 navigation failed")?;
                self.phase = 3;
            }
            3 if settled(s, "Unavailable fixture") => {
                let state = &s.tabs.active().ok_or("No 503 tab")?.content.state;
                if state.failed || !state.message.contains("HTTP 503") {
                    return Err(format!("HTTP 503 state: {}", state.message));
                }
                self.loading_events = 0;
                s.command(Command::Navigate(self.refused.clone()))
                    .map_err(|_| "Refused navigation failed")?;
                self.phase = 4;
            }
            4 if s.tabs.active().is_some_and(|t| t.content.state.failed) => {
                self.message = s
                    .tabs
                    .active()
                    .ok_or("No refused tab")?
                    .content
                    .state
                    .message
                    .clone();
                self.step = Instant::now();
                self.phase = 5;
            }
            // WebView2 follows a refused connection with a second navigation to the
            // same address that aborts. It must not replace the reported reason,
            // count as another failure or leave the tab loading.
            5 if self.step.elapsed() > Duration::from_millis(1500) => {
                let state = &s.tabs.active().ok_or("No refused tab")?.content.state;
                if self.failures.len() != 1
                    || state.message != self.message
                    || state.message.contains("status 9")
                    || state.loading
                    || !state.failed
                    || self.loading_events != 1
                {
                    return Err(format!(
                        "Connection failure reporting: events={:?} first={} now={} loading={} loading_events={}",
                        self.failures,
                        self.message,
                        state.message,
                        state.loading,
                        self.loading_events
                    ));
                }
                // Reload is a host command: it is reported again, once.
                self.failures.clear();
                s.command(Command::Reload).map_err(|_| "Reload failed")?;
                self.step = Instant::now();
                self.phase = 6;
            }
            6 if self.step.elapsed() > Duration::from_millis(1500) => {
                if self.failures.len() != 1 {
                    return Err(format!(
                        "Reload of a failed page reported {:?}",
                        self.failures
                    ));
                }
                self.failures.clear();
                s.command(Command::Navigate(format!("{}?second", self.url)))
                    .map_err(|_| "Second fixture navigation failed")?;
                self.phase = 7;
            }
            7 if settled(s, "Compatibility fixture") => {
                // A blocked app link leaves the document, and so the tab state, intact.
                s.tabs
                    .active()
                    .ok_or("No tab for blocked link")?
                    .content
                    .engine
                    .view
                    .evaluate_script("location.href='mailto:someone@example.test'")
                    .map_err(|_| "Blocked-link script failed")?;
                self.phase = 14;
            }
            14 if s.notice.contains("was blocked") => {
                let state = &s.tabs.active().ok_or("No tab")?.content.state;
                if state.failed || state.loading || !self.failures.is_empty() {
                    return Err(format!(
                        "Blocked link changed the page state: {} {:?}",
                        state.message, self.failures
                    ));
                }
                // A navigation that becomes a download is not a failed navigation.
                s.command(Command::Navigate(format!("{}download", self.url)))
                    .map_err(|_| "Download navigation failed")?;
                self.phase = 15;
            }
            15 if s
                .downloads
                .borrow()
                .records()
                .iter()
                .any(|record| record.status == "Completed") =>
            {
                self.step = Instant::now();
                self.phase = 17;
            }
            // Past the delay after which an unclaimed failure would be recorded.
            17 if self.step.elapsed() > FAILURE_RECORD_DELAY + Duration::from_millis(500) => {
                let tab = s.tabs.active().ok_or("No download source tab")?;
                let state = &tab.content.state;
                if state.failed
                    || state.loading
                    || state.message != "Download handed to manager."
                    || !tab.url.ends_with("/?second")
                    // Only the refused connection and its Reload were failures.
                    || !s.unrecorded_failures.is_empty()
                    || recorded(s, DiagnosticCode::NavigationFailed) != 2
                {
                    return Err(format!(
                        "Download navigation was treated as a failure: recorded={} failed={} message={} url_kept={}",
                        recorded(s, DiagnosticCode::NavigationFailed),
                        state.failed,
                        state.message,
                        tab.url.ends_with("/?second")
                    ));
                }
                self.failures.clear();
                self.phase = 16;
            }
            16 => {
                // Closing hides the view at once and destroys it only after the delay.
                let closing = self.second;
                s.open_tab(format!("{}?scratch", self.url))
                    .map_err(|_| "Scratch tab failed")?;
                s.activate().map_err(|_| "Scratch activation failed")?;
                let scratch = s.tabs.active().ok_or("No scratch tab")?.id;
                s.command(Command::CloseTab(scratch))
                    .map_err(|error| format!("Scratch close failed: {error}"))?;
                if s.retired.len() != 1
                    || s.tabs.items.iter().any(|t| t.id == scratch)
                    || s.tabs.active().map(|t| t.id) != Some(closing)
                {
                    return Err("Closed tab was not retired".into());
                }
                self.step = Instant::now();
                self.phase = 8;
            }
            8 if s.retired.is_empty() => {
                if self.step.elapsed() < RETIRE_DELAY {
                    return Err("Retired view was destroyed before its delay".into());
                }
                self.views = view_identity(s);
                terminate_engine(s)?;
                self.step = Instant::now();
                self.phase = 9;
            }
            9 if s.notice.contains("was restarted") && settled(s, "Compatibility fixture") => {
                let views = view_identity(s);
                let tabs = &s.tabs.items;
                if tabs.len() != 2
                    || tabs[0].id != self.first
                    || tabs[1].id != self.second
                    || !tabs[0].pinned
                    || tabs[1].pinned
                    || s.tabs.active().map(|t| t.id) != Some(self.second)
                    || views.iter().zip(&self.views).any(|(new, old)| new == old)
                    || tabs.iter().any(|t| t.content.state.failed)
                    || !tabs[1].url.ends_with("/?second")
                {
                    return Err("Engine restart did not preserve the tab model".into());
                }
                if !s
                    .diagnostics
                    .snapshot()
                    .entries
                    .iter()
                    .any(|e| e.code == DiagnosticCode::EngineRestarted)
                {
                    return Err("Engine restart was not recorded".into());
                }
                // The restart is announced in the polite, dismissible notice region.
                s.render();
                self.eval_in(true, "document.getElementById('notice').textContent.includes('was restarted') && document.getElementById('notice').getAttribute('role')==='status' && !document.getElementById('dismiss-notice').hidden && document.getElementById('alert').textContent===''")?;
                self.phase = 19;
            }
            19 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Engine restart notice was not announced accessibly".into());
                }
                self.eval("window.marker==='retained' && location.search==='?second'")?;
                self.phase = 10;
            }
            10 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Reloaded document is not usable".into());
                }
                // The other tab was rebuilt too, and ordinary tab commands work again.
                s.command(Command::SelectTab(self.first))
                    .map_err(|error| format!("Select after restart failed: {error}"))?;
                self.phase = 11;
            }
            11 if settled(s, "Compatibility fixture") => {
                self.eval("window.marker==='retained' && location.search===''")?;
                self.phase = 12;
            }
            12 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Background tab was not reloaded".into());
                }
                let s_window_id = s.window.id();
                s.command(Command::NewTab)
                    .map_err(|error| format!("New tab after restart failed: {error}"))?;
                s.command(Command::CloseActive)
                    .map_err(|error| format!("Close after restart failed: {error}"))?;
                // Reopen the scratch tab closed earlier (new-tab pages are not kept),
                // then select by position: first, and "last" for any larger index.
                s.command(Command::ReopenClosedTab)
                    .map_err(|error| format!("Reopen failed: {error}"))?;
                if s.tabs.items.len() != 3
                    || !s
                        .tabs
                        .active()
                        .is_some_and(|t| t.url.ends_with("/?scratch"))
                {
                    return Err("Closed tab was not reopened at its address".into());
                }
                let reopened = s.tabs.active().ok_or("No reopened tab")?.id;
                s.command(Command::SelectIndex(0))
                    .map_err(|_| "Select first failed")?;
                if s.tabs.active().map(|t| t.id) != Some(self.first) {
                    return Err("Ctrl+1 did not select the first tab".into());
                }
                s.command(Command::SelectIndex(usize::MAX))
                    .map_err(|_| "Select last failed")?;
                if s.tabs.active().map(|t| t.id) != Some(reopened) {
                    return Err("Ctrl+9 did not select the last tab".into());
                }
                s.command(Command::CloseActive)
                    .map_err(|error| format!("Close reopened failed: {error}"))?;
                s.command(Command::ReopenClosedTab)
                    .map_err(|_| "Second reopen failed")?;
                s.command(Command::CloseActive)
                    .map_err(|_| "Second close failed")?;
                self.failures.clear();
                if !self.failures.is_empty() {
                    return Err(format!(
                        "Engine restart reported navigation failures: {:?}",
                        self.failures
                    ));
                }
                self.app
                    .window_event(el, s_window_id, WindowEvent::CloseRequested);
                self.step = Instant::now();
                self.phase = 13;
            }
            _ => {}
        }
        Ok(())
    }
}
impl ApplicationHandler<Event> for Probe {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        match Session::create_at(el, self.app.proxy.clone(), self.root.clone(), false, None) {
            Ok(s) => {
                s.render();
                self.app.session = Some(s)
            }
            Err(_) => {
                self.failure = Some("Reliability application startup failed".into());
                el.exit();
            }
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(el, id, event)
    }
    fn user_event(&mut self, el: &ActiveEventLoop, event: Event) {
        // Production state handling, with the native folder picker replaced.
        if let Event::DownloadRequested(id, tab) = &event {
            let folder = self.root.join("downloads");
            std::fs::create_dir_all(&folder).unwrap();
            if let Some(s) = &mut self.app.session {
                s.download_requested(*tab);
                s.downloads.borrow_mut().destination(*id, Ok(Some(folder)));
                s.render();
            }
            return;
        }
        match &event {
            Event::Page(_, PageEvent::Loading) => self.loading_events += 1,
            Event::Page(
                _,
                PageEvent::Completed {
                    error: Some(text), ..
                }
                | PageEvent::Error(text),
            ) if self.failures.len() < 8 => self.failures.push(text.clone()),
            _ => {}
        }
        self.app.user_event(el, event)
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.app.about_to_wait(el);
        if let Err(error) = self.poll(el) {
            self.failure = Some(error);
            el.exit();
        }
        if self.phase == 100 {
            return;
        }
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(25),
        ));
    }
}

#[test]
#[ignore = "Requires Windows/WebView2; ends only the fixture's own disposable engine process"]
fn native_failure_reporting_tab_retirement_and_engine_restart() {
    let server = super::compat_validation::Server::start();
    // A port that was just released: connections to it are refused.
    let refused = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/", listener.local_addr().unwrap())
    };
    let root = std::env::temp_dir().join(format!(
        "rust-browser-reliability-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let event_loop = EventLoop::<Event>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut probe = Probe {
        app: App {
            session: None,
            proxy: event_loop.create_proxy(),
            failure: None,
            modifiers: ModifiersState::default(),
            closing: None,
        },
        root: root.clone(),
        url: server.url.clone(),
        refused,
        phase: 0,
        started: Instant::now(),
        step: Instant::now(),
        result: Arc::new(Mutex::new(None)),
        failure: None,
        failures: Vec::new(),
        loading_events: 0,
        first: 0,
        second: 0,
        views: Vec::new(),
        message: String::new(),
    };
    let result = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let phase = probe.phase;
    drop(probe);
    assert!(result.is_ok());
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(phase, 100);
    // Identity, order and pin state survived the restart and were persisted.
    let snapshot = storage::load(&root.join("session.json")).unwrap();
    assert_eq!(snapshot.tabs.len(), 2);
    assert_eq!(snapshot.pinned, vec![true, false]);
    let _ = std::fs::remove_dir_all(root);
}
