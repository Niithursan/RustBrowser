//! Small UI-thread content boundary. Construction, native handles and security stay
//! in the adapter; this is not a promise that engines have equivalent capabilities.
use std::{error::Error, fmt};
use url::Url;

pub type EngineResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationCommand {
    Back,
    Forward,
    Reload,
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    NativeChild,
    HostPresented,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputRouting {
    Native,
    Forwarded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostIntegration {
    pub presentation: Presentation,
    pub input: InputRouting,
}

/// A remote location is validated separately from the explicit internal blank page.
/// No constructor accepts file/data/script URLs or the trusted new-tab UI identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebLocation(Url);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidLocation;
impl fmt::Display for InvalidLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("The content engine requires an absolute HTTP(S) URL without credentials.")
    }
}
impl Error for InvalidLocation {}

impl WebLocation {
    pub fn parse(value: &str) -> Result<Self, InvalidLocation> {
        if value.len() > 8192 || value.chars().any(char::is_whitespace) {
            return Err(InvalidLocation);
        }
        let url = Url::parse(value).map_err(|_| InvalidLocation)?;
        if !matches!(url.scheme(), "http" | "https")
            || url.as_str().len() > 8192
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(InvalidLocation);
        }
        Ok(Self(url))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadTarget {
    Web(WebLocation),
    Blank,
}
impl LoadTarget {
    pub fn web(value: &str) -> Result<Self, InvalidLocation> {
        WebLocation::parse(value).map(Self::Web)
    }
    pub fn as_str(&self) -> &str {
        match self {
            Self::Web(location) => location.as_str(),
            Self::Blank => "about:blank",
        }
    }
}

/// Owned adapter events, never COM objects, rendering resources or privileged IPC.
/// `navigation_id` is an opaque sequence scoped to an individual adapter/view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageEvent {
    Loading,
    Cancelled,
    Changed,
    Completed {
        error: Option<String>,
        /// A 4xx/5xx response that still produced a document. The connection worked,
        /// so this is not a navigation failure.
        http_status: Option<u16>,
        navigation_id: u64,
        url: String,
        title: String,
    },
    Error(String),
    Title(String),
    /// The page's icon as a `data:image/png;base64,...` URL read from WebView2,
    /// or empty when the page has none.
    Favicon(String),
}

/// Views remain on their owning UI thread. Drop releases a view before its context
/// and parent surface. Explicit input/presentation drivers are a separate concern:
/// a native child must not receive synthetic duplicates of its native input.
pub trait ContentEngine {
    fn integration(&self) -> HostIntegration;
    fn load(&self, target: &LoadTarget) -> EngineResult<()>;
    fn navigate(&self, command: NavigationCommand) -> EngineResult<()>;
    fn history(&self) -> (bool, bool);
    fn set_visible(&self, visible: bool) -> EngineResult<()>;
    fn focus(&self) -> EngineResult<()>;
}
