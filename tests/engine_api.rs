use rust_browser::{
    engine_api::{LoadTarget, PageEvent, WebLocation},
    navigation,
};

#[test]
fn content_targets_never_promote_privileged_or_ambiguous_addresses() {
    for value in [
        "",
        "example.com",
        "about:blank",
        navigation::HOME,
        "file:///C:/secret",
        "data:text/html,hello",
        "javascript:alert(1)",
        "https://",
        "https://user:pass@example.com/",
        "https://example.com/with space",
        " https://example.com/",
        "https://example.com/\n",
    ] {
        assert!(WebLocation::parse(value).is_err(), "{value}");
    }
    assert!(WebLocation::parse(&format!("https://example.com/{}", "a".repeat(8192))).is_err());
    assert!(WebLocation::parse(&format!("https://example.com/{}", "道".repeat(1500))).is_err());
    assert_eq!(LoadTarget::Blank.as_str(), "about:blank");
}

#[test]
fn content_urls_preserve_encoded_navigation_and_agree_with_session_policy() {
    for value in [
        "https://example.com/a%20b?q=x%26y#fragment",
        "http://127.0.0.1:8769/",
        "https://[::1]/",
        "https://例え.テスト/道",
    ] {
        let target = LoadTarget::web(value).unwrap();
        assert!(navigation::allowed(target.as_str()));
        assert_eq!(
            navigation::address(target.as_str()).unwrap(),
            target.as_str()
        );
    }
}

#[test]
fn engine_events_are_owned_and_can_report_navigation_failure_without_native_types() {
    fn send_owned<T: Send + 'static>(value: T) -> T {
        value
    }
    let event = send_owned(PageEvent::Completed {
        error: Some("Fixture failed".into()),
        http_status: None,
        navigation_id: 7,
        url: "https://example.com/".into(),
        title: "Fixture".into(),
    });
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || tx.send(event).unwrap())
        .join()
        .unwrap();
    assert!(matches!(
        rx.recv().unwrap(),
        PageEvent::Completed {
            error: Some(_),
            navigation_id: 7,
            ..
        }
    ));
}
