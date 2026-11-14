//! Importing exported bookmarks HTML files (v0.19). Fixtures are small exports in
//! the format each browser writes, with example addresses only.
use rust_browser::{
    bookmark_import::{self, Entry, FileError, MAX_DEPTH, MAX_FILE_BYTES, MAX_LINKS, Refused},
    library::{Database, Filter},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Profile(PathBuf);
impl Profile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rust-browser-import-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn database(&self) -> Database {
        Database::open(&self.0.join("library.sqlite")).unwrap().0
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/bookmarks")
            .join(name),
    )
    .unwrap()
}

fn entry(title: &str, url: &str, folder: Option<&str>) -> Entry {
    Entry {
        title: title.into(),
        url: url.into(),
        folder: folder.map(|name| vec![name.into()]).unwrap_or_default(),
    }
}

/// A minimal export around `body` (the contents of the top-level list).
fn export(body: &str) -> Vec<u8> {
    format!(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<TITLE>Bookmarks</TITLE>\n<DL><p>\n{body}\n</DL><p>\n"
    )
    .into_bytes()
}

fn bookmarks(db: &Database) -> Vec<(String, String, Option<String>)> {
    let data = db.query(&Filter::default()).unwrap();
    let mut found: Vec<_> = data
        .bookmarks
        .iter()
        .map(|b| {
            let folder = b.folder.and_then(|id| {
                data.folders
                    .iter()
                    .find(|f| f.id == id)
                    .map(|f| f.path.join(" / "))
            });
            (b.title.clone(), b.url.clone(), folder)
        })
        .collect();
    found.sort();
    found
}

#[test]
fn a_firefox_export_is_read_with_its_folders() {
    let parsed = bookmark_import::parse(&fixture("firefox.html")).unwrap();
    assert_eq!(
        parsed.entries,
        vec![
            entry("Menu item & more", "https://www.example.org/menu", None),
            entry(
                "An article",
                "https://news.example.com/article?id=7",
                Some("Reading")
            ),
            Entry {
                title: "Read later".into(),
                url: "https://example.net/later".into(),
                folder: vec!["Reading".into(), "Later".into()],
            },
            entry(
                "Get Help",
                "https://www.mozilla.org/en-US/firefox/",
                Some("Bookmarks Toolbar")
            ),
            entry(
                "Other 'quoted' \"title\"",
                "https://example.org/other",
                Some("Other Bookmarks")
            ),
        ]
    );
    // Firefox's "place:" smart bookmark is not a web address.
    assert_eq!(parsed.refused.get(&Refused::NotWeb), Some(&1));
    // "Later" is inside "Reading".
    assert_eq!(parsed.nested, 1);
}

#[test]
fn a_chrome_export_imports_once_and_skips_duplicates() {
    let parsed = bookmark_import::parse(&fixture("chrome.html")).unwrap();
    assert_eq!(
        parsed.refused.get(&Refused::NotWeb),
        Some(&1),
        "javascript:"
    );
    assert_eq!(parsed.nested, 1, "Work is inside Bookmarks bar");
    let profile = Profile::new();
    let mut db = profile.database();
    let summary = db.import_bookmarks(&parsed).unwrap();
    assert_eq!(summary.imported, 3);
    assert_eq!(summary.duplicates, 1, "the same address twice in the file");
    assert_eq!(summary.folders_created, 3);
    assert_eq!(
        bookmarks(&db),
        vec![
            (
                "Example".into(),
                "https://www.example.com/".into(),
                Some("Bookmarks bar".into())
            ),
            (
                "Guide".into(),
                "https://docs.example.com/guide".into(),
                Some("Bookmarks bar / Work".into())
            ),
            (
                "Unicode 你好 title".into(),
                "https://example.org/%E4%BD%A0%E5%A5%BD".into(),
                Some("Other bookmarks".into())
            ),
        ]
    );
    // Importing the same file again adds nothing.
    let again = db.import_bookmarks(&parsed).unwrap();
    assert_eq!((again.imported, again.duplicates), (0, 4));
    assert_eq!(again.folders_created, 0);
    assert_eq!(bookmarks(&db).len(), 3);
}

#[test]
fn nested_imported_folders_keep_their_parent_relationship() {
    let parsed = bookmark_import::parse(&export(
        r#"<DT><H3>Reading</H3><DL><p>
<DT><H3>Research</H3><DL><p>
<DT><A HREF="https://example.com/paper">Paper</A>
</DL><p></DL><p>"#,
    ))
    .unwrap();
    let profile = Profile::new();
    let mut db = profile.database();
    let summary = db.import_bookmarks(&parsed).unwrap();
    assert_eq!(summary.imported, 1);
    assert_eq!(summary.folders_created, 2);

    let connection = rusqlite::Connection::open(profile.0.join("library.sqlite")).unwrap();
    let reading: i64 = connection
        .query_row("SELECT id FROM folders WHERE name='Reading'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let research_parent: Option<i64> = connection
        .query_row(
            "SELECT parent_id FROM folders WHERE name='Research'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(research_parent, Some(reading));
    let bookmark_folder: i64 = connection
        .query_row(
            "SELECT folder FROM bookmarks WHERE url='https://example.com/paper'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(bookmark_folder, reading);
}

#[test]
fn an_edge_export_refuses_local_data_and_credential_addresses() {
    let parsed = bookmark_import::parse(&fixture("edge.html")).unwrap();
    assert_eq!(
        parsed.entries,
        vec![
            entry(
                "Edge example",
                "https://www.example.com/edge",
                Some("Favorites bar")
            ),
            entry("Plain http", "http://intranet.example.test/", None),
        ]
    );
    assert_eq!(
        parsed.refused.get(&Refused::NotWeb),
        Some(&2),
        "file: and data:"
    );
    assert_eq!(parsed.refused.get(&Refused::Credentials), Some(&1));
    let summary = Profile::new().database().import_bookmarks(&parsed).unwrap();
    let message = summary.message();
    assert!(message.contains("2 added"), "{message}");
    assert!(message.contains("3 refused"), "{message}");
    assert!(
        message.contains("2 not an http or https address"),
        "{message}"
    );
    assert!(
        message.contains("1 contained a user name or password"),
        "{message}"
    );
    assert!(!message.contains('\u{2014}'), "no em dash: {message}");
}

#[test]
fn hostile_addresses_are_refused() {
    let parsed = bookmark_import::parse(&export(
        r#"<DT><A HREF="&#106;avascript:alert(1)">Encoded script</A>
<DT><A HREF="JAVASCRIPT:alert(1)">Upper case</A>
<DT><A HREF="vbscript:msgbox">VBScript</A>
<DT><A HREF="data:text/html;base64,PHNjcmlwdD4=">Data</A>
<DT><A HREF="file://server/share/x">UNC file</A>
<DT><A HREF="https://:pw@example.com/">Password only</A>
<DT><A HREF="https://">No host</A>
<DT><A HREF="">Empty</A>
<DT><A>No address</A>
<DT><A ICON_URI="https://icon.example/" HREF="https://good.example/">Icon first</A>"#,
    ))
    .unwrap();
    assert_eq!(
        parsed.entries,
        vec![entry("Icon first", "https://good.example/", None)]
    );
    assert_eq!(parsed.refused.get(&Refused::NotWeb), Some(&5));
    assert_eq!(parsed.refused.get(&Refused::Credentials), Some(&1));
    assert_eq!(parsed.refused.get(&Refused::Invalid), Some(&3));
}

#[test]
fn files_that_are_not_bookmark_exports_are_refused_whole() {
    assert_eq!(bookmark_import::parse(b""), Err(FileError::NotBookmarks));
    assert_eq!(
        bookmark_import::parse(b"<html><a href=\"https://example.com/\">x</a></html>"),
        Err(FileError::NotBookmarks)
    );
    assert_eq!(
        bookmark_import::parse(b"<!DOCTYPE NETSCAPE-Bookmark-file-1>\n\xff\xfe broken"),
        Err(FileError::NotUtf8)
    );
    // A byte-order mark and lower-case doctype are fine.
    let mut bom = "\u{feff}<!doctype netscape-bookmark-file-1>"
        .as_bytes()
        .to_vec();
    bom.extend_from_slice(b"<DL><DT><A HREF=\"https://example.com/\">x</A></DL>");
    assert_eq!(bookmark_import::parse(&bom).unwrap().entries.len(), 1);
}

#[test]
fn malformed_and_truncated_files_do_not_fail_or_invent_links() {
    for body in [
        r#"<DT><A HREF="https://example.com/a">No closing tag"#,
        r#"<DT><A HREF="https://example.com/a"#,
        "<DT><H3>Folder without list",
        "</DL></DL></DL><DT><A HREF='https://example.com/a'>after stray closes</A>",
        "<DT><A HREF=https://example.com/a>Unquoted</A>",
        "<<<>>><DT><A\nHREF\n=\n\"https://example.com/a\"\n>Line breaks</A>",
        "<DT><A HREF=\"https://example.com/a\">&#xD800; &#99999999; &bogus; &amp</A>",
    ] {
        let parsed = bookmark_import::parse(&export(body)).unwrap();
        assert!(parsed.entries.len() <= 1, "{body}: {:?}", parsed.entries);
        for item in &parsed.entries {
            assert_eq!(item.url, "https://example.com/a", "{body}");
            assert!(!item.title.chars().any(char::is_control), "{body}");
        }
    }
}

#[test]
fn size_count_and_depth_limits() {
    let mut big = export("");
    big.resize(MAX_FILE_BYTES as usize + 1, b' ');
    assert_eq!(bookmark_import::parse(&big), Err(FileError::TooLarge));

    let many = "<DT><A HREF=\"https://example.com/\">x</A>\n".repeat(MAX_LINKS + 1);
    assert_eq!(
        bookmark_import::parse(&export(&many)),
        Err(FileError::TooManyLinks)
    );

    // The top-level list counts as one level.
    let nest = |levels: usize| {
        let mut body = String::new();
        for i in 0..levels {
            body += &format!("<DT><H3>F{i}</H3><DL><p>");
        }
        body += "<DT><A HREF=\"https://example.com/deep\">deep</A>";
        body += &"</DL><p>".repeat(levels);
        export(&body)
    };
    let deepest = bookmark_import::parse(&nest(MAX_DEPTH - 1)).unwrap();
    assert_eq!(
        deepest.entries[0].folder.last().map(String::as_str),
        Some(&*format!("F{}", MAX_DEPTH - 2))
    );
    assert_eq!(deepest.nested, MAX_DEPTH - 2);
    assert_eq!(
        bookmark_import::parse(&nest(MAX_DEPTH)),
        Err(FileError::TooDeep)
    );
}

#[test]
fn long_titles_and_folder_names_are_shortened_and_long_addresses_refused() {
    let title = "t".repeat(400);
    let folder = format!("F\u{7}{}", "f".repeat(200));
    let long = format!("https://example.com/{}", "a".repeat(9000));
    let parsed = bookmark_import::parse(&export(&format!(
        "<DT><H3>{folder}</H3><DL><p><DT><A HREF=\"https://example.com/t\">{title}</A><DT><A HREF=\"{long}\">long</A></DL><p>"
    )))
    .unwrap();
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.entries[0].title.chars().count(), 256);
    let name = parsed.entries[0].folder.last().unwrap().clone();
    assert_eq!(name.chars().count(), 80);
    assert!(!name.chars().any(char::is_control));
    assert_eq!(parsed.refused.get(&Refused::TooLong), Some(&1));
    // And the library accepts what the parser produced.
    let summary = Profile::new().database().import_bookmarks(&parsed).unwrap();
    assert_eq!(summary.imported, 1);
}

#[test]
fn an_existing_folder_with_the_same_name_is_reused() {
    let profile = Profile::new();
    let mut db = profile.database();
    db.apply(rust_browser::library::Action::SaveFolder {
        id: None,
        name: "Other bookmarks".into(),
        parent_id: None,
    })
    .unwrap();
    let parsed = bookmark_import::parse(&fixture("chrome.html")).unwrap();
    let summary = db.import_bookmarks(&parsed).unwrap();
    assert_eq!(
        summary.folders_created, 2,
        "the existing top-level folder matches by full path"
    );
    let data = db.query(&Filter::default()).unwrap();
    assert_eq!(data.folders.len(), 3);
}

#[test]
fn an_import_that_would_pass_the_bookmark_limit_changes_nothing() {
    let profile = Profile::new();
    let mut db = profile.database();
    let body: String = (0..4990)
        .map(|i| format!("<DT><A HREF=\"https://example.com/{i}\">{i}</A>\n"))
        .collect();
    let first = bookmark_import::parse(&export(&body)).unwrap();
    assert_eq!(db.import_bookmarks(&first).unwrap().imported, 4990);
    let body: String = (0..20)
        .map(|i| format!("<DT><H3>New{i}</H3><DL><p><DT><A HREF=\"https://more.example.com/{i}\">{i}</A></DL><p>\n"))
        .collect();
    let second = bookmark_import::parse(&export(&body)).unwrap();
    let error = db.import_bookmarks(&second).unwrap_err().to_string();
    assert!(error.contains("nothing was imported"), "{error}");
    let data = db.query(&Filter::default()).unwrap();
    assert!(data.folders.is_empty(), "no folder was left behind");
    assert_eq!(rusqlite_count(&profile), 4990);
}

fn rusqlite_count(profile: &Profile) -> usize {
    let connection = rusqlite::Connection::open(profile.0.join("library.sqlite")).unwrap();
    connection
        .query_row("SELECT count(*) FROM bookmarks", [], |r| r.get::<_, i64>(0))
        .unwrap() as usize
}

#[test]
fn an_interrupted_import_leaves_bookmarks_and_folders_unchanged() {
    let profile = Profile::new();
    let mut db = profile.database();
    db.apply(rust_browser::library::Action::SaveBookmark {
        id: None,
        title: "Kept".into(),
        url: "https://kept.example/".into(),
        folder: None,
    })
    .unwrap();
    let before = bookmarks(&db);
    // Fail the transaction part way through, after two bookmarks were written.
    {
        let connection = rusqlite::Connection::open(profile.0.join("library.sqlite")).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER interrupt AFTER INSERT ON bookmarks
                 WHEN (SELECT count(*) FROM bookmarks) > 3
                 BEGIN SELECT RAISE(ABORT, 'interrupted'); END;",
            )
            .unwrap();
    }
    let parsed = bookmark_import::parse(&fixture("firefox.html")).unwrap();
    assert!(db.import_bookmarks(&parsed).is_err());
    assert_eq!(bookmarks(&db), before);
    assert!(db.query(&Filter::default()).unwrap().folders.is_empty());
}

#[test]
fn the_toolbar_cannot_send_import_entries() {
    // The import action is built only by the browser from a chosen file.
    let attempt = serde_json::from_str::<rust_browser::library::Action>(
        r#"{"kind":"import_bookmarks","entries":[{"title":"x","url":"https://example.com/"}]}"#,
    );
    assert!(attempt.is_err());
}
