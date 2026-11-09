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
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(15..=3600).contains(&self.suspend_after_seconds) || self.favorites.len() > 12 {
            return Err("Invalid background timer or too many favorites.".into());
        }
        let mut urls = std::collections::HashSet::new();
        for item in &self.favorites {
            if item.title.trim().is_empty()
                || item.title.chars().count() > 60
                || item.title.chars().any(char::is_control)
                || item.url.len() > 8192
                || crate::navigation::address(&item.url).as_deref() != Ok(item.url.as_str())
                || !urls.insert(&item.url)
            {
                return Err("Invalid or duplicate favorite.".into());
            }
        }
        Ok(())
    }
}
