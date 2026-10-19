//! Closed, explicit-file project formatting plans. Source and candidate bytes
//! remain private to the Runner; plans carry bounded content witnesses only.

use crate::runner_protocol::ShellProcessArgv;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROJECT_FORMAT_MAX_FILES: usize = 8;
pub const PROJECT_FORMAT_FILE_MAX_BYTES: usize = 64 * 1024;
pub const PROJECT_FORMAT_TOTAL_MAX_BYTES: usize = 256 * 1024;
pub const PROJECT_FORMAT_MANIFEST_MAX_BYTES: usize = 256 * 1024;
pub const PROJECT_FORMAT_PLAN_MAX_BYTES: usize = 32 * 1024;
pub const PROJECT_FORMAT_TIMEOUT_MAX_SECS: u64 = 120;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProjectFormatAdapter {
    #[default]
    Auto,
    Rust,
    Python,
    Go,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFormatRequest {
    pub project_id: String,
    pub cwd: Option<String>,
    #[serde(default)]
    pub adapter: ProjectFormatAdapter,
    /// Ordered paths relative to the registered Project, not the recipe cwd.
    pub files: Vec<String>,
}

pub fn valid_format_relative_path(path: &str, directory: bool) -> bool {
    (directory && path == ".")
        || (!path.is_empty()
            && path.len() <= 1024
            && !path.contains(['\\', ':'])
            && !path.chars().any(char::is_control)
            && path.split('/').all(|part| !matches!(part, "" | "." | "..")))
}

impl ProjectFormatRequest {
    /// Go extends the original wire vocabulary, including when inferred by auto.
    /// Callers validate the request before using this for capability admission.
    pub fn requires_go(&self) -> bool {
        self.adapter == ProjectFormatAdapter::Go
            || self.files.iter().any(|file| file.ends_with(".go"))
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.project_id.is_empty()
            || self.project_id.len() > 200
            || self.project_id.chars().any(char::is_control)
            || !valid_format_relative_path(self.cwd.as_deref().unwrap_or("."), true)
            || !(1..=PROJECT_FORMAT_MAX_FILES).contains(&self.files.len())
        {
            return Err("invalid project format request");
        }
        let mut backend = None;
        for (index, file) in self.files.iter().enumerate() {
            if !valid_format_relative_path(file, false) || self.files[..index].contains(file) {
                return Err("invalid project format files");
            }
            let selected = if file.ends_with(".rs") {
                ProjectFormatAdapter::Rust
            } else if file.ends_with(".py") || file.ends_with(".pyi") {
                ProjectFormatAdapter::Python
            } else if file.ends_with(".go") {
                ProjectFormatAdapter::Go
            } else {
                return Err("unsupported project format extension");
            };
            if backend.is_some_and(|backend| backend != selected)
                || (self.adapter != ProjectFormatAdapter::Auto && self.adapter != selected)
            {
                return Err("project format scope mismatch");
            }
            backend = Some(selected);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectFormatProfile {
    Rust { edition: String },
    Python { target_version: String },
    Go {},
}

impl ProjectFormatProfile {
    pub fn adapter(&self) -> ProjectFormatAdapter {
        match self {
            Self::Rust { .. } => ProjectFormatAdapter::Rust,
            Self::Python { .. } => ProjectFormatAdapter::Python,
            Self::Go {} => ProjectFormatAdapter::Go,
        }
    }

    pub fn is_valid(&self) -> bool {
        match self {
            Self::Go {} => true,
            Self::Rust { edition } => matches!(edition.as_str(), "2015" | "2018" | "2021" | "2024"),
            Self::Python { target_version } => matches!(
                target_version.as_str(),
                "py37" | "py38" | "py39" | "py310" | "py311" | "py312" | "py313" | "py314"
            ),
        }
    }

    /// The Runner must resolve/probe the trusted executable separately. This
    /// command shape never accepts executable, flags or scripts from the model.
    pub fn process(&self, recipe_relative_file: &str) -> Result<ShellProcessArgv, &'static str> {
        if !self.is_valid() || !valid_format_relative_path(recipe_relative_file, false) {
            return Err("invalid project format profile");
        }
        let (executable, args) = match self {
            Self::Rust { edition } if recipe_relative_file.ends_with(".rs") => (
                "rustfmt",
                vec![
                    "--emit=stdout".into(),
                    format!("--edition={edition}"),
                    "--config".into(),
                    "skip_children=true".into(),
                ],
            ),
            Self::Python { .. }
                if recipe_relative_file.ends_with(".py")
                    || recipe_relative_file.ends_with(".pyi") =>
            {
                (
                    "python",
                    vec![
                        "-I",
                        "-B",
                        "-m",
                        "ruff",
                        "format",
                        "--no-cache",
                        "--config",
                        "pyproject.toml",
                        "--stdin-filename",
                        recipe_relative_file,
                        "-",
                    ]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                )
            }
            Self::Go {} if recipe_relative_file.ends_with(".go") => ("gofmt", Vec::new()),
            _ => return Err("project format scope mismatch"),
        };
        Ok(ShellProcessArgv {
            executable: executable.into(),
            args,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFormatFileWitness {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
    pub identity_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFormatPlan {
    pub request: ProjectFormatRequest,
    pub recipe_root: String,
    pub root_digest: String,
    pub manifest_digest: String,
    pub profile: ProjectFormatProfile,
    pub files: Vec<ProjectFormatFileWitness>,
}

impl ProjectFormatPlan {
    pub fn is_valid(&self) -> bool {
        let digest = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        self.request.validate().is_ok()
            && valid_format_relative_path(&self.recipe_root, true)
            && digest(&self.root_digest)
            && digest(&self.manifest_digest)
            && self.profile.is_valid()
            && (self.request.adapter == ProjectFormatAdapter::Auto
                || self.request.adapter == self.profile.adapter())
            && self.files.len() == self.request.files.len()
            && self
                .files
                .iter()
                .zip(&self.request.files)
                .all(|(witness, path)| {
                    witness.path == *path
                        && witness.bytes <= PROJECT_FORMAT_FILE_MAX_BYTES
                        && digest(&witness.sha256)
                        && digest(&witness.identity_digest)
                        && self
                            .recipe_relative_file(path)
                            .is_some_and(|relative| self.profile.process(relative).is_ok())
                })
            && self
                .files
                .iter()
                .try_fold(0usize, |total, file| total.checked_add(file.bytes))
                .is_some_and(|total| total <= PROJECT_FORMAT_TOTAL_MAX_BYTES)
    }

    pub fn recipe_relative_file<'a>(&self, path: &'a str) -> Option<&'a str> {
        if self.recipe_root == "." {
            Some(path)
        } else {
            path.strip_prefix(&self.recipe_root)?.strip_prefix('/')
        }
    }

    pub fn digest(&self) -> Result<String, &'static str> {
        if !self.is_valid() {
            return Err("invalid project format plan");
        }
        let bytes = serde_json::to_vec(self).map_err(|_| "invalid project format plan")?;
        if bytes.len() > PROJECT_FORMAT_PLAN_MAX_BYTES {
            return Err("project format plan exceeds payload bound");
        }
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectFormatPlanningResult {
    Ready { plan: ProjectFormatPlan },
    Unavailable { code: String },
}

/// Executor-owned evidence, never derived from process exit status. Missing
/// historical evidence is unknown, including after a crash or lost delivery.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectFormatMutationReport {
    #[default]
    Unknown,
    Unchanged,
    Changed,
}

impl ProjectFormatMutationReport {
    pub fn state_changed(self) -> Option<bool> {
        match self {
            Self::Unknown => None,
            Self::Unchanged => Some(false),
            Self::Changed => Some(true),
        }
    }
}

/// Safe Job correlation only; never executable instructions or source bytes.
pub fn structured_metadata() -> crate::runner_protocol::ShellJobStructuredExecutionMetadata {
    crate::runner_protocol::ShellJobStructuredExecutionMetadata {
        execution_source: "project_format".into(),
        language: None,
        script_bytes: None,
        arg_count: 0,
        stdin_present: false,
        validation_identity: None,
        validation_tool: None,
        assertion_name: None,
    }
}

/// Missing historical receipts remain unknown. Known reports require complete
/// executor success and can never be attached to another kind of Job.
pub fn validate_mutation_report(
    kind: &str,
    status: &str,
    exit_code: Option<i32>,
    execution: Option<crate::runner_protocol::ShellCommandExecutionState>,
    report: Option<ProjectFormatMutationReport>,
    error: Option<&str>,
) -> Result<(), &'static str> {
    if let Some(report) = report {
        if kind != "project_format"
            || (report != ProjectFormatMutationReport::Unknown
                && (status != "completed"
                    || exit_code != Some(0)
                    || execution
                        != Some(crate::runner_protocol::ShellCommandExecutionState::Completed)
                    || error.is_some()))
        {
            return Err("format_mutation_invalid");
        }
    }
    Ok(())
}
