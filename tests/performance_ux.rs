use rust_browser::{
    background::{Lifecycle, Phase, Protection},
    navigation, privacy,
    settings::{Favorite, Settings, Theme},
    storage::Snapshot,
    tabs::Tabs,
};
use std::time::Duration;

#[test]
fn pinning_and_reordering_preserve_live_content_identity_and_selection() {
    let mut tabs = Tabs::default();
    let a = tabs.push("a".into(), Box::new(11)).unwrap();
    let b = tabs.push("b".into(), Box::new(22)).unwrap();
    let c = tabs.push("c".into(), Box::new(33)).unwrap();
    let pointer = &*tabs.get_mut(b).unwrap().content as *const i32;
    tabs.select(b);
    assert!(tabs.pin(c, true));
    assert_eq!(tabs.items[0].id, c);
    assert_eq!(tabs.active().unwrap().id, b);
    assert!(tabs.pin(b, true));
    assert_eq!(
        tabs.items.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![c, b, a]
    );
    assert!(tabs.move_to(b, 0));
    assert_eq!(tabs.active().unwrap().id, b);
    assert_eq!(pointer, &*tabs.get_mut(b).unwrap().content as *const i32);
    assert!(!tabs.move_to(a, 0));
    assert!(!tabs.move_to(999, 0));
    assert!(!tabs.move_to(a, 99));
    assert!(tabs.pin(b, false));
    assert_eq!(
        tabs.items.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![c, b, a]
    );
    assert_eq!(tabs.active().unwrap().id, b);
    assert_eq!(*tabs.active().unwrap().content, 22);
    tabs.close(c);
    assert_eq!(tabs.active().unwrap().id, b);
}

#[test]
fn no_protected_background_activity_is_eligible_for_suspension() {
    let guards = [
        Protection {
            active: true,
            ..Protection::default()
        },
        Protection {
            pinned: true,
            ..Protection::default()
        },
        Protection {
            keep_awake: true,
            ..Protection::default()
        },
        Protection {
            loading: true,
            ..Protection::default()
        },
        Protection {
            downloads: true,
            ..Protection::default()
        },
        Protection {
            permissions: true,
            ..Protection::default()
        },
        Protection {
            capture: true,
            ..Protection::default()
        },
        Protection {
            audio: true,
            ..Protection::default()
        },
        Protection {
            busy: true,
            ..Protection::default()
        },
    ];
    for guard in guards {
        let mut state = Lifecycle::default();
        state.visible(false, Duration::ZERO);
        assert!(
            state
                .request(
                    Duration::from_secs(200),
                    Duration::from_secs(15),
                    true,
                    guard
                )
                .is_none()
        );
        assert_eq!(state.phase, Phase::Awake);
    }
    let mut state = Lifecycle::default();
    state.visible(false, Duration::ZERO);
    assert!(
        state
            .request(
                Duration::from_secs(200),
                Duration::from_secs(15),
                false,
                Protection::default()
            )
            .is_none()
    );
}

#[test]
fn native_results_cannot_put_a_reselected_tab_back_to_sleep() {
    let mut state = Lifecycle::default();
    state.visible(false, Duration::from_secs(5));
    assert!(
        state
            .request(
                Duration::from_secs(19),
                Duration::from_secs(15),
                true,
                Protection::default()
            )
            .is_none()
    );
    let ticket = state
        .request(
            Duration::from_secs(20),
            Duration::from_secs(15),
            true,
            Protection::default(),
        )
        .unwrap();
    assert_eq!(state.phase, Phase::Pending);
    state.visible(true, Duration::from_secs(21));
    state.resumed(true);
    assert!(!state.complete(ticket, true, Duration::from_secs(22)));
    assert_eq!(state.phase, Phase::Awake);
    state.visible(false, Duration::from_secs(30));
    let new = state
        .request(
            Duration::from_secs(60),
            Duration::from_secs(15),
            true,
            Protection::default(),
        )
        .unwrap();
    assert_ne!(new, ticket);
    assert!(state.complete(new, true, Duration::from_secs(60)));
    assert_eq!(state.phase, Phase::Sleeping);
    state.resumed(true);
    assert_eq!(state.phase, Phase::Awake);
}

#[test]
fn refusal_timeouts_and_resume_failures_preserve_recovery_state() {
    let mut state = Lifecycle::default();
    state.visible(false, Duration::ZERO);
    let first = state
        .request(
            Duration::from_secs(15),
            Duration::from_secs(15),
            true,
            Protection::default(),
        )
        .unwrap();
    assert!(state.pending_expired(Duration::from_secs(45)));
    assert!(state.complete(first, false, Duration::from_secs(45)));
    assert!(
        state
            .request(
                Duration::from_secs(46),
                Duration::from_secs(15),
                true,
                Protection::default()
            )
            .is_none()
    );
    assert!(
        state
            .request(
                Duration::from_secs(75),
                Duration::from_secs(15),
                true,
                Protection::default()
            )
            .is_some()
    );
    state.resumed(false);
    assert_eq!(state.phase, Phase::ResumeFailed);
    state.resumed(true);
    state.unsupported();
    assert_eq!(state.phase, Phase::Unavailable);
    assert!(
        state
            .request(
                Duration::from_secs(200),
                Duration::from_secs(15),
                true,
                Protection::default()
            )
            .is_none()
    );
}

#[test]
fn old_sessions_upgrade_additively_and_new_metadata_is_validated() {
    let legacy:Snapshot=serde_json::from_str(r#"{"version":1,"tabs":["https://example.com/"],"active":0,"settings":{"search_engine":"bing"}}"#).unwrap();
    legacy.validate().unwrap();
    assert!(legacy.pinned.is_empty() && legacy.titles.is_empty());
    assert!(legacy.settings.automatic_suspension);
    assert!(!legacy.settings.show_recent);
    let mut current = Snapshot {
        tabs: vec!["https://example.com/".into(), navigation::HOME.into()],
        pinned: vec![true, false],
        titles: vec!["A".into(), "New tab".into()],
        keep_awake: vec![true, false],
        ..Snapshot::default()
    };
    current.validate().unwrap();
    let json = serde_json::to_string(&current).unwrap();
    let restored: Snapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(current, restored);
    current.pinned = vec![false, true];
    assert!(current.validate().is_err());
    current.pinned = vec![true, false];
    current.titles.pop();
    assert!(current.validate().is_err());
    current.titles = vec!["A".into(), "B".into()];
    current.keep_awake.pop();
    assert!(current.validate().is_err());
}

#[test]
fn new_tab_settings_reject_remote_active_content_and_invalid_values() {
    let mut settings = Settings {
        theme: Theme::Plum,
        favorites: vec![Favorite {
            title: "Rust".into(),
            url: "https://www.rust-lang.org/".into(),
        }],
        ..Settings::default()
    };
    settings.validate().unwrap();
    let restored: Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(settings, restored);
    for bad in [
        "javascript:alert(1)",
        "data:text/html,bad",
        navigation::HOME,
        "https://user:password@example.com/",
    ] {
        settings.favorites[0].url = bad.into();
        assert!(settings.validate().is_err());
    }
    settings.favorites.clear();
    settings.suspend_after_seconds = 0;
    assert!(settings.validate().is_err());
    settings.suspend_after_seconds = 120;
    settings.favorites = vec![
        Favorite {
            title: "A".into(),
            url: "https://example.com/".into()
        };
        2
    ];
    assert!(settings.validate().is_err());
    assert!(serde_json::from_str::<Settings>(r#"{"theme":"external-url"}"#).is_err());
}

#[test]
fn light_theme_is_additive_and_does_not_change_old_session_defaults() {
    let old: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(old.theme, Theme::Dark);
    let light: Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
    light.validate().unwrap();
    assert_eq!(light.theme, Theme::Light);
    assert_eq!(
        serde_json::from_str::<Settings>(&serde_json::to_string(&light).unwrap()).unwrap(),
        light
    );
}

#[test]
fn private_and_retention_disabled_sessions_discard_new_metadata() {
    let state = Snapshot {
        tabs: vec!["https://example.com/private".into()],
        pinned: vec![true],
        titles: vec!["Private title".into()],
        keep_awake: vec![true],
        ..Snapshot::default()
    };
    assert!(privacy::persistent_snapshot(state.clone(), true).is_none());
    let mut state = state;
    state.settings.restore_session = false;
    let sanitized = privacy::persistent_snapshot(state, false).unwrap();
    assert_eq!(sanitized.tabs, vec![navigation::HOME]);
    assert!(
        sanitized.pinned.is_empty()
            && sanitized.titles.is_empty()
            && sanitized.keep_awake.is_empty()
    );
}
