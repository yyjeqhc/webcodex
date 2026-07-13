//! Read-only audit query API.
//!
//! Three thin POST handlers mounted under `/api/audit/*` behind
//! `AuthMiddleware`. They wrap the existing action-session query functions in
//! `db.rs` and the decode/stats helpers in `action_audit_sessions.rs`. They perform
//! no write operations and are intentionally **not** part of the GPT Actions
//! OpenAPI schema (`/openapi.json`).

#[path = "audit_http/responses.rs"]
mod responses;
#[path = "audit_http/routes.rs"]
mod routes;
#[cfg(test)]
#[path = "audit_http/tests.rs"]
mod tests;

pub(crate) use routes::{audit_session, audit_sessions, audit_stats};
