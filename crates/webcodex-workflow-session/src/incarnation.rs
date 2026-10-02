//! Exact Workflow Session incarnation identity.
//!
//! Prune-time retention tombstones and runtime session refs both call this
//! helper, so a retained tombstone proves the same incarnation the ref pinned.

use sha2::{Digest, Sha256};

pub fn workflow_session_incarnation_fingerprint(
    session_id: &str,
    created_at: i64,
    project: Option<&str>,
    owner_authority_fingerprint: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.workflow-session-incarnation.v1\0");
    hasher.update(session_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(created_at.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(project.unwrap_or_default().as_bytes());
    hasher.update(b"\0");
    hasher.update(owner_authority_fingerprint.as_bytes());
    format!("{:x}", hasher.finalize())
}
