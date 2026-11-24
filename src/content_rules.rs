//! Turning an extension's declarative blocking rules (Chromium's
//! `declarativeNetRequest` rulesets, as uBlock Origin Lite ships them) into
//! WebKit content-blocker rules, which WebKitGTK compiles and applies itself.
//! Linux uses this because WebKitGTK cannot run extensions.
//!
//! Only what WebKit can express faithfully is kept: `block` rules, and `allow`
//! rules as `ignore-previous-rules`. Rules are written in rising priority, each
//! level's blocks before its allows, so a higher-priority block still wins over
//! a lower-priority allow and an allow wins over a block of the same priority,
//! as in Chromium. A rule WebKit cannot express exactly (redirects, header
//! changes, regular expressions, request methods, excluded request domains) is
//! skipped rather than loosened, and counted, so nothing is blocked that the
//! extension would not block.
//!
//! WebKit applies a page's domain conditions to the top document, where
//! Chromium uses the frame that made the request; for ads inside a site's own
//! frames the result is the same.
use crate::extension_manifest;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// The most rules WebKit compiles into one filter.
pub const MAX_RULES: usize = 150_000;
/// The largest ruleset file read, and the most read from one extension.
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 128 * 1024 * 1024;
const MAX_RULESETS: usize = 100;
/// Matches the scheme and any subdomains before a host (`||` in a filter).
const HOST_ANCHOR: &str = "^[^:]+://+([^/:]*\\.)?";
/// One separator character (`^` in a filter): anything but a letter, a digit
/// or one of `_ - . %`.
const SEPARATOR: &str = "[^a-zA-Z0-9_.%-]";

/// The extension's rules, ready for WebKit.
#[derive(Debug, PartialEq, Eq)]
pub struct Translated {
    /// WebKit content-blocker JSON.
    pub json: String,
    /// WebKit rules written.
    pub rules: usize,
    /// Extension rules used.
    pub used: usize,
    /// Extension rules that could not be expressed and were left out.
    pub skipped: usize,
}

#[derive(Deserialize)]
struct Rule {
    #[serde(default = "one")]
    priority: i64,
    action: Action,
    condition: Condition,
}

fn one() -> i64 {
    1
}

#[derive(Deserialize)]
struct Action {
    #[serde(rename = "type")]
    kind: String,
}

/// The conditions WebKit can express. Any other condition makes the rule fail
/// to read, so it is skipped instead of applied too widely.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Condition {
    url_filter: Option<String>,
    is_url_filter_case_sensitive: Option<bool>,
    resource_types: Option<Vec<String>>,
    excluded_resource_types: Option<Vec<String>>,
    #[serde(alias = "domains")]
    initiator_domains: Option<Vec<String>>,
    #[serde(alias = "excludedDomains")]
    excluded_initiator_domains: Option<Vec<String>>,
    request_domains: Option<Vec<String>>,
    domain_type: Option<String>,
}

/// Chromium resource types and WebKit's, for everything but documents.
const TYPES: [(&str, &str); 13] = [
    ("stylesheet", "style-sheet"),
    ("script", "script"),
    ("image", "image"),
    ("font", "font"),
    ("object", "raw"),
    ("xmlhttprequest", "raw"),
    ("ping", "ping"),
    ("csp_report", "raw"),
    ("media", "media"),
    ("websocket", "raw"),
    ("webtransport", "raw"),
    ("webbundle", "raw"),
    ("other", "raw"),
];

/// A domain as WebKit accepts it: lowercase ASCII letters, digits, dots and
/// hyphens.
fn plain_domain(domain: &str) -> Option<String> {
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    let ok = !domain.is_empty()
        && domain.len() <= 253
        && !domain.starts_with('.')
        && domain
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-');
    ok.then_some(domain)
}

/// The domains of a list WebKit can take, without those a listed parent
/// already covers (a domain also matches its subdomains). Leaving out a domain
/// narrows a list that says where a rule applies, so `narrowing` lists drop
/// what WebKit cannot take; any other list must keep every domain or is
/// unusable (`None`). An empty result is `None`: no list means everywhere.
fn domains(list: &[String], narrowing: bool) -> Option<Vec<String>> {
    let mut plain = Vec::with_capacity(list.len());
    for domain in list {
        match plain_domain(domain) {
            Some(domain) => plain.push(domain),
            None if narrowing => {}
            None => return None,
        }
    }
    let set: std::collections::HashSet<&str> = plain.iter().map(String::as_str).collect();
    let covered = |domain: &str| {
        domain
            .match_indices('.')
            .any(|(at, _)| set.contains(&domain[at + 1..]))
    };
    let mut kept: Vec<String> = plain.iter().filter(|d| !covered(d)).cloned().collect();
    kept.sort_unstable();
    kept.dedup();
    (!kept.is_empty()).then_some(kept)
}

/// `domains` for `if-domain`/`unless-domain`.
fn domain_list(list: &[String], narrowing: bool) -> Option<Vec<String>> {
    domains(list, narrowing).map(|list| list.into_iter().map(|d| format!("*{d}")).collect())
}

/// Escape one character of a filter for WebKit's regular expressions.
fn push_literal(out: &mut String, c: char) {
    if !(c.is_ascii_alphanumeric() || c == '/') {
        out.push('\\');
    }
    out.push(c);
}

/// The WebKit `url-filter` patterns that together match what a Chromium
/// `urlFilter` matches. A trailing separator also matches the end of the
/// address, which WebKit cannot say in one pattern, so it becomes two.
fn url_patterns(filter: &str) -> Option<Vec<String>> {
    if filter.is_empty() || !filter.is_ascii() || filter.chars().any(|c| c.is_ascii_control()) {
        return None;
    }
    let (mut out, rest, host) = if let Some(rest) = filter.strip_prefix("||") {
        (HOST_ANCHOR.to_string(), rest, true)
    } else if let Some(rest) = filter.strip_prefix('|') {
        ("^".to_string(), rest, false)
    } else {
        (String::new(), filter, false)
    };
    let (rest, end) = match rest.strip_suffix('|') {
        Some(rest) => (rest, true),
        None => (rest, false),
    };
    let (rest, separator_end) = match rest.strip_suffix('^') {
        Some(rest) if !end => (rest, true),
        _ => (rest, false),
    };
    if rest.contains('|') {
        return None;
    }
    for c in rest.chars() {
        match c {
            '*' => out.push_str(".*"),
            '^' => out.push_str(SEPARATOR),
            c => push_literal(&mut out, c),
        }
    }
    if end {
        out.push('$');
    }
    if out.is_empty() {
        out.push_str(".*");
    }
    Some(if !separator_end {
        vec![out]
    } else if host && !rest.contains('/') {
        // A host is always followed by a path, port or query.
        vec![out + SEPARATOR]
    } else {
        vec![format!("{out}{SEPARATOR}"), out + "$"]
    })
}

/// The address patterns of a rule: its `urlFilter`, limited to its request
/// domains when it has them.
fn rule_patterns(condition: &Condition) -> Option<Vec<String>> {
    let filter = condition.url_filter.as_deref().unwrap_or("");
    let Some(domains) = &condition.request_domains else {
        return if filter.is_empty() {
            Some(vec![".*".into()])
        } else {
            url_patterns(filter)
        };
    };
    // An anchored filter cannot follow a host pattern.
    if filter.starts_with('|') {
        return None;
    }
    let tails = if filter.is_empty() {
        vec![String::new()]
    } else {
        url_patterns(filter)?
            .into_iter()
            .map(|p| format!(".*{}", p.trim_start_matches(".*")))
            .collect()
    };
    let mut patterns = Vec::new();
    for domain in self::domains(domains, true)? {
        let mut host = HOST_ANCHOR.to_string();
        for c in domain.chars() {
            push_literal(&mut host, c);
        }
        host.push_str(SEPARATOR);
        patterns.extend(tails.iter().map(|tail| format!("{host}{tail}")));
    }
    (!patterns.is_empty()).then_some(patterns)
}

/// The resource-type parts of a rule: WebKit types for subresources, and the
/// document load contexts (top frame, child frames) it applies to.
fn type_parts(condition: &Condition) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
    let known = |name: &str| {
        name == "main_frame" || name == "sub_frame" || TYPES.iter().any(|(n, _)| *n == name)
    };
    let mut wanted: Vec<&str> = match &condition.resource_types {
        Some(list) => {
            if list.iter().any(|t| !known(t)) {
                return None;
            }
            list.iter().map(String::as_str).collect()
        }
        // Chromium's default: everything but the top document.
        None => TYPES.iter().map(|(n, _)| *n).chain(["sub_frame"]).collect(),
    };
    if let Some(excluded) = &condition.excluded_resource_types {
        if excluded.iter().any(|t| !known(t)) {
            return None;
        }
        wanted.retain(|t| !excluded.iter().any(|e| e == t));
    }
    let mut kinds: Vec<&'static str> = Vec::new();
    for (chromium, webkit) in TYPES {
        if wanted.contains(&chromium) && !kinds.contains(&webkit) {
            kinds.push(webkit);
        }
    }
    let mut contexts = Vec::new();
    if wanted.contains(&"main_frame") {
        contexts.push("top-frame");
    }
    if wanted.contains(&"sub_frame") {
        contexts.push("child-frame");
    }
    Some((kinds, contexts))
}

/// One extension rule in WebKit's terms.
struct Out {
    rules: Vec<Value>,
    /// Written as one rule for subresources and every document; the top
    /// document is then spared by the level's `TOP_EXEMPT` rule.
    merged: bool,
    /// A block that applies to top documents.
    blocks_top: bool,
}

/// Follows a level's merged blocks: whatever they matched, a top-level
/// document still loads, as Chromium's default types say.
fn top_exempt() -> Value {
    json!({
        "trigger": {"url-filter": ".*", "resource-type": ["document"], "load-context": ["top-frame"]},
        "action": {"type": "ignore-previous-rules"}
    })
}

/// The WebKit rules for one extension rule, or `None` when it cannot be
/// expressed exactly. With `merge`, a block for subresources and child frames
/// (Chromium's default) becomes one rule instead of two.
fn translate_rule(rule: &Rule, merge: bool) -> Option<Out> {
    let action = match rule.action.kind.as_str() {
        "block" => "block",
        "allow" => "ignore-previous-rules",
        _ => return None,
    };
    let c = &rule.condition;
    let mut base = serde_json::Map::new();
    if c.is_url_filter_case_sensitive == Some(true) {
        base.insert("url-filter-is-case-sensitive".into(), json!(true));
    }
    match c.domain_type.as_deref() {
        None => {}
        Some("firstParty") => {
            base.insert("load-type".into(), json!(["first-party"]));
        }
        Some("thirdParty") => {
            base.insert("load-type".into(), json!(["third-party"]));
        }
        Some(_) => return None,
    }
    match (&c.initiator_domains, &c.excluded_initiator_domains) {
        (Some(_), Some(_)) => return None,
        (Some(list), None) => {
            base.insert("if-domain".into(), json!(domain_list(list, true)?));
        }
        (None, Some(list)) => {
            base.insert("unless-domain".into(), json!(domain_list(list, false)?));
        }
        (None, None) => {}
    }
    let patterns = rule_patterns(c)?;
    let (mut kinds, contexts) = type_parts(c)?;
    let blocks_top = action == "block" && contexts.contains(&"top-frame");
    let merged = merge && action == "block" && contexts == ["child-frame"] && !kinds.is_empty();
    let mut triggers = Vec::new();
    if merged {
        kinds.push("document");
        let mut trigger = base;
        trigger.insert("resource-type".into(), json!(kinds));
        triggers.push(trigger);
    } else {
        if !kinds.is_empty() {
            let mut trigger = base.clone();
            trigger.insert("resource-type".into(), json!(kinds));
            triggers.push(trigger);
        }
        if !contexts.is_empty() {
            let mut trigger = base;
            trigger.insert("resource-type".into(), json!(["document"]));
            if contexts.len() == 1 {
                trigger.insert("load-context".into(), json!(contexts));
            }
            triggers.push(trigger);
        }
    }
    let mut rules = Vec::new();
    for trigger in &triggers {
        for pattern in &patterns {
            let mut trigger = trigger.clone();
            trigger.insert("url-filter".into(), json!(pattern));
            rules.push(json!({"trigger": trigger, "action": {"type": action}}));
        }
    }
    (!rules.is_empty()).then_some(Out {
        rules,
        merged,
        blocks_top,
    })
}

/// Translate rulesets (each a JSON array of rules, in the extension's order).
///
/// Each priority level is written as: blocks merged for subresources and
/// child frames, then `top_exempt`, then the level's other blocks, then its
/// allows. The exemption also undoes earlier levels' blocks of top documents,
/// so a level after such a block writes its blocks unmerged instead.
pub fn translate(rulesets: &[Value]) -> Result<Translated, String> {
    let mut levels: BTreeMap<i64, Vec<Rule>> = BTreeMap::new();
    let mut skipped = 0;
    for ruleset in rulesets {
        let rules = ruleset
            .as_array()
            .ok_or("A ruleset of this extension is not a list of rules.")?;
        for value in rules {
            match Rule::deserialize(value) {
                Ok(rule) => levels.entry(rule.priority).or_default().push(rule),
                Err(_) => skipped += 1,
            }
        }
    }
    let mut rules = Vec::new();
    let mut used = 0;
    let mut top_blocked = false;
    for level in levels.values() {
        let (mut merged, mut blocks, mut allows) = (Vec::new(), Vec::new(), Vec::new());
        let mut blocks_top = false;
        for rule in level {
            let allow = rule.action.kind == "allow";
            let Some(out) = translate_rule(rule, !top_blocked && !allow) else {
                skipped += 1;
                continue;
            };
            used += 1;
            blocks_top |= out.blocks_top;
            match (allow, out.merged) {
                (true, _) => allows.extend(out.rules),
                (false, true) => merged.extend(out.rules),
                (false, false) => blocks.extend(out.rules),
            }
        }
        if !merged.is_empty() {
            rules.extend(merged);
            rules.push(top_exempt());
        }
        rules.extend(blocks);
        rules.extend(allows);
        top_blocked |= blocks_top;
    }
    if rules.is_empty() {
        return Err("This extension has no blocking rules Rust Browser can use.".into());
    }
    if rules.len() > MAX_RULES {
        return Err(format!(
            "This extension's rules make {} WebKit rules; at most {MAX_RULES} can be used. Turn off some of its lists and add it again.",
            rules.len()
        ));
    }
    Ok(Translated {
        rules: rules.len(),
        used,
        json: Value::Array(rules).to_string(),
        skipped,
    })
}

/// A ruleset path from the manifest, as a file inside `folder`: relative,
/// without parent steps, and a `.json` file.
fn ruleset_path(folder: &Path, path: &str) -> Option<PathBuf> {
    let relative = Path::new(path.trim_start_matches('/'));
    let plain = relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
        && relative
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        && path.len() <= 260
        && !path.contains('\\');
    plain.then(|| folder.join(relative))
}

/// The files of the rulesets the extension turns on by default.
pub fn enabled_rulesets(folder: &Path, manifest: &Value) -> Result<Vec<PathBuf>, String> {
    let resources = manifest
        .pointer("/declarative_net_request/rule_resources")
        .and_then(Value::as_array)
        .filter(|list| !list.is_empty())
        .ok_or(
            "This extension has no declarative blocking rules, which is all Rust Browser can use on Linux. Choose a content blocker such as uBlock Origin Lite.",
        )?;
    if resources.len() > MAX_RULESETS {
        return Err("This extension lists too many rulesets.".into());
    }
    let mut files = Vec::new();
    for resource in resources {
        if resource.get("enabled").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let path = resource
            .get("path")
            .and_then(Value::as_str)
            .and_then(|path| ruleset_path(folder, path))
            .ok_or("A ruleset of this extension is not a file inside its folder.")?;
        files.push(path);
    }
    if files.is_empty() {
        return Err("This extension turns none of its rulesets on.".into());
    }
    Ok(files)
}

fn read_ruleset(path: &Path, budget: &mut u64) -> Result<Value, String> {
    let unreadable = || "A ruleset of this extension could not be read.".to_string();
    let file = std::fs::File::open(path).map_err(|_| unreadable())?;
    let size = file.metadata().map_err(|_| unreadable())?.len();
    if size > MAX_FILE || size > *budget {
        return Err("This extension's rulesets are too large.".into());
    }
    *budget -= size;
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unreadable())?;
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
    serde_json::from_slice(bytes)
        .map_err(|_| "A ruleset of this extension is not valid JSON.".into())
}

/// Read the extension in `folder` and translate the rulesets it turns on.
/// Slow for large lists (seconds): call it off the UI thread.
pub fn read_extension(folder: &Path) -> Result<Translated, String> {
    let manifest = extension_manifest::read_manifest(folder)?;
    let mut budget = MAX_TOTAL;
    let rulesets = enabled_rulesets(folder, &manifest)?
        .iter()
        .map(|path| read_ruleset(path, &mut budget))
        .collect::<Result<Vec<_>, _>>()?;
    translate(&rulesets)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One rule, unmerged.
    fn one_rule(rule: Value) -> Option<Vec<Value>> {
        translate_rule(&Rule::deserialize(&rule).ok()?, false).map(|out| out.rules)
    }

    fn filters(rules: &[Value]) -> Vec<&str> {
        rules
            .iter()
            .map(|r| r["trigger"]["url-filter"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn filters_become_webkit_patterns() {
        assert_eq!(
            url_patterns("||ads.example.com^").unwrap(),
            [format!("{HOST_ANCHOR}ads\\.example\\.com{SEPARATOR}")]
        );
        assert_eq!(
            url_patterns("||example.com/ad.js^").unwrap(),
            [
                format!("{HOST_ANCHOR}example\\.com/ad\\.js{SEPARATOR}"),
                format!("{HOST_ANCHOR}example\\.com/ad\\.js$"),
            ]
        );
        assert_eq!(
            url_patterns("|https://a.b/x|").unwrap(),
            ["^https\\://a\\.b/x$"]
        );
        assert_eq!(url_patterns("/banner*.gif").unwrap(), ["/banner.*\\.gif"]);
        assert_eq!(url_patterns("a^b").unwrap(), [format!("a{SEPARATOR}b")]);
        // Regular-expression characters are literal.
        assert_eq!(
            url_patterns("?a=(1)+[2]").unwrap(),
            ["\\?a\\=\\(1\\)\\+\\[2\\]"]
        );
        for bad in ["", "ad\u{e9}.js", "a|b", "a\nb"] {
            assert_eq!(url_patterns(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn conditions_become_triggers() {
        let rules = one_rule(json!({
            "id": 1, "action": {"type": "block"},
            "condition": {
                "urlFilter": "||tracker.test^",
                "resourceTypes": ["script", "xmlhttprequest", "sub_frame"],
                "initiatorDomains": ["News.Example", "shop.test"],
                "domainType": "thirdParty"
            }
        }))
        .unwrap();
        assert_eq!(rules.len(), 2);
        let sub = &rules[0]["trigger"];
        assert_eq!(sub["resource-type"], json!(["script", "raw"]));
        assert_eq!(sub["if-domain"], json!(["*news.example", "*shop.test"]));
        assert_eq!(sub["load-type"], json!(["third-party"]));
        assert!(sub.get("load-context").is_none());
        let frame = &rules[1]["trigger"];
        assert_eq!(frame["resource-type"], json!(["document"]));
        assert_eq!(frame["load-context"], json!(["child-frame"]));
        assert_eq!(rules[0]["action"], json!({"type": "block"}));
        // Without types: everything but the top document, as in Chromium.
        let rules = one_rule(json!({"id": 2, "action": {"type": "allow"},
            "condition": {"urlFilter": "/ok.js", "excludedInitiatorDomains": ["a.test"]}}))
        .unwrap();
        assert_eq!(rules[0]["action"], json!({"type": "ignore-previous-rules"}));
        assert_eq!(rules[0]["trigger"]["unless-domain"], json!(["*a.test"]));
        assert_eq!(
            rules[0]["trigger"]["resource-type"],
            json!([
                "style-sheet",
                "script",
                "image",
                "font",
                "raw",
                "ping",
                "media"
            ])
        );
        assert_eq!(rules[1]["trigger"]["load-context"], json!(["child-frame"]));
        // The top document only when asked for.
        let rules = one_rule(json!({"id": 3, "action": {"type": "block"},
            "condition": {"requestDomains": ["bad.test"], "resourceTypes": ["main_frame"]}}))
        .unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["trigger"]["load-context"], json!(["top-frame"]));
        assert_eq!(
            filters(&rules),
            [format!("{HOST_ANCHOR}bad\\.test{SEPARATOR}")]
        );
    }

    #[test]
    fn request_domains_limit_the_filter_to_each_host() {
        let rules = one_rule(json!({"id": 1, "action": {"type": "block"},
            "condition": {"urlFilter": "/ads/*", "requestDomains": ["a.test", "b.test"],
                "resourceTypes": ["image"]}}))
        .unwrap();
        assert_eq!(
            filters(&rules),
            [
                format!("{HOST_ANCHOR}a\\.test{SEPARATOR}.*/ads/.*"),
                format!("{HOST_ANCHOR}b\\.test{SEPARATOR}.*/ads/.*"),
            ]
        );
    }

    #[test]
    fn what_webkit_cannot_express_is_skipped_not_widened() {
        for rule in [
            json!({"id": 1, "action": {"type": "redirect", "redirect": {"extensionPath": "/x.js"}},
                "condition": {"urlFilter": "||a.test^"}}),
            json!({"id": 2, "action": {"type": "modifyHeaders"}, "condition": {}}),
            json!({"id": 3, "action": {"type": "block"}, "condition": {"regexFilter": "^https?://a"}}),
            json!({"id": 4, "action": {"type": "block"},
                "condition": {"urlFilter": "x", "requestMethods": ["post"]}}),
            json!({"id": 5, "action": {"type": "block"},
                "condition": {"urlFilter": "x", "excludedRequestDomains": ["a.test"]}}),
            json!({"id": 6, "action": {"type": "block"},
                "condition": {"urlFilter": "x", "initiatorDomains": ["a.test"],
                    "excludedInitiatorDomains": ["b.a.test"]}}),
            json!({"id": 7, "action": {"type": "block"},
                "condition": {"urlFilter": "x", "excludedInitiatorDomains": ["\u{fc}ber.test"]}}),
            json!({"id": 8, "action": {"type": "block"},
                "condition": {"urlFilter": "x", "resourceTypes": ["future_type"]}}),
            json!({"id": 9, "action": {"type": "block"},
                "condition": {"urlFilter": "||x.test", "requestDomains": ["a.test"]}}),
        ] {
            assert_eq!(one_rule(rule.clone()), None, "{rule}");
        }
    }

    #[test]
    fn allows_follow_the_blocks_they_override_and_priority_wins() {
        let set = json!([
            {"id": 1, "priority": 5, "action": {"type": "block"}, "condition": {"urlFilter": "high"}},
            {"id": 2, "action": {"type": "allow"}, "condition": {"urlFilter": "allow"}},
            {"id": 3, "action": {"type": "block"}, "condition": {"urlFilter": "low"}},
            {"id": 4, "action": {"type": "redirect"}, "condition": {"urlFilter": "skip"}},
            "not a rule"
        ]);
        let out = translate(&[set]).unwrap();
        let rules: Vec<Value> = serde_json::from_str(&out.json).unwrap();
        let order: Vec<_> = rules
            .iter()
            .map(|r| r["trigger"]["url-filter"].as_str().unwrap())
            .collect();
        // Each level: merged blocks, the top-document exemption, then allows
        // (split, so an allow never lets a blocked top document load).
        assert_eq!(order, ["low", ".*", "allow", "allow", "high", ".*"]);
        assert_eq!((out.rules, out.used, out.skipped), (6, 3, 2));
        assert_eq!(rules[1], top_exempt());
        assert_eq!(
            rules[0]["trigger"]["resource-type"],
            json!([
                "style-sheet",
                "script",
                "image",
                "font",
                "raw",
                "ping",
                "media",
                "document"
            ])
        );
        assert_eq!(rules[3]["trigger"]["load-context"], json!(["child-frame"]));
        assert!(translate(&[json!([])]).is_err());
        assert!(translate(&[json!({"rules": []})]).is_err());
    }

    #[test]
    fn blocks_after_a_top_document_block_are_not_merged() {
        // The exemption after merged blocks would undo the lower level's block
        // of a top document, so the higher level keeps its blocks split.
        let set = json!([
            {"id": 1, "action": {"type": "block"},
                "condition": {"urlFilter": "badware", "resourceTypes": ["main_frame"]}},
            {"id": 2, "priority": 2, "action": {"type": "block"}, "condition": {"urlFilter": "ad"}}
        ]);
        let out = translate(&[set]).unwrap();
        let rules: Vec<Value> = serde_json::from_str(&out.json).unwrap();
        assert!(!rules.contains(&top_exempt()), "{rules:?}");
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0]["trigger"]["load-context"], json!(["top-frame"]));
        assert_eq!(rules[2]["trigger"]["load-context"], json!(["child-frame"]));
        // In the other order, merging is safe: the exemption comes first.
        let set = json!([
            {"id": 1, "priority": 2, "action": {"type": "block"},
                "condition": {"urlFilter": "badware", "resourceTypes": ["main_frame"]}},
            {"id": 2, "action": {"type": "block"}, "condition": {"urlFilter": "ad"}}
        ]);
        let rules: Vec<Value> = serde_json::from_str(&translate(&[set]).unwrap().json).unwrap();
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[1], top_exempt());
        assert_eq!(rules[2]["trigger"]["url-filter"], json!("badware"));
    }

    #[test]
    fn only_enabled_rulesets_inside_the_folder_are_read() {
        let folder = Path::new("/ext");
        let manifest =
            |resources: Value| json!({"declarative_net_request": {"rule_resources": resources}});
        let files = enabled_rulesets(
            folder,
            &manifest(json!([
                {"id": "a", "enabled": true, "path": "/rulesets/main/a.json"},
                {"id": "b", "enabled": false, "path": "/rulesets/main/b.json"},
                {"id": "c", "enabled": true, "path": "c.json"}
            ])),
        )
        .unwrap();
        assert_eq!(
            files,
            [folder.join("rulesets/main/a.json"), folder.join("c.json")]
        );
        for bad in [
            "../a.json",
            "/rules/../../a.json",
            "a.txt",
            "rules\\a.json",
            "./a.json",
        ] {
            assert!(
                enabled_rulesets(
                    folder,
                    &manifest(json!([{"id": "x", "enabled": true, "path": bad}]))
                )
                .is_err(),
                "{bad}"
            );
        }
        assert!(enabled_rulesets(folder, &json!({"name": "x"})).is_err());
        assert!(
            enabled_rulesets(
                folder,
                &manifest(json!([{"id": "a", "enabled": false, "path": "a.json"}]))
            )
            .is_err()
        );
    }

    #[test]
    fn an_extension_folder_is_read_and_translated() {
        let root = std::env::temp_dir().join(format!(
            "rust-browser-rules-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("rules")).unwrap();
        std::fs::write(
            root.join("manifest.json"),
            r#"{"name":"B","version":"1","manifest_version":3,"permissions":["declarativeNetRequest"],
               "declarative_net_request":{"rule_resources":[{"id":"r","enabled":true,"path":"/rules/r.json"}]}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("rules/r.json"),
            r#"[{"id":1,"action":{"type":"block"},"condition":{"urlFilter":"||ads.test^","resourceTypes":["script"]}}]"#,
        )
        .unwrap();
        let out = read_extension(&root).unwrap();
        assert_eq!((out.rules, out.used, out.skipped), (1, 1, 0));
        std::fs::write(root.join("rules/r.json"), "not json").unwrap();
        assert!(read_extension(&root).is_err());
        std::fs::remove_file(root.join("rules/r.json")).unwrap();
        assert!(read_extension(&root).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn domain_lists_narrow_safely_and_drop_covered_subdomains() {
        let list = |items: &[&str]| items.iter().map(|d| d.to_string()).collect::<Vec<_>>();
        let mixed = list(&["b.test", "x.a.test", "A.test.", "\u{fc}ber.test", "a.test"]);
        assert_eq!(domains(&mixed, true), Some(list(&["a.test", "b.test"])));
        // An exception list must keep every domain, or the rule would apply
        // where the extension does not apply it.
        assert_eq!(domains(&mixed, false), None);
        assert_eq!(domains(&list(&["\u{fc}ber.test"]), true), None);
        assert_eq!(domains(&[], true), None);
    }

    #[test]
    fn the_test_blocker_fixture_translates() {
        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions/blocker");
        let out = read_extension(&folder).unwrap();
        let rules: Vec<Value> = serde_json::from_str(&out.json).unwrap();
        // Scripts, images, requests and child frames: one rule, then the
        // top-document exemption.
        assert_eq!(rules.len(), 2);
        let trigger = &rules[0]["trigger"];
        assert_eq!(trigger["url-filter"], json!("blocked\\-by\\-fixture"));
        assert_eq!(
            trigger["resource-type"],
            json!(["script", "image", "raw", "document"])
        );
        assert_eq!(rules[1], top_exempt());
    }

    /// Translate a real extension (`RUST_BROWSER_TEST_EXTENSION`, such as an
    /// unpacked uBlock Origin Lite) and report the counts.
    #[test]
    #[ignore = "needs an unpacked extension folder"]
    fn a_real_extension_translates_within_webkit_limits() {
        let folder = std::env::var_os("RUST_BROWSER_TEST_EXTENSION").expect("folder");
        let started = std::time::Instant::now();
        let out = read_extension(Path::new(&folder)).unwrap();
        eprintln!(
            "{} WebKit rules, {} skipped, {} bytes, {:?}",
            out.rules,
            out.skipped,
            out.json.len(),
            started.elapsed()
        );
        assert!(out.rules > 0 && out.rules <= MAX_RULES);
    }
}
