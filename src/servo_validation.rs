//! Opt-in, synthetic comparison fixture. This module is compiled only for tests.
#[path = "../experiments/servo/validation.rs"]
mod fixture;

use base64::Engine as _;
use std::time::{Duration, Instant};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller3;
use windows::core::Interface;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

const SCALES: [f64; 4] = [1.0, 1.25, 1.5, 2.0];

struct Harness {
    view: Option<WebView>,
    context: Option<WebContext>,
    window: Option<Window>,
    proxy: EventLoopProxy<String>,
    root: std::path::PathBuf,
    index: usize,
    failure: Option<String>,
    started: Instant,
}

impl Harness {
    fn next(&self) -> Result<(), Box<dyn std::error::Error>> {
        let view = self.view.as_ref().ok_or("Missing test view")?;
        // SAFETY: the controller is owned by this live test view on its UI thread.
        // This changes only the disposable test controller, never Windows settings.
        unsafe {
            let controller: ICoreWebView2Controller3 = view.controller().cast()?;
            controller.SetShouldDetectMonitorScaleChanges(false)?;
            controller.SetRasterizationScale(SCALES[self.index])?;
        }
        view.load_html(fixture::HTML)?;
        Ok(())
    }

    fn close(&mut self, event_loop: &ActiveEventLoop) {
        self.view.take();
        self.context.take();
        self.window.take();
        event_loop.exit();
    }
}

impl ApplicationHandler<String> for Harness {
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::CloseRequested) {
            self.failure = Some("Controlled fixture window closed before completion".into());
            self.close(event_loop);
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.view.is_some() {
            return;
        }
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let window = event_loop.create_window(
                Window::default_attributes()
                    .with_title("Rust Browser controlled WebView2 validation")
                    .with_inner_size(LogicalSize::new(1200.0, 820.0)),
            )?;
            let mut context = WebContext::new(Some(self.root.clone()));
            let proxy = self.proxy.clone();
            let fixture_navigation = format!(
                "data:text/html;charset=utf-8;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(fixture::HTML.as_bytes())
            );
            let view = WebViewBuilder::new_with_web_context(&mut context)
                .with_additional_browser_args("")
                .with_incognito(true)
                .with_devtools(false)
                .with_navigation_handler(move |url| {
                    url == "about:blank" || url == fixture_navigation
                })
                .with_document_title_changed_handler(move |title| {
                    let _ = proxy.send_event(title);
                })
                .build(&window)?;
            self.view = Some(view);
            self.context = Some(context);
            self.window = Some(window);
            self.window
                .as_ref()
                .ok_or("Missing test window")?
                .focus_window();
            self.next()
        })();
        if let Err(error) = result {
            self.failure = Some(error.to_string());
            self.close(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, title: String) {
        if self.view.is_none() || self.index >= SCALES.len() {
            return;
        }
        match fixture::parse(&title) {
            Ok(None) => return,
            Ok(Some(report)) => {
                let size = self
                    .window
                    .as_ref()
                    .map(Window::inner_size)
                    .unwrap_or_default();
                if report.composition_raf_ms.is_none() {
                    self.failure =
                        Some("WebView2 composition constructor became unavailable".into());
                } else if (report.scale - SCALES[self.index]).abs() > 0.01
                    || (report.width * report.scale - f64::from(size.width)).abs() > 2.0
                    || (report.height * report.scale - f64::from(size.height)).abs() > 2.0
                {
                    self.failure =
                        Some("WebView2 rasterization scale did not reach the fixture".into());
                } else {
                    println!("webview2 {report}");
                    self.index += 1;
                    if self.index < SCALES.len() {
                        if let Err(error) = self.next() {
                            self.failure = Some(error.to_string());
                        } else {
                            return;
                        }
                    }
                }
            }
            Err(error) => self.failure = Some(error.into()),
        }
        self.close(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.started.elapsed() > Duration::from_secs(30) {
            self.failure = Some("Controlled WebView2 fixture timed out".into());
            self.close(event_loop);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(50),
        ));
    }
}

#[test]
#[ignore = "Requires Windows WebView2; runs the shared synthetic event and frame fixture separately"]
fn native_servo_comparison_fixture() {
    let root = std::env::temp_dir().join(format!(
        "rust-browser-validation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let event_loop = EventLoop::<String>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut app = Harness {
        view: None,
        context: None,
        window: None,
        proxy: event_loop.create_proxy(),
        root,
        index: 0,
        failure: None,
        started: Instant::now(),
    };
    event_loop.run_app(&mut app).unwrap();
    assert!(app.failure.is_none(), "{:?}", app.failure);
    assert_eq!(app.index, SCALES.len());
}
