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
    RefreshDiagnostics,
    DismissNotice,
    ClearDiagnostics {
        confirmed: bool,
    },
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
    pub diagnostics: Option<rust_browser::diagnostics::Report>,
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
    // Windows reports AltGr text entry as Ctrl+Alt. Leave those keys to the page.
    if ctrl && alt {
        return None;
    }
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

pub fn repeatable(command: &Command) -> bool {
    matches!(
        command,
        Command::CycleTab(_) | Command::FocusAddress | Command::FocusPage | Command::FocusTabs
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Custom properties declared in `selector{...}` blocks of the shipped CSS.
    fn tokens(selector: &str) -> std::collections::HashMap<String, String> {
        let mut found = std::collections::HashMap::new();
        let pattern = format!("{selector}{{");
        let mut rest = HTML;
        while let Some(start) = rest.find(&pattern) {
            let block = &rest[start + pattern.len()..];
            let end = block.find('}').unwrap();
            for declaration in block[..end].split(';') {
                if let Some((name, value)) = declaration.split_once(':')
                    && name.trim().starts_with("--")
                {
                    found.insert(name.trim().to_owned(), value.trim().to_owned());
                }
            }
            rest = &block[end..];
        }
        found
    }
    fn luminance(hex: &str) -> f64 {
        let short = hex.trim_start_matches('#');
        let hex: String = if short.len() == 3 {
            short.chars().flat_map(|c| [c, c]).collect()
        } else {
            short.to_owned()
        };
        assert_eq!(hex.len(), 6, "opaque #rgb or #rrggbb token expected: {hex}");
        let channel = |i: usize| {
            let c = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
    }
    fn contrast(a: &str, b: &str) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }
    #[test]
    fn every_theme_meets_wcag_contrast_for_text_and_focus_indicators() {
        let dark = tokens(":root");
        for theme in ["dark", "slate", "plum", "light"] {
            let mut palette = dark.clone();
            palette.extend(tokens(&format!("body[data-theme={theme}]")));
            let token = |name: &str| {
                palette
                    .get(name)
                    .unwrap_or_else(|| panic!("{theme} lacks {name}"))
            };
            for background in ["--surface", "--panel", "--card", "--field"] {
                // WCAG 1.4.3: normal text at least 4.5:1.
                let ratio = contrast(token("--text"), token(background));
                assert!(
                    ratio >= 4.5,
                    "{theme}: --text on {background} is {ratio:.2}"
                );
            }
            for foreground in ["--secondary", "--error"] {
                for background in ["--surface", "--panel"] {
                    let ratio = contrast(token(foreground), token(background));
                    assert!(
                        ratio >= 4.5,
                        "{theme}: {foreground} on {background} is {ratio:.2}"
                    );
                }
            }
            for background in ["--surface", "--panel", "--card"] {
                // WCAG 1.4.11: focus rings and active-tab bar at least 3:1.
                let ratio = contrast(token("--accent"), token(background));
                assert!(
                    ratio >= 3.0,
                    "{theme}: --accent on {background} is {ratio:.2}"
                );
            }
        }
    }
    #[test]
    fn altgr_combinations_do_not_run_browser_commands() {
        for key in [0x44, 0x4c, 0x52, 0x54, 0x57, 0x48, 0x4a] {
            assert!(
                shortcut(key, true, true, false).is_none(),
                "AltGr was intercepted"
            );
        }
    }
    #[test]
    fn repeats_do_not_multiply_tab_actions_and_standard_shortcuts_survive() {
        assert!(!repeatable(&Command::NewTab));
        assert!(!repeatable(&Command::CloseActive));
        assert!(!repeatable(&Command::NewPrivateWindow));
        assert!(!repeatable(&Command::PinActive));
        assert!(repeatable(&Command::CycleTab(false)));
        assert!(matches!(
            shortcut(0x54, true, false, false),
            Some(Command::NewTab)
        ));
        assert!(matches!(
            shortcut(0x4c, true, false, false),
            Some(Command::FocusAddress)
        ));
        assert!(matches!(
            shortcut(0x44, false, true, false),
            Some(Command::FocusAddress)
        ));
        assert!(matches!(
            shortcut(0x09, true, false, true),
            Some(Command::CycleTab(true))
        ));
        assert!(shortcut(0x46, true, false, false).is_none());
    }
}
