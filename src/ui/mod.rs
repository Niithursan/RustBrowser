use rust_browser::{settings::SearchEngine, tabs::TabId};
use serde::{Deserialize, Serialize};

pub const HEIGHT: f64 = 136.0;
pub const HTML: &str = include_str!("toolbar.html");

#[derive(Debug, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
pub enum Command {
    Navigate(String),
    Back,
    Forward,
    Reload,
    Stop,
    FocusAddress,
    FocusPage,
    Home,
    Ready,
    NewTab,
    CloseActive,
    CloseTab(TabId),
    SelectTab(TabId),
    CycleTab(bool),
    SetSearchEngine(SearchEngine),
}

#[derive(Default, Serialize)]
pub struct State {
    pub url: String,
    pub loading: bool,
    pub back: bool,
    pub forward: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct TabState<'a> {
    pub id: TabId,
    pub title: &'a str,
    pub active: bool,
}

#[derive(Serialize)]
pub struct ToolbarState<'a> {
    #[serde(flatten)]
    pub page: &'a State,
    pub tabs: Vec<TabState<'a>>,
    pub active_id: TabId,
    pub search_engine: SearchEngine,
    pub notice: &'a str,
}

/// One shortcut map for WebView2 controllers and the parent window's focus fallback.
pub fn shortcut(key: u32, ctrl: bool, alt: bool, shift: bool) -> Option<Command> {
    match (key, ctrl, alt) {
        (0x4c, true, _) | (0x44, _, true) | (0x75, _, _) => Some(Command::FocusAddress),
        (0x52, true, _) | (0x74, _, _) => Some(Command::Reload),
        (0x25, _, true) => Some(Command::Back),
        (0x27, _, true) => Some(Command::Forward),
        (0x1b, _, _) => Some(Command::Stop),
        (0x54, true, _) => Some(Command::NewTab),
        (0x57, true, _) => Some(Command::CloseActive),
        (0x09, true, _) => Some(Command::CycleTab(shift)),
        (0x76, _, _) => Some(Command::FocusPage),
        _ => None,
    }
}
