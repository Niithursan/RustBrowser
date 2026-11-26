//! The shield's engine on Linux: WebKit content filters. An extension's
//! declarative blocking rules are translated (`content_rules`) and compiled by
//! WebKit into a filter kept in the profile (`content-filters`), then added to
//! every normal website view. WebKit applies the filter itself, before a
//! request leaves the browser; it reports nothing back, so there is no count.
//!
//! The safe `webkit2gtk` bindings leave out `WebKitUserContentFilter` and its
//! store, so this module calls the C API through `webkit2gtk::ffi`. Every call
//! runs on the UI thread, which owns GTK; each asynchronous call's callback
//! also runs there, from GTK's main loop.
use gtk::glib::{self, ffi as glib_ffi, gobject_ffi, translate::ToGlibPtr};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::{CString, c_void},
    path::Path,
    ptr::{self, NonNull},
    rc::Rc,
};
use webkit2gtk::{WebViewExt, ffi, gio::ffi as gio_ffi};

/// A compiled filter (a counted WebKit reference).
pub struct Filter(NonNull<ffi::WebKitUserContentFilter>);

impl Clone for Filter {
    fn clone(&self) -> Self {
        // SAFETY: `self.0` is a live filter we hold a reference to; this takes
        // another, released by the clone's `Drop`.
        unsafe { ffi::webkit_user_content_filter_ref(self.0.as_ptr()) };
        Self(self.0)
    }
}

impl Drop for Filter {
    fn drop(&mut self) {
        // SAFETY: releases the one reference this value holds.
        unsafe { ffi::webkit_user_content_filter_unref(self.0.as_ptr()) }
    }
}

/// What a store call reports: the filter, or WebKit's reason.
pub type Done = Box<dyn FnOnce(Result<Filter, String>)>;

/// WebKit's store of compiled filters, in one folder.
pub struct Store(NonNull<ffi::WebKitUserContentFilterStore>);

impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: releases the reference `webkit_user_content_filter_store_new`
        // gave. Calls still running hold their own references.
        unsafe { gobject_ffi::g_object_unref(self.0.as_ptr().cast()) }
    }
}

/// An identifier WebKit stores a filter under. Only extension IDs are used
/// (32 letters `a` to `p`), so it is also a safe file name.
fn identifier(id: &str) -> Option<CString> {
    rust_browser::extension_manifest::valid_id(id)
        .then(|| CString::new(id).ok())
        .flatten()
}

/// Turn a finished call's result into a filter, or WebKit's error message.
///
/// # Safety
/// `filter` and `error` must come from one `*_finish` call, which hands over
/// one reference to each (at most one is non-null).
unsafe fn finished(
    filter: *mut ffi::WebKitUserContentFilter,
    error: *mut glib_ffi::GError,
) -> Result<Filter, String> {
    if !error.is_null() {
        // SAFETY: a non-null error from `*_finish` is ours to free.
        let error: glib::Error = unsafe { glib::translate::from_glib_full(error) };
        return Err(error.message().to_string());
    }
    NonNull::new(filter)
        .map(Filter)
        .ok_or_else(|| "WebKit reported no filter.".to_string())
}

/// Callbacks of `save` and `load`.
///
/// # Safety
/// Called by GIO once per call, with `data` the `Box<Done>` given to it and
/// `source` the store.
unsafe extern "C" fn saved(
    source: *mut gobject_ffi::GObject,
    result: *mut gio_ffi::GAsyncResult,
    data: glib_ffi::gpointer,
) {
    // SAFETY: `data` is the box leaked by `save`, reclaimed exactly once here.
    let done = unsafe { Box::from_raw(data.cast::<Done>()) };
    let mut error = ptr::null_mut();
    // SAFETY: the store and result GIO passes for this call.
    let filter = unsafe {
        ffi::webkit_user_content_filter_store_save_finish(source.cast(), result, &mut error)
    };
    // SAFETY: both come from that one finish call.
    done(unsafe { finished(filter, error) });
}

/// # Safety
/// As `saved`, for `load`.
unsafe extern "C" fn loaded(
    source: *mut gobject_ffi::GObject,
    result: *mut gio_ffi::GAsyncResult,
    data: glib_ffi::gpointer,
) {
    // SAFETY: `data` is the box leaked by `load`, reclaimed exactly once here.
    let done = unsafe { Box::from_raw(data.cast::<Done>()) };
    let mut error = ptr::null_mut();
    // SAFETY: the store and result GIO passes for this call.
    let filter = unsafe {
        ffi::webkit_user_content_filter_store_load_finish(source.cast(), result, &mut error)
    };
    // SAFETY: both come from that one finish call.
    done(unsafe { finished(filter, error) });
}

/// # Safety
/// As `saved`, for `remove`; `data` is a `Box<Box<dyn FnOnce(bool)>>`.
unsafe extern "C" fn removed(
    source: *mut gobject_ffi::GObject,
    result: *mut gio_ffi::GAsyncResult,
    data: glib_ffi::gpointer,
) {
    // SAFETY: `data` is the box leaked by `remove`, reclaimed exactly once here.
    let done = unsafe { Box::from_raw(data.cast::<Box<dyn FnOnce(bool)>>()) };
    let mut error = ptr::null_mut();
    // SAFETY: the store and result GIO passes for this call.
    let ok = unsafe {
        ffi::webkit_user_content_filter_store_remove_finish(source.cast(), result, &mut error)
    };
    if !error.is_null() {
        // SAFETY: a non-null error from the finish call is ours to free.
        unsafe { glib_ffi::g_error_free(error) };
    }
    done(ok != 0);
}

impl Store {
    /// The store in `folder` (WebKit creates it when it first saves).
    pub fn new(folder: &Path) -> Option<Self> {
        let path = CString::new(folder.to_str()?).ok()?;
        // SAFETY: a valid C string, copied by WebKit; returns a new reference.
        NonNull::new(unsafe { ffi::webkit_user_content_filter_store_new(path.as_ptr()) }).map(Self)
    }

    /// Compile `json` (WebKit content-blocker rules) and keep it as `id`,
    /// replacing what was kept before. WebKit compiles on its own thread.
    pub fn save(&self, id: &str, json: String, done: Done) {
        let Some(name) = identifier(id) else {
            return done(Err("Not an extension ID.".into()));
        };
        let bytes = glib::Bytes::from_owned(json.into_bytes());
        let data = Box::into_raw(Box::new(done)).cast::<c_void>();
        // SAFETY: live store, C string and bytes (WebKit takes its own
        // references); `saved` reclaims `data` exactly once.
        unsafe {
            ffi::webkit_user_content_filter_store_save(
                self.0.as_ptr(),
                name.as_ptr(),
                bytes.to_glib_none().0,
                ptr::null_mut(),
                Some(saved),
                data,
            );
        }
    }

    /// The filter kept as `id`, if WebKit still has it and can read it.
    pub fn load(&self, id: &str, done: Done) {
        let Some(name) = identifier(id) else {
            return done(Err("Not an extension ID.".into()));
        };
        let data = Box::into_raw(Box::new(done)).cast::<c_void>();
        // SAFETY: as in `save`; `loaded` reclaims `data` exactly once.
        unsafe {
            ffi::webkit_user_content_filter_store_load(
                self.0.as_ptr(),
                name.as_ptr(),
                ptr::null_mut(),
                Some(loaded),
                data,
            );
        }
    }

    /// Forget the filter kept as `id`.
    pub fn remove(&self, id: &str, done: Box<dyn FnOnce(bool)>) {
        let Some(name) = identifier(id) else {
            return done(false);
        };
        let data = Box::into_raw(Box::new(done)).cast::<c_void>();
        // SAFETY: as in `save`; `removed` reclaims `data` exactly once.
        unsafe {
            ffi::webkit_user_content_filter_store_remove(
                self.0.as_ptr(),
                name.as_ptr(),
                ptr::null_mut(),
                Some(removed),
                data,
            );
        }
    }
}

/// An extension being added: shown once WebKit has compiled its rules.
pub struct Adding {
    pub id: String,
    pub name: String,
    pub version: String,
    pub folder: String,
    pub blocks: bool,
    /// Extension rules used, and those left out.
    pub rules: usize,
    pub skipped: usize,
}

/// What a finished store call delivers, by extension ID.
type Inbox = Rc<RefCell<Vec<(String, Result<Filter, String>)>>>;

/// A browser's shield: the compiled filters it has loaded, and its store
/// calls in progress. Private windows load from the profile's store but never
/// compile or remove (the browser only asks them to load).
pub struct Shield {
    store: Option<Store>,
    /// Loaded filters, by extension ID.
    filters: HashMap<String, Filter>,
    /// Extensions whose filter could neither load nor compile.
    pub failed: HashSet<String>,
    /// Extensions with a load or compile in progress.
    working: HashSet<String>,
    /// The extension being added, from choosing its folder until its filter
    /// is compiled.
    pub adding: Option<Adding>,
    inbox: Inbox,
    /// Wakes the browser when a call finishes.
    notify: Rc<dyn Fn()>,
}

impl Shield {
    /// The shield of a profile whose filters are kept in `folder` (`None` for a
    /// private window).
    pub fn new(folder: Option<&Path>, notify: Rc<dyn Fn()>) -> Self {
        Self {
            store: folder.and_then(Store::new),
            filters: HashMap::new(),
            failed: HashSet::new(),
            working: HashSet::new(),
            adding: None,
            inbox: Rc::default(),
            notify,
        }
    }

    fn deliver(&self, id: &str) -> Done {
        let (inbox, notify, id) = (
            Rc::clone(&self.inbox),
            Rc::clone(&self.notify),
            id.to_string(),
        );
        Box::new(move |result| {
            inbox.borrow_mut().push((id, result));
            notify();
        })
    }

    /// Load the filter kept for `id`, unless it is loaded or on its way.
    pub fn load(&mut self, id: &str) {
        if self.filters.contains_key(id) || !self.working.insert(id.to_string()) {
            return;
        }
        match &self.store {
            Some(store) => store.load(id, self.deliver(id)),
            None => self.deliver(id)(Err("Private windows keep no filters.".into())),
        }
    }

    /// Compile `json` as the filter of `id`, replacing any kept before.
    pub fn compile(&mut self, id: &str, json: String) {
        self.working.insert(id.to_string());
        match &self.store {
            Some(store) => store.save(id, json, self.deliver(id)),
            None => self.deliver(id)(Err("Private windows keep no filters.".into())),
        }
    }

    /// Whether `id`'s filter is loaded.
    pub fn is_loaded(&self, id: &str) -> bool {
        self.filters.contains_key(id)
    }

    /// Forget `id`'s filter, here and in the store.
    pub fn remove(&mut self, id: &str) {
        self.filters.remove(id);
        self.failed.remove(id);
        if let Some(store) = &self.store {
            store.remove(id, Box::new(|_| {}));
        }
    }

    /// Keep what finished store calls delivered: each extension ID with
    /// WebKit's error, if it failed.
    pub fn finished(&mut self) -> Vec<(String, Option<String>)> {
        let done: Vec<_> = self.inbox.borrow_mut().drain(..).collect();
        done.into_iter()
            .map(|(id, result)| {
                self.working.remove(&id);
                match result {
                    Ok(filter) => {
                        self.failed.remove(&id);
                        self.filters.insert(id.clone(), filter);
                        (id, None)
                    }
                    Err(error) => (id, Some(error)),
                }
            })
            .collect()
    }

    /// The loaded filters of the extensions `on` accepts.
    pub fn active<'a>(
        &'a self,
        on: impl Fn(&str) -> bool + 'a,
    ) -> impl Iterator<Item = &'a Filter> {
        self.filters
            .iter()
            .filter(move |(id, _)| on(id))
            .map(|(_, filter)| filter)
    }
}

/// Make `filters` the only content filters of `view`.
pub fn apply<'a>(view: &webkit2gtk::WebView, filters: impl IntoIterator<Item = &'a Filter>) {
    use gtk::glib::object::ObjectType;
    let Some(manager) = view.user_content_manager() else {
        return;
    };
    // SAFETY: a live manager and filters on the UI thread; the manager takes
    // its own reference to each filter it is given.
    unsafe {
        ffi::webkit_user_content_manager_remove_all_filters(manager.as_ptr());
        for filter in filters {
            ffi::webkit_user_content_manager_add_filter(manager.as_ptr(), filter.0.as_ptr());
        }
    }
}

/// The ID of the extension in `folder`, as Chromium gives an unpacked one.
pub fn extension_id(folder: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let digest =
        glib::compute_checksum_for_data(glib::ChecksumType::Sha256, folder.as_os_str().as_bytes())?;
    rust_browser::extension_manifest::id_from_sha256_hex(&digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Run this thread's own GLib context (where the store's callbacks arrive)
    /// until `done` says so, at most `limit`.
    fn wait(limit: Duration, done: impl Fn() -> bool) {
        let context = glib::MainContext::thread_default().expect("a thread-default context");
        let end = std::time::Instant::now() + limit;
        while !done() && std::time::Instant::now() < end {
            if !context.iteration(false) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    /// Run `test` with a GLib context of its own as this thread's default.
    fn on_own_context(test: impl FnOnce()) {
        glib::MainContext::new()
            .with_thread_default(test)
            .expect("context acquired");
    }

    fn store(name: &str) -> (Store, std::path::PathBuf) {
        let folder = std::env::temp_dir().join(format!(
            "rust-browser-filters-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        (Store::new(&folder).unwrap(), folder)
    }

    fn compile(store: &Store, id: &str, json: String) -> Result<Filter, String> {
        let result = Rc::new(RefCell::new(None));
        let slot = Rc::clone(&result);
        store.save(id, json, Box::new(move |r| *slot.borrow_mut() = Some(r)));
        wait(Duration::from_secs(300), || result.borrow().is_some());
        result.borrow_mut().take().expect("WebKit answered")
    }

    const ID: &str = "abcdefghijklmnopabcdefghijklmnop";

    #[test]
    fn ids_match_chromium_and_are_the_only_identifiers() {
        assert!(extension_id(Path::new("/home/me/ubol")).is_some_and(|id| id.len() == 32));
        assert_ne!(
            extension_id(Path::new("/home/me/a")),
            extension_id(Path::new("/home/me/b"))
        );
        assert!(identifier("../x").is_none());
        assert!(identifier(ID).is_some());
    }

    /// WebKit accepts every kind of rule `content_rules` writes, and refuses
    /// what is not a rule list. Needs WebKitGTK (no display).
    #[test]
    fn webkit_compiles_translated_rules() {
        on_own_context(compiles_translated_rules);
    }

    fn compiles_translated_rules() {
        let (store, folder) = store("compile");
        let rules = serde_json::json!([
            {"id": 1, "action": {"type": "block"}, "condition": {"urlFilter": "||ads.test^"}},
            {"id": 2, "action": {"type": "block"}, "condition": {"urlFilter": "/x/*.js^",
                "resourceTypes": ["script", "main_frame", "sub_frame"], "domainType": "thirdParty"}},
            {"id": 3, "action": {"type": "block"}, "condition": {"requestDomains": ["t.test"],
                "initiatorDomains": ["news.test"], "resourceTypes": ["sub_frame", "image"]}},
            {"id": 4, "action": {"type": "allow"}, "condition": {"urlFilter": "|https://ok.test/",
                "excludedInitiatorDomains": ["a.test"], "isUrlFilterCaseSensitive": true}},
            {"id": 5, "priority": 2, "action": {"type": "block"},
                "condition": {"urlFilter": "?ad=(1)+[2]", "resourceTypes": ["main_frame"]}}
        ]);
        let out = rust_browser::content_rules::translate(&[rules]).unwrap();
        assert_eq!(out.skipped, 0);
        compile(&store, ID, out.json).unwrap();
        assert!(compile(&store, ID, "{\"not\": \"rules\"}".into()).is_err());
        // What was saved loads again.
        let result = Rc::new(RefCell::new(None));
        let slot = Rc::clone(&result);
        store.load(ID, Box::new(move |r| *slot.borrow_mut() = Some(r.is_ok())));
        wait(Duration::from_secs(30), || result.borrow().is_some());
        assert_eq!(*result.borrow(), Some(true));
        let _ = std::fs::remove_dir_all(folder);
    }

    /// The real extension compiles (`RUST_BROWSER_TEST_EXTENSION`).
    #[test]
    #[ignore = "needs an unpacked extension folder"]
    fn webkit_compiles_a_real_extension() {
        on_own_context(compiles_a_real_extension);
    }

    fn compiles_a_real_extension() {
        let folder = std::env::var_os("RUST_BROWSER_TEST_EXTENSION").expect("folder");
        let out = rust_browser::content_rules::read_extension(Path::new(&folder)).unwrap();
        let (store, dir) = store("real");
        let started = std::time::Instant::now();
        let compiled = compile(&store, ID, out.json);
        let size: u64 = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0);
        eprintln!(
            "{} rules compiled in {:?}, {} MB kept",
            out.rules,
            started.elapsed(),
            size / 1_000_000
        );
        compiled.unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
