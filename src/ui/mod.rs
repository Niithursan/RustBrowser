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
    /// Open Windows Default apps for this browser. Windows asks the user to choose.
    OpenDefaultApps,
    /// Address-field menu: put this selected text on the clipboard.
    CopyText(String),
    /// Address-field menu: insert the clipboard's text at the selection.
    PasteAddress,
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
    /// Open another normal window (Ctrl+N, More > New window).
    NewWindow,
    /// Reopen the most recently closed window of this session.
    ReopenClosedWindow,
    /// Close every window and keep them all for the next start.
    ExitBrowser,
    SetRestoreSession(bool),
    /// Bookmarks > Import from file: the browser asks for the file natively.
    ImportBookmarks,
    /// Privacy & security > Extensions > Add from folder (native folder dialog).
    AddExtension,
    /// Turn an extension the user added on or off.
    SetExtensionEnabled {
        id: String,
        enabled: bool,
    },
    /// Remove an extension the user added.
    RemoveExtension(String),
    /// Privacy & security: "Offer to save passwords".
    SetSavePasswords(bool),
    /// Privacy & security: "Fill in forms automatically".
    SetAutofill(bool),
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
    FindInPage,
    PrintPage,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    Ready,
    NewTab,
    CloseActive,
    CloseTab(TabId),
    SelectTab(TabId),
    CycleTab(bool),
    /// Select by position; any position past the end selects the last tab.
    SelectIndex(usize),
    ReopenClosedTab,
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

/// One extension in Privacy & security > Extensions.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ExtensionRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub folder: String,
    /// "on", "off" or "missing" (no longer installed in the profile).
    pub state: &'static str,
}

#[derive(Serialize)]
pub struct ToolbarState<'a> {
    pub zoom_percent: Option<u32>,
    pub page_tools: bool,
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
    /// Extensions the user added, with their current state.
    pub extensions: Vec<ExtensionRow>,
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
        (0x21, true, _) => Some(Command::CycleTab(true)),
        (0x22, true, _) => Some(Command::CycleTab(false)),
        (0x54, true, _) if shift => Some(Command::ReopenClosedTab),
        // Ctrl+1..8 select that tab; Ctrl+9 selects the last one.
        (0x31..=0x38, true, _) if !shift => Some(Command::SelectIndex((key - 0x31) as usize)),
        (0x39, true, _) if !shift => Some(Command::SelectIndex(usize::MAX)),
        (0x73, true, _) => Some(Command::CloseActive),
        (0x24, _, true) => Some(Command::Home),
        (0x50, true, _) if shift => Some(Command::PinActive),
        (0x46, true, _) => Some(Command::FindInPage),
        (0x50, true, _) => Some(Command::PrintPage),
        (0xbb | 0x6b, true, _) => Some(Command::ZoomIn),
        (0xbd | 0x6d, true, _) => Some(Command::ZoomOut),
        (0x30 | 0x60, true, _) if !shift => Some(Command::ResetZoom),
        (0x4e, true, _) if shift => Some(Command::NewPrivateWindow),
        (0x4e, true, _) => Some(Command::NewWindow),
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
        Command::CycleTab(_)
            | Command::FocusAddress
            | Command::FocusPage
            | Command::FocusTabs
            | Command::ZoomIn
            | Command::ZoomOut
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
        assert!(matches!(
            shortcut(0x46, true, false, false),
            Some(Command::FindInPage)
        ));
    }
    #[test]
    fn everyday_shortcuts_preserve_pin_altgr_and_text_input() {
        assert!(matches!(
            shortcut(0x50, true, false, false),
            Some(Command::PrintPage)
        ));
        assert!(matches!(
            shortcut(0x50, true, false, true),
            Some(Command::PinActive)
        ));
        for key in [0xbb, 0x6b] {
            assert!(matches!(
                shortcut(key, true, false, false),
                Some(Command::ZoomIn)
            ));
        }
        for key in [0xbd, 0x6d] {
            assert!(matches!(
                shortcut(key, true, false, false),
                Some(Command::ZoomOut)
            ));
        }
        for key in [0x30, 0x60] {
            assert!(matches!(
                shortcut(key, true, false, false),
                Some(Command::ResetZoom)
            ));
        }
        for key in [0x46, 0x50, 0xbb, 0xbd, 0x30, 0x6b, 0x6d] {
            assert!(shortcut(key, false, false, false).is_none());
            assert!(shortcut(key, true, true, false).is_none());
        }
        assert!(!repeatable(&Command::PrintPage));
        assert!(!repeatable(&Command::FindInPage));
        assert!(repeatable(&Command::ZoomIn));
        assert!(repeatable(&Command::ZoomOut));
    }
    #[test]
    fn standard_tab_shortcuts_match_other_windows_browsers() {
        assert!(matches!(
            shortcut(0x54, true, false, true),
            Some(Command::ReopenClosedTab)
        ));
        assert!(matches!(
            shortcut(0x31, true, false, false),
            Some(Command::SelectIndex(0))
        ));
        assert!(matches!(
            shortcut(0x38, true, false, false),
            Some(Command::SelectIndex(7))
        ));
        assert!(matches!(
            shortcut(0x39, true, false, false),
            Some(Command::SelectIndex(usize::MAX))
        ));
        assert!(matches!(
            shortcut(0x22, true, false, false),
            Some(Command::CycleTab(false))
        ));
        assert!(matches!(
            shortcut(0x21, true, false, false),
            Some(Command::CycleTab(true))
        ));
        // Ctrl+Shift+PageUp/PageDown still move the tab instead of switching.
        assert!(matches!(
            shortcut(0x21, true, false, true),
            Some(Command::MoveActive(true))
        ));
        assert!(matches!(
            shortcut(0x73, true, false, false),
            Some(Command::CloseActive)
        ));
        assert!(matches!(
            shortcut(0x24, false, true, false),
            Some(Command::Home)
        ));
        // Plain digits, Home and F4 stay with the page; AltGr digits type symbols.
        for key in [0x31, 0x39, 0x24, 0x73] {
            assert!(shortcut(key, false, false, false).is_none());
        }
        assert!(shortcut(0x32, true, true, false).is_none());
        assert!(!repeatable(&Command::ReopenClosedTab));
    }
}
