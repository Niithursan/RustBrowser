//! Manual external-website survey through the production session. Never part of CI:
//! results depend on the network and on third-party sites. Run it through
//! validation/site-survey.ps1 with a disposable profile and a caller-supplied list of
//! public addresses. No credentials are entered and no permission is granted.
use super::*;
use std::sync::{Arc, Mutex};
use winit::platform::windows::EventLoopBuilderExtWindows;

#[derive(serde::Deserialize)]
struct Plan {
    urls: Vec<String>,
    output: PathBuf,
    #[serde(default = "default_timeout")]
    timeout_seconds: u64,
    #[serde(default = "default_settle")]
    settle_milliseconds: u64,
    /// Leave every visited tab open and exit with the last one visible.
    #[serde(default)]
    keep_tabs_open: bool,
}
fn default_timeout() -> u64 {
    30
}
fn default_settle() -> u64 {
    2000
}

#[derive(Default, serde::Serialize)]
struct Visit {
    requested: String,
    final_url: String,
    title: String,
    timed_out: bool,
    failed: bool,
    message: String,
    elapsed_milliseconds: u128,
    loading_events: u32,
    reported_failures: Vec<String>,
    popups_blocked: u32,
    page: Option<serde_json::Value>,
}

const PAGE_PROBE: &str = "JSON.stringify({ready:document.readyState,text:(document.body&&document.body.innerText||'').length,scripts:document.scripts.length,images:document.images.length,frames:frames.length,media:document.querySelectorAll('video,audio').length,controller:!!(navigator.serviceWorker&&navigator.serviceWorker.controller),secure:isSecureContext,ipc:typeof window.ipc,protocol:location.protocol})";

#[derive(serde::Serialize)]
struct Report {
    /// Browser-process exits and renderer exits seen during the whole survey,
    /// including after a visited tab was closed.
    engine_exits: u32,
    renderer_exits: u32,
    /// Crash dumps WebView2 wrote into the disposable profile, including any
    /// written while the browser was shutting down.
    crash_reports: usize,
    visits: Vec<Visit>,
}

fn crash_reports(profile: &std::path::Path) -> usize {
    let reports = profile.join("content/EBWebView/Crashpad/reports");
    std::fs::read_dir(reports).map_or(0, |entries| {
        entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|e| e == "dmp"))
            .count()
    })
}

struct Probe {
    app: App,
    root: PathBuf,
    engine_exits: u32,
    renderer_exits: u32,
    plan: Plan,
    index: usize,
    stage: u8,
    step: Instant,
    current: Visit,
    visits: Vec<Visit>,
    result: Arc<Mutex<Option<String>>>,
    failure: Option<String>,
    done: bool,
}
impl Probe {
    fn poll(&mut self) -> Result<(), String> {
        if self.done {
            return Ok(());
        }
        let value = self.result.lock().map_err(|_| "Result lock failed")?.take();
        let s = self.app.session.as_mut().ok_or("No application session")?;
        if !s.ready {
            return Ok(());
        }
        match self.stage {
            0 => {
                let Some(url) = self.plan.urls.get(self.index).cloned() else {
                    // Leave through the production shutdown path.
                    self.done = true;
                    self.app.begin_shutdown();
                    return Ok(());
                };
                // A fresh tab per visit also exercises tab creation and closure.
                s.command(Command::NewTab)
                    .map_err(|error| format!("New tab failed: {error}"))?;
                self.current = Visit {
                    requested: url.clone(),
                    ..Visit::default()
                };
                if let Err(error) = s.command(Command::Navigate(url)) {
                    self.current.failed = true;
                    self.current.message = format!("Navigate rejected: {error}");
                    self.stage = 3;
                    return Ok(());
                }
                self.step = Instant::now();
                self.stage = 1;
            }
            1 => {
                let loading = s.tabs.active().is_some_and(|t| t.content.state.loading);
                let timed_out =
                    self.step.elapsed() > Duration::from_secs(self.plan.timeout_seconds);
                if !loading || timed_out {
                    self.current.timed_out = loading && timed_out;
                    self.current.elapsed_milliseconds = self.step.elapsed().as_millis();
                    self.step = Instant::now();
                    self.stage = 2;
                }
            }
            2 if self.step.elapsed() > Duration::from_millis(self.plan.settle_milliseconds) => {
                // Client-side redirects can restart loading during the settle window.
                if s.tabs.active().is_some_and(|t| t.content.state.loading)
                    && self.step.elapsed() < Duration::from_secs(self.plan.timeout_seconds)
                {
                    return Ok(());
                }
                let tab = s.tabs.active().ok_or("No surveyed tab")?;
                self.current.failed = tab.content.state.failed;
                self.current.message = tab.content.state.message.clone();
                self.current.final_url = tab.url.clone();
                self.current.title = tab.title.clone();
                let slot = self.result.clone();
                tab.content
                    .engine
                    .view
                    .evaluate_script_with_callback(PAGE_PROBE, move |value| {
                        if let Ok(mut slot) = slot.lock() {
                            *slot = Some(value)
                        }
                    })
                    .map_err(|_| "Page probe failed")?;
                self.step = Instant::now();
                self.stage = 4;
            }
            4 if value.is_some() || self.step.elapsed() > Duration::from_secs(5) => {
                self.current.page = value
                    .and_then(|text| serde_json::from_str::<String>(&text).ok())
                    .and_then(|text| serde_json::from_str(&text).ok());
                self.stage = 3;
            }
            3 => {
                self.visits.push(std::mem::take(&mut self.current));
                if !self.plan.keep_tabs_open
                    && let Some(tab) = s.tabs.active()
                {
                    let id = tab.id;
                    s.close_tab(id)
                        .map_err(|error| format!("Tab closing failed: {error}"))?;
                }
                self.index += 1;
                self.stage = 0;
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
                self.failure = Some("Survey application startup failed".into());
                el.exit();
            }
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(el, id, event)
    }
    fn user_event(&mut self, el: &ActiveEventLoop, event: Event) {
        match &event {
            Event::Page(_, PageEvent::Loading) => self.current.loading_events += 1,
            Event::Page(
                _,
                PageEvent::Completed {
                    error: Some(text), ..
                }
                | PageEvent::Error(text),
            ) if self.current.reported_failures.len() < 8 => {
                self.current.reported_failures.push(text.clone())
            }
            Event::PopupBlocked => self.current.popups_blocked += 1,
            // Each view reports a browser exit; count the restart, not the reports.
            Event::EngineProcess(_, crate::native_activity::Failure::Browser)
                if self
                    .app
                    .session
                    .as_ref()
                    .is_some_and(|session| !session.engine_lost) =>
            {
                self.engine_exits += 1
            }
            Event::EngineProcess(_, crate::native_activity::Failure::Renderer) => {
                self.renderer_exits += 1
            }
            _ => {}
        }
        self.app.user_event(el, event)
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.app.about_to_wait(el);
        if self.done {
            return;
        }
        if let Err(error) = self.poll() {
            self.failure = Some(error);
            el.exit();
        }
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(25),
        ));
    }
}

#[test]
#[ignore = "Manual external-network survey; run validation/site-survey.ps1"]
fn native_public_site_survey() {
    let plan_path =
        std::env::var_os("RUST_BROWSER_SURVEY_PLAN").expect("Run validation/site-survey.ps1");
    let bytes = std::fs::read(plan_path).expect("Readable survey plan");
    // Windows PowerShell 5.1 writes UTF-8 with a byte-order mark.
    let plan: Plan = serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes))
        .expect("Valid survey plan");
    assert!(
        (1..=64).contains(&plan.urls.len()),
        "Survey 1 to 64 addresses"
    );
    let root = std::env::temp_dir().join(format!(
        "rust-browser-survey-{}-{}",
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
    let output = plan.output.clone();
    let mut probe = Probe {
        app: App {
            session: None,
            proxy: event_loop.create_proxy(),
            failure: None,
            modifiers: ModifiersState::default(),
            closing: None,
        },
        root: root.clone(),
        engine_exits: 0,
        renderer_exits: 0,
        plan,
        index: 0,
        stage: 0,
        step: Instant::now(),
        current: Visit::default(),
        visits: Vec::new(),
        result: Arc::new(Mutex::new(None)),
        failure: None,
        done: false,
    };
    let result = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let mut report = Report {
        engine_exits: probe.engine_exits,
        renderer_exits: probe.renderer_exits,
        crash_reports: 0,
        visits: std::mem::take(&mut probe.visits),
    };
    drop(probe);
    // The engine writes a dump shortly after the failing process ends.
    std::thread::sleep(Duration::from_secs(2));
    report.crash_reports = crash_reports(&root);
    std::fs::write(&output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(root);
    assert!(result.is_ok());
    assert!(failure.is_none(), "{failure:?}");
}
