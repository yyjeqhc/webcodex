//! Closed contracts for Runner-local managed SSH resource onboarding.
//!
//! The raw SSH target is intentionally request-only. Responses contain logical
//! names and lifecycle metadata, never host/user/port/authentication material.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SSH_RESOURCE_NAME_MAX_BYTES: usize = 80;
pub const SSH_RESOURCE_TARGET_MAX_BYTES: usize = 512;
pub const SSH_RESOURCE_DEFAULT_CWD_MAX_BYTES: usize = 4_096;
pub const MANAGED_SSH_RESOURCE_MAX_COUNT: usize = 64;
pub const MANAGED_SSH_REGISTRY_MAX_BYTES: usize = 64 * 1024;
pub const SSH_RESOURCE_REQUEST_MAX_BYTES: usize = 8 * 1024;
pub const SSH_RESOURCE_RESPONSE_MAX_BYTES: usize = 64 * 1024;
const SSH_RESOURCE_ERROR_MESSAGE_MAX_BYTES: usize = 512;

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
#[serde(deny_unknown_fields)]
pub struct SshResourceInventoryEntry {
    pub name: String,
    pub source: SshResourceSource,
    pub active: bool,
    pub pending_restart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
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
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
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

/// Validate one Runner response against the exact managed-SSH operation that
/// produced it. A stale or buggy Runner response must not change the logical
/// resource, response kind, lifecycle state, or output bounds after dispatch.
pub fn validate_response_for_request(
    request: &SshResourceRequest,
    response: &SshResourceResponse,
) -> Result<(), &'static str> {
    let encoded = serde_json::to_vec(response).map_err(|_| "ssh_resource_invalid")?;
    if encoded.len() > SSH_RESOURCE_RESPONSE_MAX_BYTES {
        return Err("ssh_resource_invalid");
    }

    if let SshResourceResponse::Error { code, message } = response {
        if !matches!(
            code.as_str(),
            "ssh_resource_not_found"
                | "ssh_resource_static_conflict"
                | "ssh_resource_static_read_only"
                | "ssh_resource_name_conflict"
                | "ssh_resource_registry_stale"
                | "ssh_resource_registry_unavailable"
                | "ssh_resource_outcome_unknown"
                | "ssh_resource_invalid"
        ) || message.is_empty()
            || message.len() > SSH_RESOURCE_ERROR_MESSAGE_MAX_BYTES
            || message.chars().any(char::is_control)
        {
            return Err("ssh_resource_invalid");
        }
        return Ok(());
    }

    match (request, response) {
        (SshResourceRequest::List, SshResourceResponse::List { resources, .. }) => {
            validate_inventory(resources)
        }
        (
            SshResourceRequest::Register {
                expected_revision,
                name,
                ..
            },
            SshResourceResponse::Register {
                revision,
                resource,
                persisted,
                active,
                restart_required,
            },
        ) => {
            validate_ssh_resource_name(resource)?;
            if resource != name
                || !persisted
                || *restart_required != !*active
                || (*revision != *expected_revision
                    && expected_revision.checked_add(1) != Some(*revision))
            {
                return Err("ssh_resource_invalid");
            }
            Ok(())
        }
        (
            SshResourceRequest::Remove {
                expected_revision,
                name,
            },
            SshResourceResponse::Remove {
                revision,
                resource,
                persisted,
                active,
                restart_required,
            },
        ) => {
            validate_ssh_resource_name(resource)?;
            if resource != name
                || !persisted
                || *restart_required != *active
                || expected_revision.checked_add(1) != Some(*revision)
            {
                return Err("ssh_resource_invalid");
            }
            Ok(())
        }
        _ => Err("ssh_resource_invalid"),
    }
}

fn validate_inventory(resources: &[SshResourceInventoryEntry]) -> Result<(), &'static str> {
    let mut names = HashSet::with_capacity(resources.len());
    let mut previous: Option<&str> = None;
    for resource in resources {
        validate_ssh_resource_name(&resource.name)?;
        if !names.insert(resource.name.as_str())
            || previous.is_some_and(|previous| previous >= resource.name.as_str())
        {
            return Err("ssh_resource_invalid");
        }
        previous = Some(resource.name.as_str());
        match resource.source {
            SshResourceSource::Static if resource.active && !resource.pending_restart => {}
            SshResourceSource::Managed if resource.active || resource.pending_restart => {}
            _ => return Err("ssh_resource_invalid"),
        }
    }
    Ok(())
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

    #[test]
    fn response_admission_is_request_correlated_bounded_and_semantic() {
        let register = SshResourceRequest::Register {
            expected_revision: 7,
            name: "w10".to_string(),
            target: "17724@w10".to_string(),
            default_cwd: None,
        };
        validate_response_for_request(
            &register,
            &SshResourceResponse::Register {
                revision: 8,
                resource: "w10".to_string(),
                persisted: true,
                active: false,
                restart_required: true,
            },
        )
        .unwrap();
        assert!(validate_response_for_request(
            &register,
            &SshResourceResponse::Register {
                revision: 8,
                resource: "other".to_string(),
                persisted: true,
                active: false,
                restart_required: true,
            },
        )
        .is_err());
        assert!(validate_response_for_request(
            &register,
            &SshResourceResponse::Register {
                revision: 8,
                resource: "w10".to_string(),
                persisted: true,
                active: true,
                restart_required: true,
            },
        )
        .is_err());

        let list = SshResourceRequest::List;
        validate_response_for_request(
            &list,
            &SshResourceResponse::List {
                revision: 9,
                resources: vec![
                    SshResourceInventoryEntry {
                        name: "spe".to_string(),
                        source: SshResourceSource::Static,
                        active: true,
                        pending_restart: false,
                    },
                    SshResourceInventoryEntry {
                        name: "w10".to_string(),
                        source: SshResourceSource::Managed,
                        active: false,
                        pending_restart: true,
                    },
                ],
            },
        )
        .unwrap();
        assert!(validate_response_for_request(
            &list,
            &SshResourceResponse::List {
                revision: 9,
                resources: vec![SshResourceInventoryEntry {
                    name: "w10".to_string(),
                    source: SshResourceSource::Managed,
                    active: false,
                    pending_restart: false,
                }],
            },
        )
        .is_err());
    }
}
