//! Reading an exported bookmarks file (the Netscape bookmark HTML format that
//! Firefox, Chrome and Edge export). The file is hostile input: it is scanned as
//! text only. Nothing in it is executed, fetched, resolved or followed; only the
//! `HREF` of each link and the text of links and folder headings are read.
use std::collections::BTreeMap;

/// Largest file read (10 MiB). Real exports with thousands of bookmarks and
/// embedded icons stay well below this.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Most links a file may contain. The library holds at most 5,000 bookmarks.
pub const MAX_LINKS: usize = 20_000;
/// Deepest folder nesting accepted.
pub const MAX_DEPTH: usize = 32;
/// Longest bookmark title kept (the library's limit); longer titles are shortened.
pub const MAX_TITLE: usize = 256;
/// Longest folder name kept (the library's limit); longer names are shortened.
pub const MAX_FOLDER: usize = 80;
/// Longest address accepted (the library's limit); longer links are refused.
pub const MAX_URL: usize = 8192;

/// Why a whole file was refused. Nothing is imported from a refused file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileError {
    TooLarge,
    NotUtf8,
    NotBookmarks,
    TooManyLinks,
    TooDeep,
}

impl FileError {
    pub fn message(self) -> String {
        match self {
            Self::TooLarge => format!(
                "The file is larger than {} MiB, so nothing was imported.",
                MAX_FILE_BYTES / (1024 * 1024)
            ),
            Self::NotUtf8 => {
                "The file is not valid UTF-8 text, so nothing was imported.".into()
            }
            Self::NotBookmarks => "This is not an exported bookmarks file (bookmarks HTML from Firefox, Chrome or Edge), so nothing was imported.".into(),
            Self::TooManyLinks => format!(
                "The file has more than {MAX_LINKS} links, so nothing was imported."
            ),
            Self::TooDeep => format!(
                "Folders are nested more than {MAX_DEPTH} levels deep, so nothing was imported."
            ),
        }
    }
}

/// Why one link was not imported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Refused {
    /// Not an http or https address (javascript:, data:, file:, place: and others).
    NotWeb,
    /// The address contains a user name or password.
    Credentials,
    /// The address is longer than `MAX_URL`.
    TooLong,
    /// The address is missing or not valid.
    Invalid,
}

impl Refused {
    pub fn describe(self) -> &'static str {
        match self {
            Self::NotWeb => "not an http or https address",
            Self::Credentials => "contained a user name or password",
            Self::TooLong => "address too long",
            Self::Invalid => "missing or invalid address",
        }
    }
}

/// One link to import. `folder` is the innermost folder holding it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub title: String,
    pub url: String,
    pub folder: Option<String>,
}

/// What a file contained.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub entries: Vec<Entry>,
    pub refused: BTreeMap<Refused, usize>,
    /// Folders found inside other folders. The library has no nesting, so they
    /// become top-level folders.
    pub nested: usize,
}

/// Read a bookmarks file's bytes.
pub fn parse(bytes: &[u8]) -> Result<Parsed, FileError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(FileError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| FileError::NotUtf8)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let start = text.trim_start();
    if !start
        .get(..35)
        .is_some_and(|head| head.eq_ignore_ascii_case("<!DOCTYPE NETSCAPE-Bookmark-file-1>"))
    {
        return Err(FileError::NotBookmarks);
    }
    let mut parsed = Parsed::default();
    // One entry per open list: the folder it belongs to, if any.
    let mut lists: Vec<Option<String>> = Vec::new();
    let mut heading: Option<String> = None;
    let mut links = 0;
    let mut rest = start;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('>') else { break };
        let tag = &rest[..close];
        rest = &rest[close + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_uppercase();
        match (name.as_str(), closing) {
            ("DL", false) => {
                if lists.len() >= MAX_DEPTH {
                    return Err(FileError::TooDeep);
                }
                lists.push(heading.take());
            }
            ("DL", true) => {
                lists.pop();
            }
            ("H3", false) => {
                let (inner, after) = element_text(rest, "</H3");
                rest = after;
                if lists.iter().any(Option::is_some) {
                    parsed.nested += 1;
                }
                let name = clean(&decode(inner), MAX_FOLDER);
                heading = Some(if name.is_empty() {
                    "Imported".into()
                } else {
                    name
                });
            }
            ("A", false) => {
                links += 1;
                if links > MAX_LINKS {
                    return Err(FileError::TooManyLinks);
                }
                let (inner, after) = element_text(rest, "</A");
                rest = after;
                let href = attribute(tag, "HREF")
                    .map(|v| decode(&v))
                    .unwrap_or_default();
                match accept(&href) {
                    Ok(url) => {
                        let mut title = clean(&decode(inner), MAX_TITLE);
                        if title.is_empty() {
                            title = clean(&url, MAX_TITLE);
                        }
                        let folder = lists.iter().rev().find_map(Clone::clone);
                        parsed.entries.push(Entry { title, url, folder });
                    }
                    Err(reason) => *parsed.refused.entry(reason).or_default() += 1,
                }
            }
            _ => {}
        }
    }
    Ok(parsed)
}

/// The address to store for a link, or why it is refused. Only explicit http
/// and https addresses are accepted, under the same rules as saved bookmarks.
fn accept(href: &str) -> Result<String, Refused> {
    let href = href.trim();
    if href.is_empty() {
        return Err(Refused::Invalid);
    }
    if href.len() > MAX_URL {
        return Err(Refused::TooLong);
    }
    let lower = href.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(Refused::NotWeb);
    }
    match url::Url::parse(href) {
        Ok(url) if !url.username().is_empty() || url.password().is_some() => {
            return Err(Refused::Credentials);
        }
        Ok(_) => {}
        Err(_) => return Err(Refused::Invalid),
    }
    let url = crate::library::bookmark_url(href).map_err(|_| Refused::Invalid)?;
    if url.len() > MAX_URL {
        return Err(Refused::TooLong);
    }
    Ok(url)
}

/// The text up to the closing tag `end` (matched without regard to case), and
/// what follows the whole element. A missing closing tag ends the text at the
/// next tag. Linear in the length scanned.
fn element_text<'a>(rest: &'a str, end: &str) -> (&'a str, &'a str) {
    let mut from = 0;
    while let Some(found) = rest[from..].find('<') {
        let at = from + found;
        if rest
            .get(at..at + end.len())
            .is_some_and(|tag| tag.eq_ignore_ascii_case(end))
        {
            let after = rest[at..].find('>').map_or("", |gt| &rest[at + gt + 1..]);
            return (&rest[..at], after);
        }
        // Another element starts: the closing tag is missing.
        if rest[at + 1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            return (&rest[..at], &rest[at..]);
        }
        from = at + 1;
    }
    (rest, "")
}

/// The value of attribute `name` in a tag's text (quoted or unquoted). Only the
/// exact attribute name matches, so `ICON_URI` or `FEEDURL` are never read as
/// `HREF`.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut i = 0;
    // Skip the tag name.
    while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
        i += 1;
    }
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let key_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let key = &tag[key_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                let start = i + 1;
                let end = tag[start..]
                    .find(quote as char)
                    .map_or(tag.len(), |at| start + at);
                value = tag[start..end].to_string();
                i = (end + 1).min(bytes.len());
            } else {
                let start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = tag[start..i].to_string();
            }
        }
        if key.eq_ignore_ascii_case(name) {
            return Some(value);
        }
        if key.is_empty() {
            i += 1;
        }
    }
    None
}

/// Decode the character references exports use. Unknown references stay as
/// written; nothing is looked up outside the file.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest.bytes().take(12).position(|b| b == b';');
        let decoded = end.and_then(|end| {
            let name = &rest[1..end];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                _ if name.starts_with("#x") || name.starts_with("#X") => {
                    u32::from_str_radix(&name[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            ch.map(|ch| (ch, end))
        });
        match decoded {
            Some((ch, end)) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Text without control characters, with whitespace runs collapsed, at most
/// `max` characters.
fn clean(text: &str, max: usize) -> String {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect();
    words.join(" ").chars().take(max).collect()
}

/// Result of importing a parsed file into the library.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub imported: usize,
    pub duplicates: usize,
    pub refused: BTreeMap<Refused, usize>,
    pub folders_created: usize,
    pub nested: usize,
    /// Bookmarks kept outside any folder because the folder limit was reached.
    pub unfoldered: usize,
}

impl Summary {
    /// The notice shown after an import.
    pub fn message(&self) -> String {
        let refused: usize = self.refused.values().sum();
        let mut text = format!(
            "Bookmarks imported: {} added, {} skipped as duplicates, {} refused",
            self.imported, self.duplicates, refused
        );
        if refused > 0 {
            let reasons: Vec<String> = self
                .refused
                .iter()
                .map(|(reason, count)| format!("{count} {}", reason.describe()))
                .collect();
            text += &format!(" ({})", reasons.join(", "));
        }
        text.push('.');
        if self.folders_created > 0 {
            text += &format!(" {} new folders.", self.folders_created);
        }
        if self.nested > 0 {
            text += &format!(
                " Folders here cannot contain folders, so {} folders that were inside other folders became top-level folders.",
                self.nested
            );
        }
        if self.unfoldered > 0 {
            text += &format!(
                " The folder limit was reached, so {} bookmarks were added outside any folder.",
                self.unfoldered
            );
        }
        text
    }
}
