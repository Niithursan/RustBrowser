//! The new-tab page's own picture ("Your image" in Customize): a PNG, JPEG or
//! WebP the user chose, copied into the profile folder and shown by the trusted
//! toolbar as a `data:` URL, the only kind of image its security policy allows.
//! Nothing is downloaded; the file is checked by its first bytes, not its name.
use base64::Engine as _;
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

/// The copy's file name in the profile folder.
pub const FILE: &str = "newtab-image";
/// The largest picture accepted: a full-screen photo fits, and every window's
/// toolbar holds at most one copy of it.
pub const MAX_BYTES: u64 = 6 * 1024 * 1024;

/// The image type of `bytes` from their signature, or `None` for anything else.
pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Read a picture, refusing anything larger than `MAX_BYTES` before reading it
/// all, and anything that is not a PNG, JPEG or WebP.
pub fn read(path: &Path) -> Result<(Vec<u8>, &'static str), String> {
    let file = fs::File::open(path).map_err(|_| "The picture could not be opened.".to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "The picture could not be read.".to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Choose a picture smaller than 6 MB.".into());
    }
    let kind = image_type(&bytes).ok_or("Choose a PNG, JPEG or WebP picture.".to_string())?;
    Ok((bytes, kind))
}

/// The picture as a `data:` URL for the toolbar.
pub fn data_url(bytes: &[u8], kind: &str) -> String {
    format!(
        "data:{kind};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Check the chosen picture, copy it into `profile` (written whole, then
/// renamed over the last one) and return it as a `data:` URL.
pub fn choose(profile: &Path, chosen: &Path) -> Result<String, String> {
    let (bytes, kind) = read(chosen)?;
    let temp = profile.join(format!("{FILE}.tmp"));
    fs::write(&temp, &bytes)
        .and_then(|()| fs::rename(&temp, profile.join(FILE)))
        .map_err(|error: io::Error| {
            let _ = fs::remove_file(&temp);
            format!("The picture could not be saved in the profile ({error}).")
        })?;
    Ok(data_url(&bytes, kind))
}

/// The saved picture as a `data:` URL, or `None` when there is none or it is
/// no longer a picture.
pub fn load(profile: &Path) -> Option<String> {
    read(&profile.join(FILE))
        .ok()
        .map(|(bytes, kind)| data_url(&bytes, kind))
}

/// The script that gives the toolbar the picture (`None`: there is none).
pub fn script(url: Option<&str>) -> String {
    let json = serde_json::to_string(&url).unwrap_or_else(|_| "null".into());
    format!("window.setWallpaper&&setWallpaper({json})")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rb-wallpaper-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pictures_are_known_by_their_first_bytes() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(image_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        for other in [
            &b"<svg xmlns"[..],
            b"GIF89a",
            b"RIFF\0\0\0\0WAVE",
            b"",
            b"\x89PN",
        ] {
            assert_eq!(image_type(other), None, "{other:?}");
        }
    }

    #[test]
    fn a_chosen_picture_is_copied_and_loads_again() {
        let dir = folder("copy");
        let chosen = dir.join("photo.anything");
        fs::write(&chosen, b"\x89PNG\r\n\x1a\npixels").unwrap();
        let url = choose(&dir, &chosen).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(load(&dir).as_deref(), Some(url.as_str()));
        assert!(!dir.join(format!("{FILE}.tmp")).exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn other_files_and_large_pictures_are_refused_and_keep_the_last_picture() {
        let dir = folder("refuse");
        let good = dir.join("good.png");
        fs::write(&good, b"\x89PNG\r\n\x1a\nkept").unwrap();
        choose(&dir, &good).unwrap();
        let svg = dir.join("image.png");
        fs::write(&svg, b"<svg onload=alert(1)>").unwrap();
        assert!(
            choose(&dir, &svg)
                .unwrap_err()
                .contains("PNG, JPEG or WebP")
        );
        let big = dir.join("big.jpg");
        let mut bytes = vec![0xFF, 0xD8, 0xFF];
        bytes.resize(MAX_BYTES as usize + 1, 0);
        fs::write(&big, bytes).unwrap();
        assert!(choose(&dir, &big).unwrap_err().contains("6 MB"));
        assert!(load(&dir).unwrap().ends_with(
            &base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nkept")
        ));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_saved_picture_loads_as_none_and_scripts_quote_the_url() {
        let dir = folder("none");
        assert_eq!(load(&dir), None);
        assert_eq!(script(None), "window.setWallpaper&&setWallpaper(null)");
        assert_eq!(
            script(Some("data:image/png;base64,AA==")),
            "window.setWallpaper&&setWallpaper(\"data:image/png;base64,AA==\")"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
