use rust_browser::library::{Action, Database, Filter, history_url};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Barrier,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Profile(PathBuf);
impl Profile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rust-browser-library-{}-{}-{}",
            std::process::id(),
            rust_browser::library::now(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn db(&self) -> PathBuf {
        self.0.join("library.sqlite")
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn bookmarks_folders_edit_search_remove_and_restart() {
    let profile = Profile::new();
    let (mut db, recovered) = Database::open(&profile.db()).unwrap();
    assert!(!recovered);
    db.apply(Action::SaveFolder {
        id: None,
        name: "Research".into(),
        parent_id: None,
    })
    .unwrap();
    let folder = db.query(&Filter::default()).unwrap().folders[0].id;
    db.apply(Action::SaveBookmark {
        id: None,
        title: "Rust <script>".into(),
        url: "https://rust-lang.org/".into(),
        folder: Some(folder),
    })
    .unwrap();
    db.apply(Action::SaveBookmark {
        id: None,
        title: "Duplicate".into(),
        url: "https://rust-lang.org/".into(),
        folder: None,
    })
    .unwrap();
    let data = db
        .query(&Filter {
            query: "RUST".into(),
            folder: Some(folder),
            ..Filter::default()
        })
        .unwrap();
    assert_eq!(data.bookmarks.len(), 1);
    let id = data.bookmarks[0].id;
    assert_eq!(data.bookmarks[0].folder, Some(folder));
    db.apply(Action::SaveBookmark {
        id: Some(id),
        title: "Reference".into(),
        url: "example.com".into(),
        folder: Some(folder),
    })
    .unwrap();
    db.apply(Action::SaveFolder {
        id: Some(folder),
        name: "Work".into(),
        parent_id: None,
    })
    .unwrap();
    drop(db);
    let (mut db, recovered) = Database::open(&profile.db()).unwrap();
    assert!(!recovered);
    let data = db.query(&Filter::default()).unwrap();
    assert_eq!(data.folders[0].name, "Work");
    assert_eq!(data.bookmarks[0].url, "https://example.com/");
    db.apply(Action::RemoveFolder { id: folder }).unwrap();
    assert!(
        db.query(&Filter::default()).unwrap().bookmarks[0]
            .folder
            .is_none()
    );
    db.apply(Action::RemoveBookmark { id }).unwrap();
    assert!(db.query(&Filter::default()).unwrap().bookmarks.is_empty());
}

#[test]
fn nested_folders_round_trip_filter_and_reject_cycles() {
    let profile = Profile::new();
    let (mut db, _) = Database::open(&profile.db()).unwrap();
    db.apply(Action::SaveFolder {
        id: None,
        name: "Reading".into(),
        parent_id: None,
    })
    .unwrap();
    let reading = db.query(&Filter::default()).unwrap().folders[0].id;
    db.apply(Action::SaveFolder {
        id: None,
        name: "Research".into(),
        parent_id: Some(reading),
    })
    .unwrap();
    let research = db
        .query(&Filter::default())
        .unwrap()
        .folders
        .into_iter()
        .find(|folder| folder.name == "Research")
        .unwrap();
    assert_eq!(research.path, ["Reading", "Research"]);
    db.apply(Action::SaveFolder {
        id: None,
        name: "Reading".into(),
        parent_id: Some(reading),
    })
    .unwrap();
    db.apply(Action::SaveFolder {
        id: None,
        name: "Reading".into(),
        parent_id: Some(research.id),
    })
    .unwrap();
    let child_with_same_name = db
        .query(&Filter::default())
        .unwrap()
        .folders
        .into_iter()
        .find(|folder| folder.parent_id == Some(research.id))
        .unwrap();
    assert_eq!(
        child_with_same_name.path,
        ["Reading", "Research", "Reading"]
    );

    db.apply(Action::SaveBookmark {
        id: None,
        title: "Paper".into(),
        url: "https://example.com/paper".into(),
        folder: Some(research.id),
    })
    .unwrap();
    let filtered = db
        .query(&Filter {
            folder: Some(research.id),
            ..Filter::default()
        })
        .unwrap();
    assert_eq!(filtered.bookmarks.len(), 1);
    assert_eq!(filtered.bookmarks[0].folder, Some(research.id));

    assert!(
        db.apply(Action::SaveFolder {
            id: Some(reading),
            name: "Reading".into(),
            parent_id: Some(research.id),
        })
        .is_err(),
        "a folder cannot be moved inside its own descendant"
    );
    assert!(
        db.apply(Action::RemoveFolder { id: research.id }).is_err(),
        "removing a folder must not create duplicate siblings"
    );
    db.apply(Action::SaveFolder {
        id: Some(child_with_same_name.id),
        name: "Archive".into(),
        parent_id: Some(research.id),
    })
    .unwrap();
    db.apply(Action::SaveFolder {
        id: None,
        name: "Research".into(),
        parent_id: Some(research.id),
    })
    .unwrap();
    let child_matching_deleted_folder = db
        .query(&Filter::default())
        .unwrap()
        .folders
        .into_iter()
        .find(|folder| folder.parent_id == Some(research.id) && folder.name == research.name)
        .unwrap();
    db.apply(Action::RemoveFolder { id: research.id }).unwrap();
    let data = db.query(&Filter::default()).unwrap();
    assert!(
        data.folders
            .iter()
            .find(|folder| folder.id == child_with_same_name.id)
            .is_some_and(|folder| {
                folder.parent_id == Some(reading) && folder.path == ["Reading", "Archive"]
            })
    );
    assert!(
        data.folders
            .iter()
            .find(|folder| folder.id == child_matching_deleted_folder.id)
            .is_some_and(|folder| {
                folder.parent_id == Some(reading) && folder.path == ["Reading", "Research"]
            })
    );
    assert!(data.bookmarks[0].folder.is_none());
}

#[test]
fn concurrent_folder_moves_cannot_create_cycles() {
    let profile = Profile::new();
    let (mut db, _) = Database::open(&profile.db()).unwrap();
    let mut pairs = Vec::new();
    for index in 0..8 {
        let first = format!("First {index}");
        let second = format!("Second {index}");
        db.apply(Action::SaveFolder {
            id: None,
            name: first.clone(),
            parent_id: None,
        })
        .unwrap();
        db.apply(Action::SaveFolder {
            id: None,
            name: second.clone(),
            parent_id: None,
        })
        .unwrap();
        let folders = db.query(&Filter::default()).unwrap().folders;
        pairs.push((
            folders
                .iter()
                .find(|folder| folder.name == first)
                .unwrap()
                .id,
            folders
                .iter()
                .find(|folder| folder.name == second)
                .unwrap()
                .id,
        ));
    }
    drop(db);

    let path = profile.db();
    for (index, (first, second)) in pairs.into_iter().enumerate() {
        let barrier = Arc::new(Barrier::new(3));
        let first_path = path.clone();
        let first_barrier = Arc::clone(&barrier);
        let first_move = thread::spawn(move || {
            let (mut db, _) = Database::open(&first_path).unwrap();
            first_barrier.wait();
            db.apply(Action::SaveFolder {
                id: Some(first),
                name: format!("First {index}"),
                parent_id: Some(second),
            })
        });
        let second_path = path.clone();
        let second_barrier = Arc::clone(&barrier);
        let second_move = thread::spawn(move || {
            let (mut db, _) = Database::open(&second_path).unwrap();
            second_barrier.wait();
            db.apply(Action::SaveFolder {
                id: Some(second),
                name: format!("Second {index}"),
                parent_id: Some(first),
            })
        });
        barrier.wait();
        let first_result = first_move.join().unwrap();
        let second_result = second_move.join().unwrap();
        assert_ne!(
            first_result.is_ok(),
            second_result.is_ok(),
            "exactly one move should succeed"
        );

        let (db, _) = Database::open(&path).unwrap();
        let folders = db.query(&Filter::default()).unwrap().folders;
        let first_folder = folders.iter().find(|folder| folder.id == first).unwrap();
        let second_folder = folders.iter().find(|folder| folder.id == second).unwrap();
        assert!(
            (first_folder.parent_id == Some(second) && second_folder.parent_id.is_none())
                || (second_folder.parent_id == Some(first) && first_folder.parent_id.is_none())
        );
    }
}

fn visit(key: &str, url: &str, timestamp: i64) -> Action {
    Action::Record {
        key: key.into(),
        url: url.into(),
        title: "Example reference".into(),
        timestamp,
    }
}
#[test]
fn history_deduplicates_filters_dates_and_requires_clear_confirmation() {
    let p = Profile::new();
    let (mut db, _) = Database::open(&p.db()).unwrap();
    db.apply(visit("a", "https://example.com/", 100)).unwrap();
    db.apply(visit("a", "https://example.com/", 100)).unwrap();
    db.apply(visit("b", "https://example.com/", 110)).unwrap();
    db.apply(visit("a", "https://example.com/", 100)).unwrap();
    db.apply(visit("c", "https://example.com/", 140)).unwrap();
    db.apply(visit("d", "https://rust-lang.org/", 200)).unwrap();
    db.apply(visit("private", "https://example.com/?token=secret", 250))
        .unwrap();
    assert_eq!(db.query(&Filter::default()).unwrap().history.len(), 3);
    let data = db
        .query(&Filter {
            query: "example.com".into(),
            from: Some(120),
            until: Some(200),
            ..Filter::default()
        })
        .unwrap();
    assert_eq!(data.history.len(), 1);
    assert_eq!(data.history[0].timestamp, 140);
    assert!(db.apply(Action::ClearHistory { confirmed: false }).is_err());
    db.apply(Action::RemoveVisit {
        id: data.history[0].id,
    })
    .unwrap();
    assert_eq!(db.query(&Filter::default()).unwrap().history.len(), 2);
    drop(db);
    let (mut db, _) = Database::open(&p.db()).unwrap();
    assert_eq!(db.query(&Filter::default()).unwrap().history.len(), 2);
    db.apply(Action::ClearHistory { confirmed: true }).unwrap();
    assert!(db.query(&Filter::default()).unwrap().history.is_empty());
}

#[test]
fn invalid_inputs_and_sql_text_are_handled_as_data() {
    let p = Profile::new();
    let (mut db, _) = Database::open(&p.db()).unwrap();
    for url in [
        "javascript:alert(1)",
        "file:///C:/secret",
        "https://user:pass@example.com",
    ] {
        assert!(
            db.apply(Action::SaveBookmark {
                id: None,
                title: "Test".into(),
                url: url.into(),
                folder: None
            })
            .is_err()
        );
    }
    assert!(
        db.apply(Action::SaveFolder {
            id: None,
            name: " ".into(),
            parent_id: None,
        })
        .is_err()
    );
    assert!(
        db.apply(Action::SaveBookmark {
            id: Some(999),
            title: "Test".into(),
            url: "https://example.com".into(),
            folder: None
        })
        .is_err()
    );
    assert!(
        db.apply(Action::SaveBookmark {
            id: None,
            title: "Test".into(),
            url: "https://example.com".into(),
            folder: Some(999)
        })
        .is_err()
    );
    db.apply(Action::SaveFolder {
        id: None,
        name: "'); DROP TABLE bookmarks;--".into(),
        parent_id: None,
    })
    .unwrap();
    assert!(
        db.query(&Filter {
            query: "' OR 1=1 --".into(),
            ..Filter::default()
        })
        .unwrap()
        .bookmarks
        .is_empty()
    );
    assert!(
        db.query(&Filter {
            from: Some(200),
            until: Some(100),
            ..Filter::default()
        })
        .is_err()
    );
    assert!(history_url("https://example.com/?key=private").is_none());
    assert!(history_url("https://user:pass@example.com/").is_none());
    assert_eq!(
        history_url("https://example.com/#fragment").unwrap(),
        "https://example.com/"
    );
}

#[test]
fn schema_migration_corruption_recovery_and_future_schema_preservation() {
    let p = Profile::new();
    let connection = rusqlite::Connection::open(p.db()).unwrap();
    connection.execute_batch("CREATE TABLE folders(id INTEGER PRIMARY KEY,name TEXT UNIQUE COLLATE NOCASE); CREATE TABLE bookmarks(id INTEGER PRIMARY KEY,title TEXT,url TEXT UNIQUE,folder INTEGER REFERENCES folders(id) ON DELETE SET NULL); INSERT INTO bookmarks VALUES(1,'Keep me','https://example.com/',NULL); PRAGMA user_version=1;").unwrap();
    drop(connection);
    let (db, _) = Database::open(&p.db()).unwrap();
    assert_eq!(db.query(&Filter::default()).unwrap().bookmarks.len(), 1);
    drop(db);
    let connection = rusqlite::Connection::open(p.db()).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, rust_browser::library::SCHEMA_VERSION);
    connection.execute_batch("PRAGMA user_version=99").unwrap();
    drop(connection);
    assert!(Database::open(&p.db()).is_err());
    let connection = rusqlite::Connection::open(p.db()).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM bookmarks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(connection);
    fs::write(p.db(), b"not a database").unwrap();
    let (db, recovered) = Database::open(&p.db()).unwrap();
    assert!(recovered);
    assert!(db.query(&Filter::default()).unwrap().bookmarks.is_empty());
    drop(db);
    let backup = fs::read_dir(&p.0)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.to_string_lossy().contains(".corrupt-"))
        .unwrap();
    assert_eq!(fs::read(backup).unwrap(), b"not a database");
}

#[test]
fn schema_v3_folder_migration_preserves_existing_bookmarks() {
    let p = Profile::new();
    let connection = rusqlite::Connection::open(p.db()).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE folders(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE COLLATE NOCASE);
             CREATE TABLE bookmarks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,url TEXT NOT NULL UNIQUE,
                 folder INTEGER REFERENCES folders(id) ON DELETE SET NULL);
             CREATE TABLE visits(id INTEGER PRIMARY KEY,event_key TEXT NOT NULL UNIQUE,url TEXT NOT NULL,
                 title TEXT NOT NULL,visited_at INTEGER NOT NULL);
             CREATE TABLE permissions(origin TEXT NOT NULL,capability TEXT NOT NULL,decision TEXT NOT NULL,
                 PRIMARY KEY(origin,capability));
             INSERT INTO folders(id,name) VALUES(7,'Research');
             INSERT INTO bookmarks(id,title,url,folder) VALUES(9,'Saved','https://example.com/',7);
             PRAGMA user_version=3;",
        )
        .unwrap();
    drop(connection);

    let (db, recovered) = Database::open(&p.db()).unwrap();
    assert!(!recovered);
    let data = db.query(&Filter::default()).unwrap();
    assert_eq!(data.bookmarks.len(), 1);
    assert_eq!(data.bookmarks[0].folder, Some(7));
    assert_eq!(data.folders[0].id, 7);
    assert_eq!(data.folders[0].parent_id, None);
    assert_eq!(data.folders[0].path, ["Research"]);
    drop(db);

    let connection = rusqlite::Connection::open(p.db()).unwrap();
    let mut check = connection.prepare("PRAGMA foreign_key_check").unwrap();
    assert!(check.query([]).unwrap().next().unwrap().is_none());
}

#[test]
fn folder_dropdowns_and_the_current_pages_bookmark() {
    use rust_browser::library::BarItem;
    let profile = Profile::new();
    let (mut db, _) = Database::open(&profile.db()).unwrap();
    for (name, parent_id) in [("Work", None), ("Deep", Some(1))] {
        db.apply(Action::SaveFolder {
            id: None,
            name: name.into(),
            parent_id,
        })
        .unwrap();
    }
    let folders = db.query(&Filter::default()).unwrap().folders;
    let work = folders.iter().find(|f| f.name == "Work").unwrap().id;
    let deep = folders.iter().find(|f| f.name == "Deep").unwrap().id;
    for (title, url, folder) in [
        ("Second", "https://example.com/2", Some(work)),
        ("First", "https://example.com/1", Some(work)),
        ("Elsewhere", "https://example.com/3", None),
    ] {
        db.apply(Action::SaveBookmark {
            id: None,
            title: title.into(),
            url: url.into(),
            folder,
        })
        .unwrap();
    }
    let data = db
        .query(&Filter {
            menu_folder: Some(work),
            page: Some("https://example.com/3".into()),
            ..Filter::default()
        })
        .unwrap();
    let menu = data.menu.unwrap();
    assert_eq!(menu.name, "Work");
    // Subfolders first, then bookmarks in the order they were added.
    assert_eq!(
        menu.items,
        vec![
            BarItem::Folder {
                id: deep,
                name: "Deep".into()
            },
            BarItem::Link {
                id: 1,
                title: "Second".into(),
                url: "https://example.com/2".into()
            },
            BarItem::Link {
                id: 2,
                title: "First".into(),
                url: "https://example.com/1".into()
            },
        ]
    );
    assert_eq!(data.page_bookmark.unwrap().title, "Elsewhere");
    // Without them nothing extra is read; a removed folder has no dropdown.
    let plain = db.query(&Filter::default()).unwrap();
    assert!(plain.menu.is_none() && plain.page_bookmark.is_none());
    let gone = db
        .query(&Filter {
            menu_folder: Some(9999),
            page: Some("https://not-bookmarked.example/".into()),
            ..Filter::default()
        })
        .unwrap();
    assert!(gone.menu.is_none() && gone.page_bookmark.is_none());
    // The toolbar cannot set them: they are not read from its JSON.
    let sent: Filter =
        serde_json::from_str(r#"{"menu_folder":1,"page":"https://example.com/3"}"#).unwrap();
    assert!(sent.menu_folder.is_none() && sent.page.is_none());
}
