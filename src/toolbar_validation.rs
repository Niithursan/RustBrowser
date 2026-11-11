//! Toolbar engine recovery. The toolbar has its own WebView2 environment and browser
//! process (profile/toolbar), separate from the content engine. This fixture ends
//! only its own disposable toolbar processes: first the toolbar's browser process,
//! then the renderer of the toolbar that replaced it.
use super::*;
use std::sync::{Arc, Mutex};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::Win32::{
    Foundation::CloseHandle,
    System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
};
use windows::core::Interface;
use winit::platform::windows::EventLoopBuilderExtWindows;
use wry::{WebView, WebViewExtWindows};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kill {
    Browser,
    Renderer,
    Hang,
}
const KILLS: [Kill; 3] = [Kill::Browser, Kill::Renderer, Kill::Hang];
const ADDRESS_PROBE: &str = "document.getElementById('address') ? 'toolbar' : 'missing'";
const NOTICE_PROBE: &str = "document.getElementById('notice').textContent";

fn browser_process(view: &WebView) -> Result<u32, String> {
    let mut process = 0;
    // SAFETY: reads a property of a live view on its own thread.
    unsafe { view.webview().BrowserProcessId(&mut process) }
        .map_err(|_| "Browser process is unknown")?;
    Ok(process)
}

/// The renderer processes of the WebView2 environment that hosts `view`.
fn renderer_processes(view: &WebView) -> Result<Vec<u32>, String> {
    // SAFETY: COM calls on a live view on its own thread; every interface is owned.
    unsafe {
        let core: ICoreWebView2_2 = view.webview().cast().map_err(|_| "No ICoreWebView2_2")?;
        let environment: ICoreWebView2Environment8 = core
            .Environment()
            .map_err(|_| "No environment")?
            .cast()
            .map_err(|_| "No ICoreWebView2Environment8")?;
        let infos = environment
            .GetProcessInfos()
            .map_err(|_| "No process information")?;
        let mut count = 0;
        infos.Count(&mut count).map_err(|_| "No process count")?;
        let mut renderers = Vec::new();
        for index in 0..count {
            let info = infos
                .GetValueAtIndex(index)
                .map_err(|_| "No process entry")?;
            let mut kind = COREWEBVIEW2_PROCESS_KIND::default();
            let mut id = 0;
            info.Kind(&mut kind).map_err(|_| "No process kind")?;
            info.ProcessId(&mut id).map_err(|_| "No process id")?;
            if kind == COREWEBVIEW2_PROCESS_KIND_RENDERER {
                renderers.push(id as u32);
            }
        }
        Ok(renderers)
    }
}

fn terminate(process: u32) -> Result<(), String> {
    // SAFETY: the process ID names one of this fixture's own disposable WebView2
    // processes; the handle is closed before returning.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, process)
            .map_err(|_| "Process could not be opened")?;
        let ended = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        ended.map_err(|_| "Process could not be ended".to_string())
    }
}

fn identity(view: &WebView) -> usize {
    view.webview().as_raw() as usize
}

fn toolbar_failures(session: &Session) -> u32 {
    session
        .diagnostics
        .snapshot()
        .entries
        .iter()
        .filter(|entry| entry.code == DiagnosticCode::ToolbarFailed)
        .map(|entry| entry.count)
        .sum()
}

struct Probe {
    app: App,
    root: PathBuf,
    url: String,
    round: usize,
    phase: u8,
    started: Instant,
    step: Instant,
    result: Arc<Mutex<Option<String>>>,
    failure: Option<String>,
    content_events: Vec<String>,
    tabs: Vec<TabId>,
    content_views: Vec<usize>,
    toolbar: usize,
    toolbar_source: u64,
    killed_browser: u32,
    log: Vec<String>,
}

impl Probe {
    fn eval_toolbar(&self, script: &str) -> Result<(), String> {
        let session = self.app.session.as_ref().ok_or("No session")?;
        let result = self.result.clone();
        session
            .toolbar
            .evaluate_script_with_callback(script, move |value| {
                if let Ok(mut slot) = result.lock() {
                    *slot = Some(value)
                }
            })
            .map_err(|error| format!("Toolbar evaluation failed: {error}"))
    }

    fn poll(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        if self.phase == 100 {
            return Ok(());
        }
        if self.started.elapsed() > Duration::from_secs(120) {
            return Err(format!(
                "Toolbar fixture timed out at round {} phase {}",
                self.round, self.phase
            ));
        }
        if self.phase == 13 {
            // Production shutdown after both recoveries: hidden at once, gone after the delay.
            match &self.app.session {
                Some(_) if self.step.elapsed() > RETIRE_DELAY + Duration::from_secs(5) => {
                    return Err("Window did not close after toolbar recovery".into());
                }
                Some(_) => {}
                None => {
                    self.log.push(format!(
                        "shutdown took {} ms",
                        self.step.elapsed().as_millis()
                    ));
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
        let kill = KILLS[self.round.min(KILLS.len() - 1)];
        match self.phase {
            0 => {
                s.command(Command::Navigate(self.url.clone()))
                    .map_err(|error| format!("Navigate failed: {error}"))?;
                s.command(Command::NewTab)
                    .map_err(|error| format!("New tab failed: {error}"))?;
                s.command(Command::Navigate(format!("{}?second", self.url)))
                    .map_err(|error| format!("Second navigate failed: {error}"))?;
                self.phase = 1;
            }
            1 if s.tabs.items.iter().all(|t| !t.content.state.loading)
                && s.tabs.active().is_some_and(|t| t.url.ends_with("?second")) =>
            {
                self.tabs = s.tabs.items.iter().map(|t| t.id).collect();
                self.content_views = s
                    .tabs
                    .items
                    .iter()
                    .map(|t| identity(&t.content.engine.view))
                    .collect();
                self.toolbar = identity(&s.toolbar);
                s.toolbar_failed(
                    s.toolbar_source,
                    crate::native_activity::Failure::Unresponsive,
                );
                let deadline = s.toolbar_probe.ok_or("No health grace scheduled")?;
                s.toolbar_failed(
                    s.toolbar_source,
                    crate::native_activity::Failure::Unresponsive,
                );
                if s.toolbar_probe != Some(deadline) {
                    return Err("Repeated hang reports extended the health grace".into());
                }
                self.step = Instant::now();
                self.phase = 20;
            }
            20 if s.toolbar_probe.is_none() => {
                if identity(&s.toolbar) != self.toolbar || !s.toolbar_restarts.is_empty() {
                    return Err("Responsive toolbar was unnecessarily restarted".into());
                }
                self.log
                    .push("Responsive toolbar answered health probe without restart".into());
                // Characterize the actual controller settings and accelerator map,
                // alongside the separate real-keyboard GUI baseline.
                unsafe {
                    let settings = s
                        .tabs
                        .active()
                        .ok_or("No content")?
                        .content
                        .engine
                        .view
                        .webview()
                        .Settings()
                        .map_err(|_| "No content settings")?;
                    let mut zoom = windows::core::BOOL::default();
                    let mut browser = windows::core::BOOL::default();
                    settings
                        .IsZoomControlEnabled(&mut zoom)
                        .map_err(|_| "No zoom setting")?;
                    settings
                        .cast::<ICoreWebView2Settings3>()
                        .map_err(|_| "No accelerator settings")?
                        .AreBrowserAcceleratorKeysEnabled(&mut browser)
                        .map_err(|_| "No accelerator setting")?;
                    self.log.push(format!("Content baseline: browser accelerators={}, zoom control={}, custom Ctrl+F={}, Ctrl+P={}, Ctrl+plus={}",
                        browser.as_bool(), zoom.as_bool(), ui::shortcut(0x46, true, false, false).is_some(),
                        ui::shortcut(0x50, true, false, false).is_some(),ui::shortcut(0xbb,true,false,true).is_some()));
                }
                self.eval_toolbar(ADDRESS_PROBE)?;
                self.phase = 2;
            }
            20 if self.step.elapsed() > Duration::from_secs(10) => {
                return Err("Responsive toolbar did not answer its health probe".into());
            }
            2 if value.is_some() => {
                if value.as_deref() != Some("\"toolbar\"") {
                    return Err(format!("Toolbar is not working before the kill: {value:?}"));
                }
                let toolbar = browser_process(&s.toolbar)?;
                let content =
                    browser_process(&s.tabs.active().ok_or("No tab")?.content.engine.view)?;
                if toolbar == content {
                    return Err("Toolbar and content share a browser process".into());
                }
                let renderers = renderer_processes(&s.toolbar)?;
                self.toolbar = identity(&s.toolbar);
                self.toolbar_source = s.toolbar_source;
                self.killed_browser = toolbar;
                match kill {
                    Kill::Browser => terminate(toolbar)?,
                    Kill::Renderer => {
                        let [renderer] = renderers[..] else {
                            return Err(format!("Expected one toolbar renderer: {renderers:?}"));
                        };
                        terminate(renderer)?
                    }
                    Kill::Hang => {
                        s.toolbar
                            .evaluate_script("while (true) {}")
                            .map_err(|_| "Could not hang fixture renderer")?;
                        // No native watchdog notification arrived in the 30-second
                        // baseline. Inject the classified event to test its response,
                        // while keeping the renderer genuinely unable to answer.
                        self.app
                            .proxy
                            .send_event(Event::ToolbarProcess(
                                s.toolbar_source,
                                crate::native_activity::Failure::Unresponsive,
                            ))
                            .map_err(|_| "Hang report delivery failed")?;
                    }
                }
                self.log.push(format!(
                    "round {}: ended the toolbar {kill:?} (browser {toolbar}, content {content}, renderers {renderers:?})",
                    self.round
                ));
                self.step = Instant::now();
                self.phase = 3;
            }
            3 => {
                if identity(&s.toolbar) != self.toolbar && s.retired_toolbar.is_some() {
                    // Deliver a deterministic late failure while the old view is retained.
                    // Native scheduling of this race is nondeterministic; the event is not.
                    self.app
                        .proxy
                        .send_event(Event::ToolbarProcess(
                            self.toolbar_source,
                            crate::native_activity::Failure::Renderer,
                        ))
                        .map_err(|_| "Late event delivery failed")?;
                    self.log.push(format!(
                        "round {}: toolbar replaced after {} ms",
                        self.round,
                        self.step.elapsed().as_millis()
                    ));
                    self.step = Instant::now();
                    self.phase = 4;
                } else if self.step.elapsed() > Duration::from_secs(30) {
                    return Err(format!(
                        "The toolbar was not replaced after its {kill:?} process ended"
                    ));
                }
            }
            // The replacement loads, announces itself and shows the restart notice.
            4 => {
                self.eval_toolbar(NOTICE_PROBE)?;
                self.phase = 5;
            }
            5 if value.is_some() => {
                if value
                    .as_deref()
                    .is_some_and(|text| text.contains("toolbar stopped and was restarted"))
                {
                    self.eval_toolbar(&format!(
                        "document.getElementById('address').value={};document.getElementById('nav').requestSubmit();'sent'",
                        serde_json::to_string(&format!("{}?via-toolbar-{}", self.url, self.round))
                            .map_err(|_| "Address encoding failed")?
                    ))?;
                    self.step = Instant::now();
                    self.phase = 6;
                } else if self.step.elapsed() > Duration::from_secs(10) {
                    return Err(format!(
                        "Replacement toolbar shows no restart notice: {value:?}"
                    ));
                } else {
                    self.phase = 4;
                }
            }
            // A command typed into the new toolbar reaches the browser over IPC.
            6 if s.tabs.active().is_some_and(|t| {
                !t.content.state.loading && t.url.ends_with(&format!("?via-toolbar-{}", self.round))
            }) =>
            {
                let tabs: Vec<TabId> = s.tabs.items.iter().map(|t| t.id).collect();
                let views: Vec<usize> = s
                    .tabs
                    .items
                    .iter()
                    .map(|t| identity(&t.content.engine.view))
                    .collect();
                if tabs != self.tabs || views != self.content_views {
                    return Err("Toolbar recovery replaced or lost content tabs".into());
                }
                if kill == Kill::Browser && browser_process(&s.toolbar)? == self.killed_browser {
                    return Err("Replacement toolbar reports the ended browser process".into());
                }
                if toolbar_failures(s) != self.round as u32 + 1 {
                    return Err(format!(
                        "Expected {} toolbar failure records, found {}",
                        self.round + 1,
                        toolbar_failures(s)
                    ));
                }
                self.phase = 7;
            }
            6 if self.step.elapsed() > Duration::from_secs(15) => {
                return Err("The replacement toolbar's address bar does not navigate".into());
            }
            // The failed view is destroyed only after the retirement delay.
            7 if s.retired_toolbar.is_none() => {
                if self.round + 1 < KILLS.len() {
                    self.round += 1;
                    s.command(Command::DismissNotice)
                        .map_err(|_| "Dismiss failed")?;
                    s.render();
                    self.eval_toolbar(ADDRESS_PROBE)?;
                    self.phase = 2;
                } else {
                    if !self.content_events.is_empty() {
                        return Err(format!(
                            "The content engine reported failures: {:?}",
                            self.content_events
                        ));
                    }
                    // Exhaust the budget without killing another renderer. A burst
                    // of terminal notifications must admit only one native alert.
                    s.toolbar_restarts = vec![Instant::now(); MAX_ENGINE_RESTARTS];
                    s.toolbar_failed(s.toolbar_source, crate::native_activity::Failure::Renderer);
                    s.toolbar_failed(s.toolbar_source, crate::native_activity::Failure::Browser);
                    if s.toolbar_alerts != 1 {
                        return Err(format!(
                            "Expected one terminal alert, got {}",
                            s.toolbar_alerts
                        ));
                    }
                    let id = s.window.id();
                    self.app.window_event(el, id, WindowEvent::CloseRequested);
                    self.step = Instant::now();
                    self.phase = 13;
                }
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
                self.failure = Some("Toolbar fixture startup failed".into());
                el.exit();
            }
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(el, id, event)
    }
    fn user_event(&mut self, el: &ActiveEventLoop, event: Event) {
        if let Event::ToolbarProcess(_, crate::native_activity::Failure::Unresponsive) = &event {
            self.log.push(
                "Classified toolbar Unresponsive event received (injected by fixture)".into(),
            );
        }
        if let Event::EngineProcess(_, failure) = &event
            && self.content_events.len() < 8
        {
            self.content_events.push(format!("{failure:?}"));
        }
        self.app.user_event(el, event)
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.app.about_to_wait(el);
        if let Err(error) = self.poll(el) {
            // A deliberately hung fixture renderer must be ended before COM teardown.
            if let Some(s) = &self.app.session
                && let Ok(processes) = renderer_processes(&s.toolbar)
            {
                for process in processes {
                    let _ = terminate(process);
                }
            }
            self.failure = Some(error);
            el.exit();
            return;
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
#[ignore = "Requires Windows/WebView2; ends only the fixture's own toolbar processes"]
fn native_toolbar_engine_failure_is_recovered() {
    let server = super::compat_validation::Server::start();
    let root = std::env::temp_dir().join(format!(
        "rust-browser-toolbar-{}-{}",
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
            start: None,
        },
        root: root.clone(),
        url: server.url.clone(),
        round: 0,
        phase: 0,
        started: Instant::now(),
        step: Instant::now(),
        result: Arc::new(Mutex::new(None)),
        failure: None,
        content_events: Vec::new(),
        tabs: Vec::new(),
        content_views: Vec::new(),
        toolbar: 0,
        toolbar_source: 0,
        killed_browser: 0,
        log: Vec::new(),
    };
    let result = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let phase = probe.phase;
    let log = std::mem::take(&mut probe.log);
    drop(probe);
    println!("{}", log.join("\n"));
    assert!(result.is_ok());
    assert!(failure.is_none(), "{failure:?}\n{}", log.join("\n"));
    assert_eq!(phase, 100);
    // Both tabs were saved through the recoveries and the production shutdown.
    let snapshot = storage::load(&root.join("session.json")).unwrap();
    assert_eq!(snapshot.tabs.len(), 2);
    let _ = std::fs::remove_dir_all(root);
}
