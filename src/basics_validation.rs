//! Content-only everyday tools. No global input, printers, saved PDFs or real profile.
//! Native API/IPC checks complement the separately recorded real-keyboard GUI checks.
use super::*;
use std::sync::{Arc, Mutex};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::core::{BOOL, Interface};
use winit::platform::windows::EventLoopBuilderExtWindows;
use wry::WebViewExtWindows;

/// Deliver a wheel message only to the render host below this fixture's content
/// controller. No global input, cursor movement or foreign window can be targeted.
fn ctrl_wheel(view: &WebView) -> Result<(), String> {
    use windows::Win32::{
        Foundation::{HWND, LPARAM, RECT, WPARAM},
        UI::WindowsAndMessaging::{
            EnumChildWindows, GetClassNameW, GetWindowRect, SendMessageW, WM_MOUSEWHEEL,
        },
    };
    unsafe extern "system" fn find(hwnd: HWND, state: LPARAM) -> BOOL {
        let mut name = [0u16; 128];
        // SAFETY: synchronous enumeration with a live, stack-owned output slot.
        unsafe {
            let len = GetClassNameW(hwnd, &mut name) as usize;
            if String::from_utf16_lossy(&name[..len]) == "Chrome_RenderWidgetHostHWND" {
                *(state.0 as *mut Option<HWND>) = Some(hwnd);
                return BOOL(0);
            }
        }
        BOOL(1)
    }
    // SAFETY: controller, descendants and synchronous message belong only to this
    // disposable fixture. Enumeration ends before the stack slot is dropped.
    unsafe {
        let mut parent = HWND::default();
        view.controller()
            .ParentWindow(&mut parent)
            .map_err(|e| e.to_string())?;
        let mut child: Option<HWND> = None;
        let _ = EnumChildWindows(
            Some(parent),
            Some(find),
            LPARAM(&mut child as *mut _ as isize),
        );
        let hwnd = child.ok_or("Owned content render host not found")?;
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
        let position = ((rect.left + 20) as u16 as u32) | (((rect.top + 20) as u16 as u32) << 16);
        let _ = SendMessageW(
            hwnd,
            WM_MOUSEWHEEL,
            Some(WPARAM((120 << 16) | 0x0008)),
            Some(LPARAM(position as isize)),
        );
    }
    Ok(())
}

struct Probe {
    app: App,
    root: PathBuf,
    url: String,
    private: bool,
    private_profile: Option<PathBuf>,
    phase: u8,
    started: Instant,
    result: Arc<Mutex<Option<String>>>,
    failure: Option<String>,
    first: Option<TabId>,
    find_ready: bool,
    zoom_event: bool,
    last_phase: u8,
    step: Instant,
}
impl Probe {
    fn eval(&self, toolbar: bool, script: &str) -> Result<(), String> {
        let s = self.app.session.as_ref().ok_or("No session")?;
        let view = if toolbar {
            &s.toolbar
        } else {
            &s.tabs.active().ok_or("No tab")?.content.engine.view
        };
        let result = self.result.clone();
        view.evaluate_script_with_callback(script, move |value| {
            *result.lock().unwrap() = Some(value);
        })
        .map_err(|e| e.to_string())
    }
    fn poll(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        if self.phase == 100 {
            return Ok(());
        }
        if self.started.elapsed() > Duration::from_secs(90) {
            return Err(format!("Page tools timed out at {}", self.phase));
        }
        if self.last_phase != self.phase {
            self.last_phase = self.phase;
            self.step = Instant::now();
        } else if self.step.elapsed() > Duration::from_secs(15) {
            return Err(format!("Page tool step {} did not complete", self.phase));
        }
        if self.phase == 30 {
            if self.app.session.is_none() {
                self.phase = 100;
            }
            return Ok(());
        }
        let value = self.result.lock().unwrap().take();
        let s = self.app.session.as_mut().ok_or("No session")?;
        if !s.ready {
            return Ok(());
        }
        let page = &s.tabs.active().ok_or("No content tab")?.content;
        match self.phase {
            0 => {
                // Check native settings rather than assuming Wry defaults.
                unsafe {
                    let content = page
                        .engine
                        .view
                        .webview()
                        .Settings()
                        .map_err(|e| e.to_string())?;
                    let toolbar = s.toolbar.webview().Settings().map_err(|e| e.to_string())?;
                    let mut enabled = BOOL::default();
                    content
                        .IsZoomControlEnabled(&mut enabled)
                        .map_err(|e| e.to_string())?;
                    if !enabled.as_bool() {
                        return Err("Content zoom controls are disabled".into());
                    }
                    content
                        .cast::<ICoreWebView2Settings3>()
                        .map_err(|e| e.to_string())?
                        .AreBrowserAcceleratorKeysEnabled(&mut enabled)
                        .map_err(|e| e.to_string())?;
                    if !enabled.as_bool() {
                        return Err("Native content accelerators are disabled".into());
                    }
                    toolbar
                        .IsZoomControlEnabled(&mut enabled)
                        .map_err(|e| e.to_string())?;
                    if enabled.as_bool() {
                        return Err("Trusted toolbar allows native zoom".into());
                    }
                    toolbar
                        .cast::<ICoreWebView2Settings3>()
                        .map_err(|e| e.to_string())?
                        .AreBrowserAcceleratorKeysEnabled(&mut enabled)
                        .map_err(|e| e.to_string())?;
                    if enabled.as_bool() {
                        return Err("Trusted toolbar allows native find/print".into());
                    }
                }
                for command in [
                    Command::FindInPage,
                    Command::PrintPage,
                    Command::ZoomIn,
                    Command::ZoomOut,
                    Command::ResetZoom,
                ] {
                    if s.command(command).is_ok() {
                        return Err("Local browser page accepted a content tool".into());
                    }
                }
                s.command(Command::Navigate(self.url.clone()))
                    .map_err(|e| e.to_string())?;
                self.phase = 1;
            }
            1 if !page.state.loading && s.tabs.active().is_some_and(|t| t.url == self.url) => {
                self.first = s.tabs.active().map(|t| t.id);
                self.eval(false,"document.body.innerHTML='<h1>needle needle needle</h1>';window.prints=0;addEventListener('beforeprint',()=>window.prints++);typeof window.ipc")?;
                self.phase = 2;
            }
            2 if value.is_some() => {
                if value.as_deref() != Some("\"undefined\"") {
                    return Err("Page has privileged IPC".into());
                }
                self.eval(true,"document.getElementById('more').click();document.getElementById('zoom-in').click();true")?;
                self.phase = 3;
            }
            3 if (page.engine.zoom_factor().map_err(|e| e.to_string())? - 1.1).abs() < 0.001 => {
                // API zoom need not raise ZoomFactorChanged. The command's normal
                // render still must show the actual controller value.
                self.eval(true,"!document.getElementById('zoom-row').hidden && document.getElementById('zoom-level').textContent.includes('110%')")?;
                self.phase = 4;
            }
            4 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err(format!("Zoom badge is incorrect: {value:?}"));
                }
                s.command(Command::NewTab).map_err(|e| e.to_string())?;
                if (s
                    .tabs
                    .active()
                    .unwrap()
                    .content
                    .engine
                    .zoom_factor()
                    .map_err(|e| e.to_string())?
                    - 1.0)
                    .abs()
                    > 0.001
                {
                    return Err("New tab inherited zoom".into());
                }
                s.command(Command::Navigate(format!("{}?second", self.url)))
                    .map_err(|e| e.to_string())?;
                self.phase = 5;
            }
            5 if !page.state.loading
                && s.tabs.active().is_some_and(|t| t.url.ends_with("?second")) =>
            {
                if (page.engine.zoom_factor().map_err(|e| e.to_string())? - 1.0).abs() > 0.001 {
                    return Err("Same-origin tab inherited zoom".into());
                }
                ctrl_wheel(&page.engine.view)?;
                self.phase = 40;
            }
            40 if self.zoom_event
                && page.engine.zoom_factor().map_err(|e| e.to_string())? > 1.0 =>
            {
                let percent =
                    (page.engine.zoom_factor().map_err(|e| e.to_string())? * 100.0).round() as u32;
                self.eval(true,&format!("!document.getElementById('zoom-row').hidden && document.getElementById('zoom-level').textContent.includes('{percent}%')"))?;
                self.phase = 41;
            }
            41 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    return Err("Native wheel event did not refresh zoom badge".into());
                }
                // Programmatic zoom is synchronous and does not emit this event.
                page.engine.view.zoom(1.37).map_err(|e| e.to_string())?;
                s.render();
                self.phase = 6;
            }
            6 => {
                self.eval(true,"!document.getElementById('zoom-row').hidden && document.getElementById('zoom-level').textContent.includes('137%')")?;
                self.phase = 7;
            }
            7 if value.is_some() => {
                if value.as_deref() != Some("true") {
                    self.phase = 6;
                    return Ok(());
                }
                self.eval(true, "document.getElementById('zoom-level').click();true")?;
                self.phase = 8;
            }
            8 if (page.engine.zoom_factor().map_err(|e| e.to_string())? - 1.0).abs() < 0.001 => {
                self.eval(true, "document.getElementById('zoom-out').click();true")?;
                self.phase = 9;
            }
            9 if (page.engine.zoom_factor().map_err(|e| e.to_string())? - 0.9).abs() < 0.001 => {
                s.command(Command::SelectTab(self.first.ok_or("First tab lost")?))
                    .map_err(|e| e.to_string())?;
                if (s
                    .tabs
                    .active()
                    .unwrap()
                    .content
                    .engine
                    .zoom_factor()
                    .map_err(|e| e.to_string())?
                    - 1.1)
                    .abs()
                    > 0.001
                {
                    return Err("Switch lost first tab zoom".into());
                }
                s.render();
                s.window.focus_window();
                self.eval(true, "document.getElementById('find-page').click();document.getElementById('find-page').disabled")?;
                self.phase = 10;
            }
            10 if value.is_some() => {
                if value.as_deref() != Some("false") {
                    return Err("Find button was disabled".into());
                }
                // Empty queries do not complete a match callback on this runtime.
                // Exercise a real query through the same content adapter and require
                // its actual native result. Empty UI is validated separately by GUI.
                page.engine
                    .find_in_page("needle", s.proxy.clone(), s.tabs.active().unwrap().id)
                    .map_err(|e| e.to_string())?;
                self.phase = 11;
            }
            10 => {
                if let Some(value) = value {
                    println!("Find button disabled: {value}");
                }
                if !s.notice.is_empty() {
                    return Err(format!("Find did not start: {}", s.notice));
                }
            }
            11 => {
                let mut matches = 0;
                unsafe {
                    page.engine
                        .view
                        .webview()
                        .cast::<ICoreWebView2_28>()
                        .map_err(|e| e.to_string())?
                        .Find()
                        .map_err(|e| e.to_string())?
                        .MatchCount(&mut matches)
                        .map_err(|e| e.to_string())?;
                }
                if matches == 3 && self.find_ready {
                    s.command(Command::ShowPanel(Panel::Settings))
                        .map_err(|e| e.to_string())?;
                    for command in [
                        Command::FindInPage,
                        Command::PrintPage,
                        Command::ZoomIn,
                        Command::ZoomOut,
                        Command::ResetZoom,
                    ] {
                        if s.command(command).is_ok() {
                            return Err("Hidden content accepted a tool".into());
                        }
                    }
                    s.command(Command::ClosePanel).map_err(|e| e.to_string())?;
                    // Match a real click in More, which starts with chrome focused.
                    s.toolbar.focus().map_err(|e| e.to_string())?;
                    s.render();
                    self.eval(true, "document.getElementById('print-page').click();true")?;
                    self.phase = 12;
                }
            }
            12 => {
                self.eval(false, "window.prints")?;
                self.phase = 13;
            }
            13 if value.is_some() => {
                if value.as_deref() != Some("1") {
                    self.phase = 12;
                    return Ok(());
                }
                unsafe {
                    let mut parent = windows::Win32::Foundation::HWND::default();
                    page.engine
                        .view
                        .controller()
                        .ParentWindow(&mut parent)
                        .map_err(|e| e.to_string())?;
                    let focus = windows::Win32::UI::Input::KeyboardAndMouse::GetFocus();
                    if focus != parent
                        && !windows::Win32::UI::WindowsAndMessaging::IsChild(parent, focus)
                            .as_bool()
                    {
                        return Err(
                            "Print preview left focus outside the content controller".into()
                        );
                    }
                }
                // Retiring the preview's tab cancels it without printing or OS input.
                s.command(Command::CloseActive).map_err(|e| e.to_string())?;
                let mut zoom = 0.0;
                unsafe {
                    s.toolbar
                        .controller()
                        .ZoomFactor(&mut zoom)
                        .map_err(|e| e.to_string())?;
                }
                if (zoom - 1.0).abs() > 0.001 {
                    return Err("Content operations changed trusted toolbar zoom".into());
                }
                s.command(Command::NewTab).map_err(|e| e.to_string())?;
                s.command(Command::Navigate(format!("{}?fresh-controller", self.url)))
                    .map_err(|e| e.to_string())?;
                self.phase = 45;
            }
            45 if !page.state.loading
                && s.tabs
                    .active()
                    .is_some_and(|t| t.url.ends_with("?fresh-controller")) =>
            {
                if s.private {
                    for name in [
                        "session.json",
                        "session.json.bak",
                        "library.sqlite",
                        "local-diagnostics",
                    ] {
                        if s.profile.join(name).exists() {
                            return Err(format!("Private tool persisted {name}"));
                        }
                    }
                }
                if (page.engine.zoom_factor().map_err(|e| e.to_string())? - 1.0).abs() > 0.001 {
                    return Err("New content controller remembered another tab's zoom".into());
                }
                s.command(Command::CloseActive).map_err(|e| e.to_string())?;
                let id = s.window.id();
                self.app.window_event(el, id, WindowEvent::CloseRequested);
                self.phase = 30;
            }
            _ => {}
        }
        Ok(())
    }
}
impl ApplicationHandler<Event> for Probe {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        match Session::create_at(
            el,
            self.app.proxy.clone(),
            self.root.clone(),
            self.private,
            None,
        ) {
            Ok(s) => {
                self.private_profile = s.private.then(|| s.profile.clone());
                s.render();
                self.app.session = Some(s);
            }
            Err(e) => {
                self.failure = Some(e.to_string());
                el.exit();
            }
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(el, id, event);
    }
    fn user_event(&mut self, el: &ActiveEventLoop, event: Event) {
        if matches!(&event, Event::FindResult(_, true)) {
            self.find_ready = true;
        }
        if let Event::FindResult(_, success) = &event {
            println!("Native Find.Start completed: {success}");
        }
        if matches!(&event, Event::Command(Command::FindInPage)) {
            println!("Find IPC reached application");
        }
        if matches!(&event, Event::ZoomChanged(_)) {
            self.zoom_event = true;
        }
        self.app.user_event(el, event);
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.app.about_to_wait(el);
        let previous = self.phase;
        if let Err(e) = self.poll(el) {
            self.failure = Some(e);
            el.exit();
        }
        if self.phase != previous {
            println!("Page tool phase {}", self.phase);
        }
        if self.phase != 100 {
            el.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(30),
            ));
        }
    }
}
#[test]
#[ignore = "Requires Windows/WebView2; controlled fixture and disposable profile only"]
fn native_content_find_zoom_print_boundary() {
    exercise(false);
}

#[test]
#[ignore = "Requires Windows/WebView2; private disposable profile only"]
fn native_private_content_tools_are_ephemeral() {
    exercise(true);
}

fn exercise(private: bool) {
    let server = super::compat_validation::Server::start();
    let root = std::env::temp_dir().join(format!(
        "rust-browser-basics-{}-{}",
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
        private,
        private_profile: None,
        phase: 0,
        started: Instant::now(),
        result: Arc::new(Mutex::new(None)),
        failure: None,
        first: None,
        find_ready: false,
        zoom_event: false,
        last_phase: 0,
        step: Instant::now(),
    };
    let result = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let phase = probe.phase;
    let private_profile = probe.private_profile.clone();
    drop(probe);
    assert!(result.is_ok());
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(phase, 100);
    if private {
        assert!(
            !private_profile.unwrap().exists(),
            "Private engine profile was not cleaned after shutdown"
        );
        for name in [
            "session.json",
            "session.json.bak",
            "library.sqlite",
            "local-diagnostics",
        ] {
            assert!(
                !root.join(name).exists(),
                "Private content tool created {name}"
            );
        }
    } else {
        let snapshot = storage::load(&root.join("session.json")).unwrap();
        assert_eq!(snapshot.tabs.len(), 1);
        let saved = std::fs::read_to_string(root.join("session.json")).unwrap();
        assert!(
            !saved.contains("zoom"),
            "Zoom must not enter session format 1"
        );
    }
    let _ = std::fs::remove_dir_all(root);
}
