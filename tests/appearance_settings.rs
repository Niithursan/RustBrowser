//! v0.21 appearance settings: optional fields that older versions ignore.
use rust_browser::settings::{
    Background, Favorite, MAX_FAVORITE_ICON, Settings, same_site, valid_icon,
};
use serde::Deserialize;

/// The settings fields as 0.20 defined them, read with 0.20's rules
/// (`serde(default)`, unknown fields ignored).
#[derive(Debug, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
struct Settings020 {
    search_engine: String,
    history_enabled: bool,
    restore_session: bool,
    automatic_suspension: bool,
    suspend_after_seconds: u64,
    theme: String,
    favorites: Vec<serde_json::Value>,
    show_recent: bool,
    save_passwords: bool,
    autofill: bool,
    toolbar_autofill_cleared: bool,
    extensions: Vec<serde_json::Value>,
}
impl Default for Settings020 {
    fn default() -> Self {
        Self {
            search_engine: "google".into(),
            history_enabled: true,
            restore_session: true,
            automatic_suspension: true,
            suspend_after_seconds: 120,
            theme: "dark".into(),
            favorites: Vec::new(),
            show_recent: false,
            save_passwords: false,
            autofill: false,
            toolbar_autofill_cleared: false,
            extensions: Vec::new(),
        }
    }
}

const SETTINGS_020: &str = r#"{"search_engine":"bing","history_enabled":true,"restore_session":true,"automatic_suspension":false,"suspend_after_seconds":600,"theme":"slate","favorites":[{"title":"Example","url":"https://example.com/"}],"show_recent":true,"save_passwords":true,"autofill":false,"toolbar_autofill_cleared":true,"extensions":[{"id":"abcdefghijklmnopabcdefghijklmnop","name":"Blocker","version":"1.0","folder":"C:\\ext"}]}"#;

#[test]
fn new_profiles_keep_the_tab_strip_and_a_plain_new_tab_page() {
    assert!(!Settings::default().vertical_tabs);
    assert_eq!(Settings::default().newtab_background, Background::Plain);
}

#[test]
fn a_0_20_settings_file_loads_with_the_tab_strip() {
    let settings: Settings = serde_json::from_str(SETTINGS_020).unwrap();
    assert!(!settings.vertical_tabs);
    assert_eq!(settings.newtab_background, Background::Plain);
    assert!(settings.favorites[0].icon.is_empty());
    // Everything 0.20 saved is kept.
    assert_eq!(settings.suspend_after_seconds, 600);
    assert!(settings.save_passwords && settings.toolbar_autofill_cleared);
    assert_eq!(settings.extensions.len(), 1);
    assert_eq!(settings.favorites.len(), 1);
    settings.validate().unwrap();
}

#[test]
fn a_0_21_settings_file_is_read_by_0_20_rules() {
    let settings: Settings = serde_json::from_str(SETTINGS_020).unwrap();
    let mut settings = Settings {
        vertical_tabs: true,
        newtab_background: Background::Glow,
        ..settings
    };
    settings.favorites[0].icon = "data:image/png;base64,iVBORw0KGgo=".into();
    settings.validate().unwrap();
    let text = serde_json::to_string(&settings).unwrap();
    assert!(
        text.contains("\"vertical_tabs\":true") && text.contains("\"newtab_background\":\"glow\"")
    );
    let old: Settings020 = serde_json::from_str(&text).unwrap();
    assert_eq!(old.suspend_after_seconds, 600);
    assert_eq!(old.theme, "slate");
    assert_eq!(old.extensions.len(), 1);
    assert_eq!(old.favorites[0]["url"], "https://example.com/");
    // And 0.21 reads its own file back unchanged.
    assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), settings);
}

#[test]
fn a_favorite_without_an_icon_is_saved_as_before() {
    let favorite = Favorite {
        title: "Example".into(),
        url: "https://example.com/".into(),
        icon: String::new(),
    };
    assert_eq!(
        serde_json::to_string(&favorite).unwrap(),
        r#"{"title":"Example","url":"https://example.com/"}"#
    );
}

#[test]
fn a_background_named_by_a_later_version_reads_as_plain() {
    let settings: Settings = serde_json::from_str(r#"{"newtab_background":"aurora"}"#).unwrap();
    assert_eq!(settings.newtab_background, Background::Plain);
    let settings: Settings = serde_json::from_str(r#"{"newtab_background":"panels"}"#).unwrap();
    assert_eq!(settings.newtab_background, Background::Panels);
}

#[test]
fn only_small_png_data_icons_are_kept() {
    assert!(valid_icon("data:image/png;base64,iVBORw0KGgo="));
    for bad in [
        "",
        "data:image/png;base64,",
        "data:image/svg+xml;base64,PHN2Zz4=",
        "https://example.com/favicon.png",
        "data:image/png;base64,iVBOR\"onerror=alert(1)",
        "data:image/png;base64,iVBOR w0KGgo=",
    ] {
        assert!(!valid_icon(bad), "{bad}");
    }
    let big = format!("data:image/png;base64,{}", "A".repeat(MAX_FAVORITE_ICON));
    assert!(!valid_icon(&big));
    // A saved file with a bad icon is refused like any other bad favourite.
    let mut settings = Settings::default();
    settings.favorites.push(Favorite {
        title: "Example".into(),
        url: "https://example.com/".into(),
        icon: "javascript:alert(1)".into(),
    });
    assert!(settings.validate().is_err());
}

#[test]
fn icons_are_matched_by_site() {
    assert!(same_site(
        "https://example.com/",
        "https://www.example.com/a?b"
    ));
    assert!(same_site(
        "http://127.0.0.1:8080/x",
        "http://127.0.0.1:8080/y"
    ));
    assert!(!same_site("https://example.com/", "https://example.org/"));
    assert!(!same_site(
        "https://mail.example.com/",
        "https://example.com/"
    ));
    assert!(!same_site("about:blank", "about:blank"));
    assert!(!same_site("rust-browser://newtab", "rust-browser://newtab"));
}
