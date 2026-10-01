//! Read-only ActionAudit history and statistics under existing authenticated
//! operational access. Audit history never grants tool execution authority.

#[path = "audit_http/responses.rs"]
mod responses;
#[path = "audit_http/routes.rs"]
mod routes;
#[cfg(test)]
#[path = "audit_http/tests.rs"]
mod tests;

pub(crate) use routes::{audit_session, audit_sessions, audit_stats};
