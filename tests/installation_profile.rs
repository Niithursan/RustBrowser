//! Representative disposable data for actual installer/upgrade/uninstall checks.
use rust_browser::{
    library::{Action, Database, Filter},
    navigation,
    security::{Capability, Decision},
    settings::{SearchEngine, Settings, Theme},
    storage::{Snapshot, Store},
};
use std::{fs, path::Path};

fn snapshot() -> Snapshot {
    Snapshot {
        tabs: vec![navigation::HOME.into(), navigation::HOME.into()],
        active: 1,
        pinned: vec![true, false],
        keep_awake: vec![true, false],
        titles: vec!["Pinned fixture".into(), "Active fixture".into()],
        settings: Settings {
            search_engine: SearchEngine::Bing,
            theme: Theme::Light,
            ..Settings::default()
        },
        ..Snapshot::default()
    }
}

fn prepare(root: &Path) {
    let profile = root.join("profile");
    fs::create_dir(&profile).unwrap();
    let (store, _, recovered) =
        Store::open(&profile, || panic!("Fixture persistence failed")).unwrap();
    assert!(!recovered);
    store.queue(snapshot());
    drop(store);
    let (mut db, recovered) = Database::open(&profile.join("library.sqlite")).unwrap();
    assert!(!recovered);
    db.apply(Action::SaveFolder {
        id: None,
        name: "Upgrade fixture".into(),
    })
    .unwrap();
    let folder = db.query(&Filter::default()).unwrap().folders[0].id;
    db.apply(Action::SaveBookmark {
        id: None,
        title: "Preserved bookmark".into(),
        url: "https://example.com/fixture".into(),
        folder: Some(folder),
    })
    .unwrap();
    db.apply(Action::Record {
        key: "installation-fixture".into(),
        url: "https://example.com/fixture".into(),
        title: "Preserved visit".into(),
        timestamp: 1_700_000_000,
    })
    .unwrap();
    db.apply(Action::SetPermission {
        origin: "https://example.com".into(),
        capability: Capability::Camera,
        decision: Some(Decision::Deny),
    })
    .unwrap();
    drop(db);
    fs::create_dir(profile.join("content")).unwrap();
    fs::write(
        profile.join("content/opaque-fixture.txt"),
        b"Synthetic site-storage sentinel; not real cookies",
    )
    .unwrap();
    fs::create_dir(root.join("downloads")).unwrap();
    fs::write(
        root.join("downloads/fixture.txt"),
        b"Synthetic downloaded bytes; never execute",
    )
    .unwrap();
}

fn verify(root: &Path) {
    let profile = root.join("profile");
    let (store, restored, recovered) =
        Store::open(&profile, || panic!("Unexpected fixture write")).unwrap();
    assert!(!recovered);
    assert_eq!(restored, snapshot());
    drop(store);
    let (db, recovered) = Database::open(&profile.join("library.sqlite")).unwrap();
    assert!(!recovered);
    let data = db.query(&Filter::default()).unwrap();
    assert_eq!(data.bookmarks.len(), 1);
    assert_eq!(data.bookmarks[0].title, "Preserved bookmark");
    assert_eq!(data.bookmarks[0].folder, Some(data.folders[0].id));
    assert_eq!(data.history.len(), 1);
    assert_eq!(data.history[0].timestamp, 1_700_000_000);
    assert_eq!(data.permissions.len(), 1);
    assert_eq!(data.permissions[0].decision, Decision::Deny);
    assert_eq!(
        fs::read(profile.join("content/opaque-fixture.txt")).unwrap(),
        b"Synthetic site-storage sentinel; not real cookies"
    );
    assert_eq!(
        fs::read(root.join("downloads/fixture.txt")).unwrap(),
        b"Synthetic downloaded bytes; never execute"
    );
}

#[test]
fn representative_installation_profile_roundtrips() {
    let root = std::env::temp_dir().join(format!(
        "rust-browser-install-profile-{}-{}",
        std::process::id(),
        rust_browser::library::now()
    ));
    fs::create_dir(&root).unwrap();
    prepare(&root);
    verify(&root);
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "Owned installer harness helper; never use a real profile"]
fn installer_profile_harness() {
    let root = std::path::PathBuf::from(
        std::env::var_os("RUST_BROWSER_INSTALL_FIXTURE")
            .expect("Explicit owned fixture root required"),
    );
    assert!(root.join(".rust-browser-install-validation").is_file());
    let root = dunce::canonicalize(root).unwrap();
    assert!(!root.starts_with(dunce::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap()));
    let selected = rust_browser::storage::profile_dir().unwrap();
    let selected = dunce::canonicalize(&selected).unwrap_or(selected);
    assert!(!root.starts_with(selected));
    match std::env::var("RUST_BROWSER_INSTALL_STAGE")
        .unwrap()
        .as_str()
    {
        "prepare" => prepare(&root),
        "verify" => verify(&root),
        _ => panic!("Unknown installer fixture stage"),
    }
}
