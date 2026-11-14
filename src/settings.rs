use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchEngine {
    #[default]
    Google,
    DuckDuckGo,
    Bing,
}

impl SearchEngine {
    pub fn search(self, query: &str) -> String {
        let base = match self {
            Self::Google => "https://www.google.com/search",
            Self::DuckDuckGo => "https://duckduckgo.com/",
            Self::Bing => "https://www.bing.com/search",
        };
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("q", query)
            .finish();
        format!("{base}?{query}")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub search_engine: SearchEngine,
    pub history_enabled: bool,
    pub restore_session: bool,
    pub automatic_suspension: bool,
    pub suspend_after_seconds: u64,
    pub theme: Theme,
    pub favorites: Vec<Favorite>,
    pub show_recent: bool,
    /// Let WebView2 offer to save passwords and fill them in (v0.19, opt-in).
    /// Missing from older files, so existing profiles start with it off.
    pub save_passwords: bool,
    /// Let WebView2 fill in forms (addresses and similar) automatically (v0.19,
    /// opt-in). Off by default even though WebView2's own default is on.
    pub autofill: bool,
    /// Set once the toolbar profile's leftover form data (WebView2 turned autofill
    /// on in the toolbar before 0.19) has been cleared. Missing from older files.
    pub toolbar_autofill_cleared: bool,
    /// Extensions the user added from a folder (v0.20). WebView2 keeps whether each
    /// is installed and enabled; this list only says which ones are the user's.
    pub extensions: Vec<SavedExtension>,
    /// Show tabs as a list in a column at the left of every window (v0.21).
    /// Missing from older files, so existing profiles keep the tab strip.
    pub vertical_tabs: bool,
    /// The new-tab page's background, drawn from the theme's own colours (v0.21).
    pub newtab_background: Background,
    /// The shield counts requests extensions block (v0.21). On when missing; off
    /// removes the network listener from every page.
    pub count_blocked: bool,
}

/// New-tab page backgrounds. Each is drawn by the page from the current theme's
/// colours; no image is stored or downloaded.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Background {
    /// From the tab strip's colour at the top to the surface.
    Dusk,
    /// A soft glow of the theme's accent at the top.
    Glow,
    /// From the panel colour to the card colour, corner to corner.
    Panels,
    /// The theme's surface colour (also used for names a later version adds;
    /// serde needs this catch-all to be the last variant).
    #[default]
    #[serde(other)]
    Plain,
}

/// Largest favourite icon kept, as its `data:image/png;base64,...` text.
pub const MAX_FAVORITE_ICON: usize = 8 * 1024;

/// Whether `icon` is a favourite icon the toolbar may show: a small PNG data URL.
pub fn valid_icon(icon: &str) -> bool {
    icon.len() <= MAX_FAVORITE_ICON
        && icon
            .strip_prefix("data:image/png;base64,")
            .is_some_and(|data| {
                !data.is_empty()
                    && data
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
            })
}

/// Whether two web addresses are on the same site for a favourite's icon: the
/// same host, ignoring a leading `www.`.
pub fn same_site(a: &str, b: &str) -> bool {
    let host = |address: &str| {
        url::Url::parse(address).ok().and_then(|u| {
            matches!(u.scheme(), "http" | "https")
                .then(|| {
                    u.host_str()
                        .map(|h| h.trim_start_matches("www.").to_owned())
                })
                .flatten()
        })
    };
    matches!((host(a), host(b)), (Some(x), Some(y)) if !x.is_empty() && x == y)
}

/// Most extensions a profile may add.
pub const MAX_EXTENSIONS: usize = 16;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SavedExtension {
    /// WebView2's extension ID (32 letters a to p).
    pub id: String,
    pub name: String,
    pub version: String,
    /// The folder it was added from, shown so the user knows what was added.
    pub folder: String,
    /// Whether its manifest asks to block or redirect requests (v0.21). Missing
    /// for extensions added before 0.21 until their folder is read again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            search_engine: SearchEngine::default(),
            history_enabled: true,
            restore_session: true,
            automatic_suspension: true,
            suspend_after_seconds: 120,
            theme: Theme::Dark,
            favorites: Vec::new(),
            show_recent: false,
            save_passwords: false,
            autofill: false,
            toolbar_autofill_cleared: false,
            extensions: Vec::new(),
            vertical_tabs: false,
            newtab_background: Background::Plain,
            count_blocked: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Slate,
    Plum,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Favorite {
    pub title: String,
    pub url: String,
    /// The site's icon, saved the last time a tab showed it (v0.21); empty for a
    /// letter tile. Left out of the file when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(15..=3600).contains(&self.suspend_after_seconds) || self.favorites.len() > 12 {
            return Err("Invalid background timer or too many favorites.".into());
        }
        if self.extensions.len() > MAX_EXTENSIONS
            || self.extensions.iter().any(|e| {
                e.id.len() != 32
                    || !e.id.bytes().all(|b| (b'a'..=b'p').contains(&b))
                    || e.name.chars().count() > 80
                    || e.version.chars().count() > 32
                    || e.folder.len() > 1024
                    || [&e.name, &e.version, &e.folder]
                        .iter()
                        .any(|t| t.chars().any(char::is_control))
            })
        {
            return Err("Invalid saved extensions.".into());
        }
        let mut urls = std::collections::HashSet::new();
        for item in &self.favorites {
            if item.title.trim().is_empty()
                || item.title.chars().count() > 60
                || item.title.chars().any(char::is_control)
                || item.url.len() > 8192
                || crate::navigation::address(&item.url).as_deref() != Ok(item.url.as_str())
                || !urls.insert(&item.url)
                || (!item.icon.is_empty() && !valid_icon(&item.icon))
            {
                return Err("Invalid or duplicate favorite.".into());
            }
        }
        Ok(())
    }
}
