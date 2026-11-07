use rust_browser::library::{Action, Data, Filter};
use rust_browser::{settings::SearchEngine, tabs::TabId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    Bookmarks,
    History,
    Downloads,
    Settings,
    NewTab,
}

pub const HEIGHT: f64 = 136.0;
pub const HTML: &str = include_str!("toolbar.html");

#[derive(Debug, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
pub enum Command {
    MoveTab {
        id: TabId,
        to: usize,
    },
    MoveActive(bool),
    PinActive,
    KeepAwake,
    WakeTab,
    SetAutomaticSuspension(bool),
    SetSuspendDelay(u64),
    SetTheme(rust_browser::settings::Theme),
    SetShowRecent(bool),
    AddFavorite {
        title: String,
        url: String,
    },
    RemoveFavorite(usize),
    ToolbarHeight(f64),
    RevokePermission {
        origin: String,
        capability: rust_browser::security::Capability,
        confirmed: bool,
    },
    NewPrivateWindow,
    SetRestoreSession(bool),
    ClearData(rust_browser::privacy::ClearRequest),
    PermissionDecision {
        id: u64,
        decision: rust_browser::security::Decision,
        remember: bool,
    },
    Navigate(String),
    Back,
    Forward,
    Reload,
    Stop,
    FocusAddress,
    FocusTabs,
    FocusPage,
    Home,
    Ready,
    NewTab,
    CloseActive,
    CloseTab(TabId),
    SelectTab(TabId),
    CycleTab(bool),
    SetSearchEngine(SearchEngine),
    BookmarkCurrent,
    ShowPanel(Panel),
    ClosePanel,
    QueryLibrary(Filter),
    Library(Action),
    OpenSaved {
        url: String,
        new_tab: bool,
    },
    SetHistoryEnabled(bool),
    CancelDownload(u64),
    OpenDownloadFolder(u64),
}

#[derive(Default, Serialize)]
pub struct State {
    pub url: String,
    pub loading: bool,
    pub failed: bool,
    pub back: bool,
    pub forward: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct TabState<'a> {
    pub id: TabId,
    pub title: &'a str,
    pub active: bool,
    pub pinned: bool,
    pub keep_awake: bool,
    pub phase: rust_browser::background::Phase,
    pub loading: bool,
}

#[derive(Serialize)]
pub struct ToolbarState<'a> {
    pub settings: &'a rust_browser::settings::Settings,
    pub private: bool,
    pub restore_session: bool,
    pub security: rust_browser::security::Indicator,
    pub permission_prompt: Option<crate::native_security::Prompt>,
    pub busy: bool,
    #[serde(flatten)]
    pub page: &'a State,
    pub tabs: Vec<TabState<'a>>,
    pub active_id: TabId,
    pub search_engine: SearchEngine,
    pub notice: &'a str,
    pub panel: Option<Panel>,
    pub library: &'a Data,
    pub history_enabled: bool,
    pub downloads: Vec<crate::native_downloads::Record>,
}

/// One shortcut map for WebView2 controllers and the parent window's focus fallback.
pub fn shortcut(key: u32, ctrl: bool, alt: bool, shift: bool) -> Option<Command> {
    match (key, ctrl, alt) {
        (0x21, true, _) if shift => Some(Command::MoveActive(true)),
        (0x22, true, _) if shift => Some(Command::MoveActive(false)),
        (0x50, true, _) if shift => Some(Command::PinActive),
        (0x4e, true, _) if shift => Some(Command::NewPrivateWindow),
        (0x44, true, _) => Some(Command::BookmarkCurrent),
        (0x48, true, _) => Some(Command::ShowPanel(Panel::History)),
        (0x4a, true, _) => Some(Command::ShowPanel(Panel::Downloads)),
        (0x4f, true, _) if shift => Some(Command::ShowPanel(Panel::Bookmarks)),
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
