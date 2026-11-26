//! Website permission requests on Linux, with the Windows rules: only camera,
//! microphone, location and notifications can be asked for, only by an HTTPS (or
//! loopback) page in the selected tab, and only Allow allows. A request waits at
//! most a minute, and leaving the page or the tab denies it. Everything else
//! WebKitGTK can ask for (screen sharing, pointer lock, clipboard, media keys,
//! storage access) is denied.
//!
//! WebKitGTK differs from WebView2 in two ways. It reports no user gesture, so
//! a request the user turned down is denied quietly in that tab until it
//! navigates, instead of asking again. And one camera request can also ask for
//! the microphone: each is asked for in turn, and the request is allowed only
//! when both are.
use rust_browser::{
    security::{self, Capability, Decision, Prompt, Rule},
    tabs::TabId,
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use webkit2gtk::{PermissionRequestExt, UserMediaPermissionRequest, UserMediaPermissionRequestExt};

pub type Shared = Rc<RefCell<Permissions>>;

/// How long a request waits for an answer before it is denied, as on Windows.
pub const PROMPT_LIFETIME: Duration = Duration::from_secs(60);
/// The most requests waiting at once.
const MAX_PENDING: usize = 4;
/// The most turned-down requests remembered for quiet denial.
const MAX_DISMISSED: usize = 64;

/// Whole seconds left before a request made `elapsed` ago expires, rounded up.
fn seconds_left(elapsed: Duration) -> u64 {
    let left = PROMPT_LIFETIME.saturating_sub(elapsed);
    left.as_secs() + u64::from(left.subsec_nanos() > 0)
}

struct Pending {
    prompt: Prompt,
    request: webkit2gtk::PermissionRequest,
    /// Capabilities the same request still needs after this one.
    rest: Vec<Capability>,
    created: Instant,
}

#[derive(Default)]
pub struct Permissions {
    pending: Vec<Pending>,
    next: u64,
    /// The selected tab: the only one that may ask.
    pub active: Option<TabId>,
    /// Remembered answers, from the library (or this private window).
    pub rules: Vec<Rule>,
    /// Requests turned down in a tab, until it navigates.
    dismissed: Vec<(TabId, String, Capability)>,
}

/// What a WebKit request asks for, in the order to ask; empty when it is
/// something the browser never allows.
fn capabilities(request: &webkit2gtk::PermissionRequest) -> Vec<Capability> {
    use gtk::glib::prelude::{Cast, ObjectExt};
    if let Some(media) = request.downcast_ref::<UserMediaPermissionRequest>() {
        // Screen sharing asks for neither device.
        let mut wanted = Vec::new();
        if media.is_for_video_device() {
            wanted.push(Capability::Camera);
        }
        if media.is_for_audio_device() {
            wanted.push(Capability::Microphone);
        }
        wanted
    } else if request.is::<webkit2gtk::GeolocationPermissionRequest>() {
        vec![Capability::Geolocation]
    } else if request.is::<webkit2gtk::NotificationPermissionRequest>() {
        vec![Capability::Notifications]
    } else {
        Vec::new()
    }
}

impl Permissions {
    /// A page in `tab`, at `url`, asks for `request`. Answers it now from the
    /// rules, or keeps it for the user; returns whether a prompt was added.
    pub fn request(
        &mut self,
        tab: TabId,
        url: Option<&str>,
        request: &webkit2gtk::PermissionRequest,
    ) -> bool {
        let wanted = capabilities(request);
        let origin = url.and_then(security::permission_origin);
        let (Some(origin), false) = (origin, wanted.is_empty()) else {
            request.deny();
            return false;
        };
        if self.active != Some(tab) {
            request.deny();
            return false;
        }
        let Some(rest) = self.undecided(&origin, &wanted) else {
            request.deny();
            return false;
        };
        let Some((&first, rest)) = rest.split_first() else {
            request.allow();
            return false;
        };
        if self.pending.len() >= MAX_PENDING
            || rest
                .iter()
                .chain([&first])
                .any(|c| self.was_dismissed(tab, &origin, *c))
            || self.pending.iter().any(|p| {
                p.prompt.tab == tab && p.prompt.origin == origin && p.prompt.capability == first
            })
        {
            request.deny();
            return false;
        }
        self.ask(tab, origin, first, rest.to_vec(), request.clone());
        true
    }

    /// The capabilities in `wanted` no rule answers yet, or `None` when a rule
    /// denies one of them.
    fn undecided(&self, origin: &str, wanted: &[Capability]) -> Option<Vec<Capability>> {
        let mut left = Vec::new();
        for &capability in wanted {
            match self
                .rules
                .iter()
                .find(|r| r.origin == origin && r.capability == capability)
                .map(|r| r.decision)
            {
                Some(Decision::Deny) => return None,
                Some(Decision::Allow) => {}
                None => left.push(capability),
            }
        }
        Some(left)
    }

    fn was_dismissed(&self, tab: TabId, origin: &str, capability: Capability) -> bool {
        self.dismissed
            .iter()
            .any(|(t, o, c)| *t == tab && o == origin && *c == capability)
    }

    fn ask(
        &mut self,
        tab: TabId,
        origin: String,
        capability: Capability,
        rest: Vec<Capability>,
        request: webkit2gtk::PermissionRequest,
    ) {
        self.next += 1;
        self.pending.push(Pending {
            prompt: Prompt {
                id: self.next,
                tab,
                origin,
                capability,
            },
            request,
            rest,
            created: Instant::now(),
        });
    }

    pub fn prompt(&self) -> Option<Prompt> {
        self.pending.first().map(|p| p.prompt.clone())
    }

    pub fn prompt_seconds_left(&self) -> Option<u64> {
        self.pending
            .first()
            .map(|p| seconds_left(p.created.elapsed()))
    }

    /// Keep a remembered answer for the rest of this run (the library keeps it
    /// for the next).
    pub fn remember(&mut self, rule: Rule) {
        self.rules
            .retain(|r| !(r.origin == rule.origin && r.capability == rule.capability));
        self.rules.push(rule);
    }

    /// Answer request `id`. Allow counts only while its tab is selected and the
    /// minute has not run out; a request that also needs another capability is
    /// then asked for that one. Returns whether a new prompt was added.
    pub fn resolve(&mut self, id: u64, decision: Decision) -> bool {
        let Some(index) = self.pending.iter().position(|p| p.prompt.id == id) else {
            return false;
        };
        let pending = self.pending.remove(index);
        let Prompt {
            tab,
            origin,
            capability,
            ..
        } = pending.prompt;
        let allowed = decision == Decision::Allow
            && self.active == Some(tab)
            && pending.created.elapsed() < PROMPT_LIFETIME;
        if !allowed {
            pending.request.deny();
            if self.dismissed.len() >= MAX_DISMISSED {
                self.dismissed.remove(0);
            }
            self.dismissed.push((tab, origin, capability));
            return false;
        }
        match self.undecided(&origin, &pending.rest) {
            None => {
                pending.request.deny();
                false
            }
            Some(rest) => match rest.split_first() {
                None => {
                    pending.request.allow();
                    false
                }
                Some((&next, rest)) => {
                    self.ask(tab, origin, next, rest.to_vec(), pending.request);
                    true
                }
            },
        }
    }

    fn deny_where(&mut self, matches: impl Fn(&Prompt) -> bool) {
        let ids: Vec<_> = self
            .pending
            .iter()
            .filter(|p| matches(&p.prompt))
            .map(|p| p.prompt.id)
            .collect();
        for id in ids {
            self.resolve(id, Decision::Deny);
        }
    }

    /// The tab navigated or closed: deny what it asked for, and let its next
    /// page ask again.
    pub fn cancel_tab(&mut self, tab: TabId) {
        self.deny_where(|p| p.tab == tab);
        self.dismissed.retain(|(t, _, _)| *t != tab);
    }

    pub fn cancel_all(&mut self) {
        self.deny_where(|_| true);
    }

    /// Deny requests whose minute ran out. Returns whether any did.
    pub fn expire(&mut self) -> bool {
        let before = self.pending.len();
        let expired: Vec<_> = self
            .pending
            .iter()
            .filter(|p| p.created.elapsed() >= PROMPT_LIFETIME)
            .map(|p| p.prompt.id)
            .collect();
        for id in expired {
            self.resolve(id, Decision::Deny);
        }
        self.pending.len() != before
    }
}

impl Drop for Permissions {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

#[cfg(test)]
mod tests {
    use super::{Permissions, seconds_left};
    use rust_browser::security::{Capability, Decision, Rule};
    use std::time::Duration;

    const SITE: &str = "https://meet.example";

    fn rule(capability: Capability, decision: Decision) -> Rule {
        Rule {
            origin: SITE.into(),
            capability,
            decision,
        }
    }

    #[test]
    fn remembered_answers_decide_before_the_user_is_asked() {
        let mut permissions = Permissions::default();
        let both = [Capability::Camera, Capability::Microphone];
        assert_eq!(permissions.undecided(SITE, &both), Some(both.to_vec()));
        permissions.remember(rule(Capability::Camera, Decision::Allow));
        assert_eq!(
            permissions.undecided(SITE, &both),
            Some(vec![Capability::Microphone])
        );
        // One remembered refusal denies a request that also wants the camera.
        permissions.remember(rule(Capability::Microphone, Decision::Deny));
        assert_eq!(permissions.undecided(SITE, &both), None);
        // Another site is not covered.
        assert_eq!(
            permissions.undecided("https://other.example", &both),
            Some(both.to_vec())
        );
    }

    #[test]
    fn remembering_again_replaces_the_earlier_answer() {
        let mut permissions = Permissions::default();
        permissions.remember(rule(Capability::Geolocation, Decision::Deny));
        permissions.remember(rule(Capability::Geolocation, Decision::Allow));
        assert_eq!(permissions.rules.len(), 1);
        assert_eq!(
            permissions.undecided(SITE, &[Capability::Geolocation]),
            Some(Vec::new())
        );
    }

    #[test]
    fn turned_down_requests_are_forgotten_when_the_tab_navigates() {
        let mut permissions = Permissions::default();
        permissions
            .dismissed
            .push((7, SITE.into(), Capability::Notifications));
        assert!(permissions.was_dismissed(7, SITE, Capability::Notifications));
        assert!(!permissions.was_dismissed(8, SITE, Capability::Notifications));
        permissions.cancel_tab(7);
        assert!(!permissions.was_dismissed(7, SITE, Capability::Notifications));
    }

    #[test]
    fn a_request_counts_down_whole_seconds_and_never_shows_zero_while_open() {
        assert_eq!(seconds_left(Duration::ZERO), 60);
        assert_eq!(seconds_left(Duration::from_millis(59_001)), 1);
        assert_eq!(seconds_left(Duration::from_secs(60)), 0);
    }
}
