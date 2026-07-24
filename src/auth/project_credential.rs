//! Exact verifier for one project-bound credential.

use sha2::{Digest, Sha256};
use std::path::Path;

use super::shared_key::project_credential_context;
use super::AuthContext;

#[derive(Clone)]
pub(crate) struct ProjectCredentialVerifier {
    grant_id: String,
    credential_hash: [u8; 32],
}

impl ProjectCredentialVerifier {
    pub(crate) fn from_file(grant_id: String, path: &Path) -> Result<Self, String> {
        Self::new(grant_id, &read_protected_secret(path)?)
    }

    pub(crate) fn new(grant_id: String, credential: &str) -> Result<Self, String> {
        validate_grant_id(&grant_id)?;
        validate_credential(credential)?;
        Ok(Self {
            grant_id,
            credential_hash: Sha256::digest(credential.as_bytes()).into(),
        })
    }

    pub(crate) fn authenticate(&self, credential: &str) -> Option<AuthContext> {
        let candidate: [u8; 32] = Sha256::digest(credential.trim().as_bytes()).into();
        crate::config::constant_time_eq(&self.credential_hash, &candidate)
            .then(|| project_credential_context(&self.grant_id))
    }

    pub(crate) fn grant_id(&self) -> &str {
        &self.grant_id
    }
}

pub(crate) fn read_protected_secret(path: &Path) -> Result<String, String> {
    let metadata =
        std::fs::metadata(path).map_err(|_| "private authentication material is unreadable")?;
    if !metadata.is_file() {
        return Err("private authentication material is not a regular file".to_string());
    }
    #[cfg(unix)]
    if {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o077 != 0
    } {
        return Err("private authentication material is not protected".to_string());
    }
    std::fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .map_err(|_| "private authentication material is unreadable".to_string())
}

fn validate_grant_id(value: &str) -> Result<(), String> {
    let suffix = value.strip_prefix("wc_pgrant_").unwrap_or_default();
    if suffix.len() < 16
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("configured project grant identity is invalid".to_string());
    }
    Ok(())
}

pub(crate) fn validate_credential(value: &str) -> Result<(), String> {
    let suffix = value.strip_prefix("webcodex_").unwrap_or_default();
    if suffix.len() != 64
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("configured project credential is invalid".to_string());
    }
    Ok(())
}
