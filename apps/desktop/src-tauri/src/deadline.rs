use std::time::Duration;
use tokio::time::Instant;

/// One absolute operation deadline. Nested work may consume the remaining
/// budget but must never manufacture a fresh duration-based timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Deadline {
    at: Instant,
}

impl Deadline {
    pub(crate) fn after(duration: Duration) -> Self {
        Self {
            at: Instant::now() + duration,
        }
    }

    pub(crate) fn at(at: Instant) -> Self {
        Self { at }
    }

    pub(crate) fn instant(self) -> Instant {
        self.at
    }

    pub(crate) fn is_elapsed(self) -> bool {
        Instant::now() >= self.at
    }

    /// Cleanup may use one small, explicit post-deadline slack window. Before
    /// the business deadline expires cleanup remains inside the same budget.
    pub(crate) fn cleanup_deadline(self, slack: Duration) -> Instant {
        let now = Instant::now();
        if now < self.at {
            std::cmp::min(self.at, now + slack)
        } else {
            now + slack
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nested_deadline_never_resets_outer_budget() {
        let deadline = Deadline::after(Duration::from_millis(80));
        tokio::time::sleep(Duration::from_millis(30)).await;
        let nested = Deadline::at(deadline.instant());
        assert!(
            nested.instant().saturating_duration_since(Instant::now()) <= Duration::from_millis(60)
        );
        tokio::time::sleep_until(nested.instant()).await;
        assert!(deadline.is_elapsed());
    }
}
