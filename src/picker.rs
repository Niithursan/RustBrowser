//! The "Who's browsing?" picker's page and choices, shared by the Windows
//! (`native_picker`) and Linux (`linux_picker`) windows that show it. It only
//! chooses; the chosen profile then starts in a process of its own
//! (`--profile=<id>`), because a process can run one event loop and each
//! profile keeps its own lock, web engine data and link channel.
use rust_browser::profiles::{self, Registry};
use serde::Deserialize;
use std::path::Path;

const HTML: &str = include_str!("ui/picker.html");

/// The window's size and smallest size, in logical pixels.
pub const SIZE: (f64, f64) = (640.0, 460.0);
pub const MIN_SIZE: (f64, f64) = (360.0, 320.0);
pub const TITLE: &str = "Rust Browser - Who's browsing?";
/// The longest message the page may send.
pub const MAX_MESSAGE: usize = 256;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
pub enum Choice {
    Open(String),
    Add,
    Close,
}

/// What the window does after a choice.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Close, and start this profile.
    Chosen(String),
    /// Stay open and show this message.
    Error(String),
    /// Close without choosing.
    Closed,
}

/// The picker page with the profiles and the Default profile's theme filled in.
/// The data is JSON with `<` escaped, so no profile name can end the script
/// element.
pub fn page(root: &Path, registry: &Registry) -> String {
    let theme = rust_browser::storage::load(&root.join("session.json"))
        .map(|snapshot| snapshot.settings.theme)
        .unwrap_or_default();
    page_with(registry, theme)
}

fn page_with(registry: &Registry, theme: rust_browser::settings::Theme) -> String {
    let data = serde_json::json!({
        "theme": theme,
        "can_add": registry.profiles.len() < profiles::MAX_PROFILES,
        "profiles": registry.profiles,
    })
    .to_string()
    .replace('<', "\\u003c");
    HTML.replacen("/*DATA*/null", &data, 1)
}

/// A message from the page, if it is one of the known choices.
pub fn read(body: &str) -> Option<Choice> {
    (body.len() <= MAX_MESSAGE)
        .then(|| serde_json::from_str(body).ok())
        .flatten()
}

/// Act on `choice`. Only a profile that still exists opens: another process
/// may have deleted it since the picker opened.
pub fn decide(root: &Path, choice: Choice) -> Outcome {
    match choice {
        Choice::Open(id) => match profiles::load(root) {
            Ok(registry) if registry.get(&id).is_some() => Outcome::Chosen(id),
            _ => Outcome::Error("That profile no longer exists.".into()),
        },
        Choice::Add => {
            let created = profiles::update(root, |registry| {
                let name = registry.suggested_name();
                let color = registry.suggested_color();
                registry.create(&name, color).map(|p| p.id)
            });
            match created {
                Ok(id) => Outcome::Chosen(id),
                Err(error) => Outcome::Error(error.to_string()),
            }
        }
        Choice::Close => Outcome::Closed,
    }
}

/// The script that shows `message` on the page.
pub fn error_script(message: &str) -> Option<String> {
    serde_json::to_string(message)
        .ok()
        .map(|text| format!("window.showError({text})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picker draws in the Default profile's theme, Black included, with the
    /// toolbar's pure black surfaces and neutral hairline.
    #[test]
    fn the_picker_is_drawn_in_black_when_the_default_profile_uses_it() {
        let html = page_with(&Registry::default(), rust_browser::settings::Theme::Black);
        assert!(html.contains("\"theme\":\"black\""));
        let block = html
            .split("body[data-theme=black]{")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("the picker has Black's colours");
        let mut declarations: Vec<_> = block.split(';').collect();
        declarations.sort_unstable();
        assert_eq!(
            declarations,
            [
                "--card:#000",
                "--line:#333333",
                "--panel:#000",
                "--strip:#000",
                "--surface:#000"
            ]
        );
    }

    #[test]
    fn profile_names_cannot_escape_the_picker_script() {
        let mut registry = Registry::default();
        registry
            .create("</script><img src=x onerror=alert(1)>", profiles::COLORS[1])
            .unwrap();
        let html = page_with(&registry, rust_browser::settings::Theme::Light);
        assert!(!html.contains("/*DATA*/null"));
        assert_eq!(
            html.matches("</script>").count(),
            1,
            "only the page's own end tag"
        );
        assert!(html.contains("\\u003c/script>\\u003cimg"));
        assert!(html.contains("\"theme\":\"light\""));
        assert!(html.contains("\"can_add\":true"));
    }

    #[test]
    fn only_known_choices_are_accepted() {
        assert_eq!(
            read(r#"{"action":"open","value":"p2"}"#),
            Some(Choice::Open("p2".into()))
        );
        assert_eq!(read(r#"{"action":"add"}"#), Some(Choice::Add));
        assert_eq!(read(r#"{"action":"delete","value":"p2"}"#), None);
        let long = format!(
            r#"{{"action":"open","value":"{}"}}"#,
            "p".repeat(MAX_MESSAGE)
        );
        assert_eq!(read(&long), None);
    }

    #[test]
    fn choices_open_only_existing_profiles_and_add_new_ones() {
        let root = std::env::temp_dir().join(format!("rust-browser-picker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            decide(&root, Choice::Open("default".into())),
            Outcome::Chosen("default".into())
        );
        assert!(matches!(
            decide(&root, Choice::Open("p2".into())),
            Outcome::Error(_)
        ));
        assert_eq!(decide(&root, Choice::Add), Outcome::Chosen("p2".into()));
        assert_eq!(
            profiles::load(&root)
                .unwrap()
                .get("p2")
                .map(|p| p.name.as_str()),
            Some("Profile 2")
        );
        assert_eq!(decide(&root, Choice::Close), Outcome::Closed);
        let _ = std::fs::remove_dir_all(root);
    }
}
