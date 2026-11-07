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
