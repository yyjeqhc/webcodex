//! Closed contracts for Runner-local managed SSH resource onboarding.
//!
//! The raw SSH target is intentionally request-only. Responses contain logical
//! names and lifecycle metadata, never host/user/port/authentication material.

use serde::{Deserialize, Serialize};

pub const SSH_RESOURCE_NAME_MAX_BYTES: usize = 80;
pub const SSH_RESOURCE_TARGET_MAX_BYTES: usize = 512;
pub const SSH_RESOURCE_DEFAULT_CWD_MAX_BYTES: usize = 4_096;
pub const MANAGED_SSH_RESOURCE_MAX_COUNT: usize = 64;
pub const MANAGED_SSH_REGISTRY_MAX_BYTES: usize = 64 * 1024;
pub const SSH_RESOURCE_REQUEST_MAX_BYTES: usize = 8 * 1024;

pub fn validate_ssh_resource_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() || name.len() > SSH_RESOURCE_NAME_MAX_BYTES {
        return Err("ssh_resource_invalid");
    }
    if name == "."
        || name.contains("..")
        || name.starts_with('-')
        || name.contains('/')
        || name.contains('\\')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err("ssh_resource_invalid");
    }
    Ok(())
}

pub fn normalize_ssh_resource_target(target: &str) -> Result<String, &'static str> {
    let target = target.trim();
    if target.is_empty()
        || target.len() > SSH_RESOURCE_TARGET_MAX_BYTES
        || target.starts_with('-')
        || target.chars().any(char::is_control)
    {
        return Err("ssh_resource_invalid");
    }
    Ok(target.to_string())
}

pub fn normalize_ssh_resource_default_cwd(
    default_cwd: Option<&str>,
) -> Result<Option<String>, &'static str> {
    let Some(default_cwd) = default_cwd else {
        return Ok(None);
    };
    let default_cwd = default_cwd.trim();
    if default_cwd.is_empty()
        || default_cwd.len() > SSH_RESOURCE_DEFAULT_CWD_MAX_BYTES
        || default_cwd.chars().any(char::is_control)
    {
        return Err("ssh_resource_invalid");
    }
    Ok(Some(default_cwd.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshResourceSource {
    Static,
    Managed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshResourceInventoryEntry {
    pub name: String,
    pub source: SshResourceSource,
    pub active: bool,
    pub pending_restart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SshResourceRequest {
    List,
    Register {
        expected_revision: u64,
        name: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_cwd: Option<String>,
    },
    Remove {
        expected_revision: u64,
        name: String,
    },
}

impl SshResourceRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::List => Ok(()),
            Self::Register {
                name,
                target,
                default_cwd,
                ..
            } => {
                validate_ssh_resource_name(name)?;
                normalize_ssh_resource_target(target)?;
                normalize_ssh_resource_default_cwd(default_cwd.as_deref())?;
                Ok(())
            }
            Self::Remove { name, .. } => validate_ssh_resource_name(name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SshResourceResponse {
    List {
        revision: u64,
        resources: Vec<SshResourceInventoryEntry>,
    },
    Register {
        revision: u64,
        resource: String,
        persisted: bool,
        active: bool,
        restart_required: bool,
    },
    Remove {
        revision: u64,
        resource: String,
        persisted: bool,
        active: bool,
        restart_required: bool,
    },
    Error {
        code: String,
        message: String,
    },
}

impl SshResourceResponse {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_is_single_destination_not_ssh_argv() {
        assert_eq!(
            normalize_ssh_resource_target("17724@w10").unwrap(),
            "17724@w10"
        );
        for invalid in ["", "   ", "-oProxyJump=x", "-p", "host\nnext"] {
            assert!(
                normalize_ssh_resource_target(invalid).is_err(),
                "{invalid:?}"
            );
        }
    }

    #[test]
    fn resource_name_is_safe_atom_and_not_option_syntax() {
        for valid in ["spe", "w10", "team.prod_1"] {
            validate_ssh_resource_name(valid).unwrap();
        }
        for invalid in ["-o", "a/b", "a\\b", ".", "..", "white space"] {
            assert!(validate_ssh_resource_name(invalid).is_err(), "{invalid:?}");
        }
    }
}
