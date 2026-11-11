//! Command-line parsing. Every argument is untrusted: other applications and the
//! Windows shell pass links here, and a link must never be read as an option.
//!
//! Accepted forms:
//! - `rust-browser.exe --single-argument <link>`: the registered form. Everything
//!   after the marker is one link, taken verbatim from the raw command line, so
//!   quotes or spaces inside it cannot create further arguments.
//! - `rust-browser.exe [--private] [--new-window] [--] [<link> ...]`.
//! - `rust-browser.exe --version`.
//!
//! Links must be explicit http:// or https:// addresses (`navigation::external`).
//! Local files are not accepted: the address bar refuses `file:` too.
use crate::navigation;

/// At most this many links in one launch or hand-off message.
pub const MAX_LINKS: usize = 16;
/// Longest raw command line considered (characters).
pub const MAX_COMMAND_LINE: usize = 32 * 1024;
pub const SINGLE_ARGUMENT: &str = "--single-argument";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    Version,
    Open {
        private: bool,
        new_window: bool,
        links: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    InvalidText,
    TooLong,
    UnknownOption(String),
    TooManyLinks,
    Link(String),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidText => write!(f, "The command line is not valid text."),
            Self::TooLong => write!(f, "The command line is too long."),
            Self::UnknownOption(option) => {
                write!(f, "The command line has an unknown option: {option}")
            }
            Self::TooManyLinks => write!(f, "At most {MAX_LINKS} links can be opened at once."),
            Self::Link(reason) => write!(f, "The link was not opened. {reason}"),
        }
    }
}

/// One argument and where it ends in the raw command line.
struct Token {
    text: Vec<u16>,
    end: usize,
}

/// Split a command line with the rules Windows programs use (CommandLineToArgvW):
/// whitespace separates arguments, double quotes group, 2n backslashes before a
/// quote become n backslashes, 2n+1 become n backslashes and a literal quote. The
/// program name (first token) only ends at a space or tab and keeps backslashes.
fn split(raw: &[u16]) -> Vec<Token> {
    const QUOTE: u16 = b'"' as u16;
    const SLASH: u16 = b'\\' as u16;
    let blank = |c: u16| c == b' ' as u16 || c == b'\t' as u16;
    let mut tokens = Vec::new();
    let mut i = 0;
    // Program name.
    let mut name = Vec::new();
    let mut quoted = false;
    while i < raw.len() && (quoted || !blank(raw[i])) {
        if raw[i] == QUOTE {
            quoted = !quoted;
        } else {
            name.push(raw[i]);
        }
        i += 1;
    }
    tokens.push(Token { text: name, end: i });
    loop {
        while i < raw.len() && blank(raw[i]) {
            i += 1;
        }
        if i >= raw.len() {
            break;
        }
        let mut text = Vec::new();
        let mut quoted = false;
        while i < raw.len() && (quoted || !blank(raw[i])) {
            if raw[i] == SLASH {
                let start = i;
                while i < raw.len() && raw[i] == SLASH {
                    i += 1;
                }
                let count = i - start;
                if i < raw.len() && raw[i] == QUOTE {
                    text.extend(std::iter::repeat_n(SLASH, count / 2));
                    if count % 2 == 1 {
                        text.push(QUOTE);
                        i += 1;
                    }
                } else {
                    text.extend(std::iter::repeat_n(SLASH, count));
                }
                continue;
            }
            if raw[i] == QUOTE {
                // A doubled quote inside quotes is a literal quote.
                if quoted && raw.get(i + 1) == Some(&QUOTE) {
                    text.push(QUOTE);
                    i += 2;
                    continue;
                }
                quoted = !quoted;
                i += 1;
                continue;
            }
            text.push(raw[i]);
            i += 1;
        }
        tokens.push(Token { text, end: i });
    }
    tokens
}

fn text(units: &[u16]) -> Result<String, LaunchError> {
    String::from_utf16(units).map_err(|_| LaunchError::InvalidText)
}

fn link(value: &str) -> Result<String, LaunchError> {
    navigation::external(value).map_err(LaunchError::Link)
}

/// Parse the raw UTF-16 command line (as GetCommandLineW returns it).
pub fn parse(raw: &[u16]) -> Result<Launch, LaunchError> {
    if raw.len() > MAX_COMMAND_LINE {
        return Err(LaunchError::TooLong);
    }
    let tokens = split(raw);
    let Some(first) = tokens.get(1) else {
        return Ok(Launch::Open {
            private: false,
            new_window: false,
            links: Vec::new(),
        });
    };
    let first_text = text(&first.text)?;
    if first_text == SINGLE_ARGUMENT {
        // Everything after the marker is one link, verbatim. Never private.
        let rest = &raw[first.end..];
        let rest = text(rest)?;
        let mut value = rest.trim_start_matches([' ', '\t']);
        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            value = &value[1..value.len() - 1];
        }
        return Ok(Launch::Open {
            private: false,
            new_window: false,
            links: vec![link(value)?],
        });
    }
    let mut private = false;
    let mut new_window = false;
    let mut links = Vec::new();
    let mut options = true;
    for token in tokens.iter().skip(1) {
        let value = text(&token.text)?;
        if options && value.starts_with('-') {
            match value.as_str() {
                "--version" if tokens.len() == 2 => return Ok(Launch::Version),
                "--private" => private = true,
                "--new-window" => new_window = true,
                "--" => options = false,
                // The marker only counts as the first argument.
                _ => {
                    let shown: String = value.chars().take(40).collect();
                    return Err(LaunchError::UnknownOption(shown));
                }
            }
            continue;
        }
        if links.len() == MAX_LINKS {
            return Err(LaunchError::TooManyLinks);
        }
        links.push(link(&value)?);
    }
    Ok(Launch::Open {
        private,
        new_window,
        links,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().collect()
    }
    fn run(args: &str) -> Result<Launch, LaunchError> {
        parse(&wide(&format!(
            "\"C:\\Program Files\\Rust Browser\\rust-browser.exe\" {args}"
        )))
    }
    fn open(links: &[&str]) -> Result<Launch, LaunchError> {
        Ok(Launch::Open {
            private: false,
            new_window: false,
            links: links.iter().map(|l| l.to_string()).collect(),
        })
    }

    #[test]
    fn no_arguments_and_version() {
        assert_eq!(parse(&wide("rust-browser.exe")), open(&[]));
        assert_eq!(run(""), open(&[]));
        assert_eq!(run("--version"), Ok(Launch::Version));
        assert!(matches!(
            run("--version --private"),
            Err(LaunchError::UnknownOption(_))
        ));
    }

    #[test]
    fn registered_form_takes_the_rest_verbatim() {
        assert_eq!(
            run("--single-argument https://example.com/a?b=1&c=2"),
            open(&["https://example.com/a?b=1&c=2"])
        );
        // Quotes or spaces inside the link cannot create a second argument or option.
        assert!(matches!(
            run("--single-argument https://example.com/\" --private --disable-web-security"),
            Err(LaunchError::Link(_))
        ));
        assert!(matches!(
            run("--single-argument https://a.example/ https://b.example/"),
            Err(LaunchError::Link(_))
        ));
        assert_eq!(
            run("--single-argument \"https://example.com/x\""),
            open(&["https://example.com/x"])
        );
        // A link that looks like an option is still only a link.
        assert!(matches!(
            run("--single-argument --private"),
            Err(LaunchError::Link(_))
        ));
        assert!(matches!(
            run("--single-argument"),
            Err(LaunchError::Link(_))
        ));
    }

    #[test]
    fn options_never_come_from_links() {
        assert!(matches!(
            run("--disable-web-security"),
            Err(LaunchError::UnknownOption(_))
        ));
        assert!(matches!(
            run("-private"),
            Err(LaunchError::UnknownOption(_))
        ));
        assert!(matches!(
            run("https://example.com/ --single-argument https://x.example/"),
            Err(LaunchError::UnknownOption(_))
        ));
        // After "--" everything is a link, so an option-like value is refused as a link.
        assert!(matches!(run("-- --private"), Err(LaunchError::Link(_))));
        assert_eq!(
            run("--private --new-window https://example.com/"),
            Ok(Launch::Open {
                private: true,
                new_window: true,
                links: vec!["https://example.com/".into()]
            })
        );
    }

    #[test]
    fn only_explicit_http_links() {
        for value in [
            "javascript:alert(1)",
            "data:text/html,<b>x</b>",
            "file:///C:/Windows/win.ini",
            "\\\\server\\share\\page.html",
            "C:\\Users\\me\\page.html",
            "example.com",
            "ftp://example.com/",
            "rust-browser:new-tab",
            "about:blank",
            "https://user:secret@example.com/",
            "https://example.com@evil.example/",
            "https://exa mple.com/",
        ] {
            assert!(
                matches!(run(&format!("-- \"{value}\"")), Err(LaunchError::Link(_))),
                "{value}"
            );
        }
        assert_eq!(run("HTTPS://Example.com"), open(&["https://example.com/"]));
        // Empty user information carries no credentials and is removed.
        assert_eq!(
            run("https://@example.com/"),
            open(&["https://example.com/"])
        );
        assert_eq!(
            run("http://localhost:8080/x"),
            open(&["http://localhost:8080/x"])
        );
    }

    #[test]
    fn quoting_rules() {
        // A doubled quote inside quotes, or 2n+1 backslashes before a quote, is a
        // literal quote in that one argument. The URL parser percent-encodes it.
        assert_eq!(
            run(r#""https://example.com/a""b""#),
            open(&["https://example.com/a%22b"])
        );
        assert_eq!(
            run(r#""https://example.com/\"x""#),
            open(&["https://example.com/%22x"])
        );
        assert_eq!(
            run(r#""https://example.com/p""#),
            open(&["https://example.com/p"])
        );
        // An unquoted program path with spaces cannot shift the arguments either.
        assert_eq!(
            parse(&wide(r"C:\Rust\rust-browser.exe https://example.com/")),
            open(&["https://example.com/"])
        );
    }

    #[test]
    fn bounds_and_invalid_text() {
        let long = format!(
            "https://example.com/{}",
            "a".repeat(navigation::MAX_ADDRESS)
        );
        assert!(matches!(run(&long), Err(LaunchError::Link(_))));
        let many = (0..=MAX_LINKS)
            .map(|i| format!("https://{i}.example/"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(run(&many), Err(LaunchError::TooManyLinks));
        let huge = vec![b'a' as u16; MAX_COMMAND_LINE + 1];
        assert_eq!(parse(&huge), Err(LaunchError::TooLong));
        let mut raw = wide("rust-browser.exe https://example.com/");
        raw.push(0xD800);
        assert_eq!(parse(&raw), Err(LaunchError::InvalidText));
        let mut raw = wide("rust-browser.exe --single-argument https://example.com/");
        raw.push(0xDC00);
        assert_eq!(parse(&raw), Err(LaunchError::InvalidText));
    }
}
