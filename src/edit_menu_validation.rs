//! The address field's own edit menu (Cut, Copy, Paste, Select all) on a real
//! toolbar. The clipboard is the in-memory test clipboard, never the user's.
use super::*;
use crate::native_clipboard::TEST_CLIPBOARD;
use std::sync::{Arc, Mutex};
use winit::platform::windows::EventLoopBuilderExtWindows;
use wry::WebViewExtWindows;

const OPEN_MENU: &str = r#"(() => { const a = document.getElementById('address');
  a.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 40 }));
  const m = document.getElementById('address-menu');
  return JSON.stringify({ hidden: m.hidden, role: m.getAttribute('role'),
    items: [...m.querySelectorAll('button')].map(b => [b.textContent, b.getAttribute('role'), b.disabled]),
    focused: document.activeElement.textContent }); })()"#;

fn select(value: &str, start: usize, end: usize) -> String {
    format!(
        "(() => {{ const a = document.getElementById('address'); a.focus(); a.value = {}; a.setSelectionRange({start}, {end}); return 'ok'; }})()",
        serde_json::to_string(value).unwrap()
    )
}
fn click(op: &str) -> String {
    format!("document.querySelector('#address-menu [data-op={op}]').click(); 'ok'")
}
const STATE: &str = r#"(() => { const a = document.getElementById('address');
  return JSON.stringify({ value: a.value, start: a.selectionStart, end: a.selectionEnd,
    hidden: document.getElementById('address-menu').hidden, focus: document.activeElement.id }); })()"#;

struct Probe {
    app: App,
    root: PathBuf,
    phase: u8,
    started: Instant,
    at: Instant,
    result: Arc<Mutex<Option<String>>>,
    asked: bool,
    failure: Option<String>,
    log: Vec<String>,
}

fn json(value: &str) -> serde_json::Value {
    let text = serde_json::from_str::<String>(value).unwrap_or_default();
    serde_json::from_str(&text).unwrap_or_default()
}
fn clipboard() -> String {
    TEST_CLIPBOARD.with(|clip| clip.borrow().clone())
}

impl Probe {
    /// Ask once; return the answer when it arrives.
    fn ask(&mut self, script: &str) -> Result<Option<String>, String> {
        if let Some(value) = self.result.lock().map_err(|_| "Result lock")?.take() {
            self.asked = false;
            return Ok(Some(value));
        }
        if !self.asked {
            let session = self.app.session.as_ref().ok_or("No session")?;
            let result = self.result.clone();
            session
                .toolbar
                .evaluate_script_with_callback(script, move |value| {
                    if let Ok(mut slot) = result.lock() {
                        *slot = Some(value)
                    }
                })
                .map_err(|_| "Toolbar evaluation failed")?;
            self.asked = true;
        }
        Ok(None)
    }

    fn next(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }

    fn poll(&mut self) -> Result<(), String> {
        if self.started.elapsed() > Duration::from_secs(60) {
            return Err(format!(
                "Edit menu fixture timed out at phase {}",
                self.phase
            ));
        }
        if !self.app.session.as_ref().is_some_and(|s| s.ready) {
            return Ok(());
        }
        match self.phase {
            // WebView2's own toolbar menus stay disabled.
            0 => {
                let session = self.app.session.as_ref().ok_or("No session")?;
                let mut enabled = windows::core::BOOL::default();
                // SAFETY: reads a setting of the fixture's own live toolbar view.
                unsafe {
                    session
                        .toolbar
                        .webview()
                        .Settings()
                        .and_then(|s| s.AreDefaultContextMenusEnabled(&mut enabled))
                }
                .map_err(|_| "Toolbar settings unreadable")?;
                if enabled.as_bool() {
                    return Err("WebView2 default context menus were enabled on the toolbar".into());
                }
                self.next();
            }
            1 if self.at.elapsed() > Duration::from_millis(800) => {
                if self.ask(&select("https://example.com/abc", 8, 8))?.is_some() {
                    self.next();
                }
            }
            // Nothing selected: Cut and Copy are disabled.
            2 => {
                if let Some(value) = self.ask(OPEN_MENU)? {
                    let menu = json(&value);
                    let expected = serde_json::json!([
                        ["Cut", "menuitem", true],
                        ["Copy", "menuitem", true],
                        ["Paste", "menuitem", false],
                        ["Select all", "menuitem", false]
                    ]);
                    if menu["hidden"] != false || menu["role"] != "menu" || menu["items"] != expected {
                        return Err(format!("Unexpected menu without a selection: {menu}"));
                    }
                    if menu["focused"] != "Paste" {
                        return Err(format!("First enabled item not focused: {menu}"));
                    }
                    self.next();
                }
            }
            // Escape closes it and returns to the address field.
            3 => {
                if self
                    .ask("document.getElementById('address-menu').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true})); 'ok'")?
                    .is_some()
                {
                    self.next();
                }
            }
            4 => {
                if let Some(value) = self.ask(STATE)? {
                    let state = json(&value);
                    if state["hidden"] != true || state["focus"] != "address" {
                        return Err(format!("Escape did not close the menu: {state}"));
                    }
                    self.next();
                }
            }
            // Copy the selected host.
            5 => {
                if self.ask(&select("https://example.com/abc", 8, 19))?.is_some() {
                    self.next();
                }
            }
            6 => {
                if let Some(value) = self.ask(OPEN_MENU)? {
                    if json(&value)["items"][1][2] != false {
                        return Err(format!("Copy disabled with a selection: {value}"));
                    }
                    self.next();
                }
            }
            7 => {
                if self.ask(&click("copy"))?.is_some() {
                    self.next();
                }
            }
            8 if clipboard() == "example.com" => self.next(),
            8 if self.at.elapsed() > Duration::from_secs(5) => {
                return Err(format!("Copy did not reach the clipboard: {:?}", clipboard()));
            }
            // Paste hostile clipboard text: one line, no control characters, bounded.
            9 => {
                let hostile = format!("a\r\nb\tc\u{7}d{}", "z".repeat(9000));
                TEST_CLIPBOARD.with(|clip| *clip.borrow_mut() = hostile);
                if self.ask(&select("x", 1, 1))?.is_some() {
                    self.next();
                }
            }
            10 => {
                if self.ask(OPEN_MENU)?.is_some() {
                    self.next();
                }
            }
            11 => {
                if self.ask(&click("paste"))?.is_some() {
                    self.next();
                }
            }
            12 if self.at.elapsed() > Duration::from_millis(500) => {
                if let Some(value) = self.ask(STATE)? {
                    let state = json(&value);
                    let expected = format!(
                        "x{}",
                        rust_browser::navigation::paste_text(&format!("a\r\nb\tc\u{7}d{}", "z".repeat(9000)))
                    );
                    if state["value"] != expected.as_str() {
                        return Err(format!("Paste inserted {:?}", state["value"].as_str().map(|v| &v[..v.len().min(40)])));
                    }
                    if !expected.starts_with("xa  b cd") || expected.chars().count() != 1 + rust_browser::navigation::MAX_ADDRESS {
                        return Err("Paste text was not cleaned and bounded".into());
                    }
                    self.next();
                }
            }
            // Cut removes the selection and copies it.
            13 => {
                if self.ask(&select("https://example.com/abc", 19, 23))?.is_some() {
                    self.next();
                }
            }
            14 => {
                if self.ask(OPEN_MENU)?.is_some() {
                    self.next();
                }
            }
            15 => {
                if self.ask(&click("cut"))?.is_some() {
                    self.next();
                }
            }
            16 if clipboard() == "/abc" => {
                if let Some(value) = self.ask(STATE)? {
                    if json(&value)["value"] != "https://example.com" {
                        return Err(format!("Cut left {value}"));
                    }
                    self.next();
                }
            }
            16 if self.at.elapsed() > Duration::from_secs(5) => {
                return Err(format!("Cut did not reach the clipboard: {:?}", clipboard()));
            }
            // Select all.
            17 => {
                if self.ask(OPEN_MENU)?.is_some() {
                    self.next();
                }
            }
            18 => {
                if self.ask(&click("select_all"))?.is_some() {
                    self.next();
                }
            }
            19 => {
                if let Some(value) = self.ask(STATE)? {
                    let state = json(&value);
                    if state["start"] != 0 || state["end"] != 19 || state["hidden"] != true {
                        return Err(format!("Select all failed: {state}"));
                    }
                    self.log.push("edit menu: cut, copy, paste and select all passed".into());
                    self.phase = 100;
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
                self.failure = Some("Edit menu fixture startup failed".into());
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
        if self.phase == 100 || self.failure.is_some() {
            el.exit();
            return;
        }
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(20),
        ));
    }
}

#[test]
#[ignore = "Requires Windows/WebView2; uses only the in-memory test clipboard"]
fn native_address_field_edit_menu() {
    let root = std::env::temp_dir().join(format!(
        "rust-browser-edit-menu-{}-{}",
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
        phase: 0,
        started: Instant::now(),
        at: Instant::now(),
        result: Arc::new(Mutex::new(None)),
        asked: false,
        failure: None,
        log: Vec::new(),
    };
    let _ = event_loop.run_app(&mut probe);
    let failure = probe.failure.clone();
    let phase = probe.phase;
    let log = std::mem::take(&mut probe.log);
    drop(probe);
    let _ = std::fs::remove_dir_all(root);
    println!("{}", log.join("\n"));
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(phase, 100);
}
