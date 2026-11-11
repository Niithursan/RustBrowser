//! User-facing text in the sources is stored as UTF-8 once. v0.18 shipped status
//! text such as "Ready Â· Ctrl+L" because app.rs had been re-encoded (UTF-8 read as
//! Windows-1252 and written again); these sequences never appear in real text.
use std::path::Path;

/// Byte patterns a re-encoding leaves behind: "Â" before a Latin-1 punctuation
/// mark, and "â€" (the start of a re-encoded en dash, ellipsis or quote).
const MARKERS: [&str; 2] = ["\u{00c2}\u{00b7}", "\u{00e2}\u{20ac}"];

fn check(dir: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            check(&path, found);
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !["rs", "html", "md", "ps1", "py", "iss", "txt"].contains(&ext) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for (number, line) in text.lines().enumerate() {
            if MARKERS.iter().any(|m| line.contains(m)) && !line.contains("MARKERS") {
                found.push(format!("{}:{}", path.display(), number + 1));
            }
        }
    }
}

#[test]
fn sources_contain_no_re_encoded_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();
    for dir in ["src", "packaging", "validation"] {
        check(&root.join(dir), &mut found);
    }
    for file in [
        "README.md",
        "SECURITY.md",
        "CHANGELOG.md",
        "INSTALL_WINDOWS.md",
    ] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        if MARKERS.iter().any(|m| text.contains(m)) {
            found.push(file.into());
        }
    }
    assert!(found.is_empty(), "re-encoded text at {found:?}");
}
