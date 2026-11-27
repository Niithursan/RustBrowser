//! Address-field suggestions from the local library: the pages that match what
//! was typed, best first, and the site the field completes inline. Nothing here
//! leaves the device; the library worker gathers the candidates and this module
//! ranks them.
use serde::Serialize;

/// Most rows the list under the address field shows.
pub const MAX_ITEMS: usize = 6;
/// Longest text suggestions are looked up for, like the library search.
pub const MAX_TEXT: usize = 512;

/// A page the library knows that matches the typed words.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub url: String,
    pub title: String,
    /// Recorded visits to this exact address.
    pub visits: u32,
    /// The latest visit, in seconds since 1970, or 0 for a bookmark never visited.
    pub last: i64,
    pub bookmarked: bool,
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Suggestion {
    pub url: String,
    pub title: String,
    pub bookmarked: bool,
}

/// What the field completes to as you type: `text` is the whole completed text
/// (it starts with what was typed, ignoring case), and Enter opens `url`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Inline {
    pub text: String,
    pub url: String,
}

/// The answer for one typed text.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Suggestions {
    /// What was typed, so the toolbar can ignore an answer for older text.
    pub text: String,
    pub inline: Option<Inline>,
    pub items: Vec<Suggestion>,
    /// The browser window that asked; set and read by the browser only.
    #[serde(skip)]
    pub window: u64,
}

/// The typed text's words, lowercased, or none for text no suggestion is made
/// for (empty or too long).
pub fn words(text: &str) -> Vec<String> {
    if text.len() > MAX_TEXT {
        return Vec::new();
    }
    text.split_whitespace().map(str::to_lowercase).collect()
}

/// How much a page counts: its visits, worth less as they age, and a bookmark.
fn score(candidate: &Candidate, now: i64) -> f64 {
    let age_days = (now - candidate.last).max(0) / 86_400;
    let weight = match age_days {
        0..=3 => 1.0,
        4..=14 => 0.7,
        15..=31 => 0.5,
        32..=90 => 0.3,
        _ => 0.1,
    };
    let visits = f64::from(candidate.visits) * if candidate.last > 0 { weight } else { 0.0 };
    // A bookmark counts as a few recent visits: above a page seen once, below
    // one used every day.
    visits + if candidate.bookmarked { 3.0 } else { 0.0 }
}

/// An address as people type it: no scheme, no `www.`, and no `/` alone after
/// the host. `None` for anything but a web address.
pub fn display(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let rest = parsed.as_str().get(parsed.scheme().len() + 3..)?;
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let bare = parsed.path() == "/" && parsed.query().is_none() && parsed.fragment().is_none();
    Some(match rest.strip_suffix('/') {
        Some(host) if bare => host.to_string(),
        _ => rest.to_string(),
    })
}

/// The site address of `url` (scheme, host and port), with its display form.
fn site(url: &str) -> Option<(String, String)> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let mut origin = format!("{}://{host}", parsed.scheme());
    if let Some(port) = parsed.port() {
        origin.push_str(&format!(":{port}"));
    }
    origin.push('/');
    let shown = display(&origin)?;
    Some((origin, shown))
}

/// Whether every word appears in the page's address or title.
fn matches(candidate: &Candidate, words: &[String]) -> bool {
    let haystack = format!("{} {}", candidate.url, candidate.title).to_lowercase();
    words.iter().all(|word| haystack.contains(word.as_str()))
}

/// The completed text for `typed` when `form` continues it, keeping the typed
/// characters as they were typed.
fn complete(typed: &str, form: &str) -> Option<String> {
    let lower = typed.to_lowercase();
    // Only text whose lowercase form has the same length can be spliced safely.
    if lower.len() != typed.len() || !form.to_lowercase().starts_with(&lower) {
        return None;
    }
    form.is_char_boundary(typed.len())
        .then(|| format!("{typed}{}", &form[typed.len()..]))
}

/// The inline completion: a site whose address starts with what was typed (the
/// site with the most weight across its pages), or, once a `/` is typed, the
/// page whose address does. Never for words with spaces.
fn inline(typed: &str, ranked: &[(f64, &Candidate)]) -> Option<Inline> {
    if typed.is_empty() || typed.chars().any(char::is_whitespace) {
        return None;
    }
    let with_scheme = typed.contains("://");
    let page_level = with_scheme || typed.contains('/');
    let mut best: Option<(f64, Inline)> = None;
    let mut offer = |weight: f64, form: &str, url: &str| {
        if let Some(text) = complete(typed, form)
            && best.as_ref().is_none_or(|(w, _)| weight > *w)
        {
            best = Some((
                weight,
                Inline {
                    text,
                    url: url.to_string(),
                },
            ));
        }
    };
    if page_level {
        for (weight, candidate) in ranked {
            if with_scheme {
                offer(*weight, &candidate.url, &candidate.url);
            } else if let Some(shown) = display(&candidate.url) {
                offer(*weight, &shown, &candidate.url);
                offer(*weight, &format!("www.{shown}"), &candidate.url);
            }
        }
    } else {
        // Each site's weight is the sum of its pages', so a site visited
        // through many pages completes before one page visited a few times.
        let mut sites: Vec<(String, String, f64)> = Vec::new();
        for (weight, candidate) in ranked {
            let Some((origin, shown)) = site(&candidate.url) else {
                continue;
            };
            match sites.iter_mut().find(|(o, ..)| *o == origin) {
                Some(entry) => entry.2 += weight,
                None => sites.push((origin, shown, *weight)),
            }
        }
        for (origin, shown, weight) in &sites {
            offer(*weight, shown, origin);
            offer(*weight, &format!("www.{shown}"), origin);
        }
    }
    best.map(|(_, inline)| inline)
}

/// Rank the candidates for `typed`: the matching pages, best first (at most
/// `MAX_ITEMS`, without the inline completion's own address), and the inline
/// completion.
pub fn rank(typed: &str, candidates: &[Candidate], now: i64) -> Suggestions {
    let words = words(typed);
    let mut result = Suggestions {
        text: typed.to_string(),
        ..Suggestions::default()
    };
    if words.is_empty() {
        return result;
    }
    let lower = typed.trim().to_lowercase();
    let mut ranked: Vec<(f64, &Candidate)> = candidates
        .iter()
        .filter(|c| display(&c.url).is_some() && matches(c, &words))
        .map(|c| {
            let mut weight = score(c, now);
            // An address that starts with the typed text is what people mean.
            if display(&c.url).is_some_and(|d| d.to_lowercase().starts_with(&lower)) {
                weight = weight * 2.0 + 1.0;
            }
            (weight, c)
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(b.1.last.cmp(&a.1.last))
            .then(a.1.url.cmp(&b.1.url))
    });
    result.inline = inline(typed.trim(), &ranked);
    let skip = result.inline.as_ref().map(|i| i.url.as_str());
    result.items = ranked
        .iter()
        .filter(|(_, c)| Some(c.url.as_str()) != skip)
        .take(MAX_ITEMS)
        .map(|(_, c)| Suggestion {
            url: c.url.clone(),
            title: c.title.clone(),
            bookmarked: c.bookmarked,
        })
        .collect();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;
    const DAY: i64 = 86_400;

    fn page(url: &str, title: &str, visits: u32, days_ago: i64) -> Candidate {
        Candidate {
            url: url.into(),
            title: title.into(),
            visits,
            last: NOW - days_ago * DAY,
            bookmarked: false,
        }
    }

    #[test]
    fn a_site_completes_inline_from_its_most_visited_pages() {
        let pages = [
            page("https://www.youtube.com/watch", "Video", 6, 1),
            page("https://www.youtube.com/", "YouTube", 2, 1),
            page("https://you.example/", "You", 3, 1),
        ];
        let result = rank("you", &pages, NOW);
        let inline = result.inline.unwrap();
        assert_eq!(inline.text, "youtube.com");
        assert_eq!(inline.url, "https://www.youtube.com/");
        // The list leaves out the completion's own address and keeps the rest.
        assert_eq!(result.items[0].url, "https://www.youtube.com/watch");
        assert!(
            result
                .items
                .iter()
                .all(|s| s.url != "https://www.youtube.com/")
        );
    }

    #[test]
    fn the_completion_keeps_what_was_typed_and_follows_www() {
        let pages = [page("https://www.github.com/", "GitHub", 1, 0)];
        assert_eq!(rank("GiT", &pages, NOW).inline.unwrap().text, "GiThub.com");
        assert_eq!(
            rank("www.gi", &pages, NOW).inline.unwrap().text,
            "www.github.com"
        );
        // Typed in full, Enter still opens the site's real address.
        let exact = rank("github.com", &pages, NOW).inline.unwrap();
        assert_eq!(
            (exact.text.as_str(), exact.url.as_str()),
            ("github.com", "https://www.github.com/")
        );
    }

    #[test]
    fn a_slash_completes_to_a_page() {
        let pages = [
            page("https://docs.rs/serde/latest/serde/", "serde", 4, 2),
            page("https://docs.rs/url/latest/url/", "url", 1, 2),
        ];
        let inline = rank("docs.rs/u", &pages, NOW).inline.unwrap();
        assert_eq!(inline.text, "docs.rs/url/latest/url/");
        assert_eq!(inline.url, "https://docs.rs/url/latest/url/");
        let typed_scheme = rank("https://docs.rs/s", &pages, NOW).inline.unwrap();
        assert_eq!(typed_scheme.text, "https://docs.rs/serde/latest/serde/");
    }

    #[test]
    fn words_never_complete_inline_but_find_titles() {
        let pages = [page(
            "https://example.com/rust-book",
            "The Rust Programming Language",
            1,
            1,
        )];
        let result = rank("rust language", &pages, NOW);
        assert!(result.inline.is_none());
        assert_eq!(result.items.len(), 1);
        assert!(rank("rust python", &pages, NOW).items.is_empty());
    }

    #[test]
    fn the_middle_of_an_address_is_listed_but_not_completed() {
        let pages = [page("https://www.youtube.com/", "YouTube", 3, 0)];
        let result = rank("tube", &pages, NOW);
        assert!(result.inline.is_none());
        assert_eq!(result.items[0].url, "https://www.youtube.com/");
    }

    #[test]
    fn recent_pages_and_bookmarks_come_before_old_pages() {
        let mut bookmarked = page("https://rust-lang.org/learn", "Learn", 0, 0);
        bookmarked.last = 0;
        bookmarked.bookmarked = true;
        let pages = [
            page("https://rust-lang.org/old", "Old", 20, 200),
            page("https://rust-lang.org/new", "New", 3, 1),
            bookmarked,
            page("https://rust-lang.org/once", "Once", 1, 1),
        ];
        let urls: Vec<_> = rank("rust", &pages, NOW)
            .items
            .into_iter()
            .map(|s| s.url)
            .collect();
        assert_eq!(
            urls,
            [
                "https://rust-lang.org/new",
                "https://rust-lang.org/learn",
                "https://rust-lang.org/old",
                "https://rust-lang.org/once"
            ]
        );
    }

    #[test]
    fn the_list_is_bounded_and_empty_or_long_text_gets_nothing() {
        let pages: Vec<_> = (0..20)
            .map(|i| page(&format!("https://a{i}.test/"), "a", 1, 1))
            .collect();
        assert_eq!(rank("a", &pages, NOW).items.len(), MAX_ITEMS);
        assert_eq!(
            rank("   ", &pages, NOW),
            Suggestions {
                text: "   ".into(),
                ..Default::default()
            }
        );
        assert!(
            rank(&"a".repeat(MAX_TEXT + 1), &pages, NOW)
                .items
                .is_empty()
        );
    }

    #[test]
    fn only_web_addresses_are_suggested() {
        let pages = [
            page("file:///etc/passwd", "passwd", 9, 0),
            page("javascript:alert(1)", "x", 9, 0),
        ];
        let result = rank("p", &pages, NOW);
        assert!(result.items.is_empty() && result.inline.is_none());
    }

    #[test]
    fn display_forms_drop_the_scheme_www_and_a_bare_slash() {
        assert_eq!(
            display("https://www.example.com/").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            display("http://example.com:8080/a/").as_deref(),
            Some("example.com:8080/a/")
        );
        assert_eq!(display("ftp://example.com/"), None);
    }

    #[test]
    fn text_that_changes_length_in_lowercase_is_not_spliced() {
        // "İ" lowercases to two characters; splicing by byte length would cut
        // the completion in the wrong place.
        let pages = [page("https://i\u{307}stanbul.example/", "x", 1, 0)];
        assert!(rank("\u{130}", &pages, NOW).inline.is_none());
    }
}
