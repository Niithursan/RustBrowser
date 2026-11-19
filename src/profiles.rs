//! Browser profiles: separate people or roles on one computer, each with their own
//! cookies, history, bookmarks, passwords, extensions and settings.
//!
//! The first profile is "Default" and lives in the original profile folder (the
//! `root`), so existing data is never moved. Every other profile lives in
//! `root/profiles/<id>`. Each profile runs in its own process with its own lock,
//! so profiles never share a WebView2 environment or a session file.
//!
//! The list of profiles is `root/profiles/profiles.json`. Several profile
//! processes may change it, so every change runs under a short exclusive lock
//! (`registry.lock`) and is written atomically.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const DEFAULT_ID: &str = "default";
pub const MAX_PROFILES: usize = 16;
pub const MAX_NAME: usize = 40;
/// Profile colours: Teal, Green, Purple, Brown, Magenta, Olive, Taupe, Slate. Each
/// is dark enough for a white initial (4.5:1 or more) and stays clear of every
/// theme's signal and error colours, so an avatar never reads as "active" or as a
/// problem (checked by `ui::tests`).
pub const COLORS: [&str; 8] = [
    "#0E6F7C", "#1F7A4D", "#6B3FA0", "#8A4B1F", "#9C2C6E", "#5E6A1E", "#6E5F52", "#56606C",
];
/// Colours of the first profile palette, which matched Light's signal and error
/// colours, and the colour each one now reads as.
const RETIRED_COLORS: [(&str, &str); 5] = [
    ("#2446B8", COLORS[0]),
    ("#A3192A", COLORS[4]),
    ("#7A3FB0", COLORS[2]),
    ("#9A4A00", COLORS[3]),
    ("#A62A63", COLORS[4]),
];
const VERSION: u32 = 1;
const MAX_FILE: u64 = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Registry {
    pub version: u32,
    pub profiles: Vec<Profile>,
    /// The profile a launch with links (and no profile) opens.
    pub last_used: String,
    /// The number in the next new profile's id. Ids are never reused, so a new
    /// profile can never pick up a deleted one's leftover folder.
    pub next: u32,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            version: VERSION,
            profiles: vec![Profile {
                id: DEFAULT_ID.into(),
                name: "Default".into(),
                color: COLORS[0].into(),
            }],
            last_used: DEFAULT_ID.into(),
            next: 2,
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.to_string())
}

/// `default`, or `p` followed by 1 to 6 digits (no leading zero). Ids name
/// folders, so nothing else is ever accepted.
pub fn valid_id(id: &str) -> bool {
    if id == DEFAULT_ID {
        return true;
    }
    let Some(digits) = id.strip_prefix('p') else {
        return false;
    };
    (1..=6).contains(&digits.len())
        && !digits.starts_with('0')
        && digits.bytes().all(|b| b.is_ascii_digit())
}

pub fn valid_color(color: &str) -> bool {
    COLORS.contains(&color)
}

/// A trimmed, non-empty name of at most `MAX_NAME` characters without control
/// characters.
pub fn clean_name(name: &str) -> io::Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("Enter a name for the profile."));
    }
    if name.chars().count() > MAX_NAME {
        return Err(invalid(
            "Use a shorter profile name (at most 40 characters).",
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(invalid("A profile name cannot contain control characters."));
    }
    Ok(name.to_string())
}

/// The folder of profile `id` under `root` (the Default profile's folder).
pub fn dir(root: &Path, id: &str) -> io::Result<PathBuf> {
    if !valid_id(id) {
        return Err(invalid("Unknown profile."));
    }
    Ok(if id == DEFAULT_ID {
        root.to_path_buf()
    } else {
        root.join("profiles").join(id)
    })
}

impl Registry {
    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// The profile a launch without a chosen profile opens: the last used one if
    /// it still exists, otherwise Default.
    pub fn last_used(&self) -> &str {
        if self.get(&self.last_used).is_some() {
            &self.last_used
        } else {
            DEFAULT_ID
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        let default_first = self.profiles.first().is_some_and(|p| p.id == DEFAULT_ID);
        let mut ids = std::collections::HashSet::new();
        let ok = self.version == VERSION
            && default_first
            && self.profiles.len() <= MAX_PROFILES
            && self.profiles.iter().all(|p| {
                valid_id(&p.id)
                    && ids.insert(p.id.as_str())
                    && valid_color(&p.color)
                    && clean_name(&p.name).is_ok_and(|n| n == p.name)
                    && (p.id == DEFAULT_ID || p.id[1..].parse::<u32>().is_ok_and(|n| n < self.next))
            })
            && valid_id(&self.last_used);
        if ok {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The profile list is not valid.",
            ))
        }
    }

    fn name_taken(&self, name: &str, except: Option<&str>) -> bool {
        self.profiles
            .iter()
            .any(|p| Some(p.id.as_str()) != except && p.name.to_lowercase() == name.to_lowercase())
    }

    /// A name not used yet: "Profile 2", "Profile 3", ...
    pub fn suggested_name(&self) -> String {
        (self.profiles.len() + 1..)
            .map(|n| format!("Profile {n}"))
            .find(|name| !self.name_taken(name, None))
            .unwrap_or_else(|| "Profile".into())
    }

    /// The first colour no profile uses yet (or cycle when all are used).
    pub fn suggested_color(&self) -> &'static str {
        COLORS
            .iter()
            .find(|c| !self.profiles.iter().any(|p| p.color == **c))
            .copied()
            .unwrap_or(COLORS[self.profiles.len() % COLORS.len()])
    }

    pub fn create(&mut self, name: &str, color: &str) -> io::Result<Profile> {
        if self.profiles.len() >= MAX_PROFILES {
            return Err(invalid(
                "At most 16 profiles can be created. Delete one first.",
            ));
        }
        let name = clean_name(name)?;
        if self.name_taken(&name, None) {
            return Err(invalid("Another profile already has that name."));
        }
        if !valid_color(color) {
            return Err(invalid("Choose one of the profile colors."));
        }
        let id = format!("p{}", self.next);
        if !valid_id(&id) {
            return Err(invalid("No more profiles can be created."));
        }
        let profile = Profile {
            id,
            name,
            color: color.into(),
        };
        self.next += 1;
        self.profiles.push(profile.clone());
        Ok(profile)
    }

    pub fn edit(&mut self, id: &str, name: &str, color: &str) -> io::Result<()> {
        let name = clean_name(name)?;
        if self.name_taken(&name, Some(id)) {
            return Err(invalid("Another profile already has that name."));
        }
        if !valid_color(color) {
            return Err(invalid("Choose one of the profile colors."));
        }
        let profile = self
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| invalid("That profile no longer exists."))?;
        profile.name = name;
        profile.color = color.into();
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> io::Result<()> {
        if id == DEFAULT_ID {
            return Err(invalid("The Default profile cannot be deleted."));
        }
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        if self.profiles.len() == before {
            return Err(invalid("That profile no longer exists."));
        }
        if self.last_used == id {
            self.last_used = DEFAULT_ID.into();
        }
        Ok(())
    }
}

fn registry_path(root: &Path) -> PathBuf {
    root.join("profiles").join("profiles.json")
}

/// The profile list, or the default list (Default only) when there is none yet.
/// A damaged list is an error, so it is never silently replaced.
pub fn load(root: &Path) -> io::Result<Registry> {
    let path = registry_path(root);
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Registry::default()),
        Err(error) => return Err(error),
    };
    if file.metadata()?.len() > MAX_FILE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The profile list is too large.",
        ));
    }
    let mut registry: Registry = serde_json::from_reader(io::BufReader::new(file))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "The profile list is damaged."))?;
    // A list saved with the first palette keeps working; the next change saves
    // the new colours.
    for profile in &mut registry.profiles {
        if let Some((_, now)) = RETIRED_COLORS.iter().find(|(old, _)| *old == profile.color) {
            profile.color = (*now).into();
        }
    }
    registry.validate()?;
    Ok(registry)
}

/// Change the profile list under its lock: load it, apply `change`, validate and
/// write it atomically. Nothing is written when `change` fails.
pub fn update<T>(
    root: &Path,
    change: impl FnOnce(&mut Registry) -> io::Result<T>,
) -> io::Result<T> {
    let folder = root.join("profiles");
    fs::create_dir_all(&folder)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(folder.join("registry.lock"))?;
    // Held only for a read and a small write, so waiting is short.
    lock.lock()?;
    let mut registry = load(root)?;
    let result = change(&mut registry)?;
    registry.validate()?;
    let bytes = serde_json::to_vec_pretty(&registry)?;
    let path = registry_path(root);
    let temp = path.with_extension("json.tmp");
    let mut file = File::create(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, &path)?;
    drop(lock);
    Ok(result)
}

/// Delete profile `id` and its data. Refused while the profile is open (its
/// session lock is held). The folder is first renamed aside, which is quick, and
/// the returned path is then removed by the caller off the UI thread; anything
/// left over is removed by `sweep_deleted` on a later start.
pub fn delete(root: &Path, id: &str) -> io::Result<Option<PathBuf>> {
    if id == DEFAULT_ID {
        return Err(invalid("The Default profile cannot be deleted."));
    }
    let folder = dir(root, id)?;
    let trash = root.join("profiles").join(format!("{id}.deleted"));
    update(root, |registry| {
        if registry.get(id).is_none() {
            return Err(invalid("That profile no longer exists."));
        }
        if folder.exists() {
            // Fails with WouldBlock while a window of that profile is open.
            let lock = crate::storage::lock(&folder).map_err(|error| {
                if error.kind() == io::ErrorKind::WouldBlock {
                    io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "That profile is open. Close its windows, then delete it.",
                    )
                } else {
                    error
                }
            })?;
            drop(lock);
            if trash.exists() {
                fs::remove_dir_all(&trash)?;
            }
            fs::rename(&folder, &trash)?;
        }
        registry.remove(id)
    })?;
    Ok(trash.exists().then_some(trash))
}

/// Remove folders of deleted profiles that could not be removed before.
pub fn sweep_deleted(root: &Path) -> usize {
    let Ok(entries) = fs::read_dir(root.join("profiles")) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.strip_suffix(".deleted").is_some_and(valid_id)
                && entry.file_type().is_ok_and(|t| t.is_dir())
        })
        .filter(|entry| fs::remove_dir_all(entry.path()).is_ok())
        .count()
}

/// Whether a window of profile `id` is open in another process (its session lock
/// is held). Never creates the profile's folder.
pub fn is_open(root: &Path, id: &str) -> bool {
    let Ok(folder) = dir(root, id) else {
        return false;
    };
    let Ok(lock) = OpenOptions::new()
        .read(true)
        .write(true)
        .open(folder.join("session.lock"))
    else {
        return false;
    };
    matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

/// Record that profile `id` was just opened.
pub fn mark_used(root: &Path, id: &str) -> io::Result<()> {
    update(root, |registry| {
        if registry.get(id).is_some() && registry.last_used != id {
            registry.last_used = id.into();
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rust-browser-profiles-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn ids_are_strict_folder_names() {
        for good in ["default", "p2", "p10", "p999999"] {
            assert!(valid_id(good), "{good}");
        }
        for bad in [
            "", "p", "p0", "p01", "p1234567", "P2", "p2 ", "..", "p2/..", "p-2", "default2", "p２",
        ] {
            assert!(!valid_id(bad), "{bad:?}");
        }
        let root = Path::new("C:/root");
        assert_eq!(dir(root, "default").unwrap(), root);
        assert_eq!(dir(root, "p3").unwrap(), root.join("profiles").join("p3"));
        assert!(dir(root, "../x").is_err());
    }

    #[test]
    fn names_are_trimmed_bounded_and_unique() {
        assert_eq!(clean_name("  Work  ").unwrap(), "Work");
        assert!(clean_name("   ").is_err());
        assert!(clean_name(&"x".repeat(41)).is_err());
        assert!(clean_name("a\u{7}b").is_err());
        let mut registry = Registry::default();
        let work = registry.create("Work", COLORS[1]).unwrap();
        assert_eq!(work.id, "p2");
        assert!(registry.create("work", COLORS[2]).is_err());
        assert!(registry.create("Home", "#123456").is_err());
        assert!(registry.edit("p2", "default", COLORS[1]).is_err());
        registry.edit("p2", "Work stuff", COLORS[3]).unwrap();
        assert_eq!(registry.get("p2").unwrap().color, COLORS[3]);
        registry.validate().unwrap();
    }

    #[test]
    fn ids_are_never_reused_and_default_stays() {
        let mut registry = Registry::default();
        registry.create("A", COLORS[1]).unwrap();
        registry.last_used = "p2".into();
        registry.remove("p2").unwrap();
        assert_eq!(registry.last_used, DEFAULT_ID);
        assert_eq!(registry.create("B", COLORS[1]).unwrap().id, "p3");
        assert!(registry.remove(DEFAULT_ID).is_err());
        assert!(registry.remove("p2").is_err());
        for n in 0..MAX_PROFILES - 2 {
            registry.create(&format!("Extra {n}"), COLORS[0]).unwrap();
        }
        assert!(registry.create("One too many", COLORS[0]).is_err());
        registry.validate().unwrap();
    }

    #[test]
    fn lists_with_the_first_palette_still_load() {
        let root = temp("retired");
        fs::create_dir_all(root.join("profiles")).unwrap();
        fs::write(
            registry_path(&root),
            r##"{"version":1,"profiles":[{"id":"default","name":"Default","color":"#2446B8"},{"id":"p2","name":"Work","color":"#A3192A"},{"id":"p3","name":"Home","color":"#0E6F7C"}],"last_used":"p2","next":4}"##,
        )
        .unwrap();
        let registry = load(&root).unwrap();
        let colors: Vec<_> = registry.profiles.iter().map(|p| p.color.as_str()).collect();
        assert_eq!(colors, [COLORS[0], COLORS[4], COLORS[0]]);
        assert!(RETIRED_COLORS.iter().all(|(old, _)| !COLORS.contains(old)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn open_profiles_are_seen_without_creating_folders() {
        let root = temp("open");
        assert!(!is_open(&root, "p2"));
        assert!(!dir(&root, "p2").unwrap().exists());
        let held = crate::storage::lock(&dir(&root, "p2").unwrap()).unwrap();
        assert!(is_open(&root, "p2"));
        drop(held);
        assert!(!is_open(&root, "p2"));
        assert!(!is_open(&root, "../x"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn suggestions_avoid_taken_names_and_colours() {
        let mut registry = Registry::default();
        assert_eq!(registry.suggested_name(), "Profile 2");
        assert_eq!(registry.suggested_color(), COLORS[1]);
        registry.create("Profile 2", COLORS[1]).unwrap();
        assert_eq!(registry.suggested_name(), "Profile 3");
        assert_eq!(registry.suggested_color(), COLORS[2]);
    }

    #[test]
    fn invalid_lists_are_rejected_not_replaced() {
        let mut registry = Registry::default();
        registry.profiles.remove(0);
        assert!(registry.validate().is_err());
        let mut registry = Registry::default();
        registry.profiles.push(Profile {
            id: "p9".into(),
            name: "Ahead".into(),
            color: COLORS[0].into(),
        });
        assert!(registry.validate().is_err(), "an id at or past `next`");
        let root = temp("damaged");
        fs::create_dir_all(root.join("profiles")).unwrap();
        fs::write(registry_path(&root), b"{not json").unwrap();
        assert!(load(&root).is_err());
        assert!(update(&root, |_| Ok(())).is_err());
        assert_eq!(fs::read(registry_path(&root)).unwrap(), b"{not json");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn updates_persist_and_failed_changes_write_nothing() {
        let root = temp("update");
        assert_eq!(load(&root).unwrap(), Registry::default());
        let id = update(&root, |r| r.create("Work", COLORS[1]).map(|p| p.id)).unwrap();
        mark_used(&root, &id).unwrap();
        let saved = load(&root).unwrap();
        assert_eq!(saved.profiles.len(), 2);
        assert_eq!(saved.last_used(), "p2");
        assert!(update(&root, |r| r.create("work", COLORS[2])).is_err());
        assert_eq!(load(&root).unwrap(), saved);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn deleting_moves_data_aside_and_refuses_open_profiles() {
        let root = temp("delete");
        update(&root, |r| r.create("Work", COLORS[1])).unwrap();
        let folder = dir(&root, "p2").unwrap();
        fs::create_dir_all(folder.join("content")).unwrap();
        fs::write(folder.join("content").join("cookie"), b"x").unwrap();
        let held = crate::storage::lock(&folder).unwrap();
        let error = delete(&root, "p2").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(load(&root).unwrap().get("p2").is_some());
        drop(held);
        let trash = delete(&root, "p2").unwrap().unwrap();
        assert!(!folder.exists());
        assert!(load(&root).unwrap().get("p2").is_none());
        assert_eq!(sweep_deleted(&root), 1);
        assert!(!trash.exists());
        assert!(delete(&root, DEFAULT_ID).is_err());
        assert!(delete(&root, "p2").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
