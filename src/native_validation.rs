//! Opt-in integration check against the actual installed Windows WebView2 runtime.
use crate::{
    app::{Event, PageEvent},
    engine::Engine,
    native_downloads, native_security,
};
use rust_browser::privacy::{ClearRequest, PrivateLease};
use std::{
    cell::RefCell,
    io::{Read, Write},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};
use wry::WebContext;

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
    url: String,
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
            return Err("Native validation timed out".into());
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
                        .load_url(&self.url)
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
                false,
            )?;
            let private = Engine::new(
                &window,
                &mut private_context,
                self.proxy.clone(),
                2,
                Rc::new(RefCell::new(native_downloads::Manager::default())),
                Rc::new(RefCell::new(native_security::Permissions::default())),
                true,
            )?;
            self.permissions.borrow_mut().active = Some(1);
            self.permissions.borrow_mut().enabled = true;
            normal.view.load_url(&self.url)?;
            private.view.load_url(&self.url)?;
            self.normal = Some(normal);
            self.private = Some(private);
            self.contexts = vec![normal_context, private_context];
            self.window = Some(window);
            self.lease = Some(lease);
            Ok(())
        })();
        if result.is_err() {
            self.failure = Some("Could not initialize native validation views".into());
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
            Event::Page(id, PageEvent::Completed { error, .. }) => {
                if error.is_some() {
                    self.failure = Some("Fixture navigation failed".into());
                    event_loop.exit();
                } else if id == 1 {
                    self.normal_loaded = true;
                } else if id == 2 {
                    self.private_loaded = true;
                }
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
                    let expected = rust_browser::security::origin(&self.url);
                    if Some(prompt.origin) != expected
                        || prompt.capability != rust_browser::security::Capability::Notifications
                    {
                        self.failure = Some("Incorrect permission origin or capability".into());
                        event_loop.exit();
                        return;
                    }
                    if matches!(self.phase, 7 | 8) {
                        self.permissions.borrow_mut().cancel_tab(1);
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
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let server = std::thread::spawn(move || {
        while !stopping.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let _ = stream.read(&mut [0; 4096]);
                    let body=b"<!doctype html><title>Native isolation fixture</title><p>Native isolation fixture</p>";
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(body);
                }
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    });
    let root = std::env::temp_dir().join(format!(
        "rust-browser-native-{}-{}",
        std::process::id(),
        rust_browser::library::now()
    ));
    std::fs::create_dir(&root).unwrap();
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
        url,
        root: root.clone(),
        lease: None,
    };
    let run = event_loop.run_app(&mut app);
    let failure = app.failure.clone();
    let phase = app.phase;
    drop(app);
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    let _ = std::fs::remove_dir_all(root);
    assert!(run.is_ok());
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(phase, 12);
}
