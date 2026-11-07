use url::Url;

/// Internal model identity only. Never passed to a remote view or accepted as a website URL.
pub const HOME: &str = "rust-browser:new-tab";

/// Resolve explicit URLs, host-like input, or a percent-encoded search query.
pub fn resolve(input: &str, engine: crate::settings::SearchEngine) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter an address or search query.".into());
    }
    if input.len() > 8192 {
        return Err("Address or query is too long.".into());
    }
    let host = input.split(['/', '?', '#']).next().unwrap_or(input);
    let host_like = host.contains('.') || host.starts_with("localhost") || host.starts_with('[');
    let explicit_scheme = input.contains("://")
        || input.find(':').is_some_and(|colon| {
            let prefix = &input[..colon];
            let port = input[colon + 1..]
                .split(['/', '?', '#'])
                .next()
                .unwrap_or("");
            let host_port =
                host_like && !port.is_empty() && port.chars().all(|c| c.is_ascii_digit());
            !host_port
                && prefix.starts_with(|c: char| c.is_ascii_alphabetic())
                && prefix
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
    let destination = if explicit_scheme || (!input.chars().any(char::is_whitespace) && host_like) {
        address(input)?
    } else {
        engine.search(input)
    };
    // Encoding can expand a short query beyond the persisted URL size limit.
    if destination.len() > 8192 {
        return Err("The resulting address or search URL is too long.".into());
    }
    Ok(destination)
}

/// Only HTTP(S) can enter the remote content view. Never execute address-bar JS.
pub fn address(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a website address.".into());
    }
    if input.chars().any(char::is_whitespace) {
        return Err("Enter a URL without spaces. Search is not implemented yet.".into());
    }
    let lower = input.to_ascii_lowercase();
    let explicit = lower.contains("://")
        || [
            "javascript:",
            "data:",
            "file:",
            "about:",
            "mailto:",
            "tel:",
            "blob:",
        ]
        .iter()
        .any(|s| lower.starts_with(s));
    let candidate = if explicit {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let parsed = Url::parse(&candidate).map_err(|_| "That website address is invalid.")?;
    if !allowed(parsed.as_str()) || parsed.host_str().is_none() {
        return Err("Only http:// and https:// websites are supported.".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Addresses containing usernames or passwords are not supported.".into());
    }
    Ok(parsed.into())
}

pub fn allowed(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_addresses() {
        assert_eq!(address(" example.com ").unwrap(), "https://example.com/");
        assert_eq!(
            address("localhost:8080/a").unwrap(),
            "https://localhost:8080/a"
        );
        assert_eq!(
            address("http://localhost:8080").unwrap(),
            "http://localhost:8080/"
        );
        assert_eq!(
            address("https://example.com/a?q=1#x").unwrap(),
            "https://example.com/a?q=1#x"
        );
    }
    #[test]
    fn rejects_unsafe_or_ambiguous_input() {
        for value in [
            "",
            "hello world",
            "javascript:alert(1)",
            "data:text/html,test",
            "file:///C:/test",
            "about:blank",
            "https://",
            "https://user:pass@example.com",
        ] {
            assert!(address(value).is_err(), "{value}");
        }
        assert!(!allowed("javascript:alert(1)"));
        assert!(allowed("https://example.com/"));
        assert!(!allowed(HOME)); // Internal tabs never enter the remote URL policy.
    }
}
