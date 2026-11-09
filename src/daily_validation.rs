//! Controlled production-application checks. Run through validation/windows-daily.ps1.
use super::*;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};
use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
use windows::core::{HSTRING, Interface};
use winit::platform::windows::EventLoopBuilderExtWindows;
use wry::WebViewExtWindows;

struct Probe {
    app: App,
    root: PathBuf,
    urls: [String; 2],
    tls: String,
    phase: u8,
    started: Instant,
    step: Instant,
    result: Arc<Mutex<Option<String>>>,
    failure: Option<String>,
    first: TabId,
    other: TabId,
    core: usize,
    navigation_failures: Vec<String>,
    emulated: Arc<AtomicBool>,
}
impl Probe {
    /// Test-only CDP media emulation on the trusted toolbar; sets `emulated` on success.
    fn emulate(&self, features: serde_json::Value) -> Result<(), String> {
        let session = self.app.session.as_ref().ok_or("No session")?;
        let done = self.emulated.clone();
        done.store(false, Ordering::SeqCst);
        let params = serde_json::json!({ "features": features }).to_string();
        // SAFETY: UI-thread call on the live toolbar WebView2 owned by this fixture.
        unsafe {
            session.toolbar.webview().CallDevToolsProtocolMethod(
                &HSTRING::from("Emulation.setEmulatedMedia"),
                &HSTRING::from(params),
                &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _| {
                    if result.is_ok() {
                        done.store(true, Ordering::SeqCst);
                    }
                    Ok(())
                })),
            )
        }
        .map_err(|_| "Media emulation request failed".into())
    }
    fn eval(&self, toolbar: bool, script: &str) -> Result<(), String> {
        let session = self.app.session.as_ref().ok_or("No session")?;
        let view = if toolbar {
            &session.toolbar
        } else {
            &session.tabs.active().ok_or("No tab")?.content.engine.view
        };
        let result = self.result.clone();
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
            let state = self
                .app
                .session
                .as_ref()
                .and_then(|s| s.tabs.active())
                .map(|t| {
                    (
                        t.title.as_str(),
                        t.content.state.loading,
                        t.content.state.failed,
                    )
                });
            return Err(format!(
                "Daily fixture timed out at phase {}: {state:?}",
                self.phase
            ));
        }
        let value = self.result.lock().map_err(|_| "Result lock failed")?.take();
        let s = self.app.session.as_mut().ok_or("No application session")?;
        if !s.ready {
            return Ok(());
        }
        let loaded = s
            .tabs
            .active()
            .is_some_and(|t| !t.content.state.loading && t.title == "Modern compatibility ready");
        if matches!(self.phase, 2 | 6)
            && let Some(tab) = s.tabs.active()
            && tab.content.state.failed
        {
            return Err(format!(
                "Controlled HTTP navigation failed at phase {}: {}",
                self.phase, tab.content.state.message
            ));
        }
        match self.phase {
            phase
                if matches!(phase, 2 | 6 | 9 | 17)
                    && s.tabs
                        .active()
                        .is_some_and(|t| t.title == "Modern compatibility failed") =>
            {
                self.eval(
                    false,
                    "JSON.stringify({stage:window.controlledStage,error:window.controlledFailure,title:document.title,state:document.readyState})",
                )?;
                self.phase = 99;
            }
            99 if value.is_some() => return Err(format!("Modern fixture API failure: {value:?}")),
            0 if s.diagnostics.snapshot().enabled => {
                self.first = s.tabs.active().ok_or("No initial tab")?.id;
                self.core = s
                    .tabs
                    .active()
                    .ok_or("No engine")?
                    .content
                    .engine
                    .view
                    .webview()
                    .as_raw() as usize;
                if s.command(Command::ClearDiagnostics { confirmed: false })
                    .is_ok()
                {
                    return Err("Unconfirmed diagnostic clear accepted".into());
                }
                s.diagnostics.record(DiagnosticCode::CommandFailed);
                s.command(Command::SetTheme(rust_browser::settings::Theme::Light))
                    .map_err(|_| "Light theme failed")?;
                s.command(Command::ShowPanel(Panel::Settings))
                    .map_err(|_| "Settings failed")?;
                s.library_action(Action::SaveBookmark {
                    id: None,
                    title: "Controlled bookmark".into(),
                    url: format!("{}modern.html", self.urls[0]),
                    folder: None,
                })
                .map_err(|_| "Bookmark setup failed")?;
                s.render();
                self.eval(true,"document.body.dataset.theme==='light' && getComputedStyle(document.getElementById('address')).color==='rgb(27, 36, 48)' && !document.getElementById('clear-diagnostics').disabled")?;
                self.phase = 1;
            }
            1 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Light theme/diagnostic controls failed".into());
                }
                s.command(Command::Navigate(format!("{}modern.html", self.urls[0])))
                    .map_err(|_| "Modern navigation failed")?;
                self.phase = 2;
            }
            2 if loaded => {
                self.eval(false,"JSON.stringify(window.controlled)==='{}' ? false : controlled.data===42 && controlled.worker===42 && controlled.digest===32 && controlled.serviceWorker===42 && controlled.cache==='cached-synthetic' && controlled.cloned===3 && controlled.aborted===true && controlled.broadcast==='broadcast-ok' && controlled.cookie && controlled.storage==='ready' && controlled.ipc==='undefined' && (localStorage.setItem('origin_only','first'),true)")?;
                self.phase = 3;
            }
            3 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err(
                        "Modules/fetch/worker/service worker/cache/crypto/storage/modern API or remote IPC fixture failed".into(),
                    );
                }
                self.eval(
                    false,
                    "document.getElementById('route').click();location.search==='?route=2'",
                )?;
                self.phase = 4;
            }
            4 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("SPA route failed".into());
                }
                s.command(Command::Back).map_err(|_| "SPA Back failed")?;
                self.phase = 5;
            }
            5 if s
                .tabs
                .active()
                .is_some_and(|t| t.content.state.url.ends_with("/modern.html")) =>
            {
                s.open_tab(format!("{}redirect-modern", self.urls[1]))
                    .map_err(|_| "Second origin failed")?;
                self.other = s.tabs.active().ok_or("No second tab")?.id;
                self.phase = 6;
            }
            6 if loaded => {
                self.eval(false,"localStorage.getItem('origin_only')===null && typeof window.ipc==='undefined' && location.pathname==='/modern.html'")?;
                self.phase = 7;
            }
            7 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Cross-origin redirect/storage boundary failed".into());
                }
                s.command(Command::SelectTab(self.first))
                    .map_err(|_| "First tab selection failed")?;
                if s.tabs
                    .active()
                    .ok_or("No first tab")?
                    .content
                    .engine
                    .view
                    .webview()
                    .as_raw() as usize
                    != self.core
                {
                    return Err("Tab switch recreated a controller".into());
                }
                s.open_tab("about:blank".into())
                    .map_err(|_| "Blank replacement setup failed")?;
                s.command(Command::Navigate(format!("{}modern.html", self.urls[0])))
                    .map_err(|_| "Blank replacement navigation failed")?;
                self.step = Instant::now();
                self.phase = 17;
            }
            17 if loaded && self.step.elapsed() > Duration::from_millis(250) => {
                if !self.navigation_failures.is_empty() {
                    return Err(format!(
                        "Superseded blank navigation reported a failure: {:?}",
                        self.navigation_failures
                    ));
                }
                s.command(Command::CloseActive)
                    .map_err(|_| "Cannot close replacement fixture")?;
                s.command(Command::SelectTab(self.first))
                    .map_err(|_| "Cannot return to first fixture")?;
                s.command(Command::Navigate(format!("{}slow", self.urls[0])))
                    .map_err(|_| "Slow navigation failed")?;
                self.phase = 8;
            }
            8 if s.tabs.active().is_some_and(|t| t.content.state.loading) => {
                s.command(Command::Navigate(format!("{}modern.html", self.urls[0])))
                    .map_err(|_| "Replacement navigation failed")?;
                self.step = Instant::now();
                self.phase = 9;
            }
            9 if loaded && self.step.elapsed() > Duration::from_secs(1) => {
                if s.tabs.active().is_some_and(|t| t.content.state.failed) {
                    return Err("Old completion overwrote the current navigation".into());
                }
                // Two host loads in one event-loop turn: the first load's
                // NavigationStarting can only be delivered after it was
                // superseded, so its abort must not surface as a failure.
                s.command(Command::Navigate(format!("{}slow?same-turn", self.urls[0])))
                    .map_err(|_| "Same-turn first navigation failed")?;
                s.command(Command::Navigate(format!(
                    "{}modern.html?same-turn",
                    self.urls[0]
                )))
                .map_err(|_| "Same-turn replacement navigation failed")?;
                self.step = Instant::now();
                self.phase = 19;
            }
            19 if loaded && self.step.elapsed() > Duration::from_secs(1) => {
                if !self.navigation_failures.is_empty() {
                    return Err(format!(
                        "Same-turn superseded navigation reported a failure: {:?}",
                        self.navigation_failures
                    ));
                }
                if s.tabs.active().is_some_and(|t| t.content.state.failed) {
                    return Err("Same-turn superseded completion overwrote the page".into());
                }
                s.command(Command::Navigate(format!("{}slow", self.urls[0])))
                    .map_err(|_| "Stop setup failed")?;
                self.phase = 10;
            }
            10 if s.tabs.active().is_some_and(|t| t.content.state.loading) => {
                s.command(Command::Stop).map_err(|_| "Stop failed")?;
                self.step = Instant::now();
                self.phase = 11;
            }
            11 if self.step.elapsed() > Duration::from_secs(1) => {
                if !self.navigation_failures.is_empty() {
                    return Err(format!(
                        "Controlled navigation or cancellation reported a failure: {:?}",
                        self.navigation_failures
                    ));
                }
                let page = &s.tabs.active().ok_or("No stopped page")?.content.state;
                if page.failed || page.loading || page.message != "Stopped." {
                    return Err(format!(
                        "Cancellation state: failed={} loading={} message={}",
                        page.failed, page.loading, page.message
                    ));
                }
                s.command(Command::Navigate(self.tls.clone()))
                    .map_err(|_| "TLS fixture navigation failed")?;
                self.phase = 12;
            }
            12 if s.tabs.active().is_some_and(|t| t.content.state.failed) => {
                if !s
                    .tabs
                    .active()
                    .ok_or("No TLS page")?
                    .content
                    .state
                    .message
                    .contains("certificate")
                {
                    return Err(format!(
                        "Untrusted TLS result: {}",
                        s.tabs.active().ok_or("No TLS state")?.content.state.message
                    ));
                }
                s.render();
                self.eval(true, "document.getElementById('alert').getAttribute('role')==='alert' && document.getElementById('alert').textContent.includes('certificate') && document.getElementById('status').className==='error'")?;
                self.phase = 22;
            }
            22 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Navigation failure was not announced assertively".into());
                }
                s.command(Command::ShowPanel(Panel::Settings))
                    .map_err(|_| "Cannot inspect diagnostics")?;
                s.render();
                // Panel heading receives focus; row actions name their item.
                self.eval(true, "(()=>{const focused=document.activeElement.id==='manager-title';renderLibrary({folders:[],bookmarks:[{id:9,title:'Controlled row',url:'https://example.test/',folder:null}],history:[]},'bookmarks');const names=[...document.querySelectorAll('#library-results button')].map(b=>b.getAttribute('aria-label'));librarySignature='';return focused&&names.join('|')==='Open Controlled row|New tab Controlled row|Edit Controlled row|Remove Controlled row'})()")?;
                self.phase = 23;
            }
            23 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Panel focus or contextual button names failed".into());
                }
                s.command(Command::ClearDiagnostics { confirmed: true })
                    .map_err(|_| "Confirmed diagnostic clear failed")?;
                self.phase = 13;
            }
            13 if s
                .diagnostics
                .snapshot()
                .entries
                .iter()
                .any(|e| e.code == DiagnosticCode::Cleared) =>
            {
                if !s
                    .library_data
                    .bookmarks
                    .iter()
                    .any(|b| b.title == "Controlled bookmark")
                {
                    return Err("Diagnostic clear changed bookmarks".into());
                }
                s.render();
                self.eval(true,"document.getElementById('address').value='synthetic draft';window.focusAddress();document.getElementById('address').selectionEnd===15")?;
                self.phase = 14;
            }
            14 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Address focus did not select text".into());
                }
                s.page_event(
                    self.other,
                    PageEvent::Title("Background title changed".into()),
                );
                let _ = s.window.request_inner_size(LogicalSize::new(640.0, 620.0));
                s.render();
                self.step = Instant::now();
                self.phase = 15;
            }
            15 if self.step.elapsed() > Duration::from_millis(250) => {
                self.eval(true,"document.getElementById('address').value==='synthetic draft' && document.getElementById('chrome').scrollWidth<=innerWidth+1 && document.getElementById('load-progress').getAttribute('aria-hidden')==='true'")?;
                self.phase = 16;
            }
            16 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err(
                        "Compact toolbar, draft preservation or loading accessibility failed"
                            .into(),
                    );
                }
                // Text scaling: 150% zoom must not overflow the narrow window, and
                // the active tab keeps a non-color accent bar plus bold label.
                self.eval(true, "(()=>{const tab=document.querySelector('.tab.active');const style=getComputedStyle(tab);const marked=style.boxShadow.includes('inset')&&getComputedStyle(tab.querySelector('.select')).fontWeight==='700';document.body.style.zoom='1.5';const chrome=document.getElementById('chrome');const fits=chrome.scrollWidth<=chrome.clientWidth+1;document.body.style.zoom='';const more=document.getElementById('more'),actions=document.getElementById('tab-actions');more.click();const opened=more.getAttribute('aria-expanded')==='true'&&!actions.hidden;more.click();const closed=more.getAttribute('aria-expanded')==='false'&&actions.hidden;const draft=input.value;input.value='https://user:pw@example.test/path?q=1';updateAddressView();const view=document.getElementById('address-view');const host=!view.hidden&&view.firstChild.textContent==='example.test'&&view.lastChild.textContent==='/path?q=1'&&!view.textContent.includes('pw');input.value=draft;updateAddressView();return marked&&fits&&opened&&closed&&host})()")?;
                self.phase = 20;
            }
            20 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Active-tab indicator, 150% scaling, More disclosure or address view failed".into());
                }
                self.emulate(serde_json::json!([
                    {"name": "forced-colors", "value": "active"},
                    {"name": "prefers-reduced-motion", "value": "reduce"}
                ]))?;
                self.phase = 24;
            }
            24 if self.emulated.load(Ordering::SeqCst) => {
                // Windows high contrast keeps a non-colour active-tab outline; reduced
                // motion stops the loading animation even while loading is shown.
                self.eval(true, "(()=>{const media=matchMedia('(forced-colors: active)').matches&&matchMedia('(prefers-reduced-motion: reduce)').matches;const tab=getComputedStyle(document.querySelector('.tab.active'));const progress=document.getElementById('load-progress');progress.setAttribute('aria-hidden','false');const animation=getComputedStyle(progress,'::after').animationName;progress.setAttribute('aria-hidden','true');return media&&tab.outlineStyle==='solid'&&tab.outlineWidth==='2px'&&animation==='none'})()")?;
                self.phase = 25;
            }
            25 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("High-contrast or reduced-motion styling failed".into());
                }
                // Emulation ends with this session's toolbar view.
                s.persist();
                self.app.session = None;
                self.phase = 100;
                el.exit();
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
                self.failure = Some("Daily application startup failed".into());
                el.exit();
            }
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(el, id, event)
    }
    fn user_event(&mut self, el: &ActiveEventLoop, event: Event) {
        if let Event::Page(
            _,
            PageEvent::Completed {
                error: Some(text), ..
            }
            | PageEvent::Error(text),
        ) = &event
            && self.navigation_failures.len() < 8
        {
            self.navigation_failures.push(text.clone());
        }
        self.app.user_event(el, event)
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.app.about_to_wait(el);
        if let Err(error) = self.poll(el) {
            self.failure = Some(error);
            el.exit();
        }
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(25),
        ));
    }
}

#[test]
#[ignore = "Requires the owned untrusted-TLS fixture; run validation/windows-daily.ps1"]
fn native_daily_navigation_diagnostics_and_modern_sites() {
    let tls = std::env::var("RUST_BROWSER_TLS_FIXTURE").expect("Run validation/windows-daily.ps1");
    let url = url::Url::parse(&tls).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert_eq!(url.scheme(), "https");
    assert!(url.port().is_some());
    let first = super::compat_validation::Server::start();
    let second = super::compat_validation::Server::start();
    let root = std::env::temp_dir().join(format!(
        "rust-browser-daily-{}-{}",
        std::process::id(),
        SystemTime::now()
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
        },
        root: root.clone(),
        urls: [first.url.clone(), second.url.clone()],
        tls,
        phase: 0,
        started: Instant::now(),
        step: Instant::now(),
        result: Arc::new(Mutex::new(None)),
        failure: None,
        first: 0,
        other: 0,
        core: 0,
        navigation_failures: Vec::new(),
        emulated: Arc::new(AtomicBool::new(false)),
    };
    let result = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let phase = probe.phase;
    drop(probe);
    assert!(result.is_ok());
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(phase, 100);
    assert_eq!(
        storage::load(&root.join("session.json"))
            .unwrap()
            .settings
            .theme,
        rust_browser::settings::Theme::Light
    );
    let _ = std::fs::remove_dir_all(root);
}
