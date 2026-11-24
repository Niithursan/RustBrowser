//! Reading an unpacked browser extension's `manifest.json`: shared by Windows,
//! where WebView2 runs the extension, and Linux, where only its declarative
//! blocking rules are used (`content_rules`). Everything here treats the folder
//! as untrusted input: sizes are bounded and paths are checked.
use std::path::Path;

/// Largest `manifest.json` read when checking a folder.
const MAX_MANIFEST: u64 = 1024 * 1024;

/// Whether `id` looks like a WebView2 extension ID (32 letters a to p).
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// Whether a manifest asks for a permission that lets an extension block or
/// redirect requests (what a content blocker needs). Asking is not proof that it
/// blocks ads; it is what Rust Browser can tell without running it.
pub fn asks_to_block(manifest: &serde_json::Value) -> bool {
    const BLOCKING: [&str; 3] = [
        "declarativeNetRequest",
        "declarativeNetRequestWithHostAccess",
        "webRequestBlocking",
    ];
    manifest
        .get("permissions")
        .and_then(|p| p.as_array())
        .is_some_and(|list| {
            list.iter()
                .any(|p| p.as_str().is_some_and(|p| BLOCKING.contains(&p)))
        })
}

/// Whether the extension in `folder` asks to block requests, if its manifest can
/// still be read (extensions added before v0.21 did not record it).
pub fn folder_asks_to_block(folder: &Path) -> Option<bool> {
    read_manifest(folder).ok().map(|m| asks_to_block(&m))
}

/// Longest popup path accepted from a manifest.
const MAX_POPUP_PATH: usize = 200;

/// The page an extension shows from its toolbar button (`action.default_popup`
/// in Manifest V3, `browser_action.default_popup` in V2), as a path inside the
/// extension. Only a plain relative path to an HTML file is accepted: no scheme,
/// query, fragment, parent step or leading slash, so it can only name a page of
/// the extension itself.
pub fn action_popup(manifest: &serde_json::Value) -> Option<String> {
    let path = ["/action/default_popup", "/browser_action/default_popup"]
        .iter()
        .find_map(|pointer| manifest.pointer(pointer).and_then(|p| p.as_str()))?
        .trim_start_matches("./");
    let plain = !path.is_empty()
        && path.len() <= MAX_POPUP_PATH
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && {
            let lower = path.to_ascii_lowercase();
            lower.ends_with(".html") || lower.ends_with(".htm")
        };
    plain.then(|| path.to_string())
}

/// The toolbar-button page of the extension in `folder`, if it has one and its
/// manifest can still be read.
pub fn folder_popup(folder: &Path) -> Option<String> {
    read_manifest(folder).ok().and_then(|m| action_popup(&m))
}

pub fn read_manifest(folder: &Path) -> Result<serde_json::Value, String> {
    if !folder.is_dir() {
        return Err("Choose the extension's folder (the one containing manifest.json).".into());
    }
    let manifest = folder.join("manifest.json");
    let metadata = std::fs::metadata(&manifest).map_err(|_| {
        "This folder has no manifest.json. Choose the unpacked extension folder.".to_string()
    })?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST {
        return Err("This folder's manifest.json is not a valid extension manifest.".into());
    }
    let text = std::fs::read(&manifest)
        .map_err(|_| "The extension's manifest.json could not be read.".to_string())?;
    let text = text.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&text);
    serde_json::from_slice(text)
        .map_err(|_| "This folder's manifest.json is not valid JSON.".to_string())
}

/// The ID Chromium (and WebView2) gives an unpacked extension, from the SHA-256
/// digest of its folder's path in hex: the first 32 hex digits, each `0` to `f`
/// written as `a` to `p`.
pub fn id_from_sha256_hex(hex: &str) -> Option<String> {
    let id: String = hex
        .chars()
        .take(32)
        .map(|c| c.to_digit(16).map(|d| char::from(b'a' + d as u8)))
        .collect::<Option<_>>()?;
    valid_id(&id).then_some(id)
}

/// Largest `messages.json` read for a localized name.
const MAX_MESSAGES: u64 = 4 * 1024 * 1024;

/// The name to show for the extension in `folder`: the manifest's `name`, or,
/// for a `__MSG_key__` name, that message in the manifest's default locale.
/// At most 80 characters, without control characters.
pub fn display_name(folder: &Path, manifest: &serde_json::Value) -> Option<String> {
    let name = manifest.get("name")?.as_str()?;
    let shown = match name
        .strip_prefix("__MSG_")
        .and_then(|key| key.strip_suffix("__"))
    {
        None => name.to_string(),
        Some(key) => {
            let locale = manifest
                .get("default_locale")
                .and_then(|l| l.as_str())
                .filter(|l| {
                    !l.is_empty()
                        && l.len() <= 16
                        && l.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                })?;
            let path = folder.join("_locales").join(locale).join("messages.json");
            let metadata = std::fs::metadata(&path).ok()?;
            if !metadata.is_file() || metadata.len() > MAX_MESSAGES {
                return None;
            }
            let bytes = std::fs::read(&path).ok()?;
            let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
            let messages: serde_json::Value = serde_json::from_slice(bytes).ok()?;
            // Message keys are case-insensitive.
            messages
                .as_object()?
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))?
                .1
                .get("message")?
                .as_str()?
                .to_string()
        }
    };
    let shown: String = shown
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect::<String>()
        .trim()
        .to_string();
    (!shown.is_empty()).then_some(shown)
}

/// Check that `folder` holds an unpacked extension before WebView2 sees it.
/// Returns the manifest's version for display and whether it asks to block
/// requests.
pub fn check_folder(folder: &Path) -> Result<(String, bool), String> {
    let value = read_manifest(folder)?;
    let version = value.get("version").and_then(|v| v.as_str());
    let manifest_version = value.get("manifest_version").and_then(|v| v.as_u64());
    match (
        value.get("name").and_then(|v| v.as_str()),
        version,
        manifest_version,
    ) {
        (Some(_), Some(version), Some(2 | 3)) => {
            Ok((version.chars().take(32).collect(), asks_to_block(&value)))
        }
        _ => Err("This folder's manifest.json is not a valid extension manifest.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_ids_are_checked() {
        assert!(valid_id("nhhbkmgedjdemoadhmhiinbgbhhgbgnk"));
        assert!(!valid_id("nhhbkmgedjdemoadhmhiinbgbhhgbgn"));
        assert!(!valid_id("NHHBKMGEDJDEMOADHMHIINBGBHHGBGNK"));
        assert!(!valid_id("zhhbkmgedjdemoadhmhiinbgbhhgbgnk"));
        assert!(!valid_id("../bkmgedjdemoadhmhiinbgbhhgbgnk"));
    }

    #[test]
    fn only_blocking_permissions_count_as_asking_to_block() {
        let ask = |json: &str| asks_to_block(&serde_json::from_str(json).unwrap());
        assert!(ask(r#"{"permissions":["declarativeNetRequest"]}"#));
        assert!(ask(
            r#"{"permissions":["storage","declarativeNetRequestWithHostAccess"]}"#
        ));
        assert!(ask(
            r#"{"permissions":["webRequest","webRequestBlocking"]}"#
        ));
        assert!(!ask(r#"{"permissions":["webRequest","storage"]}"#));
        assert!(!ask(
            r#"{"optional_permissions":["declarativeNetRequest"]}"#
        ));
        assert!(!ask(r#"{"content_scripts":[]}"#));
        assert!(!ask(r#"{"permissions":"declarativeNetRequest"}"#));
        let fixtures =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions");
        assert_eq!(folder_asks_to_block(&fixtures.join("blocker")), Some(true));
        assert_eq!(
            folder_asks_to_block(&fixtures.join("marker-a")),
            Some(false)
        );
        assert_eq!(folder_asks_to_block(&fixtures.join("missing")), None);
    }

    #[test]
    fn only_a_plain_page_of_the_extension_is_its_popup() {
        let popup = |json: &str| action_popup(&serde_json::from_str(json).unwrap());
        assert_eq!(
            popup(r#"{"action":{"default_popup":"popup.html"}}"#).as_deref(),
            Some("popup.html")
        );
        assert_eq!(
            popup(r#"{"browser_action":{"default_popup":"./ui/popup-fenced.html"}}"#).as_deref(),
            Some("ui/popup-fenced.html")
        );
        for bad in [
            r#"{"action":{}}"#,
            r#"{"action":{"default_popup":""}}"#,
            r#"{"action":{"default_popup":"https://example.test/p.html"}}"#,
            r#"{"action":{"default_popup":"/popup.html"}}"#,
            r#"{"action":{"default_popup":"../other/popup.html"}}"#,
            r#"{"action":{"default_popup":"a//popup.html"}}"#,
            r#"{"action":{"default_popup":"popup.html?x=1"}}"#,
            r#"{"action":{"default_popup":"popup.html#top"}}"#,
            r#"{"action":{"default_popup":"pop up.html"}}"#,
            r#"{"action":{"default_popup":"popup.js"}}"#,
            r#"{"action":{"default_popup":"a\\popup.html"}}"#,
            r#"{"action":{"default_popup":7}}"#,
        ] {
            assert_eq!(popup(bad), None, "{bad}");
        }
        let long = format!(
            r#"{{"action":{{"default_popup":"{}.html"}}}}"#,
            "a".repeat(MAX_POPUP_PATH)
        );
        assert_eq!(popup(&long), None);
        let fixtures =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions");
        assert_eq!(
            folder_popup(&fixtures.join("popup-blocker")).as_deref(),
            Some("popup.html")
        );
        assert_eq!(folder_popup(&fixtures.join("blocker")), None);
        assert_eq!(folder_popup(&fixtures.join("missing")), None);
    }

    #[test]
    fn folders_without_a_valid_manifest_are_refused() {
        let root = std::env::temp_dir().join(format!(
            "rust-browser-ext-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert!(check_folder(&root).is_err(), "no manifest");
        assert!(check_folder(&root.join("missing")).is_err(), "no folder");
        for bad in [
            "not json",
            r#"{"name":"x","version":"1"}"#,
            r#"{"name":"x","version":"1","manifest_version":1}"#,
            r#"{"version":"1","manifest_version":3}"#,
            r#"[1,2,3]"#,
        ] {
            std::fs::write(root.join("manifest.json"), bad).unwrap();
            assert!(check_folder(&root).is_err(), "{bad}");
        }
        std::fs::write(
            root.join("manifest.json"),
            "\u{feff}{\"name\":\"Test\",\"version\":\"1.2.3\",\"manifest_version\":3}",
        )
        .unwrap();
        assert_eq!(check_folder(&root).unwrap(), ("1.2.3".to_string(), false));
        let big = format!(
            "{{\"name\":\"x\",\"version\":\"1\",\"manifest_version\":3,\"pad\":\"{}\"}}",
            "a".repeat(MAX_MANIFEST as usize)
        );
        std::fs::write(root.join("manifest.json"), big).unwrap();
        assert!(check_folder(&root).is_err(), "too large");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ids_come_from_the_path_digest_as_chromium_makes_them() {
        assert_eq!(
            id_from_sha256_hex("0123456789abcdef0123456789ABCDEF00ff").as_deref(),
            Some("abcdefghijklmnopabcdefghijklmnop")
        );
        assert_eq!(id_from_sha256_hex("0123"), None);
        assert_eq!(id_from_sha256_hex(&"g".repeat(64)), None);
    }

    #[test]
    fn a_localized_name_is_read_from_the_default_locale() {
        let root = std::env::temp_dir().join(format!(
            "rust-browser-ext-name-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("_locales/en")).unwrap();
        std::fs::write(
            root.join("_locales/en/messages.json"),
            "\u{feff}{\"extName\":{\"message\":\"uBlock Origin Lite\"}}",
        )
        .unwrap();
        let name = |json: &str| display_name(&root, &serde_json::from_str(json).unwrap());
        assert_eq!(
            name(r#"{"name":" Plain\u0007 "}"#).as_deref(),
            Some("Plain")
        );
        assert_eq!(
            name(r#"{"name":"__MSG_EXTNAME__","default_locale":"en"}"#).as_deref(),
            Some("uBlock Origin Lite")
        );
        assert_eq!(
            name(r#"{"name":"__MSG_other__","default_locale":"en"}"#),
            None
        );
        assert_eq!(name(r#"{"name":"__MSG_extName__"}"#), None);
        assert_eq!(
            name(r#"{"name":"__MSG_extName__","default_locale":"../en"}"#),
            None
        );
        assert_eq!(name(r#"{"name":""}"#), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
