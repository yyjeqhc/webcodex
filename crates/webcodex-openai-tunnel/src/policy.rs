use crate::{deadline::ResponseTimeout, Error};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, Clone, Copy)]
pub enum DeadlinePolicy {
    /// Official malformed/absent timeout behavior: no command deadline.
    ProtocolCompatible,
    /// Reject malformed values; cap valid values and supply a finite absent budget.
    RequireFinite { max_duration: Duration },
}
impl DeadlinePolicy {
    pub fn deadline(
        self,
        timeout: ResponseTimeout,
        received: Instant,
    ) -> Result<Option<Instant>, Error> {
        let duration = match (self, timeout) {
            (Self::ProtocolCompatible, ResponseTimeout::Valid(d)) => Some(d),
            (Self::ProtocolCompatible, _) => None,
            (Self::RequireFinite { .. }, ResponseTimeout::Invalid) => return Err(Error::Protocol),
            (Self::RequireFinite { max_duration }, ResponseTimeout::Absent) => Some(max_duration),
            (Self::RequireFinite { max_duration }, ResponseTimeout::Valid(d)) => {
                Some(d.min(max_duration))
            }
        };
        duration
            .map(|d| received.checked_add(d).ok_or(Error::Protocol))
            .transpose()
    }
}
impl Default for DeadlinePolicy {
    fn default() -> Self {
        Self::RequireFinite {
            max_duration: Duration::from_secs(120),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub concurrency: usize,
    /// Maximum accepted poll batch, independent of the server's `limit` hint.
    pub ingress_commands: usize,
    /// Receipts are never evicted during a client lifetime. Exhaustion stops polling.
    pub lifetime_commands: usize,
    pub body_bytes: usize,
    pub event_bytes: usize,
    pub poll_timeout: Duration,
    pub response_attempts: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            concurrency: 8,
            ingress_commands: 128,
            lifetime_commands: 65_536,
            body_bytes: 4 * 1024 * 1024,
            event_bytes: 1024 * 1024,
            poll_timeout: Duration::from_secs(20),
            response_attempts: 3,
        }
    }
}
impl Limits {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.concurrency == 0
            || self.concurrency > 256
            || self.ingress_commands == 0
            || self.ingress_commands > 4096
            || self.lifetime_commands < self.ingress_commands
            || self.lifetime_commands > 1_000_000
            || self.body_bytes == 0
            || self.body_bytes > 64 * 1024 * 1024
            || self.event_bytes == 0
            || self.event_bytes > self.body_bytes
            || self.poll_timeout.is_zero()
            || self.poll_timeout > Duration::from_secs(120)
            || self.response_attempts == 0
            || self.response_attempts > 8
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
}
