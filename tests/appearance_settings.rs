//! v0.21 appearance settings: optional fields that older versions ignore.
use rust_browser::settings::Settings;
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
fn new_profiles_keep_the_tab_strip() {
    assert!(!Settings::default().vertical_tabs);
}

#[test]
fn a_0_20_settings_file_loads_with_the_tab_strip() {
    let settings: Settings = serde_json::from_str(SETTINGS_020).unwrap();
    assert!(!settings.vertical_tabs);
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
    let settings = Settings {
        vertical_tabs: true,
        ..settings
    };
    let text = serde_json::to_string(&settings).unwrap();
    assert!(text.contains("\"vertical_tabs\":true"));
    let old: Settings020 = serde_json::from_str(&text).unwrap();
    assert_eq!(old.suspend_after_seconds, 600);
    assert_eq!(old.theme, "slate");
    assert_eq!(old.extensions.len(), 1);
    // And 0.21 reads its own file back unchanged.
    assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), settings);
}
