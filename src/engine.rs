//! Windows engine adapter. Keep COM and WebView2 details out of application state.
use crate::app::{Event, PageEvent};
use rust_browser::{
    engine_api::{
        ContentEngine, EngineResult, HostIntegration, InputRouting, LoadTarget, NavigationCommand,
        Presentation,
    },
    navigation,
    tabs::TabId,
};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, HistoryChangedEventHandler,
    LaunchingExternalUriSchemeEventHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, SourceChangedEventHandler,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::core::{BOOL, Interface};
use winit::{event_loop::EventLoopProxy, window::Window};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

pub struct Engine {
    pub view: WebView,
    pub activity: std::rc::Rc<crate::native_activity::Activity>,
    progress: std::rc::Rc<NavigationProgress>,
}
pub struct Options {
    pub private: bool,
    pub environment: Option<ICoreWebView2Environment>,
}

/// Small bound for remembered superseded loads/rejected starts; only rapid
/// successive host loads within one native start latency populate them.
const STALE_LIMIT: usize = 4;

#[derive(Default)]
struct NavigationProgress {
    current: std::cell::Cell<Option<u64>>,
    finished: std::cell::Cell<bool>,
    stopped: std::cell::Cell<Option<u64>>,
    awaiting_web_start: std::cell::Cell<bool>,
    /// Normalized target of the host web load whose start is awaited.
    expected: std::cell::RefCell<Option<String>>,
    /// Host web loads replaced before their NavigationStarting arrived.
    superseded: std::cell::RefCell<Vec<String>>,
    /// Navigation IDs whose late start was rejected (covers their redirects).
    rejected: std::cell::RefCell<Vec<u64>>,
}

/// State captured before a host navigation so a rejected request can restore it.
struct Superseded {
    current: Option<u64>,
    finished: bool,
    awaiting: bool,
    expected: Option<String>,
    superseded: Vec<String>,
}

fn normalized(uri: &str) -> String {
    url::Url::parse(uri).map_or_else(|_| uri.to_owned(), String::from)
}

fn push_bounded<T>(items: &mut Vec<T>, item: T) {
    if items.len() == STALE_LIMIT {
        items.remove(0);
    }
    items.push(item);
}

impl NavigationProgress {
    fn supersede(&self) -> Superseded {
        // Navigate can synchronously abort the old document before the next
        // NavigationStarting callback. Its completion is already obsolete.
        Superseded {
            current: self.current.get(),
            finished: self.finished.replace(true),
            awaiting: self.awaiting_web_start.replace(false),
            expected: self.expected.borrow().clone(),
            superseded: self.superseded.borrow().clone(),
        }
    }
    /// Record a host load of `target` after `supersede`. If the previous host
    /// web load had not started yet, its late start must not become current.
    fn expect(&self, previous: &Superseded, target: &str) {
        let mut superseded = self.superseded.borrow_mut();
        match (previous.awaiting, self.expected.take()) {
            (true, Some(old)) => push_bounded(&mut superseded, old),
            (false, _) => superseded.clear(),
            (true, None) => {}
        }
        let web = target != "about:blank";
        self.awaiting_web_start.set(web);
        *self.expected.borrow_mut() = web.then(|| normalized(target));
    }
    fn restore_rejected(&self, previous: Superseded) {
        if self.current.get() == previous.current {
            self.finished.set(previous.finished);
            self.awaiting_web_start.set(previous.awaiting);
        }
        *self.expected.borrow_mut() = previous.expected;
        *self.superseded.borrow_mut() = previous.superseded;
    }
    fn start_document(&self, id: u64, uri: &str) -> bool {
        if self.rejected.borrow().contains(&id) {
            return false;
        }
        if self.awaiting_web_start.get() {
            // Initial blank navigation can start asynchronously after a host web
            // load has already replaced it. It is not that requested document.
            if uri == "about:blank" {
                return false;
            }
            // A superseded host load's start can arrive after its replacement
            // was requested. Reject only an exact remembered target; anything
            // else is accepted as before so a mismatch can never stall a tab.
            let uri = normalized(uri);
            let mut superseded = self.superseded.borrow_mut();
            if self.expected.borrow().as_deref() != Some(uri.as_str())
                && let Some(index) = superseded.iter().position(|old| *old == uri)
            {
                superseded.remove(index);
                push_bounded(&mut self.rejected.borrow_mut(), id);
                return false;
            }
        }
        self.awaiting_web_start.set(false);
        self.expected.borrow_mut().take();
        self.superseded.borrow_mut().clear();
        self.begin(id);
        true
    }
    fn begin(&self, id: u64) {
        // Redirects retain the same navigation ID.
        if self.current.get() != Some(id) {
            self.current.set(Some(id));
            self.finished.set(false);
            self.stopped.set(None);
        }
    }
    fn finish(&self, id: u64) -> bool {
        self.current.get() == Some(id) && !self.finished.replace(true)
    }
    fn cancelled(&self, id: u64, status: COREWEBVIEW2_WEB_ERROR_STATUS) -> bool {
        status == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED
            || (status == COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED
                && self.stopped.get() == Some(id))
    }
}

fn navigation_error(status: COREWEBVIEW2_WEB_ERROR_STATUS) -> &'static str {
    match status {
        COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_COMMON_NAME_IS_INCORRECT
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CLIENT_CERTIFICATE_CONTAINS_ERRORS
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_REVOKED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID => {
            "The certificate could not be validated. Check the address and Windows date, or contact the website owner"
        }
        COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED => {
            "The website name could not be found. Check the address and your network connection"
        }
        COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT => {
            "The website took too long to respond. Retry when the connection is available"
        }
        COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT
        | COREWEBVIEW2_WEB_ERROR_STATUS_SERVER_UNREACHABLE
        | COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET
        | COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED => {
            "The connection was interrupted or unavailable. Check your network and retry"
        }
        COREWEBVIEW2_WEB_ERROR_STATUS_REDIRECT_FAILED => {
            "The website could not complete its redirects. Retry or contact the website owner"
        }
        COREWEBVIEW2_WEB_ERROR_STATUS_VALID_AUTHENTICATION_CREDENTIALS_REQUIRED
        | COREWEBVIEW2_WEB_ERROR_STATUS_VALID_PROXY_AUTHENTICATION_REQUIRED => {
            "The website or network proxy requires authentication"
        }
        _ => "The page could not be loaded. Check the address and connection, then retry",
    }
}

impl Engine {
    pub fn new(
        window: &Window,
        context: &mut WebContext,
        proxy: EventLoopProxy<Event>,
        id: TabId,
        downloads: crate::native_downloads::Shared,
        permissions: crate::native_security::Shared,
        options: Options,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let nav_proxy = proxy.clone();
        let private = options.private;
        let related = options.environment.is_some();
        let builder = WebViewBuilder::new_with_web_context(context);
        let builder = if let Some(environment) = options.environment {
            builder.with_environment(environment)
        } else {
            builder
        };
        let view = builder
            // Preserve WebView2 security defaults instead of Wry's default browser arguments.
            .with_additional_browser_args("")
            .with_autoplay(false)
            .with_clipboard(false)
            .with_incognito(private)
            .with_focused(false)
            .with_navigation_handler(move |url| {
                let allowed = url == "about:blank" || navigation::allowed(&url);
                if !allowed {
                    let _ = nav_proxy.send_event(Event::Page(
                        id,
                        PageEvent::Error("Blocked a non-HTTP(S) navigation.".into()),
                    ));
                }
                allowed
            })
            .with_visible(false)
            .with_document_title_changed_handler({
                let p = proxy.clone();
                move |title| {
                    let _ = p.send_event(Event::Page(id, PageEvent::Title(title)));
                }
            })
            .build_as_child(window)?;
        if private {
            crate::native_security::verify_private(&view)?;
        }
        crate::native_security::install(&view, permissions, proxy.clone(), id)?;
        crate::native_downloads::install(&view, downloads, proxy.clone(), id)?;
        let progress = std::rc::Rc::new(NavigationProgress::default());
        // SAFETY: handlers are registered against live WebView2 objects on the UI/COM
        // apartment thread. They enqueue owned events; no COM object crosses threads.
        unsafe {
            let core = view.webview();
            let mut token = 0;
            let starting = progress.clone();
            let starting_proxy = proxy.clone();
            core.add_NavigationStarting(
                &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        let mut cancelled = BOOL::default();
                        args.Cancel(&mut cancelled)?;
                        // Wry's navigation policy runs first. Keep its blocked-page error.
                        if !cancelled.as_bool() {
                            let mut navigation_id = 0;
                            args.NavigationId(&mut navigation_id)?;
                            let mut uri = windows::core::PWSTR::null();
                            args.Uri(&mut uri)?;
                            let uri = webview2_com::take_pwstr(uri);
                            #[cfg(test)]
                            if std::env::var_os("RUST_BROWSER_TLS_FIXTURE").is_some() {
                                eprintln!("Controlled start tab={id} navigation={navigation_id} blank={} current={:?} finished={} awaiting={}",uri == "about:blank",starting.current.get(),starting.finished.get(),starting.awaiting_web_start.get());
                            }
                            if starting.start_document(navigation_id, &uri) {
                                let _ =
                                    starting_proxy.send_event(Event::Page(id, PageEvent::Loading));
                            }
                        }
                    }
                    Ok(())
                })),
                &mut token,
            )?;
            let external: ICoreWebView2_18 = core.cast()?;
            external.add_LaunchingExternalUriScheme(
                &LaunchingExternalUriSchemeEventHandler::create(Box::new(|_, args| {
                    if let Some(args) = args {
                        args.SetCancel(true)?;
                    }
                    Ok(())
                })),
                &mut token,
            )?;
            let p = proxy.clone();
            core.add_SourceChanged(
                &SourceChangedEventHandler::create(Box::new(move |_, _| {
                    let _ = p.send_event(Event::Page(id, PageEvent::Changed));
                    Ok(())
                })),
                &mut token,
            )?;
            let p = proxy.clone();
            core.add_HistoryChanged(
                &HistoryChangedEventHandler::create(Box::new(move |_, _| {
                    let _ = p.send_event(Event::Page(id, PageEvent::Changed));
                    Ok(())
                })),
                &mut token,
            )?;
            let completed_proxy = proxy.clone();
            let completion = progress.clone();
            core.add_NavigationCompleted(
                &NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                    if let Some(args) = args {
                        let mut navigation_id = 0;
                        args.NavigationId(&mut navigation_id)?;
                        #[cfg(test)]
                        if std::env::var_os("RUST_BROWSER_TLS_FIXTURE").is_some() {
                            let mut success = BOOL::default();
                            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                            let mut http = 0;
                            args.IsSuccess(&mut success)?;
                            args.WebErrorStatus(&mut status)?;
                            args.cast::<ICoreWebView2NavigationCompletedEventArgs2>()?.HttpStatusCode(&mut http)?;
                            eprintln!("Controlled complete tab={id} navigation={navigation_id} success={} status={} http={http} current={:?} finished={} stopped={:?}",success.as_bool(),status.0,completion.current.get(),completion.finished.get(),completion.stopped.get());
                        }
                        if !completion.finish(navigation_id) {
                            return Ok(());
                        }
                        let mut success = BOOL::default();
                        args.IsSuccess(&mut success)?;
                        let error = if success.as_bool() {
                            None
                        } else {
                            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                            args.WebErrorStatus(&mut status)?;
                            if completion.cancelled(navigation_id, status) {
                                let _ = completed_proxy
                                    .send_event(Event::Page(id, PageEvent::Cancelled));
                                return Ok(());
                            }
                            Some(format!(
                                "{} (WebView2 status {}).",
                                navigation_error(status),
                                status.0
                            ))
                        };
                        let mut url = String::new();
                        let mut title = String::new();
                        if let Some(sender) = sender {
                            let mut source = windows::core::PWSTR::null();
                            sender.Source(&mut source)?;
                            url = webview2_com::take_pwstr(source);
                            let mut value = windows::core::PWSTR::null();
                            sender.DocumentTitle(&mut value)?;
                            title = webview2_com::take_pwstr(value);
                        }
                        let _ = completed_proxy.send_event(Event::Page(
                            id,
                            PageEvent::Completed {
                                error,
                                navigation_id,
                                url,
                                title,
                            },
                        ));
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        }
        let activity = std::rc::Rc::new(crate::native_activity::Activity::default());
        crate::native_activity::install(&view, activity.clone(), proxy.clone(), id)?;
        if !related {
            crate::native_activity::watch_resources(&view, activity.clone())?;
        }
        Ok(Self {
            view,
            activity,
            progress,
        })
    }
}

impl ContentEngine for Engine {
    fn integration(&self) -> HostIntegration {
        HostIntegration {
            presentation: Presentation::NativeChild,
            input: InputRouting::Native,
        }
    }
    fn load(&self, target: &LoadTarget) -> EngineResult<()> {
        let previous = self.progress.supersede();
        self.progress.expect(&previous, target.as_str());
        if let Err(error) = self.view.load_url(target.as_str()) {
            self.progress.restore_rejected(previous);
            return Err(error.into());
        }
        Ok(())
    }
    fn set_visible(&self, visible: bool) -> EngineResult<()> {
        self.view.set_visible(visible)?;
        Ok(())
    }
    fn focus(&self) -> EngineResult<()> {
        self.view.focus()?;
        Ok(())
    }
    fn history(&self) -> (bool, bool) {
        // SAFETY: UI thread owns a live WebView2 COM object; outputs are valid BOOLs.
        unsafe {
            let mut back = BOOL::default();
            let mut forward = BOOL::default();
            let core = self.view.webview();
            let _ = core.CanGoBack(&mut back);
            let _ = core.CanGoForward(&mut forward);
            (back.as_bool(), forward.as_bool())
        }
    }

    fn navigate(&self, command: NavigationCommand) -> EngineResult<()> {
        // Native history preserves redirects, POST navigation and same-document entries.
        // SAFETY: all calls happen on the event-loop/COM apartment thread.
        unsafe {
            let core = self.view.webview();
            let previous = (command != NavigationCommand::Stop).then(|| self.progress.supersede());
            let result = match command {
                NavigationCommand::Back => core.GoBack(),
                NavigationCommand::Forward => core.GoForward(),
                NavigationCommand::Reload => core.Reload(),
                NavigationCommand::Stop => {
                    let previous = self.progress.stopped.replace(self.progress.current.get());
                    let result = core.Stop();
                    if result.is_err() {
                        self.progress.stopped.set(previous);
                    }
                    result
                }
            };
            if let Err(error) = result {
                if let Some(previous) = previous {
                    self.progress.restore_rejected(previous);
                }
                return Err(error.into());
            }
        }
        Ok(())
    }
}

/// Register on both controllers: embedded webviews consume keys before winit sees them.
pub fn accelerators(view: &WebView, proxy: EventLoopProxy<Event>) -> windows::core::Result<()> {
    // SAFETY: registration and callback execute in the owning UI apartment.
    unsafe {
        let mut token = 0;
        view.controller().add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                    args.KeyEventKind(&mut kind)?;
                    if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                        && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                    {
                        return Ok(());
                    }
                    let mut key = 0;
                    args.VirtualKey(&mut key)?;
                    let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                    let alt = GetKeyState(VK_MENU.0 as i32) < 0;
                    let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                    let command = crate::ui::shortcut(key, ctrl, alt, shift);
                    if let Some(command) = command {
                        // Escape must also reach HTML fullscreen, selects and dialogs.
                        args.SetHandled(key != 0x1b)?;
                        let mut physical = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
                        args.PhysicalKeyStatus(&mut physical)?;
                        if !physical.WasKeyDown.as_bool() || crate::ui::repeatable(&command) {
                            let _ = proxy.send_event(Event::Command(command));
                        }
                    }
                }
                Ok(())
            })),
            &mut token,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deferred_initial_blank_is_not_the_replacement_document() {
        let progress = NavigationProgress::default();
        let previous = progress.supersede();
        progress.awaiting_web_start.set(true);
        assert!(!progress.start_document(1, "about:blank"));
        assert!(!progress.finish(1));
        assert!(progress.start_document(2, "https://example.test/"));
        progress.restore_rejected(previous);
        assert!(progress.finish(2));
        assert!(progress.start_document(3, "about:blank"));
        assert!(progress.finish(3));
    }
    #[test]
    fn late_start_of_superseded_host_load_cannot_become_current() {
        // Regression: load A, then load B before A's NavigationStarting arrives.
        let progress = NavigationProgress::default();
        progress.begin(1);
        assert!(progress.finish(1));
        let first = progress.supersede();
        progress.expect(&first, "http://127.0.0.1:9/slow");
        let second = progress.supersede();
        progress.expect(&second, "http://127.0.0.1:9/modern.html");
        assert!(!progress.start_document(11, "http://127.0.0.1:9/slow"));
        assert!(!progress.finish(11), "stale abort must not be reported");
        // A redirect of the rejected navigation keeps its ID but changes URI.
        assert!(!progress.start_document(11, "http://127.0.0.1:9/elsewhere"));
        assert!(progress.start_document(12, "http://127.0.0.1:9/modern.html"));
        assert!(progress.finish(12));
    }
    #[test]
    fn unmatched_start_while_awaiting_is_accepted_and_never_stalls() {
        let progress = NavigationProgress::default();
        let first = progress.supersede();
        progress.expect(&first, "https://a.test/");
        let second = progress.supersede();
        progress.expect(&second, "https://b.test/");
        // Unknown URI (for example a different normalization): accept as before.
        assert!(progress.start_document(5, "https://other.test/"));
        assert!(progress.finish(5));
        // Once a start is accepted the remembered targets no longer apply.
        assert!(progress.start_document(6, "https://a.test/"));
        assert!(progress.finish(6));
    }
    #[test]
    fn started_previous_load_is_not_remembered_and_equal_targets_are_accepted() {
        let progress = NavigationProgress::default();
        let first = progress.supersede();
        progress.expect(&first, "https://a.test/");
        assert!(progress.start_document(1, "https://a.test/"));
        let second = progress.supersede();
        progress.expect(&second, "https://b.test/");
        // A already started, so a later A start is a new navigation (e.g. page script).
        assert!(progress.start_document(2, "https://a.test/"));
        // Same target twice cannot be distinguished; the start is accepted.
        let third = progress.supersede();
        progress.expect(&third, "https://c.test/");
        let fourth = progress.supersede();
        progress.expect(&fourth, "https://c.test/");
        assert!(progress.start_document(3, "https://c.test/"));
        assert!(progress.finish(3));
    }
    #[test]
    fn rejected_host_load_restores_expected_and_superseded_targets() {
        let progress = NavigationProgress::default();
        let first = progress.supersede();
        progress.expect(&first, "https://a.test/");
        let second = progress.supersede();
        progress.expect(&second, "https://b.test/");
        progress.restore_rejected(second);
        // B was never issued: A is the awaited load again and is accepted.
        assert!(progress.start_document(7, "https://a.test/"));
        assert!(progress.finish(7));
    }
    #[test]
    fn remembered_stale_state_is_bounded() {
        let progress = NavigationProgress::default();
        for n in 0..10 {
            let previous = progress.supersede();
            progress.expect(&previous, &format!("https://site.test/{n}"));
        }
        assert_eq!(progress.superseded.borrow().len(), STALE_LIMIT);
        for id in 0..10 {
            push_bounded(&mut progress.rejected.borrow_mut(), id);
        }
        assert_eq!(*progress.rejected.borrow(), vec![6, 7, 8, 9]);
    }
    #[test]
    fn host_replacement_invalidates_old_completion_before_next_start_and_reject_can_restore() {
        let progress = NavigationProgress::default();
        progress.begin(1);
        let previous = progress.supersede();
        assert!(!progress.finish(1));
        progress.restore_rejected(previous);
        assert!(progress.finish(1));
        progress.begin(2);
        let previous = progress.supersede();
        progress.begin(3);
        progress.restore_rejected(previous);
        assert!(!progress.finish(2));
        assert!(progress.finish(3));
    }
    #[test]
    fn overlapping_navigation_cannot_complete_or_cancel_the_newer_load() {
        let progress = NavigationProgress::default();
        assert!(!progress.finish(1));
        progress.begin(1);
        progress.begin(2);
        assert!(!progress.finish(1));
        progress.begin(2);
        assert!(progress.finish(2));
        assert!(!progress.finish(2));
        progress.begin(3);
        assert!(progress.finish(3));
    }
    #[test]
    fn certificate_and_network_failures_have_distinct_recovery_advice() {
        assert!(
            navigation_error(COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED)
                .contains("certificate")
        );
        assert!(
            navigation_error(COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED).contains("name")
        );
        assert!(navigation_error(COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT).contains("too long"));
    }
    #[test]
    fn only_an_explicit_stop_reclassifies_connection_aborted() {
        let progress = NavigationProgress::default();
        progress.begin(1);
        assert!(!progress.cancelled(1, COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED));
        progress.stopped.set(Some(1));
        assert!(progress.cancelled(1, COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED));
        assert!(!progress.cancelled(1, COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID));
        progress.begin(2);
        assert!(!progress.cancelled(2, COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED));
    }
}
