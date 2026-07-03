use serde_json::Value;

use crate::build_info;

#[derive(Debug, Clone)]
pub(crate) struct DoctorCheck {
    pub(crate) name: String,
    pub(crate) status: &'static str,
    pub(crate) detail: String,
}

impl DoctorCheck {
    pub(crate) fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "PASS",
            detail: detail.into(),
        }
    }

    pub(crate) fn warn(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "WARN",
            detail: detail.into(),
        }
    }

    pub(crate) fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "FAIL",
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeBuildMetadata {
    pub(crate) version: Option<String>,
    pub(crate) git_commit: Option<String>,
    pub(crate) git_dirty: Option<bool>,
    pub(crate) built_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RevisionComparison {
    Match,
    Mismatch { local: String, remote: String },
    Unknown { reason: String },
}

fn known_build_commit(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn compare_build_commits(
    local: Option<&str>,
    remote: Option<&str>,
) -> RevisionComparison {
    let local = match known_build_commit(local) {
        Some(commit) => commit,
        None => {
            return RevisionComparison::Unknown {
                reason: "local CLI did not report build.git_commit".to_string(),
            }
        }
    };
    let remote = match known_build_commit(remote) {
        Some(commit) => commit,
        None => {
            return RevisionComparison::Unknown {
                reason: "server runtime did not report build.git_commit".to_string(),
            }
        }
    };
    if local == remote || local.starts_with(&remote) || remote.starts_with(&local) {
        RevisionComparison::Match
    } else {
        RevisionComparison::Mismatch { local, remote }
    }
}

pub(crate) fn runtime_build_metadata(output: Option<&Value>) -> RuntimeBuildMetadata {
    let version = output
        .and_then(|v| v.get("version"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let build = output.and_then(|v| v.get("build"));
    RuntimeBuildMetadata {
        version,
        git_commit: build
            .and_then(|v| v.get("git_commit"))
            .and_then(Value::as_str)
            .map(str::to_string),
        git_dirty: build
            .and_then(|v| v.get("git_dirty"))
            .and_then(Value::as_bool),
        built_at: build
            .and_then(|v| v.get("built_at"))
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

pub(crate) fn local_cli_build_metadata() -> RuntimeBuildMetadata {
    let info = build_info::current();
    RuntimeBuildMetadata {
        version: Some(info.version.to_string()),
        git_commit: info.git_commit.map(str::to_string),
        git_dirty: info.git_dirty,
        built_at: info.built_at.map(str::to_string),
    }
}

pub(crate) fn render_build_metadata_block(label: &str, build: &RuntimeBuildMetadata) -> String {
    let mut out = String::new();
    out.push_str(label);
    out.push_str(":\n");
    out.push_str(&format!(
        "  version:    {}\n",
        build.version.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "  commit:     {}\n",
        build.git_commit.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "  dirty:      {}\n",
        build
            .git_dirty
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ));
    out.push_str(&format!(
        "  built_at:   {}\n",
        build.built_at.as_deref().unwrap_or("unknown")
    ));
    out
}

pub(crate) fn server_status_revision_check(comparison: &RevisionComparison) -> String {
    match comparison {
        RevisionComparison::Match => {
            "ok: local CLI and server runtime are built from the same commit".to_string()
        }
        RevisionComparison::Mismatch { local, remote } => format!(
            "warning: local CLI commit {} differs from server runtime commit {}; deploy/update one side before debugging old behavior",
            local, remote
        ),
        RevisionComparison::Unknown { reason } => format!(
            "unknown: {}; server may be older than build metadata support",
            reason
        ),
    }
}

pub(crate) fn doctor_revision_check(
    local: &RuntimeBuildMetadata,
    remote: Option<&RuntimeBuildMetadata>,
) -> DoctorCheck {
    let Some(remote) = remote else {
        return DoctorCheck::warn(
            "cli/server revision",
            "not checked; pass --server-url with --user-token-file or --token-file to read runtime_status",
        );
    };
    match compare_build_commits(local.git_commit.as_deref(), remote.git_commit.as_deref()) {
        RevisionComparison::Match => DoctorCheck::pass(
            "cli/server revision",
            format!(
                "local CLI and server runtime commit match ({})",
                local.git_commit.as_deref().unwrap_or("unknown")
            ),
        ),
        RevisionComparison::Mismatch { local, remote } => DoctorCheck::warn(
            "cli/server revision",
            format!(
                "local CLI commit {} differs from server runtime commit {}; deploy/update one side before debugging old behavior",
                local, remote
            ),
        ),
        RevisionComparison::Unknown { reason } => DoctorCheck::warn(
            "cli/server revision",
            format!(
                "{}; server may be older than build metadata support",
                reason
            ),
        ),
    }
}
