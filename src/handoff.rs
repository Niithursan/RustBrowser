//! Single-instance hand-off: the pure parts (wire format, limits, trust rule).
//! The Windows named-pipe transport lives in the application (`native_handoff`).
//!
//! A second launch of a normal window sends one fixed-format message to the
//! browser already running on the same profile and exits. The message can only
//! ask to open links or to open a new window. The receiver decodes it with the
//! same link policy as the command line (`navigation::external`). The only reply
//! is one status byte; the channel never returns data, reads files or runs
//! commands.
use crate::{launch::MAX_LINKS, navigation};
use std::time::{Duration, Instant};

pub const MAGIC: [u8; 4] = *b"RBHO";
pub const VERSION: u16 = 1;
const OPEN: u8 = 1;
const NEW_WINDOW: u8 = 2;
/// Header (magic, version, kind, count) plus 16 links of at most 8192 bytes each.
pub const MAX_MESSAGE: usize = 8 + MAX_LINKS * (2 + navigation::MAX_ADDRESS);
/// At most this many accepted requests within `RATE_WINDOW`.
pub const RATE_LIMIT: usize = 8;
pub const RATE_WINDOW: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Open(Vec<String>),
    NewWindow,
}

/// The single reply byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    Accepted = 1,
    Refused = 2,
    Busy = 3,
    Closing = 4,
    NotResponding = 5,
}

impl Status {
    pub fn from_byte(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::Accepted,
            2 => Self::Refused,
            3 => Self::Busy,
            4 => Self::Closing,
            5 => Self::NotResponding,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Malformed {
    TooLarge,
    Magic,
    Version(u16),
    Kind(u8),
    Count,
    Truncated,
    Trailing,
    Text,
    Link(String),
}

pub fn encode(request: &Request) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    match request {
        Request::NewWindow => out.extend_from_slice(&[NEW_WINDOW, 0]),
        Request::Open(links) => {
            out.extend_from_slice(&[OPEN, links.len().min(255) as u8]);
            for link in links {
                let bytes = link.as_bytes();
                out.extend_from_slice(&(bytes.len().min(u16::MAX as usize) as u16).to_le_bytes());
                out.extend_from_slice(bytes);
            }
        }
    }
    out
}

/// Decode and re-validate a message. Anything unexpected refuses the whole message.
pub fn decode(message: &[u8]) -> Result<Request, Malformed> {
    if message.len() > MAX_MESSAGE {
        return Err(Malformed::TooLarge);
    }
    if message.len() < 8 {
        return Err(Malformed::Truncated);
    }
    if message[..4] != MAGIC {
        return Err(Malformed::Magic);
    }
    let version = u16::from_le_bytes([message[4], message[5]]);
    if version != VERSION {
        return Err(Malformed::Version(version));
    }
    let (kind, count) = (message[6], message[7] as usize);
    let mut rest = &message[8..];
    let request = match kind {
        NEW_WINDOW if count == 0 => Request::NewWindow,
        NEW_WINDOW => return Err(Malformed::Count),
        OPEN if (1..=MAX_LINKS).contains(&count) => {
            let mut links = Vec::with_capacity(count);
            for _ in 0..count {
                if rest.len() < 2 {
                    return Err(Malformed::Truncated);
                }
                let length = u16::from_le_bytes([rest[0], rest[1]]) as usize;
                if length > navigation::MAX_ADDRESS {
                    return Err(Malformed::TooLarge);
                }
                let Some(bytes) = rest.get(2..2 + length) else {
                    return Err(Malformed::Truncated);
                };
                let text = std::str::from_utf8(bytes).map_err(|_| Malformed::Text)?;
                links.push(navigation::external(text).map_err(Malformed::Link)?);
                rest = &rest[2 + length..];
            }
            Request::Open(links)
        }
        OPEN => return Err(Malformed::Count),
        other => return Err(Malformed::Kind(other)),
    };
    if !rest.is_empty() {
        return Err(Malformed::Trailing);
    }
    Ok(request)
}

/// Sliding-window limit on accepted requests.
#[derive(Default)]
pub struct RateLimiter {
    recent: Vec<Instant>,
}
impl RateLimiter {
    pub fn admit(&mut self, now: Instant) -> bool {
        self.recent
            .retain(|at| now.duration_since(*at) < RATE_WINDOW);
        if self.recent.len() >= RATE_LIMIT {
            return false;
        }
        self.recent.push(now);
        true
    }
}

/// Who is on the other end of the pipe, as reported by Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// String form of the process token's user SID.
    pub user: String,
    pub session: u32,
    /// Full executable path, compared case-insensitively.
    pub executable: String,
}

/// The server must be this same user, in this same Windows session, running this
/// same executable. A different user cannot reach the pipe (its DACL allows only
/// this user) or satisfy this check; a same-user process could imitate either side,
/// which is outside the boundary: it already runs with all of the user's rights.
pub fn trusted_server(server: &Peer, me: &Peer) -> bool {
    server.user == me.user
        && server.session == me.session
        && !me.executable.is_empty()
        && server.executable.eq_ignore_ascii_case(&me.executable)
}

/// A client must be this same user in this same session. Its executable is not
/// required to match (any same-user program may ask to open a link, as it could
/// by starting the browser itself).
pub fn trusted_client(client: &Peer, me: &Peer) -> bool {
    client.user == me.user && client.session == me.session
}

/// Stable 64-bit FNV-1a, so every build derives the same endpoint name.
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The endpoint for one user, Windows session and profile. The profile path is
/// compared case-insensitively, as Windows paths are.
pub fn endpoint(user: &str, session: u32, profile: &str) -> String {
    let key = format!("{user}\n{}", profile.to_lowercase());
    format!(
        r"\\.\pipe\RustBrowser.HandOff.{session}.{:016x}",
        fnv1a(key.as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(links: &[&str]) -> Request {
        Request::Open(links.iter().map(|l| l.to_string()).collect())
    }

    #[test]
    fn round_trip() {
        for request in [open(&["https://example.com/"]), Request::NewWindow] {
            assert_eq!(decode(&encode(&request)), Ok(request));
        }
        let many: Vec<String> = (0..MAX_LINKS)
            .map(|i| format!("https://{i}.example/"))
            .collect();
        let request = Request::Open(many);
        assert_eq!(decode(&encode(&request)), Ok(request));
    }

    #[test]
    fn malformed_messages_are_refused() {
        let good = encode(&open(&["https://example.com/"]));
        let mut wrong = good.clone();
        wrong[0] = b'X';
        assert_eq!(decode(&wrong), Err(Malformed::Magic));
        let mut wrong = good.clone();
        wrong[4] = 2;
        assert_eq!(decode(&wrong), Err(Malformed::Version(2)));
        let mut wrong = good.clone();
        wrong[6] = 9;
        assert_eq!(decode(&wrong), Err(Malformed::Kind(9)));
        assert_eq!(decode(&good[..good.len() - 1]), Err(Malformed::Truncated));
        assert_eq!(decode(&good[..5]), Err(Malformed::Truncated));
        let mut wrong = good.clone();
        wrong.push(0);
        assert_eq!(decode(&wrong), Err(Malformed::Trailing));
        assert_eq!(decode(&vec![0; MAX_MESSAGE + 1]), Err(Malformed::TooLarge));
        // Zero links, too many links, a new-window request with links.
        assert_eq!(
            decode(&[b'R', b'B', b'H', b'O', 1, 0, 1, 0]),
            Err(Malformed::Count)
        );
        assert_eq!(
            decode(&[b'R', b'B', b'H', b'O', 1, 0, 1, 17]),
            Err(Malformed::Count)
        );
        assert_eq!(
            decode(&[b'R', b'B', b'H', b'O', 1, 0, 2, 1]),
            Err(Malformed::Count)
        );
        // An over-long length field.
        let mut wrong = vec![b'R', b'B', b'H', b'O', 1, 0, 1, 1];
        wrong.extend_from_slice(&((navigation::MAX_ADDRESS + 1) as u16).to_le_bytes());
        assert_eq!(decode(&wrong), Err(Malformed::TooLarge));
        // Invalid UTF-8.
        let mut wrong = vec![b'R', b'B', b'H', b'O', 1, 0, 1, 1, 2, 0];
        wrong.extend_from_slice(&[0xff, 0xfe]);
        assert_eq!(decode(&wrong), Err(Malformed::Text));
    }

    #[test]
    fn links_are_revalidated_by_the_receiver() {
        for link in [
            "javascript:alert(1)",
            "file:///C:/Windows/win.ini",
            "data:text/html,x",
            "https://user:pw@example.com/",
            "example.com",
            "--private",
        ] {
            assert!(
                matches!(decode(&encode(&open(&[link]))), Err(Malformed::Link(_))),
                "{link}"
            );
        }
    }

    #[test]
    fn rate_limit() {
        let mut limiter = RateLimiter::default();
        let start = Instant::now();
        for _ in 0..RATE_LIMIT {
            assert!(limiter.admit(start));
        }
        assert!(!limiter.admit(start));
        assert!(limiter.admit(start + RATE_WINDOW));
    }

    #[test]
    fn trust_rules() {
        let me = Peer {
            user: "S-1-5-21-1-2-3-1001".into(),
            session: 1,
            executable: r"C:\Apps\Rust Browser\rust-browser.exe".into(),
        };
        assert!(trusted_server(&me, &me));
        let other_user = Peer {
            user: "S-1-5-21-1-2-3-1002".into(),
            ..me.clone()
        };
        let other_session = Peer {
            session: 2,
            ..me.clone()
        };
        let other_program = Peer {
            executable: r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".into(),
            ..me.clone()
        };
        let case = Peer {
            executable: me.executable.to_uppercase(),
            ..me.clone()
        };
        assert!(!trusted_server(&other_user, &me));
        assert!(!trusted_server(&other_session, &me));
        assert!(!trusted_server(&other_program, &me));
        assert!(trusted_server(&case, &me));
        assert!(!trusted_server(
            &me,
            &Peer {
                executable: String::new(),
                ..me.clone()
            }
        ));
        assert!(trusted_client(&other_program, &me));
        assert!(!trusted_client(&other_user, &me));
        assert!(!trusted_client(&other_session, &me));
    }

    #[test]
    fn endpoint_names_separate_user_session_and_profile() {
        let a = endpoint(
            "S-1-5-21-1-1001",
            1,
            r"C:\Users\a\AppData\Local\RustBrowser\v0.1",
        );
        assert_eq!(
            a,
            endpoint(
                "S-1-5-21-1-1001",
                1,
                r"c:\users\A\AppData\Local\RustBrowser\v0.1"
            )
        );
        assert_ne!(
            a,
            endpoint(
                "S-1-5-21-1-1002",
                1,
                r"C:\Users\a\AppData\Local\RustBrowser\v0.1"
            )
        );
        assert_ne!(
            a,
            endpoint(
                "S-1-5-21-1-1001",
                2,
                r"C:\Users\a\AppData\Local\RustBrowser\v0.1"
            )
        );
        assert_ne!(a, endpoint("S-1-5-21-1-1001", 1, r"C:\Users\a\other"));
        assert!(a.starts_with(r"\\.\pipe\RustBrowser.HandOff.1."));
    }
}
