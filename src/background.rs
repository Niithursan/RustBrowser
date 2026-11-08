//! Engine-independent eligibility and asynchronous suspension generations.
use serde::Serialize;
use std::time::Duration;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Awake,
    Pending,
    Sleeping,
    Unavailable,
    ResumeFailed,
}
#[derive(Default, Clone, Copy)]
pub struct Protection {
    pub active: bool,
    pub pinned: bool,
    pub keep_awake: bool,
    pub loading: bool,
    pub downloads: bool,
    pub permissions: bool,
    pub capture: bool,
    pub audio: bool,
    pub busy: bool,
}
impl Protection {
    pub fn protected(self) -> bool {
        self.active
            || self.pinned
            || self.keep_awake
            || self.loading
            || self.downloads
            || self.permissions
            || self.capture
            || self.audio
            || self.busy
    }
}
#[derive(Default)]
pub struct Lifecycle {
    pub phase: Phase,
    pub generation: u64,
    hidden_at: Option<Duration>,
    retry_at: Duration,
}
impl Lifecycle {
    /// Reset the model after the view is awake, keeping previously issued tickets stale.
    pub fn reset(&mut self) {
        self.generation += 1;
        self.phase = Phase::Awake;
        self.hidden_at = None;
        self.retry_at = Duration::ZERO;
    }

    pub fn visible(&mut self, active: bool, now: Duration) {
        if active {
            self.hidden_at = None;
        } else if self.hidden_at.is_none() {
            self.hidden_at = Some(now);
        }
    }
    pub fn request(
        &mut self,
        now: Duration,
        delay: Duration,
        enabled: bool,
        protection: Protection,
    ) -> Option<u64> {
        if !enabled
            || protection.protected()
            || self.phase != Phase::Awake
            || now < self.retry_at
            || self
                .hidden_at
                .is_none_or(|since| now.saturating_sub(since) < delay)
        {
            return None;
        }
        self.generation += 1;
        self.phase = Phase::Pending;
        self.retry_at = now + Duration::from_secs(30);
        Some(self.generation)
    }
    pub fn complete(&mut self, ticket: u64, sleeping: bool, now: Duration) -> bool {
        if self.generation != ticket || self.phase != Phase::Pending {
            return false;
        }
        self.phase = if sleeping {
            Phase::Sleeping
        } else {
            Phase::Awake
        };
        self.retry_at = now + Duration::from_secs(30);
        true
    }
    pub fn resumed(&mut self, success: bool) {
        self.generation += 1;
        self.phase = if success {
            Phase::Awake
        } else {
            Phase::ResumeFailed
        };
    }
    pub fn unsupported(&mut self) {
        self.generation += 1;
        self.phase = Phase::Unavailable;
    }
    pub fn pending_expired(&self, now: Duration) -> bool {
        self.phase == Phase::Pending && now >= self.retry_at
    }
    pub fn defer(&mut self, now: Duration) {
        self.retry_at = now + Duration::from_secs(30);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resetting_lifecycle_invalidates_outstanding_suspend_results() {
        let mut lifecycle = Lifecycle::default();
        lifecycle.visible(false, Duration::ZERO);
        let first = lifecycle
            .request(Duration::ZERO, Duration::ZERO, true, Protection::default())
            .expect("an eligible hidden tab should request suspension");

        lifecycle.reset();
        lifecycle.visible(false, Duration::ZERO);
        let second = lifecycle
            .request(Duration::ZERO, Duration::ZERO, true, Protection::default())
            .expect("reset should allow a fresh suspension request");

        assert_ne!(first, second);
        assert!(!lifecycle.complete(first, true, Duration::ZERO));
        assert_eq!(lifecycle.phase, Phase::Pending);
        assert!(lifecycle.complete(second, true, Duration::ZERO));
        assert_eq!(lifecycle.phase, Phase::Sleeping);
    }
}
