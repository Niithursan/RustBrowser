//! Opt-in, synthetic workload only. Not compiled into the shipping browser.
use super::*;
use crate::native_validation::{FIXTURE_URL, map_fixture};
use serde::Serialize;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use winit::platform::windows::EventLoopBuilderExtWindows;

#[derive(Default, Serialize)]
struct Metrics {
    construction_ms: f64,
    first_usable_ms: f64,
    all_loaded_ms: f64,
    stage: String,
    tick_delta: Vec<u64>,
    switch_api_ms: Vec<f64>,
    switch_responsive_ms: Vec<f64>,
    scale_factor: f64,
    tabs: usize,
    churn_cycles: u64,
    navigation_errors: Vec<String>,
}
struct Probe {
    app: App,
    root: PathBuf,
    profile: PathBuf,
    metrics: Metrics,
    started: Instant,
    loaded: HashSet<TabId>,
    toolbar_ready: bool,
    phase: u8,
    phase_started: Instant,
    settle: Duration,
    sample: Duration,
    soak: Duration,
    recovery: Duration,
    initial_ticks: Vec<u64>,
    switch_index: usize,
    switching: Option<Instant>,
    callback: Arc<Mutex<Option<Instant>>>,
    failure: Option<String>,
    churn: bool,
    next_churn: Instant,
}
impl Probe {
    fn emit(&self) -> std::io::Result<()> {
        let path = self.root.join("metrics.json");
        let temp = self.root.join("metrics.tmp");
        std::fs::write(&temp, serde_json::to_vec(&self.metrics)?)?;
        std::fs::rename(temp, path)
    }
    fn stage(&mut self, name: &str) -> Result<(), String> {
        self.metrics.stage = name.into();
        self.emit()
            .map_err(|_| "Could not write benchmark metrics".into())
    }
    fn ticks(&self) -> Vec<u64> {
        self.app
            .session
            .as_ref()
            .map(|s| {
                s.tabs
                    .items
                    .iter()
                    .map(|t| {
                        t.title
                            .strip_prefix("Benchmark tick ")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    fn poll(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if let Some(error) = self.metrics.navigation_errors.first() {
            return Err(format!("Controlled benchmark navigation failed: {error}"));
        }
        if self.phase == 7 {
            return Ok(());
        }
        if self.started.elapsed() > Duration::from_secs(300) + self.soak {
            return Err(format!(
                "Benchmark timed out at stage {}",
                self.metrics.stage
            ));
        }
        if self.phase == 0 {
            let session = self.app.session.as_ref().ok_or("No benchmark session")?;
            if session.ready
                && self.toolbar_ready
                && session
                    .tabs
                    .active()
                    .is_some_and(|t| self.loaded.contains(&t.id))
                && self.metrics.first_usable_ms == 0.0
            {
                self.metrics.first_usable_ms = self.started.elapsed().as_secs_f64() * 1000.0;
            }
            if self.loaded.len() == self.metrics.tabs && self.metrics.first_usable_ms > 0.0 {
                self.metrics.all_loaded_ms = self.started.elapsed().as_secs_f64() * 1000.0;
                self.phase = 1;
                self.phase_started = Instant::now();
                self.stage("settling")?;
            }
        } else if self.phase == 1 && self.phase_started.elapsed() >= self.settle {
            self.initial_ticks = self.ticks();
            self.phase = 2;
            self.phase_started = Instant::now();
            self.stage("idle")?;
        } else if self.phase == 2 && self.phase_started.elapsed() >= self.sample {
            self.metrics.tick_delta = self
                .ticks()
                .into_iter()
                .zip(&self.initial_ticks)
                .map(|(a, b)| a.saturating_sub(*b))
                .collect();
            self.phase = 3;
            self.phase_started = Instant::now();
            self.stage("soak")?;
        } else if self.phase == 3
            && self.churn
            && self.phase_started.elapsed() < self.soak
            && Instant::now() >= self.next_churn
        {
            let s = self.app.session.as_mut().ok_or("No soak session")?;
            if s.tabs.items.len() > self.metrics.tabs {
                let id = s.tabs.items.last().ok_or("No churn tab")?.id;
                s.close_tab(id).map_err(|_| "Could not close churn tab")?;
            }
            s.open_tab("about:blank".into())
                .map_err(|_| "Could not create churn tab")?;
            map_fixture(
                &s.tabs.active().ok_or("No churn view")?.content.engine.view,
                &self.profile,
            )
            .map_err(|_| "Could not map churn fixture")?;
            s.command(Command::Navigate(format!("{FIXTURE_URL}?churn")))
                .map_err(|_| "Could not navigate churn fixture")?;
            let index = self.metrics.churn_cycles as usize % self.metrics.tabs;
            let id = s.tabs.items[index].id;
            s.command(Command::SelectTab(id))
                .map_err(|_| "Could not switch soak tab")?;
            s.render();
            self.metrics.churn_cycles += 1;
            self.next_churn = Instant::now() + Duration::from_secs(10);
            self.emit().map_err(|_| "Could not update soak metrics")?;
        } else if self.phase == 3 && self.phase_started.elapsed() >= self.soak {
            let s = self.app.session.as_mut().ok_or("No soak session")?;
            if s.tabs.items.len() > self.metrics.tabs {
                let id = s.tabs.items.last().ok_or("No churn tab")?.id;
                s.close_tab(id)
                    .map_err(|_| "Could not close final churn tab")?;
            }
            self.phase = 4;
            self.stage("switching")?;
        } else if self.phase == 4 {
            if let Some(start) = self.switching {
                let end = self
                    .callback
                    .lock()
                    .map_err(|_| "Benchmark callback unavailable")?
                    .take();
                if let Some(end) = end {
                    self.metrics
                        .switch_responsive_ms
                        .push(end.duration_since(start).as_secs_f64() * 1000.0);
                    self.switching = None;
                    self.switch_index += 1;
                } else if start.elapsed() > Duration::from_secs(10) {
                    return Err("Selected tab did not become responsive".into());
                }
            } else {
                let session = self.app.session.as_mut().ok_or("No session")?;
                if let Some(tab) = session.tabs.items.get(self.switch_index) {
                    let id = tab.id;
                    let start = Instant::now();
                    session
                        .command(Command::SelectTab(id))
                        .map_err(|_| "Tab selection failed")?;
                    session.render();
                    self.metrics
                        .switch_api_ms
                        .push(start.elapsed().as_secs_f64() * 1000.0);
                    let callback = self.callback.clone();
                    session
                        .tabs
                        .active()
                        .ok_or("No active tab")?
                        .content
                        .engine
                        .view
                        .evaluate_script_with_callback("document.readyState", move |_| {
                            if let Ok(mut slot) = callback.lock() {
                                *slot = Some(Instant::now());
                            }
                        })
                        .map_err(|_| "Tab responsiveness probe failed")?;
                    self.switching = Some(start);
                } else {
                    let ids: Vec<_> = session.tabs.items.iter().skip(1).map(|t| t.id).collect();
                    for id in ids {
                        session.close_tab(id).map_err(|_| "Tab closing failed")?;
                    }
                    session.persist();
                    session.render();
                    self.phase = 5;
                    self.phase_started = Instant::now();
                    self.stage("after_close")?;
                }
            }
        } else if self.phase == 5 && self.phase_started.elapsed() >= self.recovery {
            self.phase = 6;
            self.stage("done")?;
            self.app.session = None;
            self.phase = 7;
            event_loop.exit();
        }
        Ok(())
    }
}
impl ApplicationHandler<Event> for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let result = (|| -> Result<Session, Box<dyn std::error::Error>> {
            let session = Session::create_at(
                event_loop,
                self.app.proxy.clone(),
                self.profile.clone(),
                false,
                None,
            )?;
            for tab in &session.tabs.items {
                map_fixture(&tab.content.engine.view, &self.profile)?;
            }
            Ok(session)
        })();
        match result {
            Ok(session) => {
                self.metrics.construction_ms = self.started.elapsed().as_secs_f64() * 1000.0;
                self.metrics.scale_factor = session.window.scale_factor();
                session.render();
                self.app.session = Some(session);
            }
            Err(error) => {
                self.failure = Some(format!("Benchmark initialization failed: {error}"));
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(event_loop, id, event);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        if let Event::Page(
            _,
            PageEvent::Completed {
                error: Some(error), ..
            }
            | PageEvent::Error(error),
        ) = &event
            && self.metrics.navigation_errors.len() < 8
        {
            self.metrics.navigation_errors.push(error.clone());
        }
        if matches!(&event, Event::Command(Command::Ready)) {
            self.toolbar_ready = true;
        }
        if let Event::Page(
            id,
            PageEvent::Completed {
                error: None, url, ..
            },
        ) = &event
            && url.starts_with(FIXTURE_URL)
        {
            self.loaded.insert(*id);
        }
        self.app.user_event(event_loop, event);
        if self.phase == 0
            && let Err(error) = self.poll(event_loop)
        {
            self.failure = Some(error);
            event_loop.exit();
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.app.about_to_wait(event_loop);
        if let Err(error) = self.poll(event_loop) {
            self.failure = Some(error);
            event_loop.exit();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(100),
        ));
    }
}

#[test]
#[ignore = "Synthetic benchmark; use benchmarks/windows.ps1 with an explicit disposable root"]
fn windows_performance_probe() {
    let root = PathBuf::from(
        std::env::var_os("RUST_BROWSER_BENCH_ROOT").expect("Explicit benchmark root required"),
    );
    assert!(
        root.is_absolute() && root.join(".rust-browser-benchmark").is_file(),
        "Use a marked disposable benchmark directory"
    );
    let profile = root.join("profile");
    std::fs::create_dir_all(profile.join("fixture")).unwrap();
    let count = std::env::var("RUST_BROWSER_BENCH_TABS")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!((1..=20).contains(&count));
    std::fs::write(profile.join("fixture/index.html"),"<!doctype html><title>Benchmark tick 0</title><p>Synthetic timer workload</p><script>let ticks=0;setInterval(()=>{let end=performance.now()+4;while(performance.now()<end){}document.title='Benchmark tick '+(++ticks)},250)</script>").unwrap();
    // Additive JSON settings keep this probe source compatible with the v0.4 baseline.
    let snapshot = serde_json::json!({"version":1,"tabs":(0..count).map(|n|format!("{FIXTURE_URL}?tab={n}")).collect::<Vec<_>>(),"active":count-1,"settings":{"search_engine":"google","history_enabled":false,"restore_session":true,"automatic_suspension":true,"suspend_after_seconds":15}});
    std::fs::write(
        profile.join("session.json"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();
    let seconds = |key: &str, default: u64| {
        Duration::from_secs(
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
                .min(if key == "RUST_BROWSER_BENCH_SOAK" {
                    3600
                } else {
                    180
                }),
        )
    };
    let started = Instant::now();
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
        root,
        profile,
        metrics: Metrics {
            tabs: count,
            ..Metrics::default()
        },
        started,
        loaded: HashSet::new(),
        toolbar_ready: false,
        phase: 0,
        phase_started: Instant::now(),
        settle: seconds("RUST_BROWSER_BENCH_SETTLE", 30),
        sample: seconds("RUST_BROWSER_BENCH_SAMPLE", 10),
        soak: seconds("RUST_BROWSER_BENCH_SOAK", 120),
        recovery: seconds("RUST_BROWSER_BENCH_RECOVERY", 15),
        initial_ticks: Vec::new(),
        switch_index: 0,
        switching: None,
        callback: Arc::new(Mutex::new(None)),
        failure: None,
        churn: std::env::var("RUST_BROWSER_BENCH_CHURN").as_deref() == Ok("1"),
        next_churn: Instant::now(),
    };
    let result = event_loop.run_app(&mut probe);
    assert!(result.is_ok());
    assert!(probe.failure.is_none(), "{:?}", probe.failure);
    assert_eq!(probe.phase, 7);
}
