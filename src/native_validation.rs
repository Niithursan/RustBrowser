//! Opt-in integration check against the actual installed Windows WebView2 runtime.
use crate::{
    app::{Event, PageEvent},
    engine::Engine,
    native_downloads, native_security,
};
use rust_browser::privacy::{ClearRequest, PrivateLease};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY, ICoreWebView2_3,
};
use windows::core::{HSTRING, Interface};
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};
use wry::{WebContext, WebView, WebViewExtWindows};

const FIXTURE_HOST: &str = "rust-browser-validation.test";
pub(crate) const FIXTURE_URL: &str = "https://rust-browser-validation.test/index.html";

pub(crate) fn write_fixture(root: &std::path::Path) -> std::io::Result<()> {
    let folder = root.join("fixture");
    std::fs::create_dir(&folder)?;
    std::fs::write(
        folder.join("index.html"),
        b"<!doctype html><title>Native validation fixture</title><p>Native validation fixture</p>",
    )
}

pub(crate) fn map_fixture(view: &WebView, root: &std::path::Path) -> windows::core::Result<()> {
    // Test-only: local static content at a reserved origin avoids CI networking races.
    // SAFETY: the view and its COM interface are used only on the owning UI thread.
    unsafe {
        let core: ICoreWebView2_3 = view.webview().cast()?;
        core.SetVirtualHostNameToFolderMapping(
            &HSTRING::from(FIXTURE_HOST),
            &HSTRING::from(root.join("fixture").as_os_str()),
            COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY,
        )
    }
}

struct Harness {
    normal: Option<Engine>,
    private: Option<Engine>,
    contexts: Vec<WebContext>,
    window: Option<Window>,
    permissions: native_security::Shared,
    proxy: EventLoopProxy<Event>,
    normal_loaded: bool,
    private_loaded: bool,
    phase: usize,
    waiting: bool,
    cleared: bool,
    output: Arc<Mutex<Option<String>>>,
    failure: Option<String>,
    started: Instant,
    last_navigation_issue: Option<String>,
    root: std::path::PathBuf,
    lease: Option<PrivateLease>,
}
impl Harness {
    fn eval(&mut self, private: bool, script: &str) -> Result<(), String> {
        let output = self.output.clone();
        self.waiting = true;
        let engine = if private {
            self.private.as_ref()
        } else {
            self.normal.as_ref()
        }
        .ok_or("No view")?;
        engine
            .view
            .evaluate_script_with_callback(script, move |value| {
                if let Ok(mut slot) = output.lock() {
                    *slot = Some(value);
                }
            })
            .map_err(|_| "Script evaluation failed".into())
    }
    fn values(value: &str, expected: &[&str]) -> Result<(), String> {
        let actual: Vec<String> =
            serde_json::from_str(value).map_err(|_| "Invalid script result")?;
        if actual != expected {
            return Err("Native cookie/storage isolation or deletion assertion failed".into());
        }
        Ok(())
    }
    fn advance(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.started.elapsed() > Duration::from_secs(90) {
            return Err(format!(
                "Native validation timed out at phase {}; last navigation issue: {:?}",
                self.phase, self.last_navigation_issue
            ));
        }
        if !(self.normal_loaded && self.private_loaded) {
            return Ok(());
        }
        let value = self.output.lock().map_err(|_| "Result lock failed")?.take();
        if self.waiting && value.is_none() {
            return Ok(());
        }
        let value = value.unwrap_or_default();
        self.waiting = false;
        const READ: &str = "[document.cookie,localStorage.getItem('v04')||'']";
        match self.phase {
            0 => {
                self.eval(false,"(()=>{document.cookie='v04=normal; path=/; SameSite=Lax';localStorage.setItem('v04','normal');return [document.cookie,localStorage.getItem('v04')]})()")?;
                self.phase = 1;
            }
            1 => {
                Self::values(&value, &["v04=normal", "normal"])?;
                self.eval(true,"(()=>{const before=[document.cookie,localStorage.getItem('v04')||''];document.cookie='v04=private; path=/; SameSite=Lax';localStorage.setItem('v04','private');return before})()")?;
                self.phase = 2;
            }
            2 => {
                Self::values(&value, &["", ""])?;
                self.eval(false, READ)?;
                self.phase = 3;
            }
            3 => {
                Self::values(&value, &["v04=normal", "normal"])?;
                native_security::clear(
                    &self.normal.as_ref().ok_or("No view")?.view,
                    &ClearRequest {
                        cookies: true,
                        cache: true,
                        permissions: true,
                        confirmed: true,
                        ..ClearRequest::default()
                    },
                    false,
                    77,
                    self.proxy.clone(),
                )
                .map_err(|_| "Native clearing could not start")?;
                self.phase = 4;
            }
            4 => {
                if self.cleared {
                    self.eval(false, READ)?;
                    self.phase = 5;
                }
            }
            5 => {
                Self::values(&value, &["", ""])?;
                self.eval(true, READ)?;
                self.phase = 6;
            }
            6 => {
                Self::values(&value, &["v04=private", "private"])?;
                self.eval(false,"Notification.requestPermission().then(value=>window.v04Permission=value);'requested'")?;
                self.phase = 7;
            }
            7 => {
                self.eval(false, "window.v04Permission||'pending'")?;
                self.phase = 8;
            }
            8 => {
                let result: String =
                    serde_json::from_str(&value).map_err(|_| "Invalid permission response")?;
                if result == "pending" {
                    self.phase = 7;
                } else if result == "denied" {
                    self.normal_loaded = false;
                    self.normal
                        .as_ref()
                        .ok_or("No view")?
                        .view
                        .load_url(FIXTURE_URL)
                        .map_err(|_| "Could not reload permission fixture")?;
                    self.phase = 9;
                } else {
                    return Err("An unapproved native permission request was not denied".into());
                }
            }
            9 => {
                self.eval(false,"Notification.requestPermission().then(value=>window.v04Permission=value);'requested'")?;
                self.phase = 10;
            }
            10 => {
                self.eval(false, "window.v04Permission||'pending'")?;
                self.phase = 11;
            }
            11 => {
                let result: String =
                    serde_json::from_str(&value).map_err(|_| "Invalid permission result")?;
                if result == "pending" {
                    self.phase = 10;
                } else if result == "granted" {
                    self.phase = 12;
                    event_loop.exit();
                } else {
                    return Err(
                        "Explicit native permission allow did not reach the test page".into(),
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
}
impl ApplicationHandler<Event> for Harness {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let window = event_loop.create_window(
                Window::default_attributes().with_title("Rust Browser native validation"),
            )?;
            let lease = PrivateLease::create(&self.root)?;
            let mut normal_context = WebContext::new(Some(self.root.join("normal")));
            let mut private_context = WebContext::new(Some(lease.path.join("content")));
            let normal = Engine::new(
                &window,
                &mut normal_context,
                self.proxy.clone(),
                1,
                Rc::new(RefCell::new(native_downloads::Manager::default())),
                self.permissions.clone(),
                crate::engine::Options {
                    private: false,
                    environment: None,
                },
            )?;
            let private = Engine::new(
                &window,
                &mut private_context,
                self.proxy.clone(),
                2,
                Rc::new(RefCell::new(native_downloads::Manager::default())),
                Rc::new(RefCell::new(native_security::Permissions::default())),
                crate::engine::Options {
                    private: true,
                    environment: None,
                },
            )?;
            self.permissions.borrow_mut().active = Some(1);
            self.permissions.borrow_mut().enabled = true;
            map_fixture(&normal.view, &self.root)?;
            map_fixture(&private.view, &self.root)?;
            normal.view.load_url(FIXTURE_URL)?;
            private.view.load_url(FIXTURE_URL)?;
            self.normal = Some(normal);
            self.private = Some(private);
            self.contexts = vec![normal_context, private_context];
            self.window = Some(window);
            self.lease = Some(lease);
            Ok(())
        })();
        if let Err(error) = result {
            self.failure = Some(format!(
                "Could not initialize native validation views: {error}"
            ));
            event_loop.exit();
        }
    }
    fn window_event(
        &mut self,
        _: &ActiveEventLoop,
        _: winit::window::WindowId,
        _: winit::event::WindowEvent,
    ) {
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        match event {
            Event::Page(
                id,
                PageEvent::Completed {
                    error: None, url, ..
                },
            ) if url == FIXTURE_URL => {
                if id == 1 {
                    self.normal_loaded = true;
                } else if id == 2 {
                    self.private_loaded = true;
                }
            }
            Event::Page(
                id,
                PageEvent::Completed {
                    error: Some(error), ..
                },
            ) => {
                // Startup or superseded navigation can be cancelled before the fixture commits.
                self.last_navigation_issue = Some(format!("Tab {id}: {error}"));
            }
            Event::EngineCleared {
                ticket: 77,
                success,
                ..
            } => {
                if success {
                    self.cleared = true;
                } else {
                    self.failure = Some("Engine did not confirm deletion".into());
                    event_loop.exit();
                }
            }
            Event::PermissionPrompt => {
                let prompt = self.permissions.borrow().prompt();
                if let Some(prompt) = prompt {
                    let expected = rust_browser::security::origin(FIXTURE_URL);
                    if Some(prompt.origin) != expected
                        || prompt.capability != rust_browser::security::Capability::Notifications
                    {
                        self.failure = Some("Incorrect permission origin or capability".into());
                        event_loop.exit();
                        return;
                    }
                    if matches!(self.phase, 7 | 8) {
                        self.permissions.borrow_mut().cancel_tab(1);
                        if self.permissions.borrow().protected(1) {
                            self.failure =
                                Some("Cancelled permission retained sleep protection".into());
                            event_loop.exit();
                        }
                    } else if matches!(self.phase, 10 | 11) {
                        if self
                            .permissions
                            .borrow_mut()
                            .resolve(prompt.id, rust_browser::security::Decision::Allow)
                            .is_err()
                        {
                            self.failure = Some("Explicit native decision failed".into());
                            event_loop.exit();
                        }
                        if !self.permissions.borrow().protected(1) {
                            self.failure = Some("Granted document lost sleep protection".into());
                            event_loop.exit();
                        }
                    } else {
                        self.failure = Some("Unexpected native permission prompt".into());
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.advance(event_loop) {
            self.failure = Some(error);
            event_loop.exit();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(25),
        ));
    }
}

#[test]
#[ignore = "Requires an interactive Windows desktop and installed WebView2; run separately with --ignored --test-threads=1"]
fn native_private_storage_and_confirmed_engine_deletion() {
    let root = std::env::temp_dir().join(format!(
        "rust-browser-native-{}-{}",
        std::process::id(),
        rust_browser::library::now()
    ));
    std::fs::create_dir(&root).unwrap();
    write_fixture(&root).unwrap();
    let event_loop = EventLoop::<Event>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut app = Harness {
        normal: None,
        private: None,
        contexts: Vec::new(),
        window: None,
        permissions: Rc::new(RefCell::new(native_security::Permissions::default())),
        proxy: event_loop.create_proxy(),
        normal_loaded: false,
        private_loaded: false,
        phase: 0,
        waiting: false,
        cleared: false,
        output: Arc::new(Mutex::new(None)),
        failure: None,
        started: Instant::now(),
        last_navigation_issue: None,
        root: root.clone(),
        lease: None,
    };
    let run = event_loop.run_app(&mut app);
    let failure = app.failure.clone();
    let phase = app.phase;
    drop(app);
    let _ = std::fs::remove_dir_all(root);
    assert!(run.is_ok());
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(phase, 12);
}
