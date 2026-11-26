//! Settings > System > Make default browser on Linux. Unlike Windows, where
//! only the user may choose in Windows Settings, the freedesktop rules let an
//! application register itself: Rust Browser writes a launcher
//! (`rust-browser.desktop`) to the user's applications folder and makes it the
//! default for web links through GIO, which records the choice in the user's
//! `mimeapps.list`, the list GNOME, KDE and other desktops read. Only HTTP and
//! HTTPS links are claimed: local files are refused by the browser anyway.
use std::path::{Path, PathBuf};
use webkit2gtk::gio::{self, prelude::AppInfoExt};

/// The launcher's file name, which is also its desktop ID.
pub const DESKTOP_ID: &str = "rust-browser.desktop";
/// The link types the browser takes.
const LINK_TYPES: [&str; 2] = ["x-scheme-handler/http", "x-scheme-handler/https"];

/// The launcher for the browser at `exe`, or why it cannot be written. A path
/// with characters a launcher's command line would have to escape is refused
/// rather than escaped.
pub fn desktop_entry(exe: &Path) -> Result<String, String> {
    let path = exe
        .to_str()
        .filter(|p| {
            exe.is_absolute()
                && !p.chars().any(|c| {
                    c.is_control() || matches!(c, '"' | '`' | '$' | '\\' | '%')
                })
        })
        .ok_or("Rust Browser is in a folder whose name a launcher cannot use. Move it to a folder without quotes, backslashes, $, % or ` in its path and try again.")?;
    Ok(format!(
        "[Desktop Entry]
Type=Application
Version=1.5
Name=Rust Browser
GenericName=Web Browser
Comment=Browse the web
Exec=\"{path}\" %U
Icon=web-browser
Terminal=false
StartupNotify=true
Categories=Network;WebBrowser;
MimeType={types};
Actions=new-window;new-private-window;

[Desktop Action new-window]
Name=New window
Exec=\"{path}\"

[Desktop Action new-private-window]
Name=New private window
Exec=\"{path}\" --private
",
        types = LINK_TYPES.join(";")
    ))
}

/// The user's applications folder: `$XDG_DATA_HOME/applications`, or
/// `~/.local/share/applications` when that is unset or not absolute.
fn applications_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|home| home.join(".local").join("share"))
        })
        .map(|data| data.join("applications"))
}

/// Whether the desktop opens HTTPS links with this launcher.
pub fn is_default() -> bool {
    gio::AppInfo::default_for_uri_scheme("https")
        .and_then(|app| app.id())
        .is_some_and(|id| id == DESKTOP_ID)
}

/// Write the launcher for this program and make it the default for web links.
pub fn make_default() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|_| "Rust Browser could not find its own program file.".to_string())?;
    let entry = desktop_entry(&exe)?;
    let dir = applications_dir().ok_or("Your home folder could not be found.")?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("The applications folder could not be created ({e})."))?;
    let file = dir.join(DESKTOP_ID);
    // Written whole and renamed, so the desktop never reads half a launcher.
    let temp = dir.join(format!(".{DESKTOP_ID}.tmp"));
    std::fs::write(&temp, entry)
        .and_then(|()| std::fs::rename(&temp, &file))
        .map_err(|e| format!("The launcher could not be written ({e})."))?;
    let app = gio::DesktopAppInfo::from_filename(&file)
        .ok_or("The desktop could not read Rust Browser's launcher.")?;
    for kind in LINK_TYPES {
        app.set_as_default_for_type(kind)
            .map_err(|e| format!("The desktop did not accept Rust Browser for web links ({e})."))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launcher_runs_this_program_for_web_links_only() {
        let entry = desktop_entry(Path::new("/opt/Rust Browser/rust-browser")).unwrap();
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("\nExec=\"/opt/Rust Browser/rust-browser\" %U\n"));
        assert!(entry.contains("\nExec=\"/opt/Rust Browser/rust-browser\" --private\n"));
        assert!(entry.contains("\nMimeType=x-scheme-handler/http;x-scheme-handler/https;\n"));
        assert!(!entry.contains("text/html"));
        assert!(!entry.contains("file"));
    }

    #[test]
    fn paths_a_command_line_would_need_to_escape_are_refused() {
        for bad in [
            "relative/rust-browser",
            "/opt/a\"b/rust-browser",
            "/opt/$HOME/rust-browser",
            "/opt/a\\b/rust-browser",
            "/opt/100%/rust-browser",
            "/opt/a`b`/rust-browser",
            "/opt/a\nExec=evil/rust-browser",
        ] {
            assert!(desktop_entry(Path::new(bad)).is_err(), "{bad:?}");
        }
    }
}
