//! Saved passwords and form filling on Linux: the form helper script
//! (`linux_forms.js`) and what the browser accepts from it.
//!
//! The helper runs in its own script world in the top frame of normal
//! website views. Page scripts run in another world: they cannot see the
//! helper, call its fill functions or post to its message handler, so a page
//! cannot invent a login to save or read one that is filled. Every message is
//! bounded and checked, and its origin must be the view's own. Filling
//! happens only when the user chooses it in the toolbar; values reach the
//! page as call arguments, never as script text.
use gtk::glib;
use javascriptcore::ValueExt;
use serde::Deserialize;
use std::collections::BTreeMap;
use webkit2gtk::gio;
use webkit2gtk::{
    UserContentInjectedFrames, UserContentManagerExt, UserScript, UserScriptInjectionTime,
    WebViewExt,
};

/// The helper's script world and message handler.
pub const WORLD: &str = "rust-browser-forms";
const HANDLER: &str = "rustBrowserForms";
const SCRIPT: &str = include_str!("linux_forms.js");
/// The longest message the helper may send.
const MAX_MESSAGE: usize = 16 * 1024;
const MAX_USERNAME: usize = 256;
const MAX_PASSWORD: usize = 1024;
const MAX_VALUE: usize = 256;
/// The contact fields the helper may report, by autocomplete token.
pub const CONTACT_FIELDS: [&str; 15] = [
    "name",
    "given-name",
    "additional-name",
    "family-name",
    "email",
    "tel",
    "organization",
    "street-address",
    "address-line1",
    "address-line2",
    "address-level1",
    "address-level2",
    "postal-code",
    "country",
    "country-name",
];

/// What the helper reports.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Message {
    /// The page has a login form, or a contact form.
    Fields {
        origin: String,
        login: bool,
        contact: bool,
    },
    /// A login was submitted.
    Login {
        origin: String,
        username: String,
        password: String,
    },
    /// Contact details were submitted.
    Contact {
        origin: String,
        values: BTreeMap<String, String>,
    },
}

impl Message {
    pub fn origin(&self) -> &str {
        match self {
            Self::Fields { origin, .. }
            | Self::Login { origin, .. }
            | Self::Contact { origin, .. } => origin,
        }
    }
}

/// Text from a page: within `max` characters and without control characters.
fn plain(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(char::is_control)
}

/// A message from the helper, if it is well formed and within limits.
pub fn read(body: &str) -> Option<Message> {
    if body.len() > MAX_MESSAGE {
        return None;
    }
    let message: Message = serde_json::from_str(body).ok()?;
    let ok = match &message {
        Message::Fields { .. } => true,
        Message::Login {
            username, password, ..
        } => !password.is_empty() && plain(username, MAX_USERNAME) && plain(password, MAX_PASSWORD),
        Message::Contact { values, .. } => {
            !values.is_empty()
                && values.iter().all(|(field, value)| {
                    CONTACT_FIELDS.contains(&field.as_str())
                        && !value.trim().is_empty()
                        && plain(value, MAX_VALUE)
                })
        }
    };
    ok.then_some(message)
}

/// The origin passwords are saved under for a page at `url`: HTTPS sites,
/// and HTTP only on this computer (loopback), as for site permissions.
pub fn origin(url: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    let secure = match url.scheme() {
        "https" => true,
        "http" => {
            matches!(url.host(), Some(url::Host::Domain("localhost")))
                || matches!(url.host(), Some(url::Host::Ipv4(ip)) if ip.is_loopback())
                || matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback())
        }
        _ => false,
    };
    secure.then(|| url.origin().ascii_serialization())
}

/// Install the helper in a website view and pass its messages, from that view
/// only, to `message`.
pub fn install(view: &webkit2gtk::WebView, message: impl Fn(Message) + 'static) {
    let Some(manager) = view.user_content_manager() else {
        return;
    };
    if !manager.register_script_message_handler_in_world(HANDLER, WORLD) {
        return;
    }
    manager.add_script(&UserScript::for_world(
        SCRIPT,
        UserContentInjectedFrames::TopFrame,
        UserScriptInjectionTime::End,
        WORLD,
        &[],
        &[],
    ));
    let page = view.clone();
    manager.connect_script_message_received(Some(HANDLER), move |_, result| {
        let Some(body) = result.js_value().map(|value| value.to_str().to_string()) else {
            return;
        };
        // Only a message about the page the view shows now.
        let shown = page.uri().and_then(|uri| origin(&uri));
        if let Some(found) = read(&body)
            && shown.as_deref() == Some(found.origin())
        {
            message(found);
        }
    });
}

/// Call one of the helper's fill functions with `arguments`.
fn call(view: &webkit2gtk::WebView, body: &str, arguments: &[(&str, &str)]) {
    let dict = glib::VariantDict::new(None);
    for (name, value) in arguments {
        dict.insert(name, value);
    }
    view.call_async_javascript_function(
        body,
        Some(&dict.end()),
        Some(WORLD),
        None,
        gio::Cancellable::NONE,
        |_| {},
    );
}

/// Fill the page's login form with a saved login.
pub fn fill_login(view: &webkit2gtk::WebView, username: &str, password: &str) {
    call(
        view,
        "return window.rustBrowserFillLogin(username, password);",
        &[("username", username), ("password", password)],
    );
}

/// Fill the page's empty contact fields with saved details (JSON).
pub fn fill_contact(view: &webkit2gtk::WebView, json: &str) {
    call(
        view,
        "return window.rustBrowserFillContact(values);",
        &[("values", json)],
    );
}

/// Merge newly submitted contact details into the saved ones (JSON), keeping
/// only known fields.
pub fn merge_contact(saved: Option<&str>, values: BTreeMap<String, String>) -> String {
    let mut merged: BTreeMap<String, String> = saved
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    merged
        .retain(|field, value| CONTACT_FIELDS.contains(&field.as_str()) && plain(value, MAX_VALUE));
    merged.extend(values);
    serde_json::to_string(&merged).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bounded_well_formed_messages_are_read() {
        assert_eq!(
            read(r#"{"kind":"login","origin":"https://a.test","username":"me","password":"pw"}"#),
            Some(Message::Login {
                origin: "https://a.test".into(),
                username: "me".into(),
                password: "pw".into(),
            })
        );
        assert!(
            read(r#"{"kind":"fields","origin":"https://a.test","login":true,"contact":false}"#)
                .is_some()
        );
        for bad in [
            r#"{"kind":"login","origin":"https://a.test","username":"me","password":""}"#
                .to_string(),
            r#"{"kind":"login","origin":"https://a.test","username":"a\nb","password":"pw"}"#
                .into(),
            format!(
                r#"{{"kind":"login","origin":"o","username":"me","password":"{}"}}"#,
                "p".repeat(1025)
            ),
            r#"{"kind":"contact","origin":"https://a.test","values":{"cc-number":"4111"}}"#.into(),
            r#"{"kind":"contact","origin":"https://a.test","values":{}}"#.into(),
            r#"{"kind":"login","origin":"o","username":"me","password":"pw","extra":1}"#.into(),
            r#"{"kind":"run","code":"x"}"#.into(),
            "x".repeat(MAX_MESSAGE + 1),
        ] {
            assert_eq!(read(&bad), None, "{bad}");
        }
    }

    #[test]
    fn passwords_are_kept_only_for_secure_or_local_sites() {
        assert_eq!(
            origin("https://a.test/login?x=1").as_deref(),
            Some("https://a.test")
        );
        assert_eq!(
            origin("https://a.test:8443/").as_deref(),
            Some("https://a.test:8443")
        );
        assert_eq!(
            origin("http://localhost:8000/x").as_deref(),
            Some("http://localhost:8000")
        );
        assert_eq!(
            origin("http://127.0.0.1/").as_deref(),
            Some("http://127.0.0.1")
        );
        assert_eq!(origin("http://a.test/"), None);
        assert_eq!(origin("about:blank"), None);
        assert_eq!(origin("rust-browser://toolbar/"), None);
    }

    #[test]
    fn contact_details_merge_known_fields_only() {
        let first = merge_contact(None, BTreeMap::from([("email".into(), "a@b.test".into())]));
        let saved = r#"{"email":"old@b.test","tel":"123","cc-number":"4111"}"#;
        let merged: BTreeMap<String, String> = serde_json::from_str(&merge_contact(
            Some(saved),
            BTreeMap::from([("email".into(), "new@b.test".into())]),
        ))
        .unwrap();
        assert_eq!(first, r#"{"email":"a@b.test"}"#);
        assert_eq!(merged.get("email").map(String::as_str), Some("new@b.test"));
        assert_eq!(merged.get("tel").map(String::as_str), Some("123"));
        assert!(!merged.contains_key("cc-number"));
    }

    #[test]
    fn the_script_reports_only_what_the_browser_reads() {
        for field in CONTACT_FIELDS {
            assert!(SCRIPT.contains(&format!("'{field}'")), "{field}");
        }
        assert!(SCRIPT.contains(HANDLER));
        assert!(!SCRIPT.contains("cc-"));
    }
}
