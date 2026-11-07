use rust_browser::{
    navigation::{address, resolve},
    settings::SearchEngine,
};
use url::Url;

#[test]
fn queries_are_encoded_for_each_provider() {
    for (engine, host) in [
        (SearchEngine::Google, "www.google.com"),
        (SearchEngine::DuckDuckGo, "duckduckgo.com"),
        (SearchEngine::Bing, "www.bing.com"),
    ] {
        for query in [
            "rust",
            "rust browser",
            "a & b # c + d 日本",
            "weather today?",
        ] {
            let url = Url::parse(&resolve(query, engine).unwrap()).unwrap();
            assert_eq!(url.host_str(), Some(host));
            assert_eq!(
                url.query_pairs().collect::<Vec<_>>(),
                vec![("q".into(), query.into())]
            );
        }
    }
}

#[test]
fn urls_keep_existing_behavior_and_blocked_schemes_never_become_searches() {
    for input in [
        "example.com",
        "localhost:8080/path",
        "http://localhost:8080/",
        "https://example.com/a?q=1#x",
        "[::1]:8080",
    ] {
        assert_eq!(resolve(input, SearchEngine::Bing), address(input));
    }
    for input in [
        "",
        "javascript:alert(1)",
        "javascript:alert('x.y z')",
        "data:text/html,hello world.example",
        "data:text/html,test",
        "file:///C:/secret",
        "about:blank",
        "mailto:someone@example.com",
        "https://",
        "https://user:pass@example.com",
    ] {
        assert!(resolve(input, SearchEngine::Google).is_err(), "{input}");
    }
    assert!(resolve(&"x".repeat(8193), SearchEngine::Google).is_err());
    assert!(resolve(&"日本".repeat(1000), SearchEngine::Google).is_err());
    let almost_limit = format!("{}.com", "a".repeat(8188));
    assert!(resolve(&almost_limit, SearchEngine::Google).is_err());
}
