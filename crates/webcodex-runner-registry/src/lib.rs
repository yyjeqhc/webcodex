//! Server-side authoritative Runner registration and lifecycle contracts.
//!
//! The registry implementation is extracted into this crate in the following
//! refactor step. Neutral contracts land first so root authentication and
//! telemetry policy can depend on the registry domain, never the reverse.

mod access;
mod job_status;
mod telemetry;

pub use access::{DetachedInitiatorIdentity, RunnerAccess, RunnerAccessGroup};
pub use job_status::job_status_is_active;
pub use telemetry::{NoopRunnerRegistryTelemetry, RunnerRegistryTelemetry};
