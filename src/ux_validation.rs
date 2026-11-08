//! Native regression checks use only the reserved local fixture and disposable profiles.
use super::*;
use crate::native_validation::{FIXTURE_URL, map_fixture, write_fixture};
use std::sync::{Arc, Mutex};
use windows::core::Interface;
use winit::platform::windows::EventLoopBuilderExtWindows;
use wry::WebViewExtWindows;

struct Probe {
    app: App,
    root: PathBuf,
    private: bool,
    phase: u8,
    started: Instant,
    result: Arc<Mutex<Option<String>>>,
    failure: Option<String>,
    ids: Vec<TabId>,
    core: usize,
}
impl Probe {
    fn eval(&self, toolbar: bool, script: &str) -> Result<(), String> {
        let s = self.app.session.as_ref().ok_or("No session")?;
        let view = if toolbar {
            &s.toolbar
        } else {
            &s.tabs.active().ok_or("No active tab")?.content.engine.view
        };
        let result = self.result.clone();
        view.evaluate_script_with_callback(script, move |value| {
            if let Ok(mut slot) = result.lock() {
                *slot = Some(value)
            }
        })
        .map_err(|_| "Could not evaluate native UI assertion".into())
    }
    fn poll(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.phase == 20 {
            return Ok(());
        }
        if self.started.elapsed() > Duration::from_secs(60) {
            return Err(format!("Native UX check timed out at phase {}", self.phase));
        }
        let value = self.result.lock().map_err(|_| "No script result")?.take();
        let s = self.app.session.as_mut().ok_or("No session")?;
        if !s.ready {
            return Ok(());
        }
        if self.private {
            if self.phase == 0 {
                if !s.private || s.store.is_some() || !s.settings.favorites.is_empty() {
                    return Err("Private new-tab settings read normal data".into());
                }
                s.command(Command::AddFavorite {
                    title: "Private favorite".into(),
                    url: "https://example.com/private".into(),
                })
                .map_err(|_| "Could not set private favorite")?;
                s.command(Command::SetTheme(rust_browser::settings::Theme::Plum))
                    .map_err(|_| "Could not set private theme")?;
                s.command(Command::PinActive)
                    .map_err(|_| "Could not pin private tab")?;
                if s.command(Command::SetShowRecent(true)).is_ok() {
                    return Err("Private recent history became available".into());
                }
                s.persist();
                self.app.session = None;
                self.phase = 20;
                event_loop.exit();
            }
            return Ok(());
        }
        match self.phase {
            0 => {
                if s.tabs.items.iter().any(|t| {
                    t.content.state.loading
                        || t.content
                            .engine
                            .view
                            .url()
                            .ok()
                            .is_none_or(|u| !u.starts_with(FIXTURE_URL))
                }) {
                    return Ok(());
                }
                self.ids = s.tabs.items.iter().map(|t| t.id).collect();
                let bg = self.ids[0];
                s.command(Command::SelectTab(bg))
                    .map_err(|_| "Could not select fixture")?;
                self.core = s
                    .tabs
                    .active()
                    .ok_or("No tab")?
                    .content
                    .engine
                    .view
                    .webview()
                    .as_raw() as usize;
                self.eval(false, "window.v05Retained='kept';typeof window.ipc")?;
                self.phase = 1;
            }
            1 => {
                if let Some(value) = value {
                    if value != "\"undefined\"" {
                        return Err("Remote page received privileged IPC".into());
                    }
                    let bg = self.ids[0];
                    s.command(Command::SelectTab(self.ids[1]))
                        .map_err(|_| "Could not hide fixture")?;
                    s.started = Instant::now() - Duration::from_secs(200);
                    s.settings.suspend_after_seconds = 15;
                    let tab = s.tabs.get_mut(bg).ok_or("No background tab")?;
                    tab.content.lifecycle = Lifecycle::default();
                    tab.content.lifecycle.visible(false, Duration::ZERO);
                    tab.keep_awake = true;
                    s.next_background = Instant::now();
                    s.background_tick();
                    if s.tabs.get_mut(bg).ok_or("No tab")?.content.lifecycle.phase != Phase::Awake {
                        return Err("Keep awake tab slept".into());
                    }
                    s.tabs.get_mut(bg).ok_or("No tab")?.keep_awake = false;
                    s.tabs.pin(bg, true);
                    s.next_background = Instant::now();
                    s.background_tick();
                    if s.tabs.get_mut(bg).ok_or("No tab")?.content.lifecycle.phase != Phase::Awake {
                        return Err("Pinned tab slept".into());
                    }
                    s.tabs.pin(bg, false);
                    s.settings.automatic_suspension = false;
                    s.next_background = Instant::now();
                    s.background_tick();
                    if s.tabs.get_mut(bg).ok_or("No tab")?.content.lifecycle.phase != Phase::Awake {
                        return Err("Disabled suspension still slept".into());
                    }
                    s.settings.automatic_suspension = true;
                    s.next_background = Instant::now();
                    s.background_tick();
                    let ticket = s
                        .tabs
                        .get_mut(bg)
                        .ok_or("No tab")?
                        .content
                        .lifecycle
                        .generation;
                    s.command(Command::SelectTab(bg))
                        .map_err(|_| "Could not select pending tab")?;
                    s.suspension_result(bg, ticket, true);
                    if native_background::suspended(
                        &s.tabs.active().ok_or("No active tab")?.content.engine.view,
                    )
                    .map_err(|_| "Could not query suspension")?
                    {
                        return Err("A late suspend result froze the active tab".into());
                    }
                    s.command(Command::SelectTab(self.ids[1]))
                        .map_err(|_| "Could not select other tab")?;
                    let tab = s.tabs.get_mut(bg).ok_or("No tab")?;
                    tab.content.lifecycle = Lifecycle::default();
                    tab.content.lifecycle.visible(false, Duration::ZERO);
                    s.next_background = Instant::now();
                    s.background_tick();
                    self.phase = 2;
                }
            }
            2 => {
                let bg = self.ids[0];
                if s.tabs.get_mut(bg).ok_or("No tab")?.content.lifecycle.phase == Phase::Sleeping {
                    if !native_background::suspended(
                        &s.tabs.get_mut(bg).ok_or("No tab")?.content.engine.view,
                    )
                    .map_err(|_| "No native suspension state")?
                    {
                        return Err("Sleeping UI disagrees with native state".into());
                    }
                    s.last_background = Instant::now() - Duration::from_secs(31);
                    s.next_background = Instant::now();
                    s.background_tick();
                    if native_background::suspended(
                        &s.tabs.get_mut(bg).ok_or("No tab")?.content.engine.view,
                    )
                    .map_err(|_| "Pause recovery query")?
                    {
                        return Err("Event-loop pause recovery left a view suspended".into());
                    }
                    s.command(Command::SelectTab(bg))
                        .map_err(|_| "Resume failed")?;
                    let tab = s.tabs.active().ok_or("No tab")?;
                    if tab.content.engine.view.webview().as_raw() as usize != self.core
                        || native_background::suspended(&tab.content.engine.view)
                            .map_err(|_| "No native state")?
                    {
                        return Err("Resume replaced the view or left it suspended".into());
                    }
                    self.eval(false, "window.v05Retained")?;
                    self.phase = 3;
                }
            }
            3 => {
                if let Some(value) = value {
                    if value != "\"kept\"" {
                        return Err("Suspension lost document state".into());
                    }
                    s.command(Command::NewTab)
                        .map_err(|_| "Could not open local new tab")?;
                    map_fixture(
                        &s.tabs.active().ok_or("No tab")?.content.engine.view,
                        &self.root,
                    )
                    .map_err(|_| "Could not map fixture")?;
                    s.command(Command::AddFavorite {
                        title: "Example".into(),
                        url: "https://example.com/".into(),
                    })
                    .map_err(|_| "Could not add favorite")?;
                    s.command(Command::SetTheme(rust_browser::settings::Theme::Plum))
                        .map_err(|_| "Could not change theme")?;
                    s.command(Command::PinActive)
                        .map_err(|_| "Pin command failed")?;
                    s.command(Command::KeepAwake)
                        .map_err(|_| "Keep awake command failed")?;
                    if s.command(Command::Navigate("javascript:alert(1)".into()))
                        .is_ok()
                    {
                        return Err("Internal new-tab accepted an active URL".into());
                    }
                    s.render();
                    self.eval(true,"[document.getElementById('newtab-body').hidden,document.getElementById('manager').hidden,document.body.dataset.theme,document.getElementById('favorite-list').textContent.includes('Example')]")?;
                    self.phase = 4;
                }
            }
            4 => {
                if let Some(value) = value {
                    if value != "[false,false,\"plum\",true]" {
                        return Err(format!("Local new-tab UI was not rendered: {value}"));
                    }
                    self.eval(true, "(()=>{editing=false;const a=document.getElementById('address');a.focus();a.value='draft survives background titles';a.dispatchEvent(new Event('input'));return true})()")?;
                    self.phase = 9;
                }
            }
            9 if value.is_some() => {
                s.notice = "Synthetic background update".into();
                s.render();
                self.eval(true, "document.getElementById('address').value")?;
                self.phase = 10;
            }
            10 => {
                if let Some(value) = value {
                    if value != "\"draft survives background titles\"" {
                        return Err("Background render overwrote address draft".into());
                    }
                    s.command(Command::Navigate(FIXTURE_URL.into()))
                        .map_err(|_| "New-tab search navigation failed")?;
                    s.render();
                    self.phase = 5;
                }
            }
            5 => {
                if s.tabs.active().is_some_and(|t| {
                    !t.content.state.loading
                        && t.content.engine.view.url().ok().as_deref() == Some(FIXTURE_URL)
                }) {
                    if s.effective_panel().is_some() {
                        return Err("Local page covered the remote navigation".into());
                    }
                    self.eval(true, "document.getElementById('manager').hidden")?;
                    self.phase = 6;
                }
            }
            6 => {
                if let Some(value) = value {
                    if value != "true" {
                        return Err("Trusted local UI remained over a website".into());
                    }
                    s.command(Command::Home).map_err(|_| "Home failed")?;
                    self.phase = 7;
                }
            }
            7 => {
                if s.tabs.active().is_some_and(|t| {
                    !t.content.state.loading
                        && t.content.engine.view.url().ok().as_deref() == Some("about:blank")
                }) {
                    s.command(Command::Back).map_err(|_| "Native Back failed")?;
                    self.phase = 8;
                }
            }
            8 if s.tabs.active().is_some_and(|t| {
                !t.content.state.loading
                    && t.content.engine.view.url().ok().as_deref() == Some(FIXTURE_URL)
            }) =>
            {
                if s.effective_panel().is_some() {
                    return Err("Back navigation did not leave local UI".into());
                }
                s.command(Command::Home)
                    .map_err(|_| "Could not return to local page")?;
                s.persist();
                self.app.session = None;
                self.phase = 20;
                event_loop.exit();
            }
            _ => {}
        }
        Ok(())
    }
}
impl ApplicationHandler<Event> for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match Session::create_at(
            event_loop,
            self.app.proxy.clone(),
            self.root.clone(),
            self.private,
            None,
        ) {
            Ok(session) => {
                for tab in &session.tabs.items {
                    if map_fixture(&tab.content.engine.view, &self.root).is_err() {
                        self.failure = Some("Could not map test content".into());
                        event_loop.exit();
                        return;
                    }
                }
                session.render();
                self.app.session = Some(session);
            }
            Err(error) => {
                self.failure = Some(format!("Native UX initialization failed: {error}"));
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(event_loop, id, event);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        self.app.user_event(event_loop, event);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.app.about_to_wait(event_loop);
        if let Err(error) = self.poll(event_loop) {
            self.failure = Some(error);
            event_loop.exit();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(25),
        ));
    }
}
fn run(private: bool) {
    let root = std::env::temp_dir().join(format!(
        "rust-browser-ux-{}-{}",
        std::process::id(),
        rust_browser::library::now()
    ));
    std::fs::create_dir(&root).unwrap();
    write_fixture(&root).unwrap();
    let snapshot = if private {
        Snapshot {
            settings: rust_browser::settings::Settings {
                favorites: vec![rust_browser::settings::Favorite {
                    title: "Normal favorite".into(),
                    url: "https://example.com/".into(),
                }],
                ..Settings::default()
            },
            ..Snapshot::default()
        }
    } else {
        Snapshot {
            tabs: vec![
                format!("{FIXTURE_URL}?first"),
                format!("{FIXTURE_URL}?second"),
            ],
            active: 1,
            ..Snapshot::default()
        }
    };
    storage::save(&root.join("session.json"), &snapshot).unwrap();
    let before = std::fs::read(root.join("session.json")).unwrap();
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
        private,
        phase: 0,
        started: Instant::now(),
        result: Arc::new(Mutex::new(None)),
        failure: None,
        ids: Vec::new(),
        core: 0,
    };
    let result = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let phase = probe.phase;
    drop(probe);
    assert!(failure.is_none(), "{failure:?}");
    if private {
        assert_eq!(before, std::fs::read(root.join("session.json")).unwrap());
    } else {
        let restored = storage::load(&root.join("session.json")).unwrap();
        assert_eq!(restored.tabs[0], navigation::HOME);
        assert!(restored.pinned[0] && restored.keep_awake[0]);
        assert_eq!(restored.settings.theme, rust_browser::settings::Theme::Plum);
        assert_eq!(restored.settings.favorites.len(), 1);
    }
    let _ = std::fs::remove_dir_all(root);
    assert!(result.is_ok());
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(phase, 20);
}
#[test]
#[ignore = "Requires Windows/WebView2; run alone in a separate process"]
fn native_tab_sleep_resume_and_local_page_security() {
    run(false);
}
#[test]
#[ignore = "Requires Windows/WebView2; run alone in a separate process"]
fn native_private_newtab_settings_are_ephemeral() {
    run(true);
}
