//! Origin identity and UI status use parsed engine URLs, never website titles.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Camera,
    Microphone,
    Geolocation,
    Notifications,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Rule {
    pub origin: String,
    pub capability: Capability,
    pub decision: Decision,
}

pub fn origin(value: &str) -> Option<String> {
    crate::navigation::address(value).ok()?;
    let url = url::Url::parse(value).ok()?;
    Some(url.origin().ascii_serialization())
}
/// Restrict permission grants to HTTPS or Chromium's loopback secure contexts.
pub fn permission_origin(value: &str) -> Option<String> {
    let canonical = origin(value)?;
    let url = url::Url::parse(&canonical).ok()?;
    let local = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    (url.scheme() == "https" || local).then_some(canonical)
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Indicator {
    pub origin: String,
    pub scheme: String,
    pub message: String,
}
pub fn indicator(value: &str, loading: bool, failed: bool) -> Indicator {
    if value == crate::navigation::HOME {
        return Indicator {
            origin: "New tab".into(),
            scheme: "local".into(),
            message: "Local browser page. No website connection.".into(),
        };
    }
    let Some(origin) = origin(value) else {
        return Indicator {
            origin: "No website origin".into(),
            scheme: "none".into(),
            message: "No secure connection is established.".into(),
        };
    };
    let scheme = if origin.starts_with("https:") {
        "https"
    } else {
        "http"
    };
    let message = if failed {
        "Navigation failed. No successful connection is confirmed."
    } else if loading {
        "Connecting. The requested scheme does not confirm a completed connection."
    } else if scheme == "https" {
        "HTTPS encrypts this connection. It does not establish that the website is trustworthy."
    } else {
        "HTTP is not encrypted. Others on the network may read or change this traffic."
    };
    Indicator {
        origin,
        scheme: scheme.into(),
        message: message.into(),
    }
}
