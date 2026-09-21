//! Exact verifier for one project-bound credential.

use sha2::{Digest, Sha256};
use std::path::Path;

use super::shared_key::project_credential_context;
use super::{AuthContext, AuthKind, AGENT_SCOPES};

#[derive(Clone)]
pub(crate) struct ProjectCredentialVerifier {
    grant_id: String,
    credential_hash: [u8; 32],
}

/// Exact verifier for the private Agent Token generated for one project entry.
/// Unlike the project credential, this context is an Agent Token bound to
/// one client id and is therefore valid only on Runner transport routes.
#[derive(Clone)]
pub(crate) struct ProjectAgentTokenVerifier {
    grant_id: String,
    allowed_client_id: String,
    owner: String,
    token_hash: [u8; 32],
}

/// Process-local authentication material for a project-scoped Server launched by
/// `webcodex run` / `webcodex share`. This state authenticates credentials only;
/// it does not define a coding surface, Project model, Task model, or execution
/// lifecycle. Canonical ToolRuntime and Runner authorization remain authoritative.
#[derive(Clone, Default)]
pub(crate) struct ProjectAuthState {
    credential: Option<ProjectCredentialVerifier>,
    agent_token: Option<ProjectAgentTokenVerifier>,
}

pub(crate) const PROJECT_GRANT_ID_ENV: &str = "WEBPI_PROJECT_GRANT_ID";
pub(crate) const PROJECT_CREDENTIAL_FILE_ENV: &str = "WEBPI_PROJECT_CREDENTIAL_FILE";
pub(crate) const PROJECT_AGENT_TOKEN_FILE_ENV: &str = "WEBPI_PROJECT_AGENT_TOKEN_FILE";
pub(crate) const PROJECT_RUNNER_CLIENT_ID_ENV: &str = "WEBPI_PROJECT_RUNNER_CLIENT_ID";

impl ProjectAuthState {
    pub(crate) fn from_env() -> Result<Self, String> {
        let grant_id = nonempty_env(PROJECT_GRANT_ID_ENV);
        let credential_file = nonempty_env(PROJECT_CREDENTIAL_FILE_ENV);
        let agent_token_file = nonempty_env(PROJECT_AGENT_TOKEN_FILE_ENV);
        let runner_client_id = nonempty_env(PROJECT_RUNNER_CLIENT_ID_ENV);
        let present = [
            grant_id.is_some(),
            credential_file.is_some(),
            agent_token_file.is_some(),
            runner_client_id.is_some(),
        ];
        if !present.iter().any(|present| *present) {
            return Ok(Self::default());
        }
        if !present.iter().all(|present| *present) {
            return Err(format!(
                "project-scoped authentication requires {PROJECT_GRANT_ID_ENV}, {PROJECT_CREDENTIAL_FILE_ENV}, {PROJECT_AGENT_TOKEN_FILE_ENV}, and {PROJECT_RUNNER_CLIENT_ID_ENV}"
            ));
        }
        let grant_id = grant_id.expect("checked above");
        validate_grant_id(&grant_id)?;
        let runner_client_id = runner_client_id.expect("checked above");
        super::validate_allowed_client_id(&runner_client_id)?;
        let credential = ProjectCredentialVerifier::from_file(
            grant_id.clone(),
            Path::new(&credential_file.expect("checked above")),
        )?;
        let agent_token = ProjectAgentTokenVerifier::from_file(
            grant_id,
            runner_client_id,
            "local-owner".to_string(),
            Path::new(&agent_token_file.expect("checked above")),
        )?;
        Ok(Self {
            credential: Some(credential),
            agent_token: Some(agent_token),
        })
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.credential.is_some()
    }

    pub(crate) fn authenticate_project_credential(&self, token: &str) -> Option<AuthContext> {
        self.credential
            .as_ref()
            .and_then(|verifier| verifier.authenticate(token))
    }

    pub(crate) fn authenticate_project_agent_token(&self, token: &str) -> Option<AuthContext> {
        self.agent_token
            .as_ref()
            .and_then(|verifier| verifier.authenticate(token))
    }
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
}

impl ProjectAgentTokenVerifier {
    pub(crate) fn from_file(
        grant_id: String,
        allowed_client_id: String,
        owner: String,
        path: &Path,
    ) -> Result<Self, String> {
        Self::new(
            grant_id,
            allowed_client_id,
            owner,
            &read_protected_secret(path)?,
        )
    }

    pub(crate) fn new(
        grant_id: String,
        allowed_client_id: String,
        owner: String,
        token: &str,
    ) -> Result<Self, String> {
        validate_grant_id(&grant_id)?;
        super::validate_allowed_client_id(&allowed_client_id)?;
        let owner = super::validate_username(&owner)?;
        validate_agent_token(token)?;
        Ok(Self {
            grant_id,
            allowed_client_id,
            owner,
            token_hash: Sha256::digest(token.trim().as_bytes()).into(),
        })
    }

    pub(crate) fn authenticate(&self, token: &str) -> Option<AuthContext> {
        let candidate: [u8; 32] = Sha256::digest(token.trim().as_bytes()).into();
        crate::config::constant_time_eq(&self.token_hash, &candidate).then(|| AuthContext {
            role: Some("project-agent".to_string()),
            username: Some(self.owner.clone()),
            scopes: AGENT_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect(),
            token_kind: Some("agent".to_string()),
            allowed_client_id: Some(self.allowed_client_id.clone()),
            project_grant_id: Some(self.grant_id.clone()),
            ..AuthContext::new(AuthKind::AgentToken)
        })
    }
}

pub(crate) fn read_protected_secret(path: &Path) -> Result<String, String> {
    let metadata =
        std::fs::metadata(path).map_err(|_| "private authentication material is unreadable")?;
    if !metadata.is_file() {
        return Err("private authentication material is not a regular file".to_string());
    }
    #[cfg(unix)]
    let has_unsafe_permissions = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o077 != 0
    };
    #[cfg(unix)]
    if has_unsafe_permissions {
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

pub(crate) fn validate_agent_token(value: &str) -> Result<(), String> {
    let value = value.trim();
    let suffix = value.strip_prefix("wc_agent_").unwrap_or_default();
    if suffix.len() != 64
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("configured project Agent Token is invalid".to_string());
    }
    Ok(())
}
