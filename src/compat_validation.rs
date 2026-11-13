//! Controlled loopback compatibility checks; no credentials, external sites or production IPC.
use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
use windows::core::HSTRING;
use winit::platform::windows::EventLoopBuilderExtWindows;
use wry::WebViewExtWindows;

pub(super) struct Server {
    pub(super) url: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub(super) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                if let Ok((stream, _)) = listener.accept() {
                    // Windows accepted sockets inherit the listener's
                    // non-blocking mode; restore blocking reads so the read
                    // timeout applies instead of an immediate WouldBlock.
                    if stream.set_nonblocking(false).is_err() {
                        continue;
                    }
                    std::thread::spawn(move || {
                        let _ = serve(stream);
                    });
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        });
        Self {
            url,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
fn serve(mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut request = Vec::new();
    let mut byte = [0];
    while request.len() < 16384 {
        if stream.read(&mut byte)? == 0 {
            return Ok(());
        }
        request.push(byte[0]);
        if request.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let header = String::from_utf8_lossy(&request);
    if let Some(length) = header.lines().find_map(|line| {
        line.to_ascii_lowercase()
            .strip_prefix("content-length:")
            .and_then(|v| v.trim().parse::<usize>().ok())
    }) {
        if length > 1024 * 1024 {
            return Err(std::io::Error::other("Fixture body limit"));
        }
        stream.read_exact(&mut vec![0; length])?;
    }
    let path = header
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("/");
    let mut extra = "";
    let mut kind = "text/html";
    let mut status = "200 OK";
    let body=match path.split('?').next().unwrap_or("/") {
        "/modern.html"=>include_bytes!("../validation/modern.html").to_vec(),
        "/data.json"=>{kind="application/json";b"{\"value\":42}".to_vec()},
        "/worker.js"=>{kind="application/javascript";b"onmessage=e=>postMessage(e.data*2)".to_vec()},
        "/sw.js"=>{kind="application/javascript";b"self.addEventListener('install',()=>self.skipWaiting());self.addEventListener('activate',e=>e.waitUntil(self.clients.claim()));self.addEventListener('message',e=>e.ports[0].postMessage(e.data*2+2))".to_vec()},
        "/slow"=>{std::thread::sleep(Duration::from_millis(800));b"<!doctype html><title>Slow old navigation</title>old".to_vec()},
        "/redirect-modern"=>{status="302 Found";extra="Location: /modern.html\r\n";Vec::new()},
        "/auth.html"=>b"<!doctype html><title>Fixture authentication</title><button id='finish' onclick=\"document.cookie='fixture_session=joined; SameSite=Lax; path=/';window.opener.postMessage('fixture-auth-complete',location.origin);window.close()\">Complete synthetic authentication</button>".to_vec(),
        "/upload"=>b"synthetic upload accepted".to_vec(),
        // Two requests the test blocker extension stops, and one it allows.
        "/shield.html"=>b"<!doctype html><title>Shield fixture</title><h1>Shield fixture</h1><script src='/blocked-by-fixture.js'></script><img src='/blocked-by-fixture.png' alt=''><script src='/allowed.js'></script>".to_vec(),
        // 300 small requests, to measure what the shield's counting costs.
        "/many.html"=>{let mut page=b"<!doctype html><title>Many requests</title>".to_vec();for i in 0..300{page.extend(format!("<img src='/allowed.js?{i}' alt=''>").bytes())}page},
        // A page with its own icon, for the new-tab tiles.
        "/tile.html"=>b"<!doctype html><title>Tile fixture</title><link rel='icon' type='image/png' href='/tile-icon.png'><h1>Tile fixture</h1>".to_vec(),
        "/tile-icon.png"=>{kind="image/png";include_bytes!("../tests/fixtures/tile-icon.png").to_vec()},
        "/allowed.js"=>{kind="application/javascript";b"document.documentElement.dataset.allowed='yes'".to_vec()},
        "/download"=>{kind="application/octet-stream";extra="Content-Disposition: attachment; filename=fixture.txt\r\n";vec![b'x';1024*1024]},
        "/status-404"=>{status="404 Not Found";b"<!doctype html><title>Missing fixture</title>The fixture has no such page.".to_vec()},
        "/status-503"=>{status="503 Service Unavailable";b"<!doctype html><title>Unavailable fixture</title>The fixture is unavailable.".to_vec()},
        "/redirect"=>{status="302 Found";extra="Location: /\r\n";Vec::new()},
        "/media.wav"=>{kind="audio/wav";let data=16000u32;let mut bytes=Vec::new();bytes.extend(b"RIFF");bytes.extend((data+36).to_le_bytes());bytes.extend(b"WAVEfmt ");bytes.extend(16u32.to_le_bytes());bytes.extend(1u16.to_le_bytes());bytes.extend(1u16.to_le_bytes());bytes.extend(8000u32.to_le_bytes());bytes.extend(16000u32.to_le_bytes());bytes.extend(2u16.to_le_bytes());bytes.extend(16u16.to_le_bytes());bytes.extend(b"data");bytes.extend(data.to_le_bytes());for i in 0..8000 {let sample=((i as f64*440.0*std::f64::consts::TAU/8000.0).sin()*50.0)as i16;bytes.extend(sample.to_le_bytes());}bytes},
        _=>b"<!doctype html><title>Compatibility fixture</title><h1>Controlled compatibility fixture</h1><button id='popup' onclick=\"window.popup=window.open('/auth.html','fixture-auth')\">Open synthetic login</button><input type='file' id='file'><div id='full'>Fullscreen fixture</div><script>window.marker='retained';if(location.search==='?automatic=1')setTimeout(()=>window.open('/auth.html','blocked'),200);addEventListener('message',e=>{if(e.origin===location.origin)window.received=e.data})</script>".to_vec(),
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
        body.len()
    )?;
    if path == "/download" {
        for chunk in body.chunks(65536) {
            stream.write_all(chunk)?;
            std::thread::sleep(Duration::from_millis(75));
        }
        Ok(())
    } else {
        stream.write_all(&body)
    }
}
struct Probe {
    app: App,
    root: PathBuf,
    url: String,
    phase: u8,
    started: Instant,
    step: Instant,
    result: Arc<Mutex<Option<String>>>,
    failure: Option<String>,
    opener: TabId,
    crash: bool,
    quiet: Option<TabId>,
}
impl Probe {
    fn eval(&self, script: &str) -> Result<(), String> {
        let slot = self.result.clone();
        self.app
            .session
            .as_ref()
            .ok_or("No session")?
            .tabs
            .active()
            .ok_or("No tab")?
            .content
            .engine
            .view
            .evaluate_script_with_callback(script, move |value| *slot.lock().unwrap() = Some(value))
            .map_err(|_| "Script failed".into())
    }
    fn gesture(&self, script: &str) -> Result<(), String> {
        let s = self.app.session.as_ref().ok_or("No session")?;
        let view = &s.tabs.active().ok_or("No tab")?.content.engine.view;
        // SAFETY: test-only CDP user gesture; the production browser exposes no CDP/IPC to websites.
        unsafe {
            view.webview().CallDevToolsProtocolMethod(&HSTRING::from("Runtime.evaluate"),&HSTRING::from(serde_json::json!({"expression":script,"userGesture":true,"awaitPromise":true}).to_string()),&CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|result,_|result))).map_err(|_|"Synthetic user gesture failed".into())
        }
    }
    fn crash_renderer(&self) -> Result<(), String> {
        let view = &self
            .app
            .session
            .as_ref()
            .ok_or("No session")?
            .tabs
            .active()
            .ok_or("No tab")?
            .content
            .engine
            .view;
        // SAFETY: controlled test renderer only; this method is never compiled into shipping code.
        unsafe {
            view.webview()
                .CallDevToolsProtocolMethod(
                    &HSTRING::from("Page.crash"),
                    &HSTRING::from("{}"),
                    &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(()))),
                )
                .map_err(|_| "Could not request fixture renderer crash".into())
        }
    }
    fn advance(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        if self.phase == 100 {
            return Ok(());
        }
        if self.started.elapsed() > Duration::from_secs(60) {
            return Err(format!("Compatibility timeout at phase {}", self.phase));
        }
        let value = self.result.lock().unwrap().take();
        let s = self.app.session.as_mut().ok_or("No session")?;
        if !s.ready || s.tabs.active().is_none_or(|t| t.content.state.loading) {
            return Ok(());
        }
        if s.notice.starts_with("Error:") {
            return Err(s.notice.clone());
        }
        let previous = self.phase;
        match self.phase {
            0 if s
                .tabs
                .active()
                .unwrap()
                .content
                .engine
                .view
                .url()
                .ok()
                .is_some_and(|url| url.split('?').next() == Some(self.url.as_str()))
                && s.tabs.active().unwrap().title == "Compatibility fixture" =>
            {
                self.opener = s.tabs.active().unwrap().id;
                if self.crash {
                    self.crash_renderer()?;
                    self.phase = 20;
                    return Ok(());
                }
                self.step = Instant::now();
                self.phase = 1;
            }
            1 if s.notice.contains("Popup blocked")
                || self.step.elapsed() > Duration::from_secs(3) =>
            {
                if s.tabs.items.len() != 1 {
                    return Err("Automatic popup escaped blocking".into());
                }
                self.gesture("document.getElementById('popup').click()")?;
                self.phase = 2;
            }
            2 if s.tabs.items.len() == 2
                && s.tabs.active().unwrap().content.opener == Some(self.opener) =>
            {
                self.eval("[!!window.opener,typeof window.ipc,document.title]")?;
                self.phase = 3;
            }
            3 => {
                if let Some(v) = value {
                    if v != "[true,\"undefined\",\"Fixture authentication\"]" {
                        return Err(format!("Opener/security boundary failed: {v}"));
                    }
                    self.gesture("document.getElementById('finish').click()")?;
                    self.phase = 4;
                }
            }
            4 if s.tabs.items.len() == 1 => {
                if s.tabs.active().unwrap().id != self.opener {
                    return Err("Popup close did not restore opener".into());
                }
                self.eval("[window.marker,window.received,document.cookie.includes('fixture_session=joined')]")?;
                self.phase = 5;
            }
            5 => {
                if let Some(v) = value {
                    if v != "[\"retained\",\"fixture-auth-complete\",true]" {
                        return Err(format!("Authentication cookie/message/state failed: {v}"));
                    }
                    self.gesture("let link=document.createElement('a');link.href='/auth.html';link.target='_blank';link.rel='noopener';document.body.append(link);link.click()")?;
                    self.phase = 14;
                }
            }
            14 if s.tabs.items.len() == 2 => {
                self.eval("[window.opener===null,typeof window.ipc]")?;
                self.phase = 15;
            }
            15 => {
                if let Some(v) = value {
                    if v != "[true,\"undefined\"]" {
                        return Err(format!("Noopener boundary failed: {v}"));
                    }
                    s.command(Command::CloseActive)
                        .map_err(|_| "Close noopener tab")?;
                    self.eval("fetch('/upload',{method:'POST',body:'synthetic fixture only'}).then(()=>window.uploadDone=true);true")?;
                    self.phase = 6;
                }
            }
            6 if s
                .tabs
                .active()
                .unwrap()
                .content
                .engine
                .activity
                .protected
                .get() =>
            {
                s.open_tab(self.url.clone())
                    .map_err(|_| "Open second tab")?;
                s.activate().map_err(|_| "Activate second tab")?;
                s.started = Instant::now() - Duration::from_secs(200);
                let tab = s.tabs.get_mut(self.opener).unwrap();
                tab.content.lifecycle = Lifecycle::default();
                tab.content.lifecycle.visible(false, Duration::ZERO);
                s.next_background = Instant::now();
                s.background_tick();
                if s.tabs.get_mut(self.opener).unwrap().content.lifecycle.phase != Phase::Awake {
                    return Err("Upload view was suspended".into());
                }
                self.phase = 7;
            }
            7 => {
                self.gesture("window.sound=new Audio('/media.wav');sound.loop=true;sound.play()")?;
                self.step = Instant::now();
                self.phase = 8;
            }
            8 if s
                .tabs
                .active()
                .unwrap()
                .content
                .engine
                .activity
                .protected
                .get() =>
            {
                if !native_background::playing_audio(&s.tabs.active().unwrap().content.engine.view)
                    .map_err(|_| "Audio query")?
                    && self.step.elapsed() < Duration::from_secs(5)
                {
                    return Ok(());
                }
                self.eval("[sound.paused,sound.error===null]")?;
                self.phase = 9;
            }
            9 => {
                if let Some(v) = value {
                    if v != "[false,true]" {
                        return Err(format!("Native audio did not play: {v}"));
                    }
                    self.gesture(
                        "sound.pause();document.getElementById('full').requestFullscreen()",
                    )?;
                    self.phase = 10;
                }
            }
            10 if s.fullscreen.is_some() => {
                if s.window.fullscreen().is_none() {
                    return Err("Host fullscreen did not follow content".into());
                }
                self.eval("document.exitFullscreen();true")?;
                self.phase = 11;
            }
            11 if s.fullscreen.is_none() => {
                s.command(Command::Navigate(format!("{}redirect", self.url)))
                    .map_err(|_| "Redirect navigation")?;
                self.phase = 12;
            }
            12 if s.tabs.active().unwrap().url == self.url => {
                self.eval("typeof window.ipc")?;
                self.phase = 13;
            }
            13 => {
                if let Some(v) = value {
                    if v != "\"undefined\"" {
                        return Err("Redirect received IPC".into());
                    }
                    let source = s.tabs.active().unwrap().id;
                    s.open_tab(self.url.clone())
                        .map_err(|_| "Could not open quiet download guard tab")?;
                    self.quiet = Some(s.tabs.active().unwrap().id);
                    s.command(Command::SelectTab(source))
                        .map_err(|_| "Could not select download source")?;
                    s.command(Command::Navigate(format!("{}download", self.url)))
                        .map_err(|_| "Could not start fixture download")?;
                    self.phase = 16;
                }
            }
            16 if s.downloads.borrow().active_count() > 0
                && self.quiet.is_some_and(|id| {
                    s.tabs.items.iter().any(|t| {
                        t.id == id
                            && !t.content.state.loading
                            && t.content.engine.activity.observable.get()
                            && !t.content.engine.activity.protected.get()
                    })
                }) =>
            {
                let id = self.quiet.ok_or("No quiet tab")?;
                let protection = s.suspension_protection(id, false);
                if !protection.downloads
                    || protection.active
                    || protection.capture
                    || protection.loading
                    || protection.busy
                {
                    return Err(
                        "Quiet-tab download protection was confounded by other activity".into(),
                    );
                }
                s.started = Instant::now() - Duration::from_secs(200);
                let tab = s.tabs.get_mut(id).ok_or("Quiet tab closed")?;
                tab.content.lifecycle = Lifecycle::default();
                tab.content.lifecycle.visible(false, Duration::ZERO);
                s.next_background = Instant::now();
                s.background_tick();
                if s.tabs
                    .get_mut(id)
                    .ok_or("No quiet tab")?
                    .content
                    .lifecycle
                    .phase
                    != Phase::Awake
                {
                    return Err("Active download did not protect other tabs".into());
                }
                self.phase = 17;
            }
            17 if s
                .downloads
                .borrow()
                .records()
                .iter()
                .any(|r| r.status == "Completed") =>
            {
                let path = self.root.join("downloads/fixture.txt");
                if std::fs::metadata(&path)
                    .map_err(|_| "Completed fixture missing")?
                    .len()
                    != 1024 * 1024
                {
                    return Err("Fixture download was truncated".into());
                }
                s.command(Command::Navigate(format!("{}download", self.url)))
                    .map_err(|_| "Second fixture download failed")?;
                self.phase = 18;
            }
            18 if s
                .downloads
                .borrow()
                .records()
                .iter()
                .filter(|r| r.status == "Completed")
                .count()
                == 2 =>
            {
                let path = self.root.join("downloads/fixture (1).txt");
                if !path.is_file() {
                    return Err("Download collision did not preserve existing file".into());
                }
                s.persist();
                self.app.session = None;
                self.phase = 100;
                el.exit();
            }
            20 if s.tabs.active().unwrap().content.state.failed => {
                if !s
                    .tabs
                    .active()
                    .unwrap()
                    .content
                    .state
                    .message
                    .contains("process failed")
                {
                    return Err("Renderer crash had no actionable failure message".into());
                }
                s.command(Command::Reload)
                    .map_err(|_| "Renderer reload failed")?;
                self.phase = 21;
            }
            21 if !s.tabs.active().unwrap().content.state.failed
                && s.tabs.active().unwrap().title == "Compatibility fixture" =>
            {
                self.eval("[document.readyState,typeof window.ipc]")?;
                self.phase = 22;
            }
            22 => {
                if let Some(v) = value {
                    if v != "[\"complete\",\"undefined\"]" {
                        return Err(format!("Renderer reload did not recover safely: {v}"));
                    }
                    self.app.session = None;
                    self.phase = 100;
                    el.exit();
                }
            }
            _ => {}
        }
        if previous != self.phase {
            println!("Compatibility phase {}", self.phase);
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
            Err(e) => {
                self.failure = Some(e.to_string());
                el.exit();
            }
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(el, id, event)
    }
    fn user_event(&mut self, el: &ActiveEventLoop, event: Event) {
        if let Event::DownloadRequested(id, _) = &event {
            let folder = self.root.join("downloads");
            std::fs::create_dir_all(&folder).unwrap();
            if let Some(s) = &mut self.app.session {
                s.downloads.borrow_mut().destination(*id, Ok(Some(folder)));
                s.render();
            }
            return;
        }
        self.app.user_event(el, event)
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.app.about_to_wait(el);
        if let Err(e) = self.advance(el) {
            self.failure = Some(e);
            el.exit();
        }
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(25),
        ));
    }
}
#[test]
#[ignore = "Requires Windows/WebView2; run alone with the controlled loopback fixture"]
fn native_popup_opener_upload_audio_fullscreen_and_redirects() {
    run(false);
}
#[test]
#[ignore = "Requires Windows/WebView2; deliberately crashes only the disposable fixture renderer"]
fn native_renderer_crash_is_reported_and_reload_recovers() {
    run(true);
}
fn run(crash: bool) {
    let server = Server::start();
    let root = std::env::temp_dir().join(format!(
        "rust-browser-compat-{}-{}",
        std::process::id(),
        rust_browser::library::now()
    ));
    std::fs::create_dir(&root).unwrap();
    storage::save(
        &root.join("session.json"),
        &Snapshot {
            tabs: vec![format!("{}?automatic=1", server.url)],
            ..Snapshot::default()
        },
    )
    .unwrap();
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
        phase: 0,
        started: Instant::now(),
        step: Instant::now(),
        result: Arc::new(Mutex::new(None)),
        failure: None,
        opener: 0,
        crash,
        quiet: None,
    };
    let result = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let phase = probe.phase;
    drop(probe);
    let _ = std::fs::remove_dir_all(root);
    assert!(result.is_ok());
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(phase, 100);
}

/// Regression: on Windows an accepted socket inherits the listener's
/// non-blocking mode, so a request that arrives after `accept` used to fail
/// the first read with `WouldBlock` and drop the connection without a
/// response (WebView2 then reported navigation status 0 with no HTTP code).
#[test]
fn loopback_server_serves_requests_sent_after_accept() {
    let server = Server::start();
    let address = server
        .url
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned();
    let mut client = TcpStream::connect(&address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    // Give the accept loop (5 ms poll) time to accept before any bytes arrive.
    std::thread::sleep(Duration::from_millis(150));
    client
        .write_all(b"GET /data.json HTTP/1.1\r\nHost: fixture\r\n\r\n")
        .unwrap();
    let mut response = Vec::new();
    let _ = client.read_to_end(&mut response);
    let text = String::from_utf8_lossy(&response);
    assert!(text.starts_with("HTTP/1.1 200 OK"), "{text:?}");
    assert!(text.ends_with("{\"value\":42}"), "{text:?}");
}
