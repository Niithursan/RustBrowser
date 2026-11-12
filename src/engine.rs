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
    AcceleratorKeyPressedEventHandler, FaviconChangedEventHandler, GetFaviconCompletedHandler,
    HistoryChangedEventHandler, LaunchingExternalUriSchemeEventHandler,
    NavigationCompletedEventHandler, NavigationStartingEventHandler, SourceChangedEventHandler,
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
    /// Password saving and autofill for this view; forced off when private.
    pub forms: crate::native_security::Forms,
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
    /// Target of the last reported connection failure. WebView2 follows such a
    /// failure with an engine-initiated navigation to the same address.
    failed_target: std::cell::RefCell<Option<String>>,
    /// That follow-up navigation: no loading feedback, and its abort is not news.
    follow_up: std::cell::Cell<Option<u64>>,
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
        // A host command also ends any pairing with an earlier failure.
        self.failed_target.borrow_mut().take();
        self.follow_up.set(None);
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
        // Only the very next start can be the engine's follow-up to a failure.
        let failed = self.failed_target.borrow_mut().take();
        if self.follow_up.get() == Some(id) {
            // A redirect: this is a real load after all, so report it.
            self.follow_up.set(None);
        } else if !self.awaiting_web_start.get()
            && self.current.get() != Some(id)
            && failed.is_some_and(|target| target == normalized(uri))
        {
            self.begin(id);
            self.follow_up.set(Some(id));
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
    /// Remember a reported connection failure so its follow-up can be recognized.
    fn failed(&self, uri: &str) {
        *self.failed_target.borrow_mut() = Some(normalized(uri));
    }
    /// True when `id` is the engine's follow-up to an already reported failure
    /// and it merely aborted. Any other outcome of that navigation is reported.
    fn follow_up_aborted(&self, id: u64, status: Option<COREWEBVIEW2_WEB_ERROR_STATUS>) -> bool {
        if self.follow_up.get() != Some(id) {
            return false;
        }
        self.follow_up.set(None);
        status == Some(COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED)
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

/// The response code of a navigation that failed only because the server
/// answered with an error status.
fn http_error(status: COREWEBVIEW2_WEB_ERROR_STATUS, http: i32) -> Option<u16> {
    (status == COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN && (400..=599).contains(&http))
        .then_some(http as u16)
}

/// Largest page icon passed to the toolbar.
const MAX_FAVICON: usize = 32 * 1024;

/// Read a PNG icon from WebView2's stream as a data URL, refusing anything that
/// is not a PNG or is larger than `MAX_FAVICON`.
fn favicon_data(stream: &windows::Win32::System::Com::IStream) -> Option<String> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let mut read = 0u32;
        // SAFETY: reads into a local buffer from a stream WebView2 just handed us.
        let result = unsafe {
            stream.Read(
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                Some(&mut read),
            )
        };
        if result.is_err() {
            return None;
        }
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read as usize]);
        if bytes.len() > MAX_FAVICON {
            return None;
        }
    }
    favicon_url(&bytes)
}

/// Largest page still passed to the toolbar while the menu sidebar is open.
const MAX_SNAPSHOT: usize = 6 * 1024 * 1024;

/// Capture what the page shows now as a JPEG and send it as a data URL (or `None`
/// when capturing failed), for the menu sidebar to show behind itself while the
/// live page is hidden. Nothing is written to disk.
pub fn capture_snapshot(
    view: &WebView,
    reply: impl FnOnce(Option<String>) + 'static,
) -> windows::core::Result<()> {
    // SAFETY: an in-memory stream and a capture on a live view, on its UI thread;
    // the completion handler only reads the stream it was given.
    unsafe {
        let stream = windows::Win32::UI::Shell::SHCreateMemStream(None).ok_or_else(|| {
            windows::core::Error::from_hresult(windows::core::HRESULT(0x8007000Eu32 as i32))
        })?;
        let read = stream.clone();
        let mut reply = Some(reply);
        view.webview().CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_JPEG,
            &stream,
            &webview2_com::CapturePreviewCompletedHandler::create(Box::new(move |result| {
                let data = result.ok().and_then(|()| snapshot_data(&read));
                if let Some(reply) = reply.take() {
                    reply(data);
                }
                Ok(())
            })),
        )
    }
}

fn snapshot_data(stream: &windows::Win32::System::Com::IStream) -> Option<String> {
    // SAFETY: rewinds and reads a stream this process created.
    unsafe {
        stream
            .Seek(0, windows::Win32::System::Com::STREAM_SEEK_SET, None)
            .ok()?;
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 65536];
    loop {
        let mut read = 0u32;
        // SAFETY: reads into a local buffer.
        let result = unsafe {
            stream.Read(
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                Some(&mut read),
            )
        };
        if result.is_err() {
            return None;
        }
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read as usize]);
        if bytes.len() > MAX_SNAPSHOT {
            return None;
        }
    }
    snapshot_url(&bytes)
}

/// A data URL for JPEG `bytes`, or `None` when they are empty, too large or not JPEG.
pub fn snapshot_url(bytes: &[u8]) -> Option<String> {
    use base64::Engine as _;
    (!bytes.is_empty() && bytes.len() <= MAX_SNAPSHOT && bytes.starts_with(&[0xFF, 0xD8, 0xFF]))
        .then(|| {
            format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        })
}

/// A data URL for PNG `bytes`, or `None` when they are empty, too large or not PNG.
pub fn favicon_url(bytes: &[u8]) -> Option<String> {
    use base64::Engine as _;
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    (!bytes.is_empty() && bytes.len() <= MAX_FAVICON && bytes.starts_with(PNG)).then(|| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    })
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
            .with_hotkeys_zoom(true)
            .with_clipboard(false)
            .with_incognito(private)
            // Extensions run only in the normal website profile, never in private
            // windows (separate profile) or the toolbar (separate profile, never set).
            .with_browser_extensions_enabled(!private)
            .with_focused(false)
            .with_navigation_handler(move |url| {
                let allowed = url == "about:blank" || navigation::allowed(&url);
                if !allowed {
                    // The current document is untouched: this is not a page failure.
                    let _ = nav_proxy.send_event(Event::NavigationBlocked(id));
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
        // Private windows never save passwords or form data, whatever the switches.
        let forms = if private {
            crate::native_security::Forms::default()
        } else {
            options.forms
        };
        crate::native_security::apply_forms(&view, forms)?;
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
            // Tab icons: WebView2 decodes the page's icon; only bounded PNG bytes
            // reach the toolbar, which never loads anything from the website.
            let p = proxy.clone();
            core.cast::<ICoreWebView2_15>()?.add_FaviconChanged(
                &FaviconChangedEventHandler::create(Box::new(move |sender, _| {
                    let Some(sender) = sender else { return Ok(()) };
                    let view: ICoreWebView2_15 = sender.cast()?;
                    let p = p.clone();
                    view.GetFavicon(
                        COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG,
                        &GetFaviconCompletedHandler::create(Box::new(move |result, stream| {
                            let data = result
                                .ok()
                                .and(stream)
                                .and_then(|stream| favicon_data(&stream))
                                .unwrap_or_default();
                            let _ = p.send_event(Event::Page(id, PageEvent::Favicon(data)));
                            Ok(())
                        })),
                    )
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
                        let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                        let mut http = 0;
                        if !success.as_bool() {
                            args.WebErrorStatus(&mut status)?;
                            if let Ok(extended) =
                                args.cast::<ICoreWebView2NavigationCompletedEventArgs2>()
                            {
                                extended.HttpStatusCode(&mut http)?;
                            }
                        }
                        if completion
                            .follow_up_aborted(navigation_id, (!success.as_bool()).then_some(status))
                        {
                            return Ok(());
                        }
                        // WebView2 reports 4xx/5xx responses as unsuccessful with an
                        // unknown web error. The server answered: not a connection failure.
                        let http_status = http_error(status, http);
                        let error = if success.as_bool() || http_status.is_some() {
                            None
                        } else {
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
                        if error.is_some() {
                            completion.failed(&url);
                        }
                        let _ = completed_proxy.send_event(Event::Page(
                            id,
                            PageEvent::Completed {
                                error,
                                http_status,
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
        // Observe user zoom and engine navigation changes. Programmatic changes
        // render through the command handler instead of receiving this event.
        let zoom_proxy = proxy.clone();
        // SAFETY: registration and callbacks run in this content view's UI apartment.
        unsafe {
            let mut token = 0;
            view.controller().add_ZoomFactorChanged(
                &webview2_com::ZoomFactorChangedEventHandler::create(Box::new(move |_, _| {
                    let _ = zoom_proxy.send_event(Event::ZoomChanged(id));
                    Ok(())
                })),
                &mut token,
            )?;
        }
        Ok(Self {
            view,
            activity,
            progress,
        })
    }
}

/// Page operations are methods of a content engine, never of the trusted toolbar.
impl Engine {
    pub fn zoom_factor(&self) -> windows::core::Result<f64> {
        let mut factor = 1.0;
        // SAFETY: property of this content controller in its owning apartment.
        unsafe {
            self.view.controller().ZoomFactor(&mut factor)?;
        }
        Ok(factor)
    }

    pub fn zoom_step(&self, increase: bool) -> Result<(), Box<dyn std::error::Error>> {
        self.view.zoom(next_zoom(self.zoom_factor()?, increase))?;
        Ok(())
    }

    pub fn find_in_page(
        &self,
        term: &str,
        proxy: EventLoopProxy<Event>,
        id: TabId,
    ) -> windows::core::Result<()> {
        // SAFETY: content-only, UI-thread COM calls. The UI passes an empty term to open the
        // engine's own dialog; no page script, bridge or host object is introduced.
        unsafe {
            let core = self.view.webview();
            let find = core.cast::<ICoreWebView2_28>()?.Find()?;
            let environment = core.cast::<ICoreWebView2_2>()?.Environment()?;
            let options = environment
                .cast::<ICoreWebView2Environment15>()?
                .CreateFindOptions()?;
            options.SetFindTerm(&windows::core::HSTRING::from(term))?;
            options.SetSuppressDefaultFindDialog(false)?;
            self.view
                .controller()
                .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)?;
            find.Start(
                &options,
                &webview2_com::FindStartCompletedHandler::create(Box::new(move |result| {
                    let _ = proxy.send_event(Event::FindResult(id, result.is_ok()));
                    Ok(())
                })),
            )
        }
    }

    pub fn print_preview(&self) -> windows::core::Result<()> {
        // SAFETY: only this live content view, using the engine's browser preview.
        // The user chooses output and confirms it; no shell, file write or silent print.
        unsafe {
            // More starts in the toolbar. Give the page ownership of keyboard
            // input so the native preview, including Escape, is controllable.
            self.view
                .controller()
                .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)?;
            self.view
                .webview()
                .cast::<ICoreWebView2_16>()?
                .ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_BROWSER)
        }
    }
}

fn next_zoom(current: f64, increase: bool) -> f64 {
    const LEVELS: [f64; 17] = [
        0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0,
    ];
    if !current.is_finite() || current <= 0.0 {
        return 1.0;
    }
    if increase {
        LEVELS
            .into_iter()
            .find(|level| *level > current + 0.001)
            .unwrap_or(5.0)
    } else {
        LEVELS
            .into_iter()
            .rev()
            .find(|level| *level < current - 0.001)
            .unwrap_or(0.25)
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
    fn zoom_steps_bound_and_normalize_wheel_values() {
        assert_eq!(next_zoom(1.0, true), 1.1);
        assert_eq!(next_zoom(1.0, false), 0.9);
        assert_eq!(next_zoom(1.37, true), 1.5);
        assert_eq!(next_zoom(1.37, false), 1.25);
        assert_eq!(next_zoom(5.0, true), 5.0);
        assert_eq!(next_zoom(0.25, false), 0.25);
        for invalid in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
            assert_eq!(next_zoom(invalid, true), 1.0);
        }
    }
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
    fn engine_follow_up_to_a_failure_is_silent_and_its_abort_is_not_reported() {
        // Regression: an unresolved host reported status 13, then WebView2 started
        // a second navigation to the same address that aborted with status 9 and
        // replaced the real reason.
        let aborted = Some(COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED);
        let progress = NavigationProgress::default();
        let first = progress.supersede();
        progress.expect(&first, "https://missing.invalid/");
        assert!(progress.start_document(5, "https://missing.invalid/"));
        assert!(progress.finish(5));
        assert!(!progress.follow_up_aborted(
            5,
            Some(COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED)
        ));
        progress.failed("https://missing.invalid/");
        assert!(!progress.start_document(6, "https://missing.invalid/"));
        assert!(progress.finish(6));
        assert!(progress.follow_up_aborted(6, aborted));
        // The pairing is consumed: a later retry of the same address is ordinary.
        assert!(progress.start_document(7, "https://missing.invalid/"));
        assert!(progress.finish(7));
        assert!(!progress.follow_up_aborted(7, aborted));
    }
    #[test]
    fn follow_up_that_loads_or_fails_differently_is_still_reported() {
        let aborted = Some(COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED);
        let progress = NavigationProgress::default();
        progress.failed("https://flaky.test/");
        assert!(!progress.start_document(2, "https://flaky.test/"));
        assert!(progress.finish(2));
        assert!(
            !progress.follow_up_aborted(2, None),
            "a successful follow-up must be shown"
        );
        progress.failed("https://flaky.test/");
        assert!(!progress.start_document(3, "https://flaky.test/"));
        assert!(!progress.follow_up_aborted(3, Some(COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT)));
        // A redirect of the follow-up is a real load and regains loading feedback.
        progress.failed("https://flaky.test/");
        assert!(!progress.start_document(4, "https://flaky.test/"));
        assert!(progress.start_document(4, "https://flaky.test/elsewhere"));
        assert!(!progress.follow_up_aborted(4, aborted));
    }
    #[test]
    fn only_the_next_engine_start_for_the_failed_address_is_a_follow_up() {
        let progress = NavigationProgress::default();
        progress.failed("https://a.test/");
        // A different address consumes the pairing.
        assert!(progress.start_document(2, "https://b.test/"));
        assert!(progress.start_document(3, "https://a.test/"));
        // A host command (Reload, new address) ends it as well.
        progress.failed("https://a.test/");
        let previous = progress.supersede();
        progress.expect(&previous, "https://a.test/");
        assert!(progress.start_document(4, "https://a.test/"));
        progress.failed("https://a.test/");
        let _ = progress.supersede();
        assert!(progress.start_document(5, "https://a.test/"));
    }
    #[test]
    fn server_error_responses_are_not_connection_failures() {
        let unknown = COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN;
        assert_eq!(http_error(unknown, 404), Some(404));
        assert_eq!(http_error(unknown, 503), Some(503));
        assert_eq!(http_error(unknown, 0), None);
        assert_eq!(http_error(unknown, 200), None);
        // A real transport or certificate failure keeps its own diagnosis.
        assert_eq!(
            http_error(COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED, 404),
            None
        );
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

#[cfg(test)]
mod favicon_tests {
    use super::*;

    #[test]
    fn only_bounded_png_icons_reach_the_toolbar() {
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        let url = favicon_url(&png).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(favicon_url(b"").is_none());
        assert!(favicon_url(b"GIF89a....").is_none());
        assert!(favicon_url(b"<svg onload=alert(1)>").is_none());
        let mut big = png.clone();
        big.resize(MAX_FAVICON + 1, 0);
        assert!(favicon_url(&big).is_none());
    }

    #[test]
    fn only_bounded_jpeg_snapshots_reach_the_toolbar() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
        assert!(
            snapshot_url(&jpeg)
                .unwrap()
                .starts_with("data:image/jpeg;base64,")
        );
        assert!(snapshot_url(b"").is_none());
        assert!(snapshot_url(b"\x89PNG\r\n\x1a\n").is_none());
        assert!(snapshot_url(b"<script>").is_none());
        let mut big = jpeg.to_vec();
        big.resize(MAX_SNAPSHOT + 1, 0);
        assert!(snapshot_url(&big).is_none());
    }
}
