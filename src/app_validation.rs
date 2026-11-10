//! Actual application orchestration check, separate from the native adapter probe.
use super::*;
use crate::native_validation::{FIXTURE_URL, map_fixture, write_fixture};
use winit::platform::windows::EventLoopBuilderExtWindows;

struct Probe {
    app: App,
    profile: PathBuf,
    phase: u8,
    started: Instant,
    error: Option<String>,
}
impl Probe {
    fn poll(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.phase == 4 {
            return Ok(());
        }
        if self.started.elapsed() > Duration::from_secs(90) {
            return Err("Application validation timed out".into());
        }
        let session = self.app.session.as_mut().ok_or("No validation session")?;
        if !session.ready
            || session
                .tabs
                .active()
                .is_none_or(|t| t.content.state.loading)
        {
            return Ok(());
        }
        match self.phase {
            0 => {
                let fixture_loaded = session.tabs.active().is_some_and(|tab| {
                    !tab.content.state.failed
                        && tab.content.engine.view.url().ok().as_deref() == Some(FIXTURE_URL)
                });
                if !fixture_loaded {
                    return Ok(());
                }
                let before = session.tabs.items.len();
                if session
                    .begin_clear(
                        ClearRequest {
                            history: true,
                            confirmed: false,
                            ..ClearRequest::default()
                        },
                        None,
                    )
                    .is_ok()
                    || session.tabs.items.len() != before
                {
                    return Err("Unconfirmed clear changed application state".into());
                }
                session
                    .library_action(Action::SaveBookmark {
                        id: None,
                        title: "Keep this bookmark".into(),
                        url: "https://example.com/".into(),
                        folder: None,
                    })
                    .map_err(|_| "Could not seed bookmark")?;
                session
                    .library_action(Action::SetPermission {
                        origin: "https://example.com".into(),
                        capability: security::Capability::Camera,
                        decision: Some(Decision::Allow),
                    })
                    .map_err(|_| "Could not seed permission")?;
                std::fs::write(
                    self.profile.join("session-invalid-123.json"),
                    b"synthetic old URL",
                )
                .map_err(|_| "Could not seed session backup")?;
                std::fs::write(
                    self.profile.join("library.corrupt-123"),
                    b"synthetic bookmark recovery",
                )
                .map_err(|_| "Could not seed library backup")?;
                self.phase = 1;
            }
            1 => {
                if session.library_data.bookmarks.len() == 1
                    && !session.library_data.history.is_empty()
                    && session.library_data.permissions.len() == 1
                {
                    session
                        .begin_clear(
                            ClearRequest {
                                history: true,
                                cookies: true,
                                cache: true,
                                session: true,
                                permissions: true,
                                confirmed: true,
                            },
                            None,
                        )
                        .map_err(|_| "Application clearing did not start")?;
                    self.phase = 2;
                }
            }
            2 => {
                if session.clearing.is_none() {
                    if !session.notice.contains("WebView2 confirmed")
                        || session.notice.contains("failed")
                        || session.notice.contains("unconfirmed")
                    {
                        return Err("Application did not report confirmed deletion".into());
                    }
                    if !session.library_data.history.is_empty()
                        || !session.library_data.permissions.is_empty()
                        || session.library_data.bookmarks.len() != 1
                    {
                        return Err("Application clearing did not preserve the correct data".into());
                    }
                    if session.settings.restore_session
                        || session
                            .tabs
                            .active()
                            .and_then(|t| t.content.engine.view.url().ok())
                            .as_deref()
                            != Some("about:blank")
                    {
                        return Err(
                            "Clearing did not prevent active-tab/session recontamination".into(),
                        );
                    }
                    if self.profile.join("session-invalid-123.json").exists()
                        || !self.profile.join("library.corrupt-123").exists()
                    {
                        return Err("Incorrect recovery-file handling".into());
                    }
                    session
                        .begin_clear(
                            ClearRequest {
                                permissions: true,
                                confirmed: true,
                                ..ClearRequest::default()
                            },
                            None,
                        )
                        .map_err(|_| "Could not start clearing failure probe")?;
                    self.phase = 3;
                }
            }
            3 if session.clearing.is_none() => {
                if session.permissions.borrow().enabled
                    || !session.notice.contains("denied until restart")
                    || !session.notice.contains("clearing failure")
                {
                    return Err(
                        "Unconfirmed permission clearing was not reported or failed open".into(),
                    );
                }
                self.phase = 4;
                self.app.session = None;
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
            self.profile.clone(),
            false,
            None,
        ) {
            Ok(session) => {
                if let Some(tab) = session.tabs.active()
                    && let Err(error) = map_fixture(&tab.content.engine.view, &self.profile)
                {
                    self.error = Some(format!("Could not map application fixture: {error}"));
                    event_loop.exit();
                    return;
                }
                session.render();
                self.app.session = Some(session);
            }
            Err(_) => {
                self.error = Some("Application initialization failed".into());
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(event_loop, id, event);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        // Fault injection: exercise the application response to an unsuccessful
        // callback without pretending the real runtime failed the first operation.
        let event = match event {
            Event::EngineCleared {
                startup: false,
                ticket,
                ..
            } if self.phase == 3 => Event::EngineCleared {
                startup: false,
                success: false,
                ticket,
            },
            other => other,
        };
        self.app.user_event(event_loop, event);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.app.about_to_wait(event_loop);
        if let Err(error) = self.poll(event_loop) {
            self.error = Some(error);
            event_loop.exit();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(25),
        ));
    }
}
#[test]
#[ignore = "Requires Windows desktop/WebView2; run this test alone in a separate process"]
fn native_application_clear_preserves_bookmarks_and_prevents_session_rewrites() {
    let root = std::env::temp_dir().join(format!(
        "rust-browser-app-probe-{}-{}",
        std::process::id(),
        rust_browser::library::now()
    ));
    std::fs::create_dir(&root).unwrap();
    write_fixture(&root).unwrap();
    storage::save(
        &root.join("session.json"),
        &Snapshot {
            tabs: vec![FIXTURE_URL.into()],
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
        },
        profile: root.clone(),
        phase: 0,
        started: Instant::now(),
        error: None,
    };
    let result = event_loop.run_app(&mut probe);
    let error = probe.error.clone();
    let phase = probe.phase;
    drop(probe);
    let snapshot = storage::load(&root.join("session.json")).unwrap();
    assert!(!snapshot.settings.restore_session);
    assert_eq!(snapshot.tabs, vec![navigation::HOME]);
    let (db, _) = rust_browser::library::Database::open(&root.join("library.sqlite")).unwrap();
    let data = db.query(&Filter::default()).unwrap();
    assert_eq!(data.bookmarks.len(), 1);
    assert!(data.history.is_empty() && data.permissions.is_empty());
    drop(db);
    let _ = std::fs::remove_dir_all(root);
    assert!(result.is_ok());
    assert!(error.is_none(), "{error:?}");
    assert_eq!(phase, 4);
}
