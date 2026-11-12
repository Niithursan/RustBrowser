//! v0.19 password saving and autofill switches in the settings and clearing formats.
use rust_browser::{privacy::ClearRequest, settings::Settings};
use serde::Deserialize;

/// The settings fields as 0.18 defined them, read with 0.18's rules
/// (`serde(default)`, unknown fields ignored).
#[derive(Debug, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
struct Settings018 {
    search_engine: String,
    history_enabled: bool,
    restore_session: bool,
    automatic_suspension: bool,
    suspend_after_seconds: u64,
    theme: String,
    favorites: Vec<serde_json::Value>,
    show_recent: bool,
}
impl Default for Settings018 {
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
        }
    }
}

const SETTINGS_018: &str = r#"{"search_engine":"duck_duck_go","history_enabled":false,"restore_session":true,"automatic_suspension":true,"suspend_after_seconds":300,"theme":"light","favorites":[],"show_recent":true}"#;

#[test]
fn new_profiles_have_both_switches_off() {
    let settings = Settings::default();
    assert!(!settings.save_passwords);
    assert!(!settings.autofill);
}

#[test]
fn a_0_18_settings_file_loads_with_both_switches_off() {
    let settings: Settings = serde_json::from_str(SETTINGS_018).unwrap();
    assert!(!settings.save_passwords);
    assert!(!settings.autofill);
    assert!(
        !settings.toolbar_autofill_cleared,
        "the toolbar cleanup still runs"
    );
    // Everything 0.18 saved is kept.
    assert!(!settings.history_enabled);
    assert_eq!(settings.suspend_after_seconds, 300);
    assert!(settings.show_recent);
    settings.validate().unwrap();
}

#[test]
fn a_0_19_settings_file_is_read_by_0_18_rules() {
    let settings = Settings {
        save_passwords: true,
        autofill: true,
        suspend_after_seconds: 300,
        ..Settings::default()
    };
    let text = serde_json::to_string(&settings).unwrap();
    assert!(text.contains("\"save_passwords\":true") && text.contains("\"autofill\":true"));
    let old: Settings018 = serde_json::from_str(&text).unwrap();
    assert_eq!(old.suspend_after_seconds, 300);
    assert!(old.restore_session);
}

#[test]
fn passwords_and_autofill_are_separate_clearing_choices() {
    for (passwords, autofill) in [(true, false), (false, true), (true, true)] {
        assert!(
            ClearRequest {
                passwords,
                autofill,
                confirmed: true,
                ..ClearRequest::default()
            }
            .validate()
            .is_ok()
        );
    }
    // Unchecked by default, and never implied by clearing history.
    let request = ClearRequest::default();
    assert!(!request.passwords && !request.autofill);
    let history: ClearRequest =
        serde_json::from_str(r#"{"history":true,"confirmed":true}"#).unwrap();
    assert!(!history.passwords && !history.autofill);
    assert!(
        ClearRequest {
            passwords: true,
            ..ClearRequest::default()
        }
        .validate()
        .is_err(),
        "clearing still needs confirmation"
    );
}
