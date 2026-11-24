//! Saved passwords and form data on Linux, kept in the desktop keyring (the
//! Secret Service: GNOME Keyring, KWallet and others) as GNOME Web does. The
//! keyring encrypts the items and unlocks them with the user's login; Rust
//! Browser keeps no copy and writes no cryptography of its own.
//!
//! Every item carries the profile it belongs to, so profiles never see each
//! other's passwords. Calls are asynchronous and their callbacks run on the UI
//! thread from GTK's main loop; the keyring may ask the user to unlock it.
use gtk::glib;
use libsecret::{Schema, SchemaAttributeType, SchemaFlags, SearchFlags, prelude::*};
use std::{collections::HashMap, path::Path};
use webkit2gtk::gio;

/// The schema name the keyring shows and searches by.
const SCHEMA: &str = "org.rustbrowser.Credentials";
/// A login, or the profile's saved contact details.
const PASSWORD: &str = "password";
const CONTACT: &str = "contact";
/// The most saved logins listed for one site.
pub const MAX_LOGINS: usize = 20;

fn schema() -> Schema {
    let attributes = HashMap::from([
        ("profile", SchemaAttributeType::String),
        ("kind", SchemaAttributeType::String),
        ("origin", SchemaAttributeType::String),
        ("username", SchemaAttributeType::String),
    ]);
    Schema::new(SCHEMA, SchemaFlags::NONE, attributes)
}

/// A keyring for one profile.
#[derive(Clone)]
pub struct Keyring {
    /// Names the profile in every item: part of a digest of its folder.
    profile: String,
}

/// What the keyring reported when it could not do something.
fn reason(error: &glib::Error) -> String {
    format!(
        "The keyring could not be used ({}). Check that a keyring such as GNOME Keyring or KWallet is running and unlocked.",
        error.message()
    )
}

impl Keyring {
    /// The keyring items of the profile in `folder`.
    pub fn for_profile(folder: &Path) -> Self {
        use std::os::unix::ffi::OsStrExt;
        let digest = glib::compute_checksum_for_data(
            glib::ChecksumType::Sha256,
            folder.as_os_str().as_bytes(),
        )
        .map(|d| d[..32].to_string())
        .unwrap_or_default();
        Self { profile: digest }
    }

    fn attributes<'a>(&'a self, kind: &'a str) -> HashMap<&'a str, &'a str> {
        HashMap::from([("profile", self.profile.as_str()), ("kind", kind)])
    }

    /// Save `password` for `username` on `origin`, replacing a saved one.
    pub fn save_login(
        &self,
        origin: &str,
        username: &str,
        password: &str,
        done: impl FnOnce(Result<(), String>) + 'static,
    ) {
        let mut attributes = self.attributes(PASSWORD);
        attributes.insert("origin", origin);
        attributes.insert("username", username);
        let label = format!("Rust Browser: {username} on {origin}");
        libsecret::password_store(
            Some(&schema()),
            attributes,
            Some(libsecret::COLLECTION_DEFAULT),
            &label,
            password,
            gio::Cancellable::NONE,
            move |result| done(result.map_err(|e| reason(&e))),
        );
    }

    /// The usernames saved for `origin`, without unlocking any password.
    pub fn usernames(
        &self,
        origin: &str,
        done: impl FnOnce(Result<Vec<String>, String>) + 'static,
    ) {
        let mut attributes = self.attributes(PASSWORD);
        attributes.insert("origin", origin);
        libsecret::password_search(
            Some(&schema()),
            attributes,
            SearchFlags::ALL,
            gio::Cancellable::NONE,
            move |result| {
                done(result.map_err(|e| reason(&e)).map(|items| {
                    let mut names: Vec<String> = items
                        .iter()
                        .filter_map(|item| item.attributes().get("username").cloned())
                        .collect();
                    names.sort();
                    names.dedup();
                    names.truncate(MAX_LOGINS);
                    names
                }))
            },
        );
    }

    /// The password saved for `username` on `origin`, if any. The keyring
    /// may ask the user to unlock it first.
    pub fn password(
        &self,
        origin: &str,
        username: &str,
        done: impl FnOnce(Result<Option<String>, String>) + 'static,
    ) {
        let mut attributes = self.attributes(PASSWORD);
        attributes.insert("origin", origin);
        attributes.insert("username", username);
        libsecret::password_lookup(
            Some(&schema()),
            attributes,
            gio::Cancellable::NONE,
            move |result| {
                done(
                    result
                        .map(|p| p.map(|p| p.to_string()))
                        .map_err(|e| reason(&e)),
                )
            },
        );
    }

    /// Save the profile's contact details (JSON of field kind to value).
    pub fn save_contact(&self, json: &str, done: impl FnOnce(Result<(), String>) + 'static) {
        libsecret::password_store(
            Some(&schema()),
            self.attributes(CONTACT),
            Some(libsecret::COLLECTION_DEFAULT),
            "Rust Browser: contact details for filling forms",
            json,
            gio::Cancellable::NONE,
            move |result| done(result.map_err(|e| reason(&e))),
        );
    }

    /// Whether contact details are saved, without unlocking them.
    pub fn has_contact(&self, done: impl FnOnce(bool) + 'static) {
        libsecret::password_search(
            Some(&schema()),
            self.attributes(CONTACT),
            SearchFlags::NONE,
            gio::Cancellable::NONE,
            move |result| done(result.is_ok_and(|items| !items.is_empty())),
        );
    }

    /// The profile's saved contact details (JSON), if any.
    pub fn contact(&self, done: impl FnOnce(Result<Option<String>, String>) + 'static) {
        libsecret::password_lookup(
            Some(&schema()),
            self.attributes(CONTACT),
            gio::Cancellable::NONE,
            move |result| {
                done(
                    result
                        .map(|p| p.map(|p| p.to_string()))
                        .map_err(|e| reason(&e)),
                )
            },
        );
    }

    /// Delete every saved login (`passwords`) or the contact details of this
    /// profile; other profiles' items are untouched.
    pub fn clear(&self, passwords: bool, done: impl FnOnce(Result<(), String>) + 'static) {
        libsecret::password_clear(
            Some(&schema()),
            self.attributes(if passwords { PASSWORD } else { CONTACT }),
            gio::Cancellable::NONE,
            move |result| match result {
                // Nothing saved is nothing to clear.
                Err(e) if e.matches(gio::IOErrorEnum::NotFound) => done(Ok(())),
                other => done(other.map_err(|e| reason(&e))),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_have_separate_items() {
        let a = Keyring::for_profile(Path::new("/home/me/.local/share/rust-browser/v0.1"));
        let b = Keyring::for_profile(Path::new(
            "/home/me/.local/share/rust-browser/v0.1/profiles/p2",
        ));
        assert_eq!(a.profile.len(), 32);
        assert_ne!(a.profile, b.profile);
        assert_eq!(
            a.attributes(PASSWORD).get("profile"),
            Some(&a.profile.as_str())
        );
        assert_eq!(a.attributes(CONTACT).get("kind"), Some(&CONTACT));
    }
}
