//! The built-in ad blocker on Linux: hBlock's list of ad, tracking and malware
//! domains (<https://github.com/hectorm/hblock>, MIT), turned into WebKit
//! content-blocker rules. hBlock itself writes the system's hosts file as root;
//! Rust Browser only reads the list hBlock publishes and blocks those domains in
//! its own website views.
//!
//! The list is downloaded by each user's browser, never shipped with it: some
//! of the lists hBlock combines allow only non-commercial use, which a personal
//! download meets but redistribution in an MIT-licensed browser would not.
//!
//! A domain blocks itself and its subdomains, for everything a page loads
//! (scripts, images, frames, requests). Opening such a site as the page itself
//! is still allowed, as with an extension's rules (`content_rules`): a link that
//! passes through a tracking domain keeps working.
use serde_json::json;

/// Where hBlock publishes its list, one domain per line.
pub const LIST_URL: &str = "https://hblock.molinero.dev/hosts_domains.txt";
/// The largest list read (the list is about 10 MB).
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
/// How old the kept list may be before it is downloaded again.
pub const REFRESH_SECONDS: u64 = 7 * 24 * 60 * 60;
/// The most filters the list may need (each holds `content_rules::MAX_RULES`).
pub const MAX_FILTERS: usize = 8;
/// Rules in one filter: WebKit's limit, less the top-document exemption.
const PER_FILTER: usize = crate::content_rules::MAX_RULES - 1;
/// A downloaded list with fewer valid domains than this is not hBlock's list
/// (an error page, a cut-off download).
const MIN_DOMAINS: usize = 1000;

/// The list, ready for WebKit.
#[derive(Debug, PartialEq, Eq)]
pub struct Prepared {
    /// Domains blocked (subdomains of listed domains are not counted).
    pub domains: usize,
    /// Each filter's rules, as WebKit content-blocker JSON.
    pub filters: Vec<String>,
}

/// The name WebKit keeps filter `index` under. Filters are stored by extension
/// ID (32 letters `a` to `p`), and `hblock` fits that alphabet.
pub fn filter_id(index: usize) -> Option<String> {
    (index < MAX_FILTERS).then(|| {
        let mut id = String::from("hblock");
        id.push_str(&"a".repeat(25));
        id.push(char::from(b'a' + index as u8));
        id
    })
}

/// Whether `id` names one of the list's filters.
pub fn is_filter(id: &str) -> bool {
    (0..MAX_FILTERS).any(|index| filter_id(index).as_deref() == Some(id))
}

/// A domain as hBlock writes it: lowercase labels of letters, digits, `-` and
/// `_`, at least two labels, within DNS lengths.
fn valid_domain(domain: &str) -> bool {
    domain.len() <= 253
        && domain.contains('.')
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_')
                })
        })
}

/// The domains in hBlock's list, without those a listed parent already covers.
fn domains(text: &[u8]) -> Result<Vec<&str>, String> {
    if text.len() > MAX_BYTES {
        return Err("The ad blocker's list is larger than expected.".into());
    }
    let text = std::str::from_utf8(text).map_err(|_| "The ad blocker's list is not plain text.")?;
    let mut listed = std::collections::HashSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !valid_domain(line) {
            return Err("The ad blocker's list has a line that is not a domain.".into());
        }
        listed.insert(line);
    }
    let covered = |domain: &str| {
        domain
            .match_indices('.')
            .any(|(at, _)| listed.contains(&domain[at + 1..]))
    };
    let mut kept: Vec<&str> = listed.iter().copied().filter(|d| !covered(d)).collect();
    if kept.len() < MIN_DOMAINS {
        return Err("The ad blocker's list has too few domains.".into());
    }
    kept.sort_unstable();
    Ok(kept)
}

/// Turn hBlock's list into WebKit filters.
pub fn prepare(text: &[u8]) -> Result<Prepared, String> {
    let domains = domains(text)?;
    let chunks: Vec<&[&str]> = domains.chunks(PER_FILTER).collect();
    if chunks.len() > MAX_FILTERS {
        return Err("The ad blocker's list is larger than expected.".into());
    }
    // Opening a listed site as the page itself stays allowed (see the module).
    let exempt = json!({
        "trigger": {"url-filter": ".*", "resource-type": ["document"], "load-context": ["top-frame"]},
        "action": {"type": "ignore-previous-rules"}
    });
    let filters = chunks
        .iter()
        .map(|chunk| {
            let mut rules: Vec<_> = chunk
                .iter()
                .map(|domain| {
                    json!({
                        "trigger": {"url-filter": format!("^[^:]+://+([^/:]*\\.)?{}[:/]", domain.replace('.', "\\."))},
                        "action": {"type": "block"}
                    })
                })
                .collect();
            rules.push(exempt.clone());
            serde_json::Value::Array(rules).to_string()
        })
        .collect();
    Ok(Prepared {
        domains: domains.len(),
        filters,
    })
}

/// What a profile keeps about its prepared list (`hblock.json` beside the
/// filters).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    /// When the list was downloaded (seconds since 1970, UTC).
    pub updated: u64,
    pub domains: usize,
    /// How many filters (`filter_id(0..filters)`) hold it.
    pub filters: usize,
}

impl Kept {
    /// Whether a kept record is usable: its filters fit and it is not from the
    /// future by more than a day (a changed clock).
    pub fn valid(&self, now: u64) -> bool {
        (1..=MAX_FILTERS).contains(&self.filters) && self.updated <= now + 86_400
    }

    /// Whether the list should be downloaded again.
    pub fn due(&self, now: u64) -> bool {
        now.saturating_sub(self.updated) >= REFRESH_SECONDS
    }
}

/// `seconds` since 1970 as a UTC date, `YYYY-MM-DD`.
pub fn date(seconds: u64) -> String {
    // Howard Hinnant's days-to-civil algorithm.
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_refresh_follow_the_clock() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
        assert_eq!(date(1_795_651_200), "2026-11-26");
        let kept = Kept {
            updated: 1_000_000,
            domains: 5,
            filters: 3,
        };
        assert!(kept.valid(1_000_000) && !kept.due(1_000_000));
        assert!(kept.due(1_000_000 + REFRESH_SECONDS));
        assert!(!kept.valid(1_000_000 - 2 * 86_400), "from the future");
        assert!(
            !Kept {
                filters: 0,
                ..kept.clone()
            }
            .valid(1_000_000)
        );
        assert!(
            !Kept {
                filters: MAX_FILTERS + 1,
                ..kept
            }
            .valid(1_000_000)
        );
    }

    fn list(extra: &str) -> String {
        let mut text = String::from("# hBlock\n");
        for i in 0..MIN_DOMAINS {
            text.push_str(&format!("ads{i}.example\n"));
        }
        text.push_str(extra);
        text
    }

    #[test]
    fn filter_ids_are_kept_under_extension_shaped_names() {
        for index in 0..MAX_FILTERS {
            let id = filter_id(index).unwrap();
            assert!(crate::extension_manifest::valid_id(&id), "{id}");
            assert!(is_filter(&id));
        }
        assert_eq!(filter_id(MAX_FILTERS), None);
        assert!(!is_filter("cfphbanobpnnciofefmbfhfildacefon"));
    }

    #[test]
    fn domains_block_their_subdomains_and_covered_ones_are_dropped() {
        let text = list("tracker.test\nsub.tracker.test\na.b.tracker.test\nlone.test\n\n");
        let prepared = prepare(text.as_bytes()).unwrap();
        assert_eq!(prepared.domains, MIN_DOMAINS + 2);
        assert_eq!(prepared.filters.len(), 1);
        let rules: serde_json::Value = serde_json::from_str(&prepared.filters[0]).unwrap();
        let rules = rules.as_array().unwrap();
        assert_eq!(rules.len(), MIN_DOMAINS + 3);
        let filter = |i: usize| {
            rules[i]["trigger"]["url-filter"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let all: Vec<String> = (0..rules.len() - 1).map(filter).collect();
        assert!(all.contains(&"^[^:]+://+([^/:]*\\.)?tracker\\.test[:/]".to_owned()));
        assert!(!all.iter().any(|f| f.contains("sub\\.tracker")));
        // The last rule lets a listed site open as the page itself.
        let last = rules.last().unwrap();
        assert_eq!(last["action"]["type"], "ignore-previous-rules");
        assert_eq!(last["trigger"]["load-context"], json!(["top-frame"]));
    }

    #[test]
    fn large_lists_are_split_within_webkit_limits() {
        let mut text = String::new();
        for i in 0..PER_FILTER + 10 {
            text.push_str(&format!("d{i}.test\n"));
        }
        let prepared = prepare(text.as_bytes()).unwrap();
        assert_eq!(prepared.filters.len(), 2);
        for filter in &prepared.filters {
            let rules: serde_json::Value = serde_json::from_str(filter).unwrap();
            assert!(rules.as_array().unwrap().len() <= crate::content_rules::MAX_RULES);
        }
    }

    #[test]
    fn anything_but_hblocks_list_is_refused() {
        for bad in [
            list("<html>\n"),
            list("0.0.0.0 ads.test\n"),
            list("Upper.test\n"),
            list("nodot\n"),
            list(&format!("{}.test\n", "a".repeat(64))),
            "ads.test\n".to_owned(),
        ] {
            assert!(prepare(bad.as_bytes()).is_err(), "{bad:.60}");
        }
        assert!(prepare(&[0xff, 0xfe]).is_err());
        assert!(prepare(&vec![b'a'; MAX_BYTES + 1]).is_err());
        assert!(list("x_y.test\n0-0.fr\n").parse::<String>().is_ok());
        assert!(prepare(list("x_y.test\n0-0.fr\n").as_bytes()).is_ok());
    }
}
