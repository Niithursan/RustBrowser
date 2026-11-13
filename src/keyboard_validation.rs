//! Keyboard reachability and scaled layout of the trusted toolbar, without OS input.
//!
//! Key presses are dispatched with the DevTools protocol (`Input.dispatchKeyEvent`)
//! to the toolbar's own renderer, so Chromium's sequential focus navigation decides
//! where Tab goes; no key reaches any other window. Display scaling is emulated in
//! the renderer (`Emulation.setDeviceMetricsOverride`) at the CSS width a maximized
//! 1920-pixel-wide window has at each scale. That exercises the toolbar's CSS layout
//! only, not native DPI handling, window bounds or monitor changes.
//!
//! The same checks then run in the two side views (v0.21): the vertical tab column
//! and the library column, each a trusted view of its own beside a live page.
use super::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
use windows::core::HSTRING;
use winit::platform::windows::EventLoopBuilderExtWindows;
use wry::WebViewExtWindows;

/// Display scale and the CSS width of a maximized window on a 1920-pixel display.
const SCALES: [(f64, u32); 4] = [(1.0, 1920), (1.25, 1536), (1.5, 1280), (2.0, 960)];
const MAX_PRESSES: usize = 60;

/// Describes the focused element, or "outside" when focus left the document.
const ACTIVE: &str = r#"(() => { const e = document.activeElement;
  if (!e || e === document.body) return 'outside';
  return (e.id || '') + '|' + (e.getAttribute('role') || e.tagName.toLowerCase()) + '|' +
    (e.getAttribute('aria-label') || e.textContent || '').trim().slice(0, 60); })()"#;

/// Every visible, enabled control a keyboard user must be able to reach. Tabs use a
/// roving tab stop (one tab in the Tab order, arrows for the rest) and are excluded.
/// Controls deliberately taken out of the Tab order (tabindex -1) are returned
/// separately so the test can check each one against the intended exceptions.
const EXPECTED: &str = r#"(() => { const describe = e => (e.id || '') + '|' + (e.getAttribute('role') || e.tagName.toLowerCase()) + '|' +
    (e.getAttribute('aria-label') || e.textContent || '').trim().slice(0, 60);
  const visible = [...document.querySelectorAll('button, input:not([type=hidden]), select, textarea, a[href], [tabindex]')]
    .filter(e => !e.disabled && e.getAttribute('role') !== 'tab' && !e.closest('[hidden],[inert]')
      && e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden');
  return JSON.stringify({
    expected: visible.filter(e => e.getAttribute('tabindex') !== '-1').map(describe),
    excluded: visible.filter(e => e.getAttribute('tabindex') === '-1')
      .map(e => (e.classList.contains('close') && e.closest('#tabs') ? 'tab-close|' : '') + describe(e)) }); })()"#;

/// Layout problems at the current emulated width: horizontal overflow, visible
/// controls outside the viewport, and visible controls overlapping each other.
const LAYOUT: &str = r#"(() => { const vw = document.documentElement.clientWidth;
  const controls = [...document.querySelectorAll('#tabrow button, #tabs [role=tab], #nav button, #nav input, #nav select, #more, #new, #tab-picker')]
    .filter(e => !e.closest('[hidden]') && e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden');
  const name = e => e.id || e.getAttribute('aria-label') || e.textContent.trim().slice(0, 30);
  const out = [];
  if (document.documentElement.scrollWidth > vw + 1) out.push('page overflows horizontally');
  const rects = controls.map(e => [e, e.getBoundingClientRect()]);
  for (const [e, r] of rects) if (r.left < -1 || r.right > vw + 1) out.push('outside viewport: ' + name(e));
  for (let i = 0; i < rects.length; i++) for (let j = i + 1; j < rects.length; j++) {
    const [a, p] = rects[i], [b, q] = rects[j];
    if (a.contains(b) || b.contains(a)) continue;
    const x = Math.min(p.right, q.right) - Math.max(p.left, q.left), y = Math.min(p.bottom, q.bottom) - Math.max(p.top, q.top);
    if (x > 1 && y > 1) out.push('overlap: ' + name(a) + ' / ' + name(b));
  }
  return JSON.stringify({ width: vw, dpr: devicePixelRatio, controls: controls.length, problems: out }); })()"#;

/// Layout problems in a side view: horizontal overflow, visible controls outside
/// the viewport, and visible controls overlapping each other.
const SIDE_LAYOUT: &str = r#"(() => { const vw = document.documentElement.clientWidth;
  const controls = [...document.querySelectorAll('button, input:not([type=hidden]), select, [role=tab]')]
    .filter(e => !e.closest('[hidden]') && e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden');
  const name = e => e.id || e.getAttribute('aria-label') || e.textContent.trim().slice(0, 30);
  const out = [];
  if (document.documentElement.scrollWidth > vw + 1) out.push('page overflows horizontally');
  const rects = controls.map(e => [e, e.getBoundingClientRect()]);
  for (const [e, r] of rects) if (r.left < -1 || r.right > vw + 1) out.push('outside viewport: ' + name(e));
  for (let i = 0; i < rects.length; i++) for (let j = i + 1; j < rects.length; j++) {
    const [a, p] = rects[i], [b, q] = rects[j];
    if (a.contains(b) || b.contains(a)) continue;
    const x = Math.min(p.right, q.right) - Math.max(p.left, q.left), y = Math.min(p.bottom, q.bottom) - Math.max(p.top, q.top);
    if (x > 1 && y > 1) out.push('overlap: ' + name(a) + ' / ' + name(b));
  }
  return JSON.stringify({ role: document.documentElement.dataset.role, width: vw, dpr: devicePixelRatio, controls: controls.length, problems: out }); })()"#;

/// Which trusted view the fixture drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Toolbar,
    Tabs,
    Library,
}

impl Target {
    /// Prefix for control descriptions from this view, so the views' lists stay apart.
    fn prefix(self) -> &'static str {
        match self {
            Self::Toolbar => "",
            Self::Tabs => "[tabs]",
            Self::Library => "[library]",
        }
    }
    /// Where a Tab walk in this view starts.
    fn start(self) -> &'static str {
        match self {
            Self::Toolbar => "document.getElementById('address').focus(); 'ok'",
            Self::Tabs => "document.getElementById('new').focus(); 'ok'",
            Self::Library => "document.getElementById('close-manager').focus(); 'ok'",
        }
    }
    /// The emulated scale and CSS width for each pass: the tab column is always 240
    /// wide; the library column is checked at its default, smallest and largest widths.
    fn sizes(self) -> [(f64, u32); 4] {
        match self {
            Self::Toolbar => SCALES,
            Self::Tabs => [(1.0, 240), (1.25, 240), (1.5, 240), (2.0, 240)],
            Self::Library => [(1.0, 360), (1.25, 260), (1.5, 640), (2.0, 260)],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Start,
    Settle,
    ScaleSet(usize),
    ScaleRead(usize),
    ScaleReset,
    Expected(bool),
    Focus(bool),
    Press(bool, usize),
    Read(bool, usize),
    OpenMore,
    ArrowFocus,
    Arrow,
    /// Vertical tabs on and history beside a live page.
    SideStart,
    SideWait,
    SideScaleSet(Target, usize),
    SideScaleRead(Target, usize),
    SideScaleReset(Target),
    /// A Tab walk in a side view.
    SideWalk(Target),
    Done,
}

struct Probe {
    app: App,
    root: PathBuf,
    step: Step,
    started: Instant,
    at: Instant,
    result: Arc<Mutex<std::collections::VecDeque<(Instant, String)>>>,
    eval_pending: std::cell::Cell<Option<&'static str>>,
    cdp_done: Arc<AtomicBool>,
    failure: Option<String>,
    expected: Vec<String>,
    excluded: Vec<String>,
    reached: Vec<String>,
    shift: bool,
    first: String,
    layout: Vec<String>,
    active_tab: Option<TabId>,
    log: Vec<String>,
    /// The view `eval`, `cdp` and the Tab walk act on.
    target: Target,
    /// A website for the side views to sit beside.
    server: Option<super::compat_validation::Server>,
}

fn decode(value: &str) -> String {
    serde_json::from_str::<String>(value).unwrap_or_else(|_| value.to_string())
}

impl Probe {
    fn eval(&self, script: &'static str) -> Result<(), String> {
        if let Some(pending) = self.eval_pending.get() {
            if pending == script {
                return Ok(());
            }
            return Err("Keyboard fixture changed query before consuming its response".into());
        }
        self.eval_pending.set(Some(script));
        let session = self.app.session.as_ref().ok_or("No session")?;
        let view = self.view(session)?;
        let result = self.result.clone();
        let prefix = self.target.prefix();
        // Controlled response latency reproduces duplicate in-flight focus probes.
        // Do not block the renderer or UI thread to emulate it.
        let delay = if script.ends_with("'ok'") {
            Duration::from_millis(120)
        } else {
            Duration::ZERO
        };
        view.evaluate_script_with_callback(script, move |value| {
            // Descriptions from a side view carry its name (see `Target::prefix`).
            let value = match serde_json::from_str::<String>(&value) {
                Ok(text) if !prefix.is_empty() && text.contains('|') && !text.starts_with('{') => {
                    serde_json::to_string(&format!("{prefix}{text}")).unwrap_or(value)
                }
                _ => value,
            };
            if let Ok(mut slot) = result.lock() {
                slot.push_back((Instant::now() + delay, value));
            }
        })
        .map_err(|_| "Toolbar evaluation failed".into())
    }
    /// The live view the fixture drives now.
    fn view<'a>(&self, session: &'a Session) -> Result<&'a wry::WebView, String> {
        let window = &session.windows[0];
        match self.target {
            Target::Toolbar => Ok(&window.toolbar),
            Target::Tabs => window
                .tabs_view
                .as_ref()
                .map(|v| &v.view)
                .ok_or_else(|| "No tab column".into()),
            Target::Library => window
                .library_view
                .as_ref()
                .map(|v| &v.view)
                .ok_or_else(|| "No library column".into()),
        }
    }

    /// Test-only DevTools call on the fixture's own toolbar; sets `cdp_done` on success.
    fn cdp(&self, method: &str, params: serde_json::Value) -> Result<(), String> {
        let session = self.app.session.as_ref().ok_or("No session")?;
        let view = self.view(session)?;
        let done = self.cdp_done.clone();
        done.store(false, Ordering::SeqCst);
        // SAFETY: UI-thread call on a live trusted WebView2 owned by this fixture.
        unsafe {
            view.webview().CallDevToolsProtocolMethod(
                &HSTRING::from(method),
                &HSTRING::from(params.to_string()),
                &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _| {
                    if result.is_ok() {
                        done.store(true, Ordering::SeqCst);
                    }
                    Ok(())
                })),
            )
        }
        .map_err(|_| format!("DevTools call {method} failed"))
    }

    /// One key press (down and up) through Chromium's own input handling. A key that
    /// produces text (Enter) is sent as `keyDown` with that text, as a real key press
    /// arrives; buttons activate on it. Other keys are `rawKeyDown`.
    fn key(&self, key: &str, code: &str, vk: u32, shift: bool) -> Result<(), String> {
        let modifiers = if shift { 8 } else { 0 };
        let text = if key == "Enter" { "\r" } else { "" };
        let event = |kind: &str| {
            serde_json::json!({ "type": kind, "key": key, "code": code, "text": text,
                "unmodifiedText": text, "windowsVirtualKeyCode": vk,
                "nativeVirtualKeyCode": vk, "modifiers": modifiers })
        };
        self.cdp(
            "Input.dispatchKeyEvent",
            event(if text.is_empty() {
                "rawKeyDown"
            } else {
                "keyDown"
            }),
        )?;
        self.cdp("Input.dispatchKeyEvent", event("keyUp"))
    }

    fn ready(&self) -> bool {
        self.cdp_done.load(Ordering::SeqCst)
    }

    fn poll(&mut self) -> Result<(), String> {
        if self.step == Step::Done {
            return Ok(());
        }
        if self.started.elapsed() > Duration::from_secs(90) {
            return Err(format!("Keyboard fixture timed out at {:?}", self.step));
        }
        if !self.app.session.as_ref().is_some_and(|s| s.ready) {
            return Ok(());
        }
        let value = {
            let mut queue = self.result.lock().map_err(|_| "Result lock failed")?;
            if queue.front().is_some_and(|(at, _)| Instant::now() >= *at) {
                queue.pop_front().map(|(_, value)| value)
            } else {
                None
            }
        };
        if value.is_some() {
            self.eval_pending.set(None);
        }
        let ready = self.ready();
        match self.step {
            Step::Start => {
                // Two tabs so the arrow keys have somewhere to go.
                let s = self.app.session.as_mut().ok_or("No session")?;
                s.command(Command::NewTab).map_err(|_| "New tab failed")?;
                s.render();
                self.at = Instant::now();
                self.step = Step::Settle;
            }
            Step::Settle if self.at.elapsed() > Duration::from_secs(2) => {
                self.cdp(
                    "Emulation.setFocusEmulationEnabled",
                    serde_json::json!({ "enabled": true }),
                )?;
                self.step = Step::ScaleSet(0);
            }
            Step::ScaleSet(i) if ready => {
                let (scale, width) = SCALES[i];
                self.cdp(
                    "Emulation.setDeviceMetricsOverride",
                    serde_json::json!({ "width": width, "height": 400,
                        "deviceScaleFactor": scale, "mobile": false }),
                )?;
                self.at = Instant::now();
                self.step = Step::ScaleRead(i);
            }
            Step::ScaleRead(i) if ready && self.at.elapsed() > Duration::from_millis(300) => {
                if value.is_none() && self.layout.len() == i {
                    self.eval(LAYOUT)?;
                    self.layout.push(String::new());
                } else if let Some(text) = value {
                    let report: serde_json::Value = serde_json::from_str(&decode(&text))
                        .map_err(|_| "Layout report unreadable")?;
                    self.log.push(format!("scale {}: {report}", SCALES[i].0));
                    self.layout[i] = report.to_string();
                    let problems = report["problems"].as_array().map_or(0, Vec::len);
                    if report["controls"].as_u64().unwrap_or(0) < 10 || problems > 0 {
                        return Err(format!("Toolbar layout at {}x: {report}", SCALES[i].0));
                    }
                    self.step = if i + 1 < SCALES.len() {
                        Step::ScaleSet(i + 1)
                    } else {
                        Step::ScaleReset
                    };
                    // ScaleSet waits for the previous call; nothing is outstanding now.
                    self.cdp_done.store(true, Ordering::SeqCst);
                }
            }
            Step::ScaleReset if ready => {
                self.cdp(
                    "Emulation.clearDeviceMetricsOverride",
                    serde_json::json!({}),
                )?;
                self.step = Step::Expected(false);
            }
            Step::Expected(more) if ready => {
                if let Some(text) = value {
                    let lists: serde_json::Value = serde_json::from_str(&decode(&text))
                        .map_err(|_| "Control list unreadable")?;
                    let strings = |key: &str| -> Vec<String> {
                        lists[key]
                            .as_array()
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default()
                    };
                    let prefix = self.target.prefix();
                    for item in strings("expected") {
                        let item = format!("{prefix}{item}");
                        if !self.expected.contains(&item) {
                            self.expected.push(item);
                        }
                    }
                    for item in strings("excluded") {
                        let item = format!("{prefix}{item}");
                        if !self.excluded.contains(&item) {
                            self.excluded.push(item);
                        }
                    }
                    self.step = Step::Focus(more);
                } else if self.at.elapsed() > Duration::from_millis(200) {
                    self.eval(EXPECTED)?;
                    self.at = Instant::now() + Duration::from_secs(3600);
                }
            }
            // Start from the address field, then walk forward and backward.
            Step::Focus(more) => {
                self.eval(self.target.start())?;
                self.shift = false;
                self.first.clear();
                self.at = Instant::now();
                self.step = Step::Press(more, 0);
            }
            Step::Press(more, n) if value.is_some() || n > 0 => {
                if n == 0 {
                    self.eval(ACTIVE)?;
                    // Already asked: Read only waits for this answer.
                    self.at = Instant::now() + Duration::from_secs(3600);
                    self.step = Step::Read(more, 0);
                    return Ok(());
                }
                self.key("Tab", "Tab", 9, self.shift)?;
                self.at = Instant::now();
                self.step = Step::Read(more, n);
            }
            Step::Read(more, n) if ready || n == 0 => {
                match value {
                    None if self.at.elapsed() > Duration::from_millis(80) => {
                        self.eval(ACTIVE)?;
                        self.at = Instant::now() + Duration::from_secs(3600);
                    }
                    None => {}
                    Some(text) => {
                        let focused = decode(&text);
                        // Focus outside a side view's document reads "outside" too.
                        let focused = if focused == format!("{}outside", self.target.prefix()) {
                            "outside".to_string()
                        } else {
                            focused
                        };
                        if n == 0 {
                            self.first = focused.clone();
                        }
                        let wrapped = n > 0 && focused == self.first;
                        if focused != "outside" && !self.reached.contains(&focused) {
                            self.reached.push(focused.clone());
                        }
                        if focused == "outside" || wrapped || n >= MAX_PRESSES {
                            if !self.shift {
                                // Back to the address field and walk backwards.
                                self.shift = true;
                                self.eval(self.target.start())?;
                                self.step = Step::Press(more, 0);
                                self.first.clear();
                                return Ok(());
                            }
                            self.step = match (self.target, more) {
                                (Target::Toolbar, false) => Step::OpenMore,
                                (Target::Library, _) => Step::SideWalk(Target::Tabs),
                                _ => Step::ArrowFocus,
                            };
                        } else {
                            self.step = Step::Press(more, n + 1);
                            self.cdp_done.store(true, Ordering::SeqCst);
                        }
                    }
                }
            }
            // Open the More disclosure with the keyboard, then walk again.
            Step::OpenMore => {
                if value.is_none() {
                    self.eval("document.getElementById('more').focus(); 'ok'")?;
                    self.at = Instant::now() + Duration::from_secs(3600);
                    return Ok(());
                }
                self.key("Enter", "Enter", 13, false)?;
                self.at = Instant::now();
                self.step = Step::Expected(true);
            }
            Step::ArrowFocus => {
                let s = self.app.session.as_ref().ok_or("No session")?;
                self.active_tab = s.windows[0].tabs.active().map(|t| t.id);
                self.eval(
                    "document.querySelector('#tabs [role=tab][aria-selected=true]').focus(); 'ok'",
                )?;
                self.step = Step::Arrow;
                self.at = Instant::now();
            }
            Step::Arrow => {
                let (key, code, vk) = if self.target == Target::Tabs {
                    ("ArrowDown", "ArrowDown", 40)
                } else {
                    ("ArrowLeft", "ArrowLeft", 37)
                };
                if value.is_some() {
                    self.key(key, code, vk, false)?;
                    self.at = Instant::now();
                } else if self.at.elapsed() > Duration::from_secs(2) {
                    let s = self.app.session.as_ref().ok_or("No session")?;
                    if s.windows[0].tabs.active().map(|t| t.id) == self.active_tab {
                        return Err(format!(
                            "{key} on the selected tab did not select another tab"
                        ));
                    }
                    self.step = if self.target == Target::Tabs {
                        Step::Done
                    } else {
                        Step::SideStart
                    };
                }
            }
            Step::SideStart => {
                let server = super::compat_validation::Server::start();
                let url = format!("{}?side-views", server.url);
                self.server = Some(server);
                let s = self.app.session.as_mut().ok_or("No session")?;
                s.current = 0;
                s.command(Command::Navigate(url))
                    .map_err(|_| "Navigate failed")?;
                // Views are created from events, as in the browser, not from this poll.
                self.app
                    .proxy
                    .send_event(Event::Command(Command::SetVerticalTabs(true)))
                    .map_err(|_| "proxy")?;
                self.at = Instant::now();
                self.step = Step::SideWait;
            }
            Step::SideWait
                if self.at.elapsed() > Duration::from_secs(2)
                    && self.app.session.as_ref().is_some_and(|s| {
                        s.windows[0]
                            .tabs
                            .items
                            .iter()
                            .all(|t| !t.content.state.loading)
                    }) =>
            {
                self.app
                    .proxy
                    .send_event(Event::Command(Command::ShowPanel(Panel::History)))
                    .map_err(|_| "proxy")?;
                self.at = Instant::now();
                self.step = Step::SideScaleSet(Target::Tabs, 0);
            }
            Step::SideScaleSet(target, i)
                if ready && self.at.elapsed() > Duration::from_millis(800) =>
            {
                let s = self.app.session.as_ref().ok_or("No session")?;
                if s.windows[0].library_view.is_none() || s.windows[0].tabs_view.is_none() {
                    return Err("The side views were not created".into());
                }
                self.target = target;
                let (scale, width) = target.sizes()[i];
                self.cdp(
                    "Emulation.setDeviceMetricsOverride",
                    serde_json::json!({ "width": width, "height": 640,
                        "deviceScaleFactor": scale, "mobile": false }),
                )?;
                self.at = Instant::now();
                self.step = Step::SideScaleRead(target, i);
            }
            Step::SideScaleRead(target, i) if ready => {
                if let Some(text) = value {
                    let report: serde_json::Value = serde_json::from_str(&decode(&text))
                        .map_err(|_| "Side layout report unreadable")?;
                    let (scale, _) = target.sizes()[i];
                    self.log.push(format!("{target:?} at {scale}: {report}"));
                    let problems = report["problems"].as_array().map_or(0, Vec::len);
                    let role = if target == Target::Tabs {
                        "tabs"
                    } else {
                        "library"
                    };
                    let least = if target == Target::Tabs { 3 } else { 5 };
                    if report["role"] != role
                        || report["controls"].as_u64().unwrap_or(0) < least
                        || problems > 0
                    {
                        return Err(format!("{target:?} layout at {scale}x: {report}"));
                    }
                    self.at = Instant::now();
                    self.step = if i + 1 < 4 {
                        Step::SideScaleSet(target, i + 1)
                    } else {
                        Step::SideScaleReset(target)
                    };
                    self.cdp_done.store(true, Ordering::SeqCst);
                } else if self.at.elapsed() > Duration::from_millis(300) {
                    self.eval(SIDE_LAYOUT)?;
                    self.at = Instant::now() + Duration::from_secs(3600);
                }
            }
            Step::SideScaleReset(target) if ready => {
                self.cdp(
                    "Emulation.clearDeviceMetricsOverride",
                    serde_json::json!({}),
                )?;
                self.at = Instant::now();
                self.step = if target == Target::Tabs {
                    Step::SideScaleSet(Target::Library, 0)
                } else {
                    Step::SideWalk(Target::Library)
                };
            }
            Step::SideWalk(target) if ready => {
                self.target = target;
                self.cdp(
                    "Emulation.setFocusEmulationEnabled",
                    serde_json::json!({ "enabled": true }),
                )?;
                self.at = Instant::now();
                self.step = Step::Expected(true);
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
                self.failure = Some("Keyboard fixture startup failed".into());
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
        if let Err(error) = self.poll() {
            self.failure = Some(error);
        }
        if self.step == Step::Done || self.failure.is_some() {
            el.exit();
            return;
        }
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(20),
        ));
    }
}

#[test]
#[ignore = "Requires Windows/WebView2; sends key events only to the fixture's own toolbar"]
fn native_toolbar_keyboard_reachability_and_scaled_layout() {
    let root = std::env::temp_dir().join(format!(
        "rust-browser-keyboard-{}-{}",
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
        step: Step::Start,
        started: Instant::now(),
        at: Instant::now(),
        result: Arc::new(Mutex::new(std::collections::VecDeque::new())),
        eval_pending: std::cell::Cell::new(None),
        cdp_done: Arc::new(AtomicBool::new(true)),
        failure: None,
        expected: Vec::new(),
        excluded: Vec::new(),
        reached: Vec::new(),
        shift: false,
        first: String::new(),
        layout: Vec::new(),
        active_tab: None,
        log: Vec::new(),
        target: Target::Toolbar,
        server: None,
    };
    let _ = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let expected = std::mem::take(&mut probe.expected);
    let excluded = std::mem::take(&mut probe.excluded);
    let reached = std::mem::take(&mut probe.reached);
    let log = std::mem::take(&mut probe.log);
    drop(probe);
    let _ = std::fs::remove_dir_all(root);
    println!("{}", log.join("\n"));
    assert!(failure.is_none(), "{failure:?}\n{}", log.join("\n"));
    let missing: Vec<&String> = expected.iter().filter(|e| !reached.contains(e)).collect();
    println!(
        "{} controls expected, {} reached by Tab or Shift+Tab",
        expected.len(),
        reached.len()
    );
    assert!(
        expected.len() >= 15,
        "Too few toolbar controls found: {expected:?}"
    );
    // Enter on More must have revealed the tab and library actions.
    for id in [
        "pin-active|",
        "move-left|",
        "keep-awake|",
        "show-settings|",
        "new-private|",
    ] {
        assert!(
            expected.iter().any(|e| e.starts_with(id)),
            "More (Enter) did not reveal {id}: {expected:?}"
        );
    }
    // Kept out of the Tab order on purpose: each tab's close button (Ctrl+W and Ctrl+F4
    // close tabs from the keyboard) and the panel heading, which receives focus by
    // script when a panel opens and is not a control.
    let unexpected: Vec<&String> = excluded
        .iter()
        .map(|e| {
            (
                e,
                e.trim_start_matches("[tabs]")
                    .trim_start_matches("[library]"),
            )
        })
        .filter(|(_, e)| !e.starts_with("tab-close|") && !e.starts_with("manager-title|h1|"))
        .map(|(e, _)| e)
        .collect();
    assert!(
        unexpected.is_empty(),
        "Controls taken out of the Tab order: {unexpected:?}"
    );
    assert!(
        missing.is_empty(),
        "Not reachable with Tab or Shift+Tab: {missing:?}\nreached: {reached:?}"
    );
    assert!(
        reached.iter().any(|r| r.contains("|tab|")),
        "No browser tab is in the Tab order: {reached:?}"
    );
    // The side views (v0.21): the tab column's own tab stop and New tab button, and
    // the library column's controls, all reached by Tab or Shift+Tab.
    assert!(
        reached
            .iter()
            .any(|r| r.starts_with("[tabs]") && r.contains("|tab|")),
        "No tab in the tab column's Tab order: {reached:?}"
    );
    for id in [
        "[tabs]new|",
        "[library]close-manager|",
        "[library]library-resizer|",
    ] {
        assert!(
            expected.iter().any(|e| e.starts_with(id)),
            "Side view control {id} not found: {expected:?}"
        );
    }
}
