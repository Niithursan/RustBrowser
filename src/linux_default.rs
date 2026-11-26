//! Settings > System > Make default browser on Linux. Unlike Windows, where
//! only the user may choose in Windows Settings, the freedesktop rules let an
//! application register itself: Rust Browser writes a launcher
//! (`rust-browser.desktop`) to the user's applications folder and makes it the
//! default for web links through GIO, which records the choice in the user's
//! `mimeapps.list`, the list GNOME, KDE and other desktops read. Only HTTP and
//! HTTPS links are claimed: local files are refused by the browser anyway.
//!
//! An installed package (`.deb` or Arch package) already puts the same launcher
//! in `/usr/share/applications` (`packaging/linux/rust-browser.desktop`, which a
//! test keeps equal to [`desktop_entry`]). When that launcher runs this
//! program, it is made the default as it is, and nothing is written.
use std::path::{Path, PathBuf};
use webkit2gtk::gio::{self, prelude::AppInfoExt};

/// The launcher's file name, which is also its desktop ID.
pub const DESKTOP_ID: &str = "rust-browser.desktop";
/// The icon's name in the icon theme. The packages install it in `hicolor`;
/// [`make_default`] copies it to the user's icons for any other copy.
const ICON_NAME: &str = "rust-browser";
/// The icon itself (the same drawing as the Windows icon).
const ICON_SVG: &str = include_str!("../packaging/windows/rust-browser.svg");
/// The X11 window class (`WM_CLASS`), which winit takes from the program's
/// file name; it lets the desktop match the browser's windows to the launcher.
const WINDOW_CLASS: &str = "rust-browser";
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
Icon={ICON_NAME}
Terminal=false
StartupNotify=true
StartupWMClass={WINDOW_CLASS}
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

/// The user's data folder: `$XDG_DATA_HOME`, or `~/.local/share` when that is
/// unset or not absolute.
fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|home| home.join(".local").join("share"))
        })
}

/// The launcher a package installed outside the user's folder `user_dir`, when
/// it runs the program at `exe`. A launcher in the user's own folder is not
/// one: that is the one [`make_default`] writes, and it is written again.
fn installed_launcher(exe: &Path, user_dir: &Path) -> Option<gio::DesktopAppInfo> {
    let app = gio::DesktopAppInfo::new(DESKTOP_ID)?;
    let file = app.filename()?;
    if file.parent() == Some(user_dir) {
        return None;
    }
    same_file(&app.executable(), exe).then_some(app)
}

/// Whether two paths name the same file once links are followed.
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Write `text` to `file` whole: into a hidden file beside it, then renamed, so
/// the desktop never reads half a file.
fn write_whole(file: &Path, text: &str) -> std::io::Result<()> {
    let (Some(dir), Some(name)) = (file.parent(), file.file_name()) else {
        return Err(std::io::ErrorKind::InvalidInput.into());
    };
    std::fs::create_dir_all(dir)?;
    let temp = dir.join(format!(".{}.tmp", name.to_string_lossy()));
    std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, file))
}

/// Whether the desktop opens HTTPS links with this launcher.
pub fn is_default() -> bool {
    gio::AppInfo::default_for_uri_scheme("https")
        .and_then(|app| app.id())
        .is_some_and(|id| id == DESKTOP_ID)
}

/// Make this program the default for web links: through the package's own
/// launcher when that runs this program, otherwise through a launcher (and the
/// icon) written to the user's folders.
pub fn make_default() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|_| "Rust Browser could not find its own program file.".to_string())?;
    let data = data_dir().ok_or("Your home folder could not be found.")?;
    let dir = data.join("applications");
    let app = match installed_launcher(&exe, &dir) {
        Some(app) => app,
        None => {
            let entry = desktop_entry(&exe)?;
            let file = dir.join(DESKTOP_ID);
            write_whole(&file, &entry)
                .map_err(|e| format!("The launcher could not be written ({e})."))?;
            // Without the icon the launcher still works, with the theme's
            // fallback icon, so a failure here is not reported.
            let icon = data
                .join("icons/hicolor/scalable/apps")
                .join(format!("{ICON_NAME}.svg"));
            let _ = write_whole(&icon, ICON_SVG);
            gio::DesktopAppInfo::from_filename(&file)
                .ok_or("The desktop could not read Rust Browser's launcher.")?
        }
    };
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
        assert!(entry.contains("\nIcon=rust-browser\n"));
        assert!(entry.contains("\nStartupWMClass=rust-browser\n"));
        assert!(!entry.contains("text/html"));
        assert!(!entry.contains("file"));
    }

    #[test]
    fn the_packaged_launcher_is_the_one_for_the_installed_program() {
        let packaged = include_str!("../packaging/linux/rust-browser.desktop");
        assert_eq!(
            packaged,
            desktop_entry(Path::new("/usr/bin/rust-browser")).unwrap()
        );
    }

    #[test]
    fn the_window_class_and_icon_match_the_installed_program() {
        // winit names X11 windows after the program file, which the packages
        // install as /usr/bin/rust-browser, beside the icon of the same name.
        assert_eq!(WINDOW_CLASS, env!("CARGO_PKG_NAME"));
        assert_eq!(ICON_NAME, env!("CARGO_PKG_NAME"));
        assert!(ICON_SVG.starts_with("<svg "));
    }

    #[test]
    fn launcher_files_are_replaced_whole() {
        let dir = std::env::temp_dir().join(format!("rb-default-{}", std::process::id()));
        let file = dir.join("applications").join(DESKTOP_ID);
        write_whole(&file, "first").unwrap();
        write_whole(&file, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "second");
        assert!(
            !dir.join("applications")
                .join(".rust-browser.desktop.tmp")
                .exists()
        );
        std::fs::remove_dir_all(&dir).unwrap();
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
