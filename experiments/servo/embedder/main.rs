//! Standalone, local-fixture host for the pinned upstream Servo workspace.
use std::{
    cell::Cell,
    error::Error,
    fs,
    io::Write,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

use euclid::Scale;
use servo::{
    AllowOrDenyRequest, CreateNewWebViewRequest, DevicePoint, EmbedderControl, EmbedderControlId,
    InputEvent, LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent,
    MouseLeftViewportEvent, MouseMoveEvent, NavigationRequest, Opts, PermissionRequest,
    RenderingContext, Servo, ServoBuilder, ServoDelegate, ServoError, WebResourceLoad,
    WebResourceResponse, WebView, WebViewBuilder, WebViewDelegate, WheelEvent,
    WindowRenderingContext,
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, Ime, KeyEvent, MouseButton as NativeButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{Window, WindowId},
};

pub mod engine_api;
mod input;
#[path = "../../../../ports/servoshell/desktop/keyutils.rs"]
mod keyutils;
mod policy;
mod validation;

use engine_api::NavigationCommand;
use policy::Fixtures;

const TITLE: &str = "Rust Browser Servo Embedder";
const CSP: &str = "<meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\">";
const FIXTURE: &str = include_str!("fixture.html");
const SECOND: &str = include_str!("second.html");
const SCALES: [f64; 4] = [1.0, 1.25, 1.5, 2.0];

#[derive(Clone, Debug)]
enum Event {
    Wake,
    Frame,
    Status(&'static str),
    Failed,
    Close,
    Report(String),
    Ime(Option<(i32, i32, u32, u32)>),
}

#[derive(Clone)]
struct Waker(EventLoopProxy<Event>);
impl servo::EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn servo::EventLoopWaker> {
        Box::new(self.clone())
    }

    fn wake(&self) {
        let _ = self.0.send_event(Event::Wake);
    }
}

struct Delegate {
    proxy: EventLoopProxy<Event>,
    fixtures: Fixtures,
    loaded: Cell<bool>,
    ime: Cell<Option<EmbedderControlId>>,
}

impl Delegate {
    fn post(&self, event: Event) {
        let _ = self.proxy.send_event(event);
    }

    fn restrict_resource(&self, load: WebResourceLoad) {
        let request = load.request();
        let url = request.url.as_url();
        if !self
            .fixtures
            .allows_resource(url, &request.method, request.is_for_main_frame)
        {
            let response = WebResourceResponse::new(url.clone());
            load.intercept(response).cancel();
        }
        // The default response proceeds only for the two local fixture documents.
    }
}

impl ServoDelegate for Delegate {
    fn notify_error(&self, _error: ServoError) {
        self.post(Event::Failed);
    }

    fn request_devtools_connection(&self, request: AllowOrDenyRequest) {
        request.deny();
    }

    fn load_web_resource(&self, load: WebResourceLoad) {
        self.restrict_resource(load);
    }
}

impl WebViewDelegate for Delegate {
    fn notify_page_title_changed(&self, _view: WebView, title: Option<String>) {
        if let Some(title) = title.filter(|title| title.starts_with("RBVALID1|")) {
            if title.len() <= 256 {
                self.post(Event::Report(title));
            } else {
                self.post(Event::Failed);
            }
        }
    }

    fn show_embedder_control(&self, _view: WebView, control: EmbedderControl) {
        if let EmbedderControl::InputMethod(method) = control {
            self.ime.set(Some(method.id()));
            let rect = method.position();
            self.post(Event::Ime(Some((
                rect.min.x,
                rect.min.y,
                rect.width().max(0) as u32,
                rect.height().max(0) as u32,
            ))));
        }
    }

    fn hide_embedder_control(&self, _view: WebView, id: EmbedderControlId) {
        if self.ime.get() == Some(id) {
            self.ime.set(None);
            self.post(Event::Ime(None));
        }
    }

    fn notify_new_frame_ready(&self, _view: WebView) {
        self.post(Event::Frame);
    }

    fn notify_load_status_changed(&self, _view: WebView, status: LoadStatus) {
        self.loaded.set(status == LoadStatus::Complete);
        let label = match status {
            LoadStatus::Started | LoadStatus::HeadParsed => "Loading fixture",
            LoadStatus::Complete => "Fixture loaded",
            LoadStatus::Stopped => "Load stopped",
        };
        self.post(Event::Status(label));
    }

    fn notify_crashed(&self, _view: WebView, _reason: String, _backtrace: Option<String>) {
        self.post(Event::Failed);
    }

    fn notify_traversal_complete(&self, _view: WebView, _id: servo::TraversalId) {
        self.post(Event::Status("Fixture history restored"));
    }

    fn notify_closed(&self, _view: WebView) {
        self.post(Event::Close);
    }

    fn request_navigation(&self, _view: WebView, request: NavigationRequest) {
        if self.fixtures.allows(&request.url) {
            request.allow();
        } else {
            request.deny();
            self.post(Event::Status("Navigation blocked"));
        }
    }

    fn request_permission(&self, _view: WebView, request: PermissionRequest) {
        request.deny();
        self.post(Event::Status("Permission denied"));
    }

    fn request_create_new(&self, _view: WebView, _request: CreateNewWebViewRequest) {
        // Dropping the pinned API's request refuses the new view.
        self.post(Event::Status("Popup blocked"));
    }

    fn load_web_resource(&self, _view: WebView, load: WebResourceLoad) {
        self.restrict_resource(load);
    }
}

struct Session {
    view: WebView,
    servo: Servo,
    context: Rc<WindowRenderingContext>,
    window: Window,
    delegate: Rc<Delegate>,
    modifiers: ModifiersState,
    pointer: Option<DevicePoint>,
}

impl Session {
    fn new(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
        run: &Path,
        validate: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let fixtures = prepare_fixtures(run, validate)?;
        let window = event_loop.create_window(
            Window::default_attributes()
                .with_title(TITLE)
                .with_inner_size(LogicalSize::new(1200.0, 820.0))
                .with_min_inner_size(LogicalSize::new(360.0, 260.0)),
        )?;
        let context = WindowRenderingContext::new(
            event_loop.display_handle()?,
            window.window_handle()?,
            window.inner_size(),
        )
        .map_err(|error| format!("Could not create the rendering surface: {error:?}"))?;
        let context = Rc::new(context);
        context
            .make_current()
            .map_err(|error| format!("Could not activate the rendering surface: {error:?}"))?;
        println!("scale_factor={}", window.scale_factor());
        let servo = ServoBuilder::default()
            .opts(Opts {
                config_dir: Some(run.join("profile")),
                temporary_storage: true,
                ..Default::default()
            })
            .event_loop_waker(Box::new(Waker(proxy.clone())))
            .build();
        let delegate = Rc::new(Delegate {
            proxy,
            fixtures,
            loaded: Cell::new(false),
            ime: Cell::new(None),
        });
        servo.set_delegate(delegate.clone());
        let view = WebViewBuilder::new(&servo, context.clone())
            .url(delegate.fixtures.page(0).clone())
            .hidpi_scale_factor(Scale::new(if validate {
                1.0
            } else {
                window.scale_factor() as f32
            }))
            .delegate(delegate.clone())
            .build();
        view.set_focused(window.has_focus());
        servo.spin_event_loop();
        Ok(Self {
            view,
            servo,
            context,
            window,
            delegate,
            modifiers: ModifiersState::empty(),
            pointer: None,
        })
    }

    fn input(&self, event: InputEvent) {
        self.view.notify_input_event(event);
        self.servo.spin_event_loop();
    }

    fn navigate(&self, action: NavigationCommand) {
        match action {
            NavigationCommand::Back if self.view.can_go_back() => {
                self.view.go_back(1);
            }
            NavigationCommand::Forward if self.view.can_go_forward() => {
                self.view.go_forward(1);
            }
            NavigationCommand::Reload => self.view.reload(),
            _ => return,
        }
        self.servo.spin_event_loop();
    }

    fn key(&self, event: KeyEvent) {
        let mut key = keyutils::keyboard_event_from_winit(&event, self.modifiers);
        key.event.repeat = event.repeat;
        let modifiers = key.event.modifiers;
        let shortcut = match &event.logical_key {
            Key::Named(NamedKey::F1) => Some(0),
            Key::Named(NamedKey::F2) => Some(1),
            _ => None,
        };
        if let Some(index) = shortcut {
            if event.state == ElementState::Pressed && !event.repeat {
                self.view.load(self.delegate.fixtures.page(index).clone());
                self.servo.spin_event_loop();
            }
            return;
        }
        let action = match &event.logical_key {
            Key::Named(NamedKey::ArrowLeft) if modifiers == keyutils::CMD_OR_ALT => {
                Some(NavigationCommand::Back)
            }
            Key::Named(NamedKey::ArrowRight) if modifiers == keyutils::CMD_OR_ALT => {
                Some(NavigationCommand::Forward)
            }
            Key::Named(NamedKey::F5) => Some(NavigationCommand::Reload),
            Key::Character(value) if modifiers == keyutils::CMD_OR_CONTROL && value == "r" => {
                Some(NavigationCommand::Reload)
            }
            _ => None,
        };
        if let Some(action) = action {
            if event.state == ElementState::Pressed && !event.repeat {
                self.navigate(action);
            }
        } else {
            self.input(InputEvent::Keyboard(key));
        }
    }

    fn close(self) {
        // Delegates hold only a proxy and policy, so the view cannot retain the host.
        drop(self.view);
        drop(self.servo);
        drop(self.context);
        drop(self.window);
        println!("teardown_complete");
    }
}

fn prepare_fixtures(run: &Path, validate: bool) -> Result<Fixtures, Box<dyn Error>> {
    if !run.join(".rust-browser-servo-spike").is_file() {
        return Err("Use a newly allocated experimental run directory".into());
    }
    // Fails if an existing profile would be reused.
    fs::create_dir(run.join("profile"))?;
    let pages = ["fixture.html", "second.html"];
    let first = if validate { validation::HTML } else { FIXTURE };
    for (name, content) in pages.iter().zip([first, SECOND]) {
        let (head, body) = content
            .split_once("<meta charset=\"utf-8\">")
            .ok_or("Fixture is missing its security-header insertion point")?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(run.join(name))?;
        // Insert the CSP before inline fixture code is processed.
        write!(file, "{head}{CSP}{body}")?;
    }
    Fixtures::new(run, pages)
}

struct App {
    proxy: EventLoopProxy<Event>,
    run: std::path::PathBuf,
    session: Option<Session>,
    started: Instant,
    first_ready_present: bool,
    smoke: bool,
    validation_index: Option<usize>,
    pending_report: Option<validation::Report>,
    failure: Option<String>,
}

impl App {
    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(session) = self.session.take() {
            session.close();
        }
        event_loop.exit();
    }
}

impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.session.is_some() {
            return;
        }
        match Session::new(
            event_loop,
            self.proxy.clone(),
            &self.run,
            self.validation_index.is_some(),
        ) {
            Ok(session) => self.session = Some(session),
            Err(error) => {
                self.failure = Some(error.to_string());
                self.finish(event_loop);
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        let Some(session) = &self.session else { return };
        match event {
            Event::Wake => session.servo.spin_event_loop(),
            Event::Frame => session.window.request_redraw(),
            Event::Status(status) => session.window.set_title(&format!("{TITLE} — {status}")),
            Event::Failed => {
                self.failure = Some("Servo reported an engine failure".into());
                self.finish(event_loop);
            }
            Event::Close => self.finish(event_loop),
            Event::Ime(rect) => {
                session.window.set_ime_allowed(rect.is_some());
                if let Some((x, y, width, height)) = rect {
                    session.window.set_ime_cursor_area(
                        PhysicalPosition::new(x, y),
                        PhysicalSize::new(width, height),
                    );
                }
            }
            Event::Report(title) if self.validation_index.is_some() => {
                match validation::parse(&title) {
                    Ok(Some(report)) => {
                        self.pending_report = Some(report);
                        session.window.request_redraw();
                    }
                    result => {
                        self.failure =
                            Some(result.err().unwrap_or("Missing fixture report").into());
                        self.finish(event_loop);
                    }
                }
            }
            Event::Report(_) => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if (self.smoke && !self.first_ready_present) || self.validation_index.is_some() {
            let deadline = self.started + Duration::from_secs(30);
            if Instant::now() >= deadline {
                self.failure = Some("Timed out waiting for the first ready fixture frame".into());
                self.finish(event_loop);
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if id != session.window.id() {
            return;
        }
        match event {
            WindowEvent::CloseRequested => self.finish(event_loop),
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                session.view.resize(size);
                session.servo.spin_event_loop();
                session.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                session
                    .view
                    .set_hidpi_scale_factor(Scale::new(scale_factor as f32));
                session.servo.spin_event_loop();
                session.window.request_redraw();
            }
            WindowEvent::Focused(focused) => {
                session.view.set_focused(focused);
                session.servo.spin_event_loop();
            }
            WindowEvent::ModifiersChanged(state) => session.modifiers = state.state(),
            WindowEvent::Ime(ime) => {
                let active = if matches!(ime, Ime::Disabled) {
                    session.delegate.ime.take().is_some()
                } else {
                    session.delegate.ime.get().is_some()
                };
                for event in input::ime_events(ime, active) {
                    session.input(event);
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => session.key(event),
            WindowEvent::CursorMoved { position, .. } => {
                let point = DevicePoint::new(position.x as f32, position.y as f32);
                session.pointer = Some(point);
                session.input(InputEvent::MouseMove(MouseMoveEvent::new(point.into())));
            }
            WindowEvent::CursorLeft { .. } => {
                session.pointer = None;
                session.input(InputEvent::MouseLeftViewport(
                    MouseLeftViewportEvent::default(),
                ));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(point) = session.pointer {
                    let action = if state == ElementState::Pressed {
                        MouseButtonAction::Down
                    } else {
                        MouseButtonAction::Up
                    };
                    let button = match button {
                        NativeButton::Left => MouseButton::Primary,
                        NativeButton::Right => MouseButton::Secondary,
                        NativeButton::Middle => MouseButton::Auxiliary,
                        NativeButton::Back => MouseButton::Back,
                        NativeButton::Forward => MouseButton::Forward,
                        NativeButton::Other(button) => MouseButton::Other(button),
                    };
                    session.input(InputEvent::MouseButton(MouseButtonEvent::new(
                        action,
                        button,
                        point.into(),
                    )));
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(point) = session.pointer {
                    session.input(InputEvent::Wheel(WheelEvent::new(
                        input::wheel_delta(delta),
                        point.into(),
                        keyutils::keyboard_modifiers_from_winit_modifiers(session.modifiers),
                    )));
                }
            }
            WindowEvent::RedrawRequested => {
                let size = session.window.inner_size();
                if size.width == 0 || size.height == 0 {
                    return;
                }
                if let Err(error) = session.context.make_current() {
                    self.failure = Some(format!(
                        "Could not activate the rendering surface: {error:?}"
                    ));
                    self.finish(event_loop);
                    return;
                }
                session.view.paint();
                session.window.pre_present_notify();
                session.context.present();
                if let (Some(report), Some(index)) =
                    (self.pending_report.take(), self.validation_index)
                {
                    if (report.scale - SCALES[index]).abs() > 0.01
                        || (report.width * report.scale - f64::from(size.width)).abs() > 2.0
                        || (report.height * report.scale - f64::from(size.height)).abs() > 2.0
                    {
                        self.failure = Some("Servo device scale did not reach the fixture".into());
                        self.finish(event_loop);
                        return;
                    }
                    println!("servo {report}");
                    if index + 1 == SCALES.len() {
                        self.finish(event_loop);
                        return;
                    }
                    self.validation_index = Some(index + 1);
                    session
                        .view
                        .set_hidpi_scale_factor(Scale::new(SCALES[index + 1] as f32));
                    session.view.reload();
                    session.servo.spin_event_loop();
                }
                if !self.first_ready_present
                    && session.delegate.loaded.get()
                    && session.view.page_title().as_deref() == Some("Rust Browser engine fixture")
                {
                    self.first_ready_present = true;
                    println!(
                        "first_ready_present_ms={}",
                        self.started.elapsed().as_millis()
                    );
                    if self.smoke {
                        self.finish(event_loop);
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (smoke, validate) = match (args.next().as_deref(), args.next()) {
        (None, None) => (false, false),
        (Some("--smoke"), None) => (true, false),
        (Some("--validate"), None) => (false, true),
        _ => return Err("Usage: rust_browser_embedder [--smoke | --validate]".into()),
    };
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| "Could not install the default TLS provider")?;
    let event_loop = EventLoop::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        proxy: event_loop.create_proxy(),
        run: std::env::current_dir()?,
        session: None,
        started: Instant::now(),
        first_ready_present: false,
        smoke,
        validation_index: validate.then_some(0),
        pending_report: None,
        failure: None,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    if smoke && !app.first_ready_present {
        return Err("No fixture frame was presented".into());
    }
    Ok(())
}
