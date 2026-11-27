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

/// The toolbar's starting height on Windows (Linux starts from `DEFAULT_CHROME`).
#[cfg(windows)]
pub const HEIGHT: f64 = 136.0;
pub const HTML: &str = include_str!("toolbar.html");

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResizeEdge {
    North,
    NorthWest,
    NorthEast,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
pub enum Command {
    MinimizeWindow,
    ToggleMaximize,
    CloseWindow,
    DragWindow,
    ResizeWindow(ResizeEdge),
    ShowWindowMenu {
        x: f64,
        y: f64,
    },
    RefreshDiagnostics,
    /// Open Windows Default apps for this browser. Windows asks the user to choose.
    OpenDefaultApps,
    /// Address-field menu: put this selected text on the clipboard.
    CopyText(String),
    /// Address-field menu: insert the clipboard's text at the selection.
    PasteAddress,
    /// The address field's text changed: look up suggestions for it.
    Suggest(String),
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
    /// A tab's right-click menu: pin or unpin tab `id` without selecting it.
    PinTab(TabId),
    /// A tab's right-click menu: open tab `id`'s address in a new tab beside it.
    DuplicateTab(TabId),
    /// A tab's right-click menu: close every unpinned tab except `id`.
    CloseOtherTabs(TabId),
    KeepAwake,
    WakeTab,
    SetAutomaticSuspension(bool),
    SetSuspendDelay(u64),
    SetTheme(rust_browser::settings::Theme),
    SetShowRecent(bool),
    /// Show tabs in a column at the left of every window (Settings > Appearance).
    SetVerticalTabs(bool),
    /// The new-tab page's background.
    SetNewtabBackground(rust_browser::settings::Background),
    /// Choose a picture for the new-tab page in a file picker; it becomes the
    /// background once checked and copied into the profile.
    ChooseWallpaper,
    /// The new-tab page shows the user's picture: send it (`wallpaper::script`).
    RequestWallpaper,
    /// Whether the shield counts requests extensions block.
    SetCountBlocked(bool),
    /// Turn the ad blocker that comes with Rust Browser on or off.
    SetBuiltInBlocker(bool),
    /// Linux: download hBlock's list again now.
    UpdateBuiltInBlocker,
    /// Linux: block requests on the selected tab's site (true), or pause it.
    SetSiteBlocking(bool),
    /// Linux: answer the offer to save a submitted login.
    AnswerPasswordOffer {
        id: u64,
        answer: OfferAnswer,
    },
    /// Linux: fill the page's login form with the saved login of this username.
    FillLogin(String),
    /// Linux: fill the page's empty contact fields with saved details.
    FillContact,
    /// Show the bookmarks bar under the address line (Settings > Appearance).
    SetBookmarksBar(bool),
    /// A folder on the bookmarks bar (or inside its dropdown): list its contents
    /// in the dropdown, as `library.menu`.
    OpenBookmarkFolder(i64),
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
    /// Read the list of profiles again (the profile menu or Settings opened).
    RefreshProfiles,
    /// Open profile `id` in its own process, or a new window of it if it is open.
    OpenProfile(String),
    /// Create a profile and open it.
    /// Without a name or colour, the next free ones are used.
    AddProfile {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        color: Option<String>,
    },
    /// Rename a profile or change its colour.
    EditProfile {
        id: String,
        name: String,
        color: String,
    },
    /// Delete another profile and its data (the toolbar asked first).
    DeleteProfile(String),
    /// A dropdown (the menu, the star, the shield, site information or a
    /// bookmarks-bar folder) opened or closed.
    SetMenuOpen(bool),
    /// Where the open dropdown is drawn, in the toolbar view's CSS pixels.
    SetMenuArea {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        /// Corner radius of the dropdown, so the clip follows its rounded shape.
        radius: f64,
    },
    /// The shield was pressed and an extension that blocks requests has a popup:
    /// show that popup with its right edge at `right` and its top at `top`, in
    /// the toolbar view's CSS pixels. Pressed again, it closes.
    OpenExtensionPopup {
        id: String,
        right: f64,
        top: f64,
    },
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
    /// Close the library column beside the page.
    CloseLibrary,
    /// Show or hide the library column (Ctrl+Shift+B).
    ToggleLibrary,
    /// The library column's new width, in logical pixels.
    SetLibraryWidth(f64),
    QueryLibrary(Filter),
    Library(Action),
    OpenSaved {
        url: String,
        new_tab: bool,
    },
    /// A bookmarks-bar folder's menu: open these bookmarks in new tabs, the first
    /// one selected.
    OpenSavedTabs(Vec<String>),
    /// A bookmark's menu: open it in a new window of this profile.
    OpenSavedWindow(String),
    /// A bookmark's menu: open it in a new private window.
    OpenSavedPrivate(String),
    SetHistoryEnabled(bool),
    CancelDownload(u64),
    OpenDownloadFolder(u64),
    /// The find bar (Linux; WebView2 has its own): find `text` in the page, or
    /// move to the next (`forward`) or previous match of the same text.
    #[cfg(target_os = "linux")]
    FindText {
        text: String,
        forward: bool,
    },
    /// The find bar closed: stop highlighting matches.
    #[cfg(target_os = "linux")]
    CloseFind,
}

#[derive(Default, Serialize)]
pub struct State {
    pub url: String,
    pub loading: bool,
    pub failed: bool,
    pub back: bool,
    pub forward: bool,
    pub message: String,
    /// The page icon as a PNG data URL (empty when none); sent per tab instead.
    #[serde(skip)]
    pub favicon: String,
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
    /// PNG data URL of the page icon, or empty.
    pub favicon: &'a str,
}

/// One profile in the profile menu and Settings > Profiles.
#[derive(Serialize)]
pub struct ProfileRow<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub color: &'a str,
    /// This window's profile.
    pub current: bool,
    /// Another profile with a window open (it cannot be deleted until closed).
    pub open: bool,
}

/// How the user answered the offer to save a login.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OfferAnswer {
    Save,
    /// Never offer for this site again.
    Never,
    NotNow,
}

/// The selected tab's site in the password panel.
#[derive(Serialize)]
pub struct PasswordState<'a> {
    pub host: &'a str,
    /// A submitted login waiting for an answer.
    pub offer: Option<PasswordOffer<'a>>,
    /// Usernames saved for this site, to fill.
    pub logins: &'a [String],
    /// Saved contact details can fill a form on this page.
    pub contact: bool,
}

/// A submitted login the browser offers to save. Never its password.
#[derive(Serialize)]
pub struct PasswordOffer<'a> {
    pub id: u64,
    pub username: &'a str,
    /// A password is already saved for this username and differs.
    pub update: bool,
}

/// The site the shield's per-site switch acts on.
#[derive(Serialize)]
pub struct ShieldSite<'a> {
    pub host: &'a str,
    pub paused: bool,
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
    /// Whether it asks to block requests; `None` when Rust Browser cannot tell.
    pub blocks: Option<bool>,
    /// Whether it has a toolbar-button page (popup) the shield can open.
    pub popup: bool,
    /// Windows: the uBlock Origin that comes with Rust Browser.
    pub built_in: bool,
}

/// The ad blocker that comes with Rust Browser, for Settings and the shield.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct BuiltInBlocker {
    /// "uBlock Origin" (Windows) or "hBlock" (Linux).
    pub name: &'static str,
    /// The Settings switch.
    pub enabled: bool,
    /// Whether it blocks in this window now.
    pub active: bool,
    /// One sentence about its state (version, list date, or what is pending).
    pub detail: String,
    /// Linux: whether "Update now" applies (a list can be downloaded).
    pub can_update: bool,
}

/// "<host> is now full screen", drawn at the top of a fullscreen page.
#[derive(Serialize)]
pub struct FullscreenNotice<'a> {
    pub host: &'a str,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
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
    pub permission_prompt: Option<rust_browser::security::Prompt>,
    /// Seconds before `permission_prompt` is denied unanswered.
    pub permission_seconds: Option<u64>,
    pub busy: bool,
    #[serde(flatten)]
    pub page: &'a State,
    pub tabs: Vec<TabState<'a>>,
    pub active_id: TabId,
    pub search_engine: SearchEngine,
    pub notice: &'a str,
    /// "info" or "error": how the toolbar presents `notice`.
    pub notice_kind: &'static str,
    pub panel: Option<Panel>,
    /// The menu sidebar is open (the toolbar fills the window).
    pub menu_open: bool,
    /// What the library column beside the page shows, if it is open.
    pub library_panel: Option<Panel>,
    /// Tabs are shown in the column at the left (the toolbar has no tab strip).
    pub vertical_tabs: bool,
    /// Whether this window is maximized (switches Maximize to Restore and removes top drag-resize padding).
    pub maximized: bool,
    /// Whether a webpage is in borderless fullscreen (hides window caption buttons).
    pub fullscreen: bool,
    /// The site a page that just entered fullscreen belongs to, and where to draw
    /// its notice (the toolbar view shows nothing else then).
    pub fullscreen_notice: Option<FullscreenNotice<'a>>,
    /// Whether the library column sits at the top-right corner of the window (`New tab + library`), hosting `#window-controls`.
    pub controls_in_library: bool,
    /// Requests extensions stopped for the active page, as the engine reported
    /// them; `None` when nothing is counted (no extension added, counting off, or a
    /// private window).
    pub blocked: Option<u32>,
    /// The "Count blocked requests" switch.
    pub count_blocked: bool,
    /// Linux: the selected tab's site and whether blocking is paused there,
    /// for the shield's per-site switch; never sent on Windows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shield_site: Option<ShieldSite<'a>>,
    /// Linux: saved logins and form filling for the selected tab's site; never
    /// sent on Windows, where WebView2 shows its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passwords: Option<PasswordState<'a>>,
    /// Extensions the user added, with their current state.
    pub extensions: Vec<ExtensionRow>,
    /// The ad blocker that comes with Rust Browser.
    pub built_in_blocker: BuiltInBlocker,
    /// The extension whose popup is open under the shield, if any.
    pub extension_popup: Option<&'a str>,
    pub library: &'a Data,
    /// The folder the library column is filtered to, if any.
    pub library_folder: Option<i64>,
    pub history_enabled: bool,
    pub downloads: Vec<rust_browser::downloads::Record>,
    /// Every profile, this window's first.
    pub profiles: Vec<ProfileRow<'a>>,
    pub profile_colors: &'static [&'static str],
    /// Whether another profile can be added.
    pub can_add_profile: bool,
}

/// One shortcut map for WebView2 controllers and the parent window's focus fallback.
/// "info" when `notice` is the text last shown as information, otherwise "error".
/// Unmarked notices stay errors, so a problem is never shown as routine.
pub fn notice_kind(notice: &str, info: &str) -> &'static str {
    if !notice.is_empty() && notice == info {
        "info"
    } else {
        "error"
    }
}

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
        (0x42, true, _) if shift => Some(Command::ToggleLibrary),
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

/// The script that gives the toolbar the suggestions for what was typed. The
/// toolbar shows them only while its text is still the text they are for.
pub fn suggestions_script(found: &rust_browser::suggestions::Suggestions) -> String {
    let json = serde_json::to_string(found).unwrap_or_else(|_| "null".into());
    format!("window.addressSuggestions&&addressSuggestions({json})")
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The tab strip gives each tab a 240-pixel flex basis; in the vertical tab
    /// column (a flex column) that basis would become each tab's height.
    #[test]
    fn vertical_tabs_keep_their_row_height() {
        let rule = HTML
            .split("html[data-role=tabs] .tab{")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("vertical tab rule");
        assert!(rule.contains("flex:0 0 auto"), "{rule}");
        assert!(rule.contains("height:36px"), "{rule}");
    }

    #[test]
    fn action_feedback_respects_reduced_motion() {
        assert!(
            HTML.contains("button,select,input{font:inherit;color:inherit;transition:"),
            "interactive controls should animate state changes"
        );
        let reduced = HTML
            .split("@media(prefers-reduced-motion:reduce){")
            .nth(1)
            .expect("reduced-motion media query");
        assert!(
            reduced.starts_with("button,select,input{transition:none!important}"),
            "reduced-motion preferences disable control transitions"
        );
        assert!(
            reduced.contains("button:active:not(:disabled){transform:none}"),
            "reduced-motion preferences disable button movement"
        );
    }

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
    /// Hue (degrees), saturation and lightness of an opaque `#rrggbb` colour.
    fn hsl(hex: &str) -> (f64, f64, f64) {
        let c = |i: usize| f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()) / 255.0;
        let (r, g, b) = (c(1), c(3), c(5));
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let l = (max + min) / 2.0;
        let d = max - min;
        if d == 0.0 {
            return (0.0, 0.0, l);
        }
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            60.0 * (((g - b) / d).rem_euclid(6.0))
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        (h, s, l)
    }

    /// A profile avatar never looks like a theme's signal or error colour: each
    /// profile colour differs from every theme's `--accent` and `--error` by at least
    /// 20 degrees of hue or 0.3 of lightness (hue only counts between saturated
    /// colours), and a white initial on it meets 4.5:1.
    #[test]
    fn profile_colors_stay_clear_of_signal_and_error_colors() {
        let dark = tokens(":root");
        let mut meaning = Vec::new();
        for theme in ["dark", "black", "slate", "plum", "light"] {
            let mut palette = dark.clone();
            palette.extend(tokens(&format!("body[data-theme={theme}]")));
            for name in ["--accent", "--error"] {
                meaning.push((theme, name, palette[name].clone()));
            }
        }
        for color in rust_browser::profiles::COLORS {
            assert!(
                contrast(color, "#FFFFFF") >= 4.5,
                "a white initial on {color} is too faint"
            );
            let (h, s, l) = hsl(color);
            for (theme, name, token) in &meaning {
                assert!(
                    !color.eq_ignore_ascii_case(token),
                    "{color} is {theme} {name}"
                );
                let (th, ts, tl) = hsl(token);
                let hue = (h - th).abs().min(360.0 - (h - th).abs());
                let distinct = s < 0.25 || ts < 0.25 || hue >= 20.0 || (l - tl).abs() >= 0.3;
                assert!(
                    distinct,
                    "{color} reads like {theme} {name} {token} (hue {hue:.0}, lightness {:.2})",
                    (l - tl).abs()
                );
            }
        }
    }

    /// Black is Dark with every surface pure black for OLED screens, and neutral
    /// grey edges, since a graphite tint shows on true black.
    #[test]
    fn black_has_pure_black_surfaces_and_neutral_hairlines() {
        let black = tokens("body[data-theme=black]");
        for surface in ["--strip", "--surface", "--panel", "--card", "--field"] {
            assert_eq!(
                black.get(surface).map(String::as_str),
                Some("#000"),
                "{surface}"
            );
        }
        assert_eq!(black.get("--line").map(String::as_str), Some("#333333"));
        assert_eq!(
            black.get("--field-line").map(String::as_str),
            Some("#6B6B6B")
        );
        // Text, warnings and the amber signal come from Dark.
        for inherited in [
            "--text",
            "--secondary",
            "--error",
            "--accent",
            "--on-accent",
        ] {
            assert!(
                !black.contains_key(inherited),
                "Black redefines {inherited}"
            );
        }
    }

    /// Settings and the new-tab page offer the same themes, Black next to Dark.
    #[test]
    fn both_theme_menus_list_black_after_dark() {
        for menu in ["<select id=\"settings-theme\"", "<select id=\"home-theme\""] {
            let options = HTML
                .split(menu)
                .nth(1)
                .and_then(|rest| rest.split("</select>").next())
                .unwrap_or_else(|| panic!("{menu} missing"));
            let values: Vec<_> = options
                .split("<option value=\"")
                .skip(1)
                .filter_map(|o| o.split('"').next())
                .collect();
            assert_eq!(
                values,
                ["dark", "black", "slate", "plum", "light"],
                "{menu}"
            );
            assert!(options.contains("<option value=\"black\">Black</option>"));
        }
    }

    #[test]
    fn every_theme_meets_wcag_contrast_for_text_and_focus_indicators() {
        assert!(HTML.contains("input::placeholder{color:var(--secondary);opacity:1}"));
        let dark = tokens(":root");
        for theme in ["dark", "black", "slate", "plum", "light"] {
            let mut palette = dark.clone();
            palette.extend(tokens(&format!("body[data-theme={theme}]")));
            let token = |name: &str| {
                palette
                    .get(name)
                    .unwrap_or_else(|| panic!("{theme} lacks {name}"))
            };
            for background in ["--surface", "--panel", "--card", "--field", "--strip"] {
                // WCAG 1.4.3: normal text at least 4.5:1.
                let ratio = contrast(token("--text"), token(background));
                assert!(
                    ratio >= 4.5,
                    "{theme}: --text on {background} is {ratio:.2}"
                );
            }
            for foreground in ["--secondary", "--error"] {
                for background in ["--surface", "--panel", "--field", "--strip"] {
                    let ratio = contrast(token(foreground), token(background));
                    assert!(
                        ratio >= 4.5,
                        "{theme}: {foreground} on {background} is {ratio:.2}"
                    );
                }
            }
            for background in ["--surface", "--panel", "--card", "--strip"] {
                // WCAG 1.4.11: focus rings and active-tab bar at least 3:1.
                let ratio = contrast(token("--accent"), token(background));
                assert!(
                    ratio >= 3.0,
                    "{theme}: --accent on {background} is {ratio:.2}"
                );
            }
        }
    }
    /// `a` mixed into `b` by `share` (0 to 1), as CSS `color-mix(in srgb, ...)` does.
    fn mix(a: &str, b: &str, share: f64) -> String {
        let rgb = |hex: &str| {
            let hex = hex.trim_start_matches('#');
            let hex: String = if hex.len() == 3 {
                hex.chars().flat_map(|c| [c, c]).collect()
            } else {
                hex.to_owned()
            };
            [0, 2, 4].map(|i| f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()))
        };
        let (x, y) = (rgb(a), rgb(b));
        let c = |i: usize| (x[i] * share + y[i] * (1.0 - share)).round() as u8;
        format!("#{:02x}{:02x}{:02x}", c(0), c(1), c(2))
    }
    #[test]
    fn new_tab_backgrounds_and_the_shield_keep_contrast_in_every_theme() {
        // Each background's colour stops, as the CSS draws them.
        assert!(HTML.contains("#manager[data-background=dusk]{background:linear-gradient(170deg,var(--strip),var(--surface) 70%)}"));
        assert!(HTML.contains("#manager[data-background=glow]{background:radial-gradient(ellipse at 50% 0,color-mix(in srgb,var(--accent) 10%,var(--surface)),var(--surface) 70%)}"));
        assert!(HTML.contains("#manager[data-background=panels]{background:linear-gradient(135deg,var(--panel),var(--card))}"));
        // The shield reads in the text colour with a Dial Grey count; the signal stays
        // free for state that needs the user.
        assert!(HTML.contains("#shield[data-on=true]{color:var(--text)}"));
        assert!(
            HTML.contains("#shield-count{font-size:12px;font-weight:600;color:var(--secondary)")
        );
        assert!(HTML.contains("border:1px solid var(--field-line);border-radius:18px"));
        let dark = tokens(":root");
        for theme in ["dark", "black", "slate", "plum", "light"] {
            let mut palette = dark.clone();
            palette.extend(tokens(&format!("body[data-theme={theme}]")));
            let token = |name: &str| palette[name].clone();
            let stops = [
                token("--strip"),
                token("--surface"),
                token("--panel"),
                token("--card"),
                mix(&token("--accent"), &token("--surface"), 0.10),
            ];
            for stop in &stops {
                for text in ["--text", "--secondary"] {
                    let ratio = contrast(&token(text), stop);
                    assert!(
                        ratio >= 4.5,
                        "{theme}: {text} on a background stop {stop} is {ratio:.2}"
                    );
                }
            }
            // The shield, when an extension is on, is a text-colour icon on the field
            // with its count in Dial Grey.
            for (name, minimum) in [("--text", 3.0), ("--secondary", 4.5)] {
                let ratio = contrast(&token(name), &token("--field"));
                assert!(
                    ratio >= minimum,
                    "{theme}: the shield's {name} on --field is {ratio:.2}"
                );
            }
            // WCAG 1.4.11: the address line's edge is a UI component boundary, so it
            // keeps 3:1 against the toolbar around it and the field inside it.
            for background in ["--surface", "--field"] {
                let ratio = contrast(&token("--field-line"), &token(background));
                assert!(
                    ratio >= 3.0,
                    "{theme}: --field-line on {background} is {ratio:.2}"
                );
            }
        }
    }
    #[test]
    fn the_toolbar_policy_allows_only_embedded_images_beyond_its_own_code() {
        let policy = HTML
            .split("Content-Security-Policy\" content=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("toolbar policy");
        assert_eq!(
            policy,
            "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; form-action 'none'; base-uri 'none'"
        );
        // Icons are inline SVG elements, never fetched or embedded as images.
        assert!(!HTML.contains("<img src=\"http"));
        // The only CSS images are the drawn new-tab wallpapers, inline SVG that
        // refers to nothing outside itself, and the user's own picture, set from
        // a data: URL that `setWallpaper` checks first.
        let svg_start = "url(\"data:image/svg+xml,";
        let mut wallpapers = 0;
        for (at, _) in HTML.match_indices(svg_start) {
            let rest = &HTML[at + svg_start.len()..];
            let svg = &rest[..rest.find('"').expect("closed data URL")];
            // No link of any kind; the one address is the SVG namespace.
            assert!(!svg.contains("href"), "{at}");
            assert_eq!(svg.matches("http").count(), 1, "{at}");
            assert_eq!(svg.matches("http://www.w3.org/2000/svg").count(), 1);
            // Its own fragment references (gradients, filters) only.
            assert!(
                svg.split("url(").skip(1).all(|r| r.starts_with("%23")),
                "{at}"
            );
            wallpapers += 1;
        }
        // Four drawn wallpapers, each in a dark and a light rendition.
        assert_eq!(wallpapers, 8);
        for (index, rest) in HTML.split("url(").enumerate().skip(1) {
            assert!(
                rest.starts_with("\"data:image/svg+xml,")
                    || rest.starts_with("%23")
                    || rest.starts_with("\"'+url+'\")"),
                "unexpected url( {index}"
            );
        }
        assert!(
            HTML.contains(r"/^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+\/]+=*$/.test(url)")
        );
    }
    #[test]
    fn every_toolbar_control_has_an_accessible_name() {
        // Icon-only buttons must carry aria-label or title; text buttons name themselves.
        for (index, part) in HTML.split("<button").enumerate().skip(1) {
            let tag = &part[..part.find('>').unwrap()];
            let body = &part[part.find('>').unwrap() + 1..part.find("</button>").unwrap()];
            let text: String = {
                let mut out = String::new();
                let mut inside = false;
                for c in body.chars() {
                    match c {
                        '<' => inside = true,
                        '>' => inside = false,
                        _ if !inside => out.push(c),
                        _ => {}
                    }
                }
                out
            };
            assert!(
                tag.contains("aria-label=") || tag.contains("title=") || !text.trim().is_empty(),
                "button {index} has no accessible name: <button{tag}>"
            );
        }
    }
    #[test]
    fn only_notices_marked_as_information_read_as_information() {
        assert_eq!(
            notice_kind("Extension removed.", "Extension removed."),
            "info"
        );
        assert_eq!(
            notice_kind("Could not resize webpage.", "Extension removed."),
            "error"
        );
        assert_eq!(notice_kind("", ""), "error");
        assert_eq!(notice_kind("Extension removed.", ""), "error");
    }

    /// The tab right-click menu and Menu's new rows send these messages; each must
    /// reach the browser as the command it names, carrying the tab it acts on.
    #[test]
    fn tab_menu_and_menu_messages_parse_as_their_commands() {
        let parse = |body: &str| serde_json::from_str::<Command>(body).unwrap();
        assert!(matches!(
            parse(r#"{"action":"pin_tab","value":3}"#),
            Command::PinTab(3)
        ));
        assert!(matches!(
            parse(r#"{"action":"open_extension_popup","value":{"id":"abc","right":900.5,"top":44}}"#),
            Command::OpenExtensionPopup { id, right, top } if id == "abc" && right == 900.5 && top == 44.0
        ));
        assert!(matches!(
            parse(r#"{"action":"duplicate_tab","value":4}"#),
            Command::DuplicateTab(4)
        ));
        assert!(matches!(
            parse(r#"{"action":"close_other_tabs","value":5}"#),
            Command::CloseOtherTabs(5)
        ));
        assert!(matches!(
            parse(r#"{"action":"close_tab","value":6}"#),
            Command::CloseTab(6)
        ));
        assert!(matches!(
            parse(r#"{"action":"close_active"}"#),
            Command::CloseActive
        ));
        assert!(matches!(
            parse(r#"{"action":"reopen_closed_tab"}"#),
            Command::ReopenClosedTab
        ));
        assert!(matches!(
            parse(r#"{"action":"open_saved_tabs","value":["https://a.example/","https://b.example/"]}"#),
            Command::OpenSavedTabs(urls) if urls.len() == 2
        ));
        assert!(matches!(
            parse(r#"{"action":"open_saved_window","value":"https://a.example/"}"#),
            Command::OpenSavedWindow(url) if url == "https://a.example/"
        ));
        assert!(matches!(
            parse(r#"{"action":"open_saved_private","value":"https://a.example/"}"#),
            Command::OpenSavedPrivate(url) if url == "https://a.example/"
        ));
        // A tab menu message without its tab is refused rather than guessed.
        assert!(serde_json::from_str::<Command>(r#"{"action":"close_other_tabs"}"#).is_err());
        // The toolbar sends exactly these names.
        for name in [
            "duplicate_tab",
            "pin_tab",
            "close_tab",
            "close_other_tabs",
            "reopen_closed_tab",
            "open_saved_tabs",
            "open_saved_window",
            "open_saved_private",
        ] {
            assert!(
                HTML.contains(&format!("'{name}'")),
                "the toolbar never sends {name}"
            );
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
