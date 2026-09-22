use super::project_resolution::ResolvedProject;
use super::runtime_metrics::{
    observe_skill_source, RuntimeMetrics, SkillSourceMetricObservation, SkillSourceMetricOperation,
    SkillSourceMetricOutcomeClass, SkillSourceMetricSource,
};
use super::startup_brief::{
    bounded_extension_description, StartupSkillEntry, StartupSkillsCatalog,
};
use super::{ExecutionPurpose, SuggestedToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::json_measurement::serialized_json_len;
use crate::runner_http::{EnqueueRunnerSkillError, RunnerFeature};
use crate::runner_protocol::{ShellFileOpRequest, ShellRunResponse};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Component;
use std::time::{Duration, Instant};
use unicase::UniCase;
use webcodex_core::runner_skill::{
    RunnerSkillDescriptor, RunnerSkillExecutionRequest, RunnerSkillListResponse,
    RunnerSkillReadResponse, RunnerSkillRequest, RunnerSkillResolveResponse, RunnerSkillSource,
};
use webcodex_core::skill_metadata::parse_skill_metadata;
pub(crate) use webcodex_core::skill_metadata::{
    MAX_SKILL_DEFINITION_BYTES, MAX_SKILL_DESCRIPTION_CHARS, MAX_SKILL_NAME_CHARS,
};
use webcodex_core::skill_store::{
    valid_lower_sha256, valid_package_revision, valid_skill_key, valid_state_revision,
    SkillStoreActivateResponse, SkillStoreInstallResponse, SkillStoreRemoveResponse,
    SkillStoreVersionsResponse, MAX_OPERATOR_REVISIONS_PER_SKILL, MAX_SKILL_STORE_FILE_COUNT,
    MAX_SKILL_STORE_TOTAL_BYTES, MAX_SKILL_STORE_VERSIONS_LIMIT, SKILL_STORE_RESPONSE_FORMAT,
};

pub(crate) const SKILL_ROOT: &str = ".agents/skills";
pub(crate) const SKILL_DEFINITION_FILE: &str = "SKILL.md";
pub(crate) const MAX_SKILL_DISCOVERY_PACKAGES: usize = 256;
pub(crate) const MAX_SKILL_INVALID_DIAGNOSTICS: usize = 8;
pub(crate) const MAX_SKILL_RESOURCE_FILE_BYTES: usize = 512 * 1024;
pub(crate) const MAX_SKILL_CATALOG_RESULT_BYTES: usize = 64 * 1024;
pub(crate) const MAX_SKILL_SIDECAR_CATALOG_BYTES: usize = 8 * 1024;
pub(crate) use webcodex_core::runtime_contract::{
    MAX_SKILL_LIST_LIMIT, MAX_SKILL_QUERY_CHARS, MAX_SKILL_READ_LINES,
    MAX_SKILL_RESOURCE_PATH_CHARS,
};
pub(crate) const MAX_SKILL_READ_TEXT_BYTES: usize = 48 * 1024;
pub(crate) const MAX_SKILL_READ_RESULT_BYTES: usize = 64 * 1024;
const MAX_SKILL_DISCOVERY_READ_LINES: usize = 128;
const DEFAULT_SKILL_LIST_LIMIT: usize = 20;
const DEFAULT_SKILL_READ_LINES: usize = 200;
const SKILL_PACKAGE_LIST_FORMAT: &str = "webcodex.skill_package_list.v1";
const SKILL_FILE_READ_FORMAT: &str = "webcodex.skill_file_read.v1";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SkillDescriptor {
    pub(crate) skill_id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) definition_revision: String,
    pub(crate) package_revision: Option<String>,
    pub(crate) source_scope: &'static str,
    pub(crate) trust: &'static str,
    pub(crate) name_conflict: bool,
}

#[derive(Debug, Clone)]
struct CatalogSkill {
    descriptor: SkillDescriptor,
    order_key: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SkillCatalog {
    pub(crate) catalog_revision: String,
    skills: Vec<CatalogSkill>,
    pub(crate) invalid_count: usize,
    pub(crate) diagnostics: Vec<Value>,
    pub(crate) discovery_truncated: bool,
    sources: Vec<SkillCatalogSourceSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct SkillCatalogSourceSummary {
    kind: &'static str,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    root_hint: Option<&'static str>,
    skill_count: usize,
    invalid_count: usize,
    discovery_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<&'static str>,
}

fn project_skill_source_summary(
    skill_count: usize,
    invalid_count: usize,
    discovery_truncated: bool,
) -> SkillCatalogSourceSummary {
    SkillCatalogSourceSummary {
        kind: "project",
        status: "available",
        root_hint: Some(SKILL_ROOT),
        skill_count,
        invalid_count,
        discovery_truncated,
        reason_code: None,
    }
}

fn runner_skill_source_summary(
    kind: &'static str,
    skill_count: usize,
    invalid_count: usize,
    discovery_truncated: bool,
    available: bool,
) -> SkillCatalogSourceSummary {
    SkillCatalogSourceSummary {
        kind,
        status: if available {
            "available"
        } else {
            "unavailable"
        },
        root_hint: None,
        skill_count,
        invalid_count,
        discovery_truncated,
        reason_code: (!available).then_some("runner_skill_sources_unavailable"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillIoError {
    NotFound,
    SensitivePath,
    InvalidUtf8,
    TooLarge,
    InvalidPath,
    Unavailable,
}

impl SkillIoError {
    fn invalid_reason(self) -> Option<&'static str> {
        match self {
            Self::NotFound => Some("missing_skill_definition"),
            Self::SensitivePath => Some("sensitive_skill_definition"),
            Self::InvalidUtf8 => Some("invalid_utf8_skill_definition"),
            Self::TooLarge => Some("skill_definition_too_large"),
            Self::InvalidPath => Some("invalid_skill_package"),
            Self::Unavailable => None,
        }
    }

    fn resource_error_kind(self) -> &'static str {
        match self {
            Self::NotFound => "skill_resource_not_found",
            Self::SensitivePath => "skill_sensitive_path",
            Self::InvalidUtf8 => "skill_resource_unsupported_encoding",
            Self::TooLarge => "skill_resource_too_large",
            Self::InvalidPath => "skill_resource_path_invalid",
            Self::Unavailable => "skill_resource_unavailable",
        }
    }
}

#[derive(Debug, Deserialize)]
struct AgentSkillPackageList {
    format: String,
    entries: Vec<AgentSkillPackageEntry>,
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct AgentSkillPackageEntry {
    name: String,
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentSkillFileRead {
    format: String,
    content: String,
    sha256: String,
    file_bytes: usize,
    total_lines: usize,
    start_line: usize,
    limit: usize,
    returned_lines: usize,
    end_line: Option<usize>,
    has_more: bool,
    next_start_line: Option<usize>,
}

#[derive(Debug, Clone)]
enum ExactSkillCandidate {
    Project {
        package_name: String,
        name: String,
        definition_revision: String,
    },
    Runner(RunnerSkillDescriptor),
}

#[derive(Debug, Clone)]
enum ExactSkillProbeOutcome {
    NotApplicable,
    Absent,
    Candidate(ExactSkillCandidate),
    Ambiguous,
    SourceUnavailable,
}

fn merge_exact_skill_probe(
    candidate: &mut Option<ExactSkillCandidate>,
    outcome: ExactSkillProbeOutcome,
) -> Result<(), &'static str> {
    match outcome {
        ExactSkillProbeOutcome::NotApplicable | ExactSkillProbeOutcome::Absent => Ok(()),
        ExactSkillProbeOutcome::Candidate(next) if candidate.is_none() => {
            *candidate = Some(next);
            Ok(())
        }
        ExactSkillProbeOutcome::Candidate(_)
        | ExactSkillProbeOutcome::Ambiguous
        | ExactSkillProbeOutcome::SourceUnavailable => Err("skills_catalog_unavailable"),
    }
}

fn observe_skill_source_request(
    metrics: &dyn RuntimeMetrics,
    source: SkillSourceMetricSource,
    operation: SkillSourceMetricOperation,
    started: Instant,
    response: Option<&ShellRunResponse>,
    outcome_class: SkillSourceMetricOutcomeClass,
    item_count: Option<usize>,
) {
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let response_bytes = response.and_then(|response| {
        response
            .stdout
            .as_ref()
            .map(|stdout| u64::try_from(stdout.len()).unwrap_or(u64::MAX))
    });
    observe_skill_source(
        metrics,
        SkillSourceMetricObservation {
            source,
            operation,
            outcome_class,
            elapsed_ms,
            runner_duration_ms: response.and_then(|response| response.duration_ms),
            response_bytes,
            item_count: item_count.map(|count| u64::try_from(count).unwrap_or(u64::MAX)),
        },
    );
}

impl ToolRuntime {
    async fn runner_skill_request(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
        operation: RunnerSkillRequest,
        optional_if_unsupported: bool,
        metric_operation: Option<SkillSourceMetricOperation>,
    ) -> Result<Option<ShellRunResponse>, String> {
        let client_id = project.config.client_id.clone();
        let access = crate::runner_http::runner_access_from_auth(auth);
        let management = operation.requires_management_capability();
        let mutation = operation.is_mutation();
        let unavailable_kind = if management {
            "skill_store_runner_unavailable"
        } else {
            "skill_catalog_unavailable"
        };
        let capability_kind = if management {
            "skill_store_capability_unavailable"
        } else {
            "skill_catalog_unavailable"
        };
        let view = self
            .runner_registry
            .get_runner_semantic_view_checked_for_auth(&client_id, access.as_ref())
            .await
            .map_err(|_| unavailable_kind.to_string())?;
        let required = if management {
            RunnerFeature::SkillManagement
        } else {
            RunnerFeature::SkillRuntime
        };
        if !view.supports(required) {
            return if optional_if_unsupported {
                Ok(None)
            } else {
                Err(capability_kind.to_string())
            };
        }
        if !view.view.connected {
            return Err(unavailable_kind.to_string());
        }
        let requested_by = if management {
            "skill_management"
        } else {
            "skill_runtime"
        };
        let (request_id, rx) = self
            .runner_registry
            .enqueue_runner_skill_typed(
                &client_id,
                &view.view.runner_instance_id,
                operation,
                access.as_ref(),
                requested_by.to_string(),
            )
            .await
            .map_err(|error| runner_skill_enqueue_error_kind(&error, management).to_string())?;
        let request_started = Instant::now();
        let wait_secs = if management { 120 } else { 30 };
        match tokio::time::timeout(Duration::from_secs(wait_secs), rx).await {
            Ok(Ok(response)) => Ok(Some(response)),
            Ok(Err(_)) => {
                if let Some(operation) = metric_operation {
                    observe_skill_source_request(
                        self.metrics.as_ref(),
                        SkillSourceMetricSource::RunnerLocal,
                        operation,
                        request_started,
                        None,
                        SkillSourceMetricOutcomeClass::Unavailable,
                        None,
                    );
                }
                Err(if mutation {
                    "skill_store_outcome_unknown".to_string()
                } else {
                    unavailable_kind.to_string()
                })
            }
            Err(_) => {
                let dispatched = self
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                if let Some(operation) = metric_operation {
                    observe_skill_source_request(
                        self.metrics.as_ref(),
                        SkillSourceMetricSource::RunnerLocal,
                        operation,
                        request_started,
                        None,
                        SkillSourceMetricOutcomeClass::Unavailable,
                        None,
                    );
                }
                if mutation && dispatched != Some(false) {
                    Err("skill_store_outcome_unknown".to_string())
                } else {
                    Err(unavailable_kind.to_string())
                }
            }
        }
    }

    async fn observe_runner_skill_catalog(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
    ) -> Result<Option<RunnerSkillListResponse>, String> {
        let observation_started = Instant::now();
        let Some(response) = self
            .runner_skill_request(
                project,
                auth,
                RunnerSkillRequest::List,
                true,
                Some(SkillSourceMetricOperation::CatalogList),
            )
            .await?
        else {
            return Ok(None);
        };
        if response.exit_code != Some(0) || response.error.is_some() {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::RunnerLocal,
                SkillSourceMetricOperation::CatalogList,
                observation_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::RunnerError,
                None,
            );
            return Err("skills_catalog_unavailable".to_string());
        }
        let parsed: RunnerSkillListResponse =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(parsed) => parsed,
                Err(_) => {
                    observe_skill_source_request(
                        self.metrics.as_ref(),
                        SkillSourceMetricSource::RunnerLocal,
                        SkillSourceMetricOperation::CatalogList,
                        observation_started,
                        Some(&response),
                        SkillSourceMetricOutcomeClass::InvalidResponse,
                        None,
                    );
                    return Err("skills_catalog_unavailable".to_string());
                }
            };
        if parsed.validate().is_err() {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::RunnerLocal,
                SkillSourceMetricOperation::CatalogList,
                observation_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::InvalidResponse,
                None,
            );
            return Err("skills_catalog_unavailable".to_string());
        }
        observe_skill_source_request(
            self.metrics.as_ref(),
            SkillSourceMetricSource::RunnerLocal,
            SkillSourceMetricOperation::CatalogList,
            observation_started,
            Some(&response),
            SkillSourceMetricOutcomeClass::Success,
            Some(parsed.skills.len()),
        );
        Ok(Some(parsed))
    }

    async fn probe_project_skill_exact(
        &self,
        project: &ResolvedProject,
        target_skill_id: &str,
    ) -> ExactSkillProbeOutcome {
        let packages = match self
            .list_agent_skill_packages(project, SkillSourceMetricOperation::ExactResolve)
            .await
        {
            Ok(packages) => packages,
            Err(_) => return ExactSkillProbeOutcome::SourceUnavailable,
        };
        let mut candidate = None;
        for package in packages.entries {
            if !valid_package_name(&package.name) || package.kind != "dir" {
                continue;
            }
            if skill_id(&project.resolved_id, &package.name) != target_skill_id {
                continue;
            }
            let package_root = format!("{}/{}", SKILL_ROOT, package.name);
            let definition_path = format!("{package_root}/{SKILL_DEFINITION_FILE}");
            let read = match self
                .read_agent_skill_file(
                    project,
                    SkillSourceMetricOperation::ExactResolve,
                    &package_root,
                    &definition_path,
                    1,
                    MAX_SKILL_DISCOVERY_READ_LINES,
                    MAX_SKILL_DEFINITION_BYTES,
                    MAX_SKILL_DEFINITION_BYTES,
                )
                .await
            {
                Ok(read) => read,
                Err(SkillIoError::Unavailable) => return ExactSkillProbeOutcome::SourceUnavailable,
                Err(_) => continue,
            };
            let metadata = match parse_skill_metadata(&read.content) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let next = ExactSkillCandidate::Project {
                package_name: package.name,
                name: metadata.name,
                definition_revision: read.sha256,
            };
            if candidate.replace(next).is_some() {
                return ExactSkillProbeOutcome::Ambiguous;
            }
        }
        candidate
            .map(ExactSkillProbeOutcome::Candidate)
            .unwrap_or(ExactSkillProbeOutcome::Absent)
    }

    async fn probe_runner_skill_exact(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
        skill_id: &str,
    ) -> ExactSkillProbeOutcome {
        let observation_started = Instant::now();
        let response = match self
            .runner_skill_request(
                project,
                auth,
                RunnerSkillRequest::Resolve {
                    skill_id: skill_id.to_string(),
                },
                true,
                Some(SkillSourceMetricOperation::ExactResolve),
            )
            .await
        {
            Ok(Some(response)) => response,
            Ok(None) => return ExactSkillProbeOutcome::NotApplicable,
            Err(_) => return ExactSkillProbeOutcome::SourceUnavailable,
        };
        if response.exit_code != Some(0) || response.error.is_some() {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::RunnerLocal,
                SkillSourceMetricOperation::ExactResolve,
                observation_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::RunnerError,
                None,
            );
            return ExactSkillProbeOutcome::SourceUnavailable;
        }
        let resolved: RunnerSkillResolveResponse =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(resolved) => resolved,
                Err(_) => {
                    observe_skill_source_request(
                        self.metrics.as_ref(),
                        SkillSourceMetricSource::RunnerLocal,
                        SkillSourceMetricOperation::ExactResolve,
                        observation_started,
                        Some(&response),
                        SkillSourceMetricOutcomeClass::InvalidResponse,
                        None,
                    );
                    return ExactSkillProbeOutcome::SourceUnavailable;
                }
            };
        if resolved.validate_for_request(skill_id).is_err() {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::RunnerLocal,
                SkillSourceMetricOperation::ExactResolve,
                observation_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::InvalidResponse,
                None,
            );
            return ExactSkillProbeOutcome::SourceUnavailable;
        }
        observe_skill_source_request(
            self.metrics.as_ref(),
            SkillSourceMetricSource::RunnerLocal,
            SkillSourceMetricOperation::ExactResolve,
            observation_started,
            Some(&response),
            SkillSourceMetricOutcomeClass::Success,
            None,
        );
        resolved
            .skill
            .map(|skill| ExactSkillProbeOutcome::Candidate(ExactSkillCandidate::Runner(skill)))
            .unwrap_or(ExactSkillProbeOutcome::Absent)
    }

    async fn resolve_exact_skill(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
        skill_id: &str,
    ) -> Result<Option<ExactSkillCandidate>, &'static str> {
        let mut candidate = None;
        merge_exact_skill_probe(
            &mut candidate,
            self.probe_project_skill_exact(project, skill_id).await,
        )?;
        merge_exact_skill_probe(
            &mut candidate,
            self.probe_runner_skill_exact(project, auth, skill_id).await,
        )?;
        Ok(candidate)
    }

    async fn read_runner_skill(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
        resolved: &RunnerSkillDescriptor,
        path: &str,
        start_line: usize,
        limit: usize,
        user_expected_package_revision: Option<&str>,
        user_expected_definition_revision: Option<&str>,
    ) -> ToolResult {
        let skill_id = resolved.skill_id();
        let source = resolved.source();
        let (expected_package_revision, expected_definition_revision) = match source {
            RunnerSkillSource::Configured => {
                (None, Some(resolved.definition_revision().to_string()))
            }
            RunnerSkillSource::Managed => (
                user_expected_package_revision.map(str::to_string),
                user_expected_definition_revision.map(str::to_string),
            ),
        };
        let observation_started = Instant::now();
        let response = match self
            .runner_skill_request(
                project,
                auth,
                RunnerSkillRequest::Read {
                    skill_id: skill_id.to_string(),
                    expected_source: source,
                    path: path.to_string(),
                    start_line,
                    limit,
                    expected_package_revision,
                    expected_definition_revision,
                },
                false,
                Some(SkillSourceMetricOperation::ResourceRead),
            )
            .await
        {
            Ok(Some(response)) => response,
            Ok(None) => {
                return skill_error("skill_catalog_unavailable", &project.resolved_id, None)
            }
            Err(kind) => {
                return skill_error_dynamic(
                    &kind,
                    &project.resolved_id,
                    Some(json!({"skill_id": skill_id})),
                    false,
                )
            }
        };
        if response.exit_code != Some(0) || response.error.is_some() {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::RunnerLocal,
                SkillSourceMetricOperation::ResourceRead,
                observation_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::RunnerError,
                None,
            );
            let kind = stable_runner_skill_read_error(response.error.as_deref());
            return skill_error_dynamic(
                kind,
                &project.resolved_id,
                Some(json!({"skill_id": skill_id, "path": path})),
                false,
            );
        }
        let read: RunnerSkillReadResponse =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(read) => read,
                Err(_) => {
                    observe_skill_source_request(
                        self.metrics.as_ref(),
                        SkillSourceMetricSource::RunnerLocal,
                        SkillSourceMetricOperation::ResourceRead,
                        observation_started,
                        Some(&response),
                        SkillSourceMetricOutcomeClass::InvalidResponse,
                        None,
                    );
                    return skill_error("skill_catalog_unavailable", &project.resolved_id, None);
                }
            };
        if read
            .validate_for_request(skill_id, source, path, start_line, limit)
            .is_err()
            || (source == RunnerSkillSource::Configured
                && read.skill.definition_revision() != resolved.definition_revision())
        {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::RunnerLocal,
                SkillSourceMetricOperation::ResourceRead,
                observation_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::InvalidResponse,
                None,
            );
            return skill_error("skill_catalog_unavailable", &project.resolved_id, None);
        }
        observe_skill_source_request(
            self.metrics.as_ref(),
            SkillSourceMetricSource::RunnerLocal,
            SkillSourceMetricOperation::ResourceRead,
            observation_started,
            Some(&response),
            SkillSourceMetricOutcomeClass::Success,
            None,
        );
        let (name, package_revision, definition_revision, trust) = match &read.skill {
            RunnerSkillDescriptor::Configured {
                name,
                definition_revision,
                ..
            } => (
                name.clone(),
                None,
                definition_revision.clone(),
                "operator_configured_guidance",
            ),
            RunnerSkillDescriptor::Managed {
                name,
                package_revision,
                definition_revision,
                ..
            } => (
                name.clone(),
                Some(package_revision.clone()),
                definition_revision.clone(),
                "operator_installed_guidance",
            ),
        };
        let output = json!({
            "project": project.resolved_id,
            "skill_id": skill_id,
            "name": name,
            "source_scope": "runner",
            "trust": trust,
            "package_revision": package_revision,
            "definition_revision": definition_revision,
            "path": read.path,
            "sha256": read.sha256,
            "text": read.text,
            "start_line": read.start_line,
            "end_line": read.end_line,
            "returned_lines": read.returned_lines,
            "has_more": read.has_more,
            "next_start_line": read.next_start_line,
        });
        if serialized_json_len(&output)
            .map(|bytes| bytes > MAX_SKILL_READ_RESULT_BYTES)
            .unwrap_or(true)
        {
            return skill_error(
                "skill_read_result_too_large",
                &project.resolved_id,
                Some(json!({"skill_id": skill_id, "path": path})),
            );
        }
        ToolResult::ok(output)
    }

    async fn discover_skills(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
    ) -> Result<SkillCatalog, &'static str> {
        let mut catalog = self.discover_project_skills(project).await?;
        let runner = self
            .observe_runner_skill_catalog(project, auth)
            .await
            .map_err(|_| "skills_catalog_unavailable")?;
        let mut seen_ids = catalog
            .skills
            .iter()
            .map(|skill| skill.descriptor.skill_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(runner) = runner {
            let RunnerSkillListResponse {
                skills: runner_skills,
                invalid_count: runner_invalid_count,
                diagnostics: runner_diagnostics,
                discovery_truncated: runner_discovery_truncated,
                ..
            } = runner;
            catalog.invalid_count = catalog.invalid_count.saturating_add(runner_invalid_count);
            catalog.discovery_truncated |= runner_discovery_truncated;
            for reason_code in runner_diagnostics {
                push_diagnostic(&mut catalog.diagnostics, &reason_code);
            }
            let mut configured_count = 0usize;
            let mut managed_count = 0usize;
            for skill in runner_skills {
                if !seen_ids.insert(skill.skill_id().to_string()) {
                    return Err("skills_catalog_unavailable");
                }
                let (descriptor, order_key) = match skill {
                    RunnerSkillDescriptor::Configured {
                        skill_id,
                        name,
                        description,
                        definition_revision,
                    } => {
                        configured_count += 1;
                        let order_key = skill_id.clone();
                        (
                            SkillDescriptor {
                                skill_id,
                                name,
                                description,
                                definition_revision,
                                package_revision: None,
                                source_scope: "runner",
                                trust: "operator_configured_guidance",
                                name_conflict: false,
                            },
                            order_key,
                        )
                    }
                    RunnerSkillDescriptor::Managed {
                        skill_id,
                        skill_key,
                        name,
                        description,
                        package_revision,
                        definition_revision,
                    } => {
                        managed_count += 1;
                        (
                            SkillDescriptor {
                                skill_id,
                                name,
                                description,
                                definition_revision,
                                package_revision: Some(package_revision),
                                source_scope: "runner",
                                trust: "operator_installed_guidance",
                                name_conflict: false,
                            },
                            skill_key,
                        )
                    }
                };
                catalog.skills.push(CatalogSkill {
                    descriptor,
                    order_key,
                });
            }
            catalog.sources[1] = runner_skill_source_summary(
                "configured_runner_roots",
                configured_count,
                runner_invalid_count,
                runner_discovery_truncated,
                true,
            );
            catalog.sources[2] =
                runner_skill_source_summary("managed_runner_store", managed_count, 0, false, true);
        }
        recompute_name_conflicts(&mut catalog.skills);
        catalog.catalog_revision = catalog_revision(
            &catalog.skills,
            catalog.invalid_count,
            &catalog.diagnostics,
            catalog.discovery_truncated,
            &catalog.sources,
        );
        Ok(catalog)
    }

    pub(crate) async fn startup_skills_catalog(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
    ) -> StartupSkillsCatalog {
        let catalog = match self.discover_skills(project, auth).await {
            Ok(catalog) => catalog,
            Err(_) => return StartupSkillsCatalog::unavailable("skills_catalog_unavailable"),
        };
        let entries = catalog
            .skills
            .iter()
            .map(|skill| StartupSkillEntry {
                skill_id: skill.descriptor.skill_id.clone(),
                name: skill.descriptor.name.clone(),
                description: bounded_extension_description(&skill.descriptor.description),
                source_scope: skill.descriptor.source_scope.to_string(),
                trust: skill.descriptor.trust.to_string(),
                name_conflict: skill.descriptor.name_conflict,
            })
            .collect();
        StartupSkillsCatalog::available(
            catalog.catalog_revision,
            catalog.discovery_truncated,
            entries,
        )
    }

    pub(crate) async fn skills_catalog_context_projection(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
    ) -> Result<Value, &'static str> {
        let catalog = self
            .discover_skills(project, auth)
            .await
            .map_err(|_| "skills_catalog_unavailable")?;
        Ok(catalog.page_value(
            &project.resolved_id,
            None,
            0,
            MAX_SKILL_LIST_LIMIT,
            MAX_SKILL_SIDECAR_CATALOG_BYTES,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_skill_resource(
        &self,
        project: &ResolvedProject,
        skill_id: String,
        path: String,
        expected_definition_revision: String,
        expected_package_revision: Option<String>,
        args: Vec<String>,
        cwd: Option<String>,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        purpose: Option<ExecutionPurpose>,
        session_id: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !valid_skill_id(&skill_id) {
            return skill_execution_error("skill_id_invalid", Some(json!({"skill_id": skill_id})));
        }
        let resource_path = match validate_resource_path(&path) {
            Ok(path) => path,
            Err(kind) => return skill_execution_error(kind, Some(json!({"skill_id": skill_id}))),
        };
        if !resource_path.starts_with("scripts/") {
            return skill_execution_error(
                "skill_resource_not_executable",
                Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
            );
        }
        if let Err(kind) = validate_skill_resource_interpreter(&resource_path) {
            return skill_execution_error(
                kind,
                Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
            );
        }
        if !is_lower_sha256(&expected_definition_revision) {
            return skill_execution_error(
                "skill_definition_revision_invalid",
                Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
            );
        }
        if expected_package_revision
            .as_deref()
            .is_some_and(|revision| !valid_package_revision(revision))
        {
            return skill_execution_error(
                "skill_package_revision_invalid",
                Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
            );
        }

        let resolved = match self.resolve_exact_skill(project, auth, &skill_id).await {
            Ok(Some(ExactSkillCandidate::Runner(resolved))) => resolved,
            Ok(Some(ExactSkillCandidate::Project { .. })) => {
                return skill_execution_error(
                    "skill_execution_trust_denied",
                    Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
                )
            }
            Ok(None) => {
                return skill_execution_error(
                    "skill_not_found",
                    Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
                )
            }
            Err(_) => return skill_execution_error("skill_catalog_unavailable", None),
        };

        match resolved.source() {
            RunnerSkillSource::Configured => {
                if expected_package_revision.is_some() {
                    return skill_execution_error(
                        "skill_package_revision_not_supported",
                        Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
                    );
                }
                if expected_definition_revision != resolved.definition_revision() {
                    return skill_execution_error(
                        "skill_definition_changed",
                        Some(json!({
                            "skill_id": skill_id,
                            "skill_path": resource_path,
                            "skill_definition_revision": resolved.definition_revision(),
                        })),
                    );
                }
            }
            RunnerSkillSource::Managed if expected_package_revision.is_none() => {
                return skill_execution_error(
                    "skill_package_revision_required",
                    Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
                );
            }
            RunnerSkillSource::Managed => {}
        }

        let read = self
            .read_runner_skill(
                project,
                auth,
                &resolved,
                &resource_path,
                1,
                MAX_SKILL_READ_LINES,
                expected_package_revision.as_deref(),
                Some(&expected_definition_revision),
            )
            .await;
        if !read.success {
            let kind = read
                .output
                .get("error_kind")
                .and_then(Value::as_str)
                .unwrap_or("skill_resource_unavailable")
                .to_string();
            return skill_execution_error(
                &kind,
                Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
            );
        }
        let Some(read_output) = read.output.as_object() else {
            return skill_execution_error("skill_resource_unavailable", None);
        };
        if read_output.get("has_more").and_then(Value::as_bool) == Some(true) {
            return skill_execution_error(
                "skill_resource_too_large_for_execution",
                Some(json!({"skill_id": skill_id, "skill_path": resource_path})),
            );
        }
        let Some(resource_sha256) = read_output.get("sha256").and_then(Value::as_str) else {
            return skill_execution_error("skill_resource_unavailable", None);
        };
        let metadata = json!({
            "skill_id": skill_id,
            "skill_name": read_output.get("name").cloned().unwrap_or(Value::Null),
            "skill_path": resource_path,
            "skill_sha256": read_output.get("sha256").cloned().unwrap_or(Value::Null),
            "skill_trust": read_output.get("trust").cloned().unwrap_or(Value::Null),
            "skill_definition_revision": read_output.get("definition_revision").cloned().unwrap_or(Value::Null),
            "skill_package_revision": read_output.get("package_revision").cloned().unwrap_or(Value::Null),
        });

        let execution_request = RunnerSkillExecutionRequest {
            skill_id: resolved.skill_id().to_string(),
            expected_source: resolved.source(),
            path: resource_path.clone(),
            expected_definition_revision: expected_definition_revision.clone(),
            expected_package_revision: expected_package_revision.clone(),
            expected_resource_sha256: resource_sha256.to_string(),
            args,
        };
        let mut result = self
            .run_skill_resource_with_contract(
                project.resolved_id.clone(),
                execution_request,
                timeout_secs,
                sync_wait_secs,
                cwd,
                purpose,
                session_id,
                auth,
            )
            .await;
        if let Some(output) = result.output.as_object_mut() {
            if let Some(metadata) = metadata.as_object() {
                output.extend(metadata.clone());
            }
            output.insert(
                "execution_source".to_string(),
                Value::String("run_skill_resource".to_string()),
            );
            output.remove("suggested_call");
        }
        result
    }

    pub(crate) async fn skill_load(
        &self,
        project: &ResolvedProject,
        name: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let name = match validate_skill_load_name(name) {
            Ok(name) => name,
            Err(kind) => return skill_error(kind, &project.resolved_id, None),
        };
        let catalog = match self.discover_skills(project, auth).await {
            Ok(catalog) => catalog,
            Err(_) => return skill_error("skill_catalog_unavailable", &project.resolved_id, None),
        };
        let matches = match exact_skill_name_matches(&catalog, &name) {
            Ok(matches) => matches,
            Err(kind) => {
                return skill_error(
                    kind,
                    &project.resolved_id,
                    Some(json!({
                        "catalog_revision": catalog.catalog_revision,
                        "discovery_truncated": true,
                    })),
                )
            }
        };
        if matches.is_empty() {
            return skill_error("skill_not_found", &project.resolved_id, None);
        }
        if matches.len() != 1 {
            let candidates = matches
                .iter()
                .take(8)
                .map(|skill| {
                    json!({
                        "skill_id": skill.descriptor.skill_id,
                        "name": skill.descriptor.name,
                        "source_scope": skill.descriptor.source_scope,
                        "trust": skill.descriptor.trust,
                        "package_revision": skill.descriptor.package_revision,
                        "definition_revision": skill.descriptor.definition_revision,
                    })
                })
                .collect::<Vec<_>>();
            return skill_error(
                "skill_name_ambiguous",
                &project.resolved_id,
                Some(json!({
                    "candidate_count": matches.len(),
                    "candidates": candidates,
                    "candidates_truncated": matches.len() > 8,
                })),
            );
        }
        let descriptor = matches[0].descriptor.clone();
        let mut result = self
            .skill_read_file(
                project,
                descriptor.skill_id.clone(),
                Some(SKILL_DEFINITION_FILE.to_string()),
                Some(1),
                Some(MAX_SKILL_READ_LINES),
                Some(descriptor.definition_revision.clone()),
                descriptor.package_revision.clone(),
                auth,
            )
            .await;
        if !result.success {
            return result;
        }
        let Some(output) = result.output.as_object_mut() else {
            return skill_error("skill_load_result_invalid", &project.resolved_id, None);
        };
        output.insert(
            "catalog_revision".to_string(),
            Value::String(catalog.catalog_revision),
        );
        output.insert(
            "descriptor".to_string(),
            serde_json::to_value(&descriptor).expect("SkillDescriptor serialization is infallible"),
        );
        if serialized_json_len(&result.output)
            .map(|bytes| bytes > MAX_SKILL_READ_RESULT_BYTES)
            .unwrap_or(true)
        {
            return skill_error(
                "skill_load_result_too_large",
                &project.resolved_id,
                Some(json!({"skill_id": descriptor.skill_id})),
            );
        }
        result
    }

    pub(crate) async fn skill_list(
        &self,
        project: &ResolvedProject,
        query: Option<String>,
        offset: Option<usize>,
        limit: Option<usize>,
        expected_catalog_revision: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let query = match validate_query(query) {
            Ok(query) => query,
            Err(kind) => return skill_error(kind, &project.resolved_id, None),
        };
        let limit = limit.unwrap_or(DEFAULT_SKILL_LIST_LIMIT);
        if !(1..=MAX_SKILL_LIST_LIMIT).contains(&limit) {
            return skill_error("skill_list_limit_invalid", &project.resolved_id, None);
        }
        let offset = offset.unwrap_or(0);
        if expected_catalog_revision
            .as_deref()
            .is_some_and(|revision| !valid_catalog_revision(revision))
        {
            return skill_error("skill_catalog_revision_invalid", &project.resolved_id, None);
        }
        let catalog = match self.discover_skills(project, auth).await {
            Ok(catalog) => catalog,
            Err(_) => return skill_error("skill_catalog_unavailable", &project.resolved_id, None),
        };
        if expected_catalog_revision
            .as_deref()
            .is_some_and(|expected| expected != catalog.catalog_revision)
        {
            return skill_error(
                "skill_catalog_changed",
                &project.resolved_id,
                Some(json!({"catalog_revision": catalog.catalog_revision})),
            );
        }
        ToolResult::ok(catalog.page_value(
            &project.resolved_id,
            query.as_deref(),
            offset,
            limit,
            MAX_SKILL_CATALOG_RESULT_BYTES,
        ))
    }

    pub(crate) async fn skill_read_file(
        &self,
        project: &ResolvedProject,
        skill_id: String,
        path: Option<String>,
        start_line: Option<usize>,
        limit: Option<usize>,
        expected_definition_revision: Option<String>,
        expected_package_revision: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !valid_skill_id(&skill_id) {
            return skill_error("skill_id_invalid", &project.resolved_id, None);
        }
        let resource_path =
            match validate_resource_path(path.as_deref().unwrap_or(SKILL_DEFINITION_FILE)) {
                Ok(path) => path,
                Err(kind) => {
                    return skill_error(
                        kind,
                        &project.resolved_id,
                        Some(json!({"skill_id": skill_id})),
                    )
                }
            };
        let limit = limit.unwrap_or(DEFAULT_SKILL_READ_LINES);
        if !(1..=MAX_SKILL_READ_LINES).contains(&limit) || start_line == Some(0) {
            return skill_error(
                "skill_read_range_invalid",
                &project.resolved_id,
                Some(json!({"skill_id": skill_id})),
            );
        }
        let start_line = start_line.unwrap_or(1);
        if expected_definition_revision
            .as_deref()
            .is_some_and(|revision| !is_lower_sha256(revision))
        {
            return skill_error(
                "skill_definition_revision_invalid",
                &project.resolved_id,
                Some(json!({"skill_id": skill_id})),
            );
        }
        if expected_package_revision
            .as_deref()
            .is_some_and(|revision| !valid_package_revision(revision))
        {
            return skill_error(
                "skill_package_revision_invalid",
                &project.resolved_id,
                Some(json!({"skill_id": skill_id})),
            );
        }
        let candidate = match self.resolve_exact_skill(project, auth, &skill_id).await {
            Ok(Some(candidate)) => candidate,
            Ok(None) => {
                return skill_error(
                    "skill_not_found",
                    &project.resolved_id,
                    Some(json!({"skill_id": skill_id})),
                )
            }
            Err(_) => return skill_error("skill_catalog_unavailable", &project.resolved_id, None),
        };
        let (package_name, name, definition_revision) = match candidate {
            ExactSkillCandidate::Runner(resolved) => {
                if resolved.source() == RunnerSkillSource::Configured {
                    if expected_package_revision.is_some() {
                        return skill_error(
                            "skill_package_revision_not_supported",
                            &project.resolved_id,
                            Some(json!({"skill_id": skill_id})),
                        );
                    }
                    if expected_definition_revision
                        .as_deref()
                        .is_some_and(|expected| expected != resolved.definition_revision())
                    {
                        return skill_error(
                            "skill_definition_changed",
                            &project.resolved_id,
                            Some(json!({
                                "skill_id": skill_id,
                                "definition_revision": resolved.definition_revision(),
                            })),
                        );
                    }
                }
                // Managed Resolve revisions prove membership/source identity only. The final
                // Read receives only the caller's explicit managed revision expectations.
                return self
                    .read_runner_skill(
                        project,
                        auth,
                        &resolved,
                        &resource_path,
                        start_line,
                        limit,
                        expected_package_revision.as_deref(),
                        expected_definition_revision.as_deref(),
                    )
                    .await;
            }
            ExactSkillCandidate::Project {
                package_name,
                name,
                definition_revision,
            } => (package_name, name, definition_revision),
        };
        if expected_package_revision.is_some() {
            return skill_error(
                "skill_package_revision_not_supported",
                &project.resolved_id,
                Some(json!({"skill_id": skill_id})),
            );
        }
        if expected_definition_revision
            .as_deref()
            .is_some_and(|expected| expected != definition_revision)
        {
            return skill_error(
                "skill_definition_changed",
                &project.resolved_id,
                Some(json!({
                    "skill_id": skill_id,
                    "definition_revision": definition_revision,
                })),
            );
        }
        let package_root = format!("{}/{}", SKILL_ROOT, package_name);
        let full_path = format!("{package_root}/{resource_path}");
        if crate::sensitive_paths::is_secret_path(&full_path) {
            return skill_error(
                "skill_sensitive_path",
                &project.resolved_id,
                Some(json!({"skill_id": skill_id, "path": resource_path})),
            );
        }
        let max_file_bytes = if resource_path == SKILL_DEFINITION_FILE {
            MAX_SKILL_DEFINITION_BYTES
        } else {
            MAX_SKILL_RESOURCE_FILE_BYTES
        };
        let read = match self
            .read_agent_skill_file(
                project,
                SkillSourceMetricOperation::ResourceRead,
                &package_root,
                &full_path,
                start_line,
                limit,
                max_file_bytes,
                MAX_SKILL_READ_TEXT_BYTES,
            )
            .await
        {
            Ok(read) => read,
            Err(error) => {
                return skill_error(
                    error.resource_error_kind(),
                    &project.resolved_id,
                    Some(json!({"skill_id": skill_id, "path": resource_path})),
                )
            }
        };
        // Do not pair a resource body with a definition revision that changed
        // after discovery. SKILL.md carries its own full-file SHA; other package
        // resources require a post-read definition recheck because the resource
        // read itself cannot prove that the selected Skill definition is still
        // the one the caller/discovery observed.
        if resource_path == SKILL_DEFINITION_FILE {
            if read.sha256 != definition_revision {
                return skill_error(
                    "skill_definition_changed",
                    &project.resolved_id,
                    Some(json!({
                        "skill_id": skill_id,
                        "definition_revision": read.sha256,
                    })),
                );
            }
        } else {
            let definition_path = format!("{package_root}/{SKILL_DEFINITION_FILE}");
            let current_definition = match self
                .read_agent_skill_file(
                    project,
                    SkillSourceMetricOperation::DefinitionRecheck,
                    &package_root,
                    &definition_path,
                    1,
                    1,
                    MAX_SKILL_DEFINITION_BYTES,
                    MAX_SKILL_DEFINITION_BYTES,
                )
                .await
            {
                Ok(read) => read,
                Err(_) => {
                    return skill_error(
                        "skill_definition_changed",
                        &project.resolved_id,
                        Some(json!({"skill_id": skill_id})),
                    )
                }
            };
            if current_definition.sha256 != definition_revision {
                return skill_error(
                    "skill_definition_changed",
                    &project.resolved_id,
                    Some(json!({
                        "skill_id": skill_id,
                        "definition_revision": current_definition.sha256,
                    })),
                );
            }
        }
        let output = json!({
            "project": project.resolved_id,
            "skill_id": skill_id,
            "name": name,
            "source_scope": "project",
            "trust": "project_content",
            "package_revision": null,
            "definition_revision": definition_revision,
            "path": resource_path,
            "sha256": read.sha256,
            "text": read.content,
            "start_line": read.start_line,
            "end_line": read.end_line,
            "returned_lines": read.returned_lines,
            "has_more": read.has_more,
            "next_start_line": read.next_start_line,
        });
        if serialized_json_len(&output)
            .map(|bytes| bytes > MAX_SKILL_READ_RESULT_BYTES)
            .unwrap_or(true)
        {
            return skill_error(
                "skill_read_result_too_large",
                &project.resolved_id,
                Some(json!({"skill_id": skill_id, "path": resource_path})),
            );
        }
        ToolResult::ok(output)
    }

    pub(crate) async fn skill_versions(
        &self,
        project: &ResolvedProject,
        skill_key: String,
        offset: Option<usize>,
        limit: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !valid_skill_key(&skill_key) {
            return skill_error("skill_key_invalid", &project.resolved_id, None);
        }
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(20);
        if !(1..=MAX_SKILL_STORE_VERSIONS_LIMIT).contains(&limit) {
            return skill_error("skill_versions_limit_invalid", &project.resolved_id, None);
        }
        let response = match self
            .runner_skill_request(
                project,
                auth,
                RunnerSkillRequest::Versions {
                    skill_key: skill_key.clone(),
                    offset,
                    limit,
                },
                false,
                None,
            )
            .await
        {
            Ok(Some(response)) => response,
            Ok(None) => {
                return skill_error(
                    "skill_store_capability_unavailable",
                    &project.resolved_id,
                    None,
                )
            }
            Err(kind) => return skill_error_dynamic(&kind, &project.resolved_id, None, false),
        };
        if response.exit_code != Some(0) || response.error.is_some() {
            let kind = stable_skill_store_error(response.error.as_deref());
            return skill_error_dynamic(&kind, &project.resolved_id, None, false);
        }
        let parsed: SkillStoreVersionsResponse =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(parsed) => parsed,
                Err(_) => {
                    return skill_error("skill_store_response_invalid", &project.resolved_id, None)
                }
            };
        let expected_offset = offset.min(parsed.total_count);
        let expected_next_offset = expected_offset
            .saturating_add(parsed.versions.len())
            .lt(&parsed.total_count)
            .then_some(expected_offset.saturating_add(parsed.versions.len()));
        if parsed.format != SKILL_STORE_RESPONSE_FORMAT
            || parsed.skill_key != skill_key
            || !valid_skill_id(&parsed.skill_id)
            || !valid_state_revision(&parsed.state_revision)
            || parsed.total_count > MAX_OPERATOR_REVISIONS_PER_SKILL
            || parsed.offset != expected_offset
            || parsed.next_offset != expected_next_offset
            || parsed
                .active_package_revision
                .as_deref()
                .is_some_and(|value| !valid_package_revision(value))
            || parsed.versions.len() > limit
            || parsed.versions.iter().any(|version| {
                !valid_package_revision(&version.package_revision)
                    || !is_lower_sha256(&version.definition_revision)
                    || version.name.chars().count() > MAX_SKILL_NAME_CHARS
                    || version.description.chars().count() > MAX_SKILL_DESCRIPTION_CHARS
                    || version.file_count > MAX_SKILL_STORE_FILE_COUNT
                    || version.total_bytes > MAX_SKILL_STORE_TOTAL_BYTES
            })
        {
            return skill_error("skill_store_response_invalid", &project.resolved_id, None);
        }
        ToolResult::ok(json!({
            "project": project.resolved_id,
            "skill_id": parsed.skill_id,
            "skill_key": parsed.skill_key,
            "state_revision": parsed.state_revision,
            "active_package_revision": parsed.active_package_revision,
            "total_count": parsed.total_count,
            "offset": parsed.offset,
            "next_offset": parsed.next_offset,
            "versions": parsed.versions,
            "state_changed": false,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn skill_install(
        &self,
        project: &ResolvedProject,
        skill_key: String,
        artifact_path: String,
        expected_artifact_sha256: String,
        idempotency_key: String,
        activate: bool,
        expected_state_revision: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !valid_skill_key(&skill_key)
            || !valid_lower_sha256(&expected_artifact_sha256)
            || expected_state_revision
                .as_deref()
                .is_some_and(|value| !valid_state_revision(value))
            || crate::tool_runtime::files::validate_artifact_file_path(&artifact_path).is_err()
        {
            return skill_error(
                "skill_install_invalid_arguments",
                &project.resolved_id,
                None,
            );
        }
        let response = match self
            .runner_skill_request(
                project,
                auth,
                RunnerSkillRequest::Install {
                    skill_key: skill_key.clone(),
                    source_project_id: project.resolved_id.clone(),
                    source_project_root: project.config.path.clone(),
                    artifact_path: artifact_path.clone(),
                    expected_artifact_sha256: expected_artifact_sha256.clone(),
                    idempotency_key,
                    activate,
                    expected_state_revision,
                },
                false,
                None,
            )
            .await
        {
            Ok(Some(response)) => response,
            Ok(None) => {
                return skill_error(
                    "skill_store_capability_unavailable",
                    &project.resolved_id,
                    None,
                )
            }
            Err(kind) => {
                return skill_error_dynamic(
                    &kind,
                    &project.resolved_id,
                    Some(json!({
                        "skill_key": skill_key,
                        "artifact_path": artifact_path,
                        "expected_artifact_sha256": expected_artifact_sha256,
                    })),
                    kind == "skill_store_outcome_unknown",
                )
            }
        };
        if response.exit_code != Some(0) || response.error.is_some() {
            let kind = stable_skill_store_error(response.error.as_deref());
            return skill_error_dynamic(
                &kind,
                &project.resolved_id,
                Some(json!({
                    "skill_key": skill_key,
                    "artifact_path": artifact_path,
                    "expected_artifact_sha256": expected_artifact_sha256,
                })),
                uncertain_skill_store_error(&kind),
            );
        }
        let parsed: SkillStoreInstallResponse =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(parsed) => parsed,
                Err(_) => {
                    return skill_error("skill_store_response_invalid", &project.resolved_id, None)
                }
            };
        if parsed.format != SKILL_STORE_RESPONSE_FORMAT
            || parsed.skill_key != skill_key
            || !valid_skill_id(&parsed.skill_id)
            || !valid_package_revision(&parsed.package_revision)
            || !is_lower_sha256(&parsed.definition_revision)
            || parsed.artifact_sha256 != expected_artifact_sha256
            || !valid_state_revision(&parsed.state_revision)
            || parsed
                .active_package_revision
                .as_deref()
                .is_some_and(|value| !valid_package_revision(value))
        {
            return skill_error("skill_store_response_invalid", &project.resolved_id, None);
        }
        ToolResult::ok(json!({
            "project": project.resolved_id,
            "skill_id": parsed.skill_id,
            "skill_key": parsed.skill_key,
            "package_revision": parsed.package_revision,
            "definition_revision": parsed.definition_revision,
            "artifact_sha256": parsed.artifact_sha256,
            "file_count": parsed.file_count,
            "total_bytes": parsed.total_bytes,
            "installed": parsed.installed,
            "activated": parsed.activated,
            "replayed": parsed.replayed,
            "state_revision": parsed.state_revision,
            "active_package_revision": parsed.active_package_revision,
            "outcome_unknown": false,
            "state_changed": parsed.installed || parsed.activated,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn skill_activate(
        &self,
        project: &ResolvedProject,
        skill_key: String,
        package_revision: String,
        expected_state_revision: String,
        idempotency_key: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !valid_skill_key(&skill_key)
            || !valid_package_revision(&package_revision)
            || !valid_state_revision(&expected_state_revision)
        {
            return skill_error(
                "skill_activate_invalid_arguments",
                &project.resolved_id,
                None,
            );
        }
        let response = match self
            .runner_skill_request(
                project,
                auth,
                RunnerSkillRequest::Activate {
                    skill_key: skill_key.clone(),
                    package_revision: package_revision.clone(),
                    expected_state_revision,
                    idempotency_key,
                },
                false,
                None,
            )
            .await
        {
            Ok(Some(response)) => response,
            Ok(None) => {
                return skill_error(
                    "skill_store_capability_unavailable",
                    &project.resolved_id,
                    None,
                )
            }
            Err(kind) => {
                return skill_error_dynamic(
                    &kind,
                    &project.resolved_id,
                    Some(json!({"skill_key": skill_key, "package_revision": package_revision})),
                    kind == "skill_store_outcome_unknown",
                )
            }
        };
        if response.exit_code != Some(0) || response.error.is_some() {
            let kind = stable_skill_store_error(response.error.as_deref());
            return skill_error_dynamic(
                &kind,
                &project.resolved_id,
                Some(json!({"skill_key": skill_key, "package_revision": package_revision})),
                uncertain_skill_store_error(&kind),
            );
        }
        let parsed: SkillStoreActivateResponse =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(parsed) => parsed,
                Err(_) => {
                    return skill_error("skill_store_response_invalid", &project.resolved_id, None)
                }
            };
        if parsed.format != SKILL_STORE_RESPONSE_FORMAT
            || parsed.skill_key != skill_key
            || parsed.active_package_revision != package_revision
            || !valid_skill_id(&parsed.skill_id)
            || !valid_state_revision(&parsed.state_revision)
        {
            return skill_error("skill_store_response_invalid", &project.resolved_id, None);
        }
        ToolResult::ok(json!({
            "project": project.resolved_id,
            "skill_id": parsed.skill_id,
            "skill_key": parsed.skill_key,
            "previous_active_package_revision": parsed.previous_active_package_revision,
            "active_package_revision": parsed.active_package_revision,
            "state_revision": parsed.state_revision,
            "changed": parsed.changed,
            "replayed": parsed.replayed,
            "outcome_unknown": false,
            "state_changed": parsed.changed,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn skill_remove_revision(
        &self,
        project: &ResolvedProject,
        skill_key: String,
        package_revision: String,
        expected_state_revision: String,
        idempotency_key: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !valid_skill_key(&skill_key)
            || !valid_package_revision(&package_revision)
            || !valid_state_revision(&expected_state_revision)
        {
            return skill_error("skill_remove_invalid_arguments", &project.resolved_id, None);
        }
        let response = match self
            .runner_skill_request(
                project,
                auth,
                RunnerSkillRequest::RemoveRevision {
                    skill_key: skill_key.clone(),
                    package_revision: package_revision.clone(),
                    expected_state_revision,
                    idempotency_key,
                },
                false,
                None,
            )
            .await
        {
            Ok(Some(response)) => response,
            Ok(None) => {
                return skill_error(
                    "skill_store_capability_unavailable",
                    &project.resolved_id,
                    None,
                )
            }
            Err(kind) => {
                return skill_error_dynamic(
                    &kind,
                    &project.resolved_id,
                    Some(json!({"skill_key": skill_key, "package_revision": package_revision})),
                    kind == "skill_store_outcome_unknown",
                )
            }
        };
        if response.exit_code != Some(0) || response.error.is_some() {
            let kind = stable_skill_store_error(response.error.as_deref());
            return skill_error_dynamic(
                &kind,
                &project.resolved_id,
                Some(json!({"skill_key": skill_key, "package_revision": package_revision})),
                uncertain_skill_store_error(&kind),
            );
        }
        let parsed: SkillStoreRemoveResponse =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(parsed) => parsed,
                Err(_) => {
                    return skill_error("skill_store_response_invalid", &project.resolved_id, None)
                }
            };
        if parsed.format != SKILL_STORE_RESPONSE_FORMAT
            || parsed.skill_key != skill_key
            || parsed.package_revision != package_revision
            || !valid_skill_id(&parsed.skill_id)
            || !valid_state_revision(&parsed.state_revision)
        {
            return skill_error("skill_store_response_invalid", &project.resolved_id, None);
        }
        ToolResult::ok(json!({
            "project": project.resolved_id,
            "skill_id": parsed.skill_id,
            "skill_key": parsed.skill_key,
            "package_revision": parsed.package_revision,
            "state_revision": parsed.state_revision,
            "removed": parsed.removed,
            "replayed": parsed.replayed,
            "outcome_unknown": false,
            "state_changed": parsed.removed,
        }))
    }

    pub(crate) async fn discover_project_skills(
        &self,
        project: &ResolvedProject,
    ) -> Result<SkillCatalog, &'static str> {
        let packages = self
            .list_agent_skill_packages(project, SkillSourceMetricOperation::CatalogList)
            .await?;
        let discovery_truncated = packages.truncated;
        let mut invalid_count = 0usize;
        let mut diagnostics = Vec::new();
        let mut skills = Vec::new();
        for package in packages.entries {
            if !valid_package_name(&package.name) || package.kind != "dir" {
                invalid_count += 1;
                push_diagnostic(&mut diagnostics, "invalid_skill_package");
                continue;
            }
            let package_root = format!("{}/{}", SKILL_ROOT, package.name);
            let definition_path = format!("{package_root}/{SKILL_DEFINITION_FILE}");
            let read = match self
                .read_agent_skill_file(
                    project,
                    SkillSourceMetricOperation::CatalogDefinitionRead,
                    &package_root,
                    &definition_path,
                    1,
                    MAX_SKILL_DISCOVERY_READ_LINES,
                    MAX_SKILL_DEFINITION_BYTES,
                    MAX_SKILL_DEFINITION_BYTES,
                )
                .await
            {
                Ok(read) => read,
                Err(error) => {
                    if error == SkillIoError::Unavailable {
                        return Err("skills_catalog_unavailable");
                    }
                    invalid_count += 1;
                    push_diagnostic(
                        &mut diagnostics,
                        error.invalid_reason().unwrap_or("invalid_skill_definition"),
                    );
                    continue;
                }
            };
            let metadata = match parse_skill_metadata(&read.content) {
                Ok(metadata) => metadata,
                Err(reason) => {
                    invalid_count += 1;
                    push_diagnostic(&mut diagnostics, reason);
                    continue;
                }
            };
            let skill_id = skill_id(&project.resolved_id, &package.name);
            skills.push(CatalogSkill {
                descriptor: SkillDescriptor {
                    skill_id,
                    name: metadata.name,
                    description: metadata.description,
                    definition_revision: read.sha256,
                    package_revision: None,
                    source_scope: "project",
                    trust: "project_content",
                    name_conflict: false,
                },
                order_key: package.name,
            });
        }
        skills.sort_by(|left, right| left.order_key.cmp(&right.order_key));
        recompute_name_conflicts(&mut skills);
        let sources = vec![
            project_skill_source_summary(skills.len(), invalid_count, discovery_truncated),
            runner_skill_source_summary("configured_runner_roots", 0, 0, false, false),
            runner_skill_source_summary("managed_runner_store", 0, 0, false, false),
        ];
        let catalog_revision = catalog_revision(
            &skills,
            invalid_count,
            &diagnostics,
            discovery_truncated,
            &sources,
        );
        Ok(SkillCatalog {
            catalog_revision,
            skills,
            invalid_count,
            diagnostics,
            discovery_truncated,
            sources,
        })
    }

    async fn list_agent_skill_packages(
        &self,
        project: &ResolvedProject,
        operation: SkillSourceMetricOperation,
    ) -> Result<AgentSkillPackageList, &'static str> {
        let client_id = project.config.client_id.clone();
        let payload = json!({"limit": MAX_SKILL_DISCOVERY_PACKAGES + 1}).to_string();
        let wait_timeout = 20_u64;
        let (request_id, rx) = self
            .runner_registry
            .enqueue_skill_file_op(
                ShellFileOpRequest {
                    op: "skill_list_packages".to_string(),
                    client_id,
                    path: SKILL_ROOT.to_string(),
                    cwd: Some(project.config.path.clone()),
                    content: Some(payload),
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: wait_timeout,
                },
                "skill_runtime".to_string(),
            )
            .await
            .map_err(|_| "skills_catalog_unavailable")?;
        let request_started = Instant::now();
        let response = match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
            Ok(Ok(response)) => response,
            _ => {
                observe_skill_source_request(
                    self.metrics.as_ref(),
                    SkillSourceMetricSource::Project,
                    operation,
                    request_started,
                    None,
                    SkillSourceMetricOutcomeClass::Unavailable,
                    None,
                );
                return Err("skills_catalog_unavailable");
            }
        };
        if response.exit_code != Some(0) || response.error.is_some() {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::Project,
                operation,
                request_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::RunnerError,
                None,
            );
            self.runner_registry.cancel_request(&request_id).await;
            return Err("skills_catalog_unavailable");
        }
        let parsed: AgentSkillPackageList =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(parsed) => parsed,
                Err(_) => {
                    observe_skill_source_request(
                        self.metrics.as_ref(),
                        SkillSourceMetricSource::Project,
                        operation,
                        request_started,
                        Some(&response),
                        SkillSourceMetricOutcomeClass::InvalidResponse,
                        None,
                    );
                    return Err("skills_catalog_unavailable");
                }
            };
        if parsed.format != SKILL_PACKAGE_LIST_FORMAT
            || parsed.entries.len() > MAX_SKILL_DISCOVERY_PACKAGES + 1
        {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::Project,
                operation,
                request_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::InvalidResponse,
                None,
            );
            return Err("skills_catalog_unavailable");
        }
        let observed_count = parsed.entries.len();
        observe_skill_source_request(
            self.metrics.as_ref(),
            SkillSourceMetricSource::Project,
            operation,
            request_started,
            Some(&response),
            SkillSourceMetricOutcomeClass::Success,
            Some(observed_count),
        );
        Ok(AgentSkillPackageList {
            entries: parsed
                .entries
                .into_iter()
                .take(MAX_SKILL_DISCOVERY_PACKAGES)
                .collect(),
            truncated: parsed.truncated || observed_count > MAX_SKILL_DISCOVERY_PACKAGES,
            format: parsed.format,
        })
    }

    async fn read_agent_skill_file(
        &self,
        project: &ResolvedProject,
        operation: SkillSourceMetricOperation,
        package_root: &str,
        full_path: &str,
        start_line: usize,
        limit: usize,
        max_file_bytes: usize,
        text_budget: usize,
    ) -> Result<AgentSkillFileRead, SkillIoError> {
        let client_id = project.config.client_id.clone();
        let payload = json!({
            "package_root": package_root,
            "max_file_bytes": max_file_bytes,
        })
        .to_string();
        let wait_timeout = 20_u64;
        let end_line = start_line.saturating_add(limit).saturating_sub(1);
        let (request_id, rx) = self
            .runner_registry
            .enqueue_skill_file_op(
                ShellFileOpRequest {
                    op: "skill_read_file".to_string(),
                    client_id,
                    path: full_path.to_string(),
                    cwd: Some(project.config.path.clone()),
                    content: Some(payload),
                    max_bytes: Some(text_budget),
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: Some(start_line),
                    end_line: Some(end_line),
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: wait_timeout,
                },
                "skill_runtime".to_string(),
            )
            .await
            .map_err(|_| SkillIoError::Unavailable)?;
        let request_started = Instant::now();
        let response = match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
            Ok(Ok(response)) => response,
            _ => {
                observe_skill_source_request(
                    self.metrics.as_ref(),
                    SkillSourceMetricSource::Project,
                    operation,
                    request_started,
                    None,
                    SkillSourceMetricOutcomeClass::Unavailable,
                    None,
                );
                self.runner_registry.cancel_request(&request_id).await;
                return Err(SkillIoError::Unavailable);
            }
        };
        if response.exit_code != Some(0) || response.error.is_some() {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::Project,
                operation,
                request_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::RunnerError,
                None,
            );
            return Err(classify_skill_io_error(
                response.error.as_deref().or(response.stderr.as_deref()),
            ));
        }
        let read: AgentSkillFileRead =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(read) => read,
                Err(_) => {
                    observe_skill_source_request(
                        self.metrics.as_ref(),
                        SkillSourceMetricSource::Project,
                        operation,
                        request_started,
                        Some(&response),
                        SkillSourceMetricOutcomeClass::InvalidResponse,
                        None,
                    );
                    return Err(SkillIoError::Unavailable);
                }
            };
        if read.format != SKILL_FILE_READ_FORMAT
            || read.file_bytes > max_file_bytes
            || read.start_line != start_line
            || read.limit != limit
            || !is_lower_sha256(&read.sha256)
            || read.returned_lines > limit
            || read.returned_lines > read.total_lines
            || read.content.len() > text_budget
            || read.end_line.is_some_and(|end| end < read.start_line)
            || read.has_more != read.next_start_line.is_some()
        {
            observe_skill_source_request(
                self.metrics.as_ref(),
                SkillSourceMetricSource::Project,
                operation,
                request_started,
                Some(&response),
                SkillSourceMetricOutcomeClass::InvalidResponse,
                None,
            );
            return Err(SkillIoError::Unavailable);
        }
        observe_skill_source_request(
            self.metrics.as_ref(),
            SkillSourceMetricSource::Project,
            operation,
            request_started,
            Some(&response),
            SkillSourceMetricOutcomeClass::Success,
            None,
        );
        Ok(read)
    }
}

impl SkillCatalog {
    fn page_value(
        &self,
        project: &str,
        query: Option<&str>,
        offset: usize,
        limit: usize,
        byte_budget: usize,
    ) -> Value {
        let query = query.map(str::to_lowercase);
        let filtered = self
            .skills
            .iter()
            .filter(|skill| {
                query.as_ref().is_none_or(|query| {
                    skill.descriptor.name.to_lowercase().contains(query)
                        || skill.descriptor.description.to_lowercase().contains(query)
                })
            })
            .collect::<Vec<_>>();
        let total_count = filtered.len();
        let sources = self
            .sources
            .iter()
            .map(|source| json!(source))
            .collect::<Vec<_>>();
        let offset = offset.min(total_count);
        let mut descriptors = Vec::new();
        let hard_end = offset.saturating_add(limit).min(total_count);
        for skill in filtered.iter().skip(offset).take(limit) {
            descriptors.push(json!(skill.descriptor));
            if catalog_page_serialized_len(
                project,
                &self.catalog_revision,
                total_count,
                offset,
                hard_end,
                &descriptors,
                &sources,
                self.invalid_count,
                &self.diagnostics,
                self.discovery_truncated,
            )
            .map(|bytes| bytes <= byte_budget)
            .unwrap_or(false)
            {
                continue;
            }
            descriptors.pop();
            break;
        }
        let next_offset = offset.saturating_add(descriptors.len());
        catalog_page_envelope(
            project,
            &self.catalog_revision,
            total_count,
            offset,
            next_offset,
            descriptors,
            sources,
            self.invalid_count,
            &self.diagnostics,
            self.discovery_truncated,
        )
    }
}

#[derive(Serialize)]
struct SkillCatalogPageMeasure<'a> {
    project: &'a str,
    catalog_revision: &'a str,
    total_count: usize,
    returned_count: usize,
    offset: usize,
    next_offset: Option<usize>,
    truncated: bool,
    skills: &'a [Value],
    sources: &'a [Value],
    invalid_count: usize,
    diagnostics: &'a [Value],
    discovery_truncated: bool,
}

#[allow(clippy::too_many_arguments)]
fn catalog_page_serialized_len(
    project: &str,
    catalog_revision: &str,
    total_count: usize,
    offset: usize,
    next_offset: usize,
    skills: &[Value],
    sources: &[Value],
    invalid_count: usize,
    diagnostics: &[Value],
    discovery_truncated: bool,
) -> Result<usize, serde_json::Error> {
    let truncated = next_offset < total_count;
    serialized_json_len(&SkillCatalogPageMeasure {
        project,
        catalog_revision,
        total_count,
        returned_count: skills.len(),
        offset,
        next_offset: truncated.then_some(next_offset),
        truncated,
        skills,
        sources,
        invalid_count,
        diagnostics,
        discovery_truncated,
    })
}

fn catalog_page_envelope(
    project: &str,
    catalog_revision: &str,
    total_count: usize,
    offset: usize,
    next_offset: usize,
    skills: Vec<Value>,
    sources: Vec<Value>,
    invalid_count: usize,
    diagnostics: &[Value],
    discovery_truncated: bool,
) -> Value {
    let returned_count = skills.len();
    let truncated = next_offset < total_count;
    json!({
        "project": project,
        "catalog_revision": catalog_revision,
        "total_count": total_count,
        "returned_count": returned_count,
        "offset": offset,
        "next_offset": if truncated { Some(next_offset) } else { None },
        "truncated": truncated,
        "skills": skills,
        "sources": sources,
        "invalid_count": invalid_count,
        "diagnostics": diagnostics,
        "discovery_truncated": discovery_truncated,
    })
}

fn validate_skill_resource_interpreter(resource_path: &str) -> Result<(), &'static str> {
    match std::path::Path::new(resource_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "py" | "sh" => Ok(()),
        _ => Err("skill_resource_interpreter_unsupported"),
    }
}

fn skill_execution_error(kind: &str, extra: Option<Value>) -> ToolResult {
    let mut output = json!({
        "execution_state": "not_started",
        "command_started": false,
        "command_completed": false,
        "command_ok": false,
        "exit_code": null,
        "failure_kind": kind,
        "tool_failure": true,
        "state_changed": false,
    });
    if let (Some(target), Some(extra)) = (
        output.as_object_mut(),
        extra.and_then(|value| value.as_object().cloned()),
    ) {
        target.extend(extra);
    }
    ToolResult::err_with_output(kind.to_string(), output)
}

fn skill_error(kind: &'static str, project: &str, extra: Option<Value>) -> ToolResult {
    let mut output = json!({
        "error_kind": kind,
        "project": project,
        "state_changed": false,
    });
    if let (Some(target), Some(extra)) = (
        output.as_object_mut(),
        extra.and_then(|v| v.as_object().cloned()),
    ) {
        target.extend(extra);
    }
    ToolResult::err_with_output(kind, output)
}

fn skill_error_dynamic(
    kind: &str,
    project: &str,
    extra: Option<Value>,
    outcome_unknown: bool,
) -> ToolResult {
    let mut output = json!({
        "error_kind": kind,
        "project": project,
        "outcome_unknown": outcome_unknown,
        "state_changed": if outcome_unknown { Value::Null } else { Value::Bool(false) },
    });
    if let (Some(target), Some(extra)) = (
        output.as_object_mut(),
        extra.and_then(|v| v.as_object().cloned()),
    ) {
        target.extend(extra);
    }
    if outcome_unknown || kind == "skill_install_reconcile_required" {
        let recovery_skill_key = output
            .get("skill_key")
            .and_then(Value::as_str)
            .filter(|skill_key| valid_skill_key(skill_key))
            .map(str::to_string);
        let target = output
            .as_object_mut()
            .expect("Skill store error projection is always an object");
        if !project.is_empty() {
            if let Some(skill_key) = recovery_skill_key.as_deref() {
                target.insert(
                    "suggested_call".to_string(),
                    SuggestedToolCall::new(
                        "skill_versions",
                        json!({"project": project, "skill_key": skill_key}),
                    )
                    .to_value(),
                );
            } else {
                target.insert("recovery_kind".to_string(), json!("reconcile"));
                target.insert("reconcile_with".to_string(), json!("skill_versions"));
            }
        } else {
            target.insert("recovery_kind".to_string(), json!("reconcile"));
            target.insert("reconcile_with".to_string(), json!("skill_versions"));
        }
        if outcome_unknown {
            target.insert("retry_same_idempotency_key".to_string(), json!(true));
        }
    }
    ToolResult::err_with_output(kind.to_string(), output)
}

fn runner_skill_enqueue_error_kind(
    error: &EnqueueRunnerSkillError,
    management: bool,
) -> &'static str {
    if !management {
        return "skill_catalog_unavailable";
    }
    match error {
        EnqueueRunnerSkillError::UnsupportedCapability { .. } => {
            "skill_store_capability_unavailable"
        }
        EnqueueRunnerSkillError::ExactRunnerUnavailable { .. }
        | EnqueueRunnerSkillError::ExactRunnerOffline { .. }
        | EnqueueRunnerSkillError::RunnerChanged { .. } => "skill_store_runner_changed",
        EnqueueRunnerSkillError::InvalidRequest { .. }
        | EnqueueRunnerSkillError::DispatchUnavailable { .. } => "skill_store_runner_unavailable",
    }
}

fn stable_skill_store_error(error: Option<&str>) -> String {
    let base = error
        .unwrap_or("skill_store_unavailable")
        .split(':')
        .next()
        .unwrap_or("skill_store_unavailable")
        .trim();
    if base == "skill_capability_unavailable" {
        return "skill_store_capability_unavailable".to_string();
    }
    if base.starts_with("skill_")
        && base.len() <= 96
        && base
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        base.to_string()
    } else {
        "skill_store_unavailable".to_string()
    }
}

fn stable_runner_skill_read_error(error: Option<&str>) -> &'static str {
    let base = error
        .unwrap_or("skill_catalog_unavailable")
        .split(':')
        .next()
        .unwrap_or("skill_catalog_unavailable")
        .trim();
    match base {
        "skill_package_changed" => "skill_package_changed",
        "skill_definition_changed" => "skill_definition_changed",
        "skill_sensitive_path" => "skill_sensitive_path",
        "skill_resource_path_invalid" => "skill_resource_path_invalid",
        "skill_resource_not_found" => "skill_resource_not_found",
        "skill_resource_too_large" => "skill_resource_too_large",
        "skill_resource_unsupported_encoding" => "skill_resource_unsupported_encoding",
        "skill_resource_unavailable" => "skill_resource_unavailable",
        "skill_read_result_too_large" => "skill_read_result_too_large",
        _ => "skill_catalog_unavailable",
    }
}

fn uncertain_skill_store_error(kind: &str) -> bool {
    matches!(
        kind,
        "skill_store_outcome_unknown"
            | "skill_store_replay_commit_failed"
            | "skill_store_state_write_failed"
            | "skill_store_revision_commit_failed"
            | "skill_store_revision_commit_raced"
            | "skill_remove_cleanup_failed"
    )
}

fn skill_name_key(name: &str) -> String {
    UniCase::unicode(name).to_folded_case()
}

fn exact_skill_name_matches<'a>(
    catalog: &'a SkillCatalog,
    name: &str,
) -> Result<Vec<&'a CatalogSkill>, &'static str> {
    if catalog.discovery_truncated {
        return Err("skill_catalog_truncated");
    }
    let key = skill_name_key(name);
    Ok(catalog
        .skills
        .iter()
        .filter(|skill| skill_name_key(&skill.descriptor.name) == key)
        .collect())
}

fn recompute_name_conflicts(skills: &mut [CatalogSkill]) {
    let mut counts = BTreeMap::<String, usize>::new();
    for skill in skills.iter() {
        *counts
            .entry(skill_name_key(&skill.descriptor.name))
            .or_default() += 1;
    }
    for skill in skills {
        skill.descriptor.name_conflict = counts
            .get(&skill_name_key(&skill.descriptor.name))
            .copied()
            .unwrap_or_default()
            > 1;
    }
}

fn validate_skill_load_name(name: String) -> Result<String, &'static str> {
    if name.is_empty()
        || name.trim() != name
        || name.chars().count() > MAX_SKILL_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err("skill_name_invalid");
    }
    Ok(name)
}

fn validate_query(query: Option<String>) -> Result<Option<String>, &'static str> {
    let Some(query) = query else { return Ok(None) };
    let query = query.trim();
    if query.chars().count() > MAX_SKILL_QUERY_CHARS || query.chars().any(char::is_control) {
        return Err("skill_query_invalid");
    }
    if query.is_empty() {
        Ok(None)
    } else {
        Ok(Some(query.to_string()))
    }
}

fn validate_resource_path(path: &str) -> Result<String, &'static str> {
    let path = path.trim();
    if path.is_empty()
        || path.chars().count() > MAX_SKILL_RESOURCE_PATH_CHARS
        || path.chars().any(char::is_control)
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.as_bytes().get(1) == Some(&b':')
    {
        return Err("skill_resource_path_invalid");
    }
    let normalized = path.replace('\\', "/");
    if normalized
        .split('/')
        .any(|component| component.is_empty() || component == "." || component == "..")
        || std::path::Path::new(&normalized)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("skill_resource_path_invalid");
    }
    Ok(normalized)
}

fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 160
        && !name.contains(['/', '\\'])
        && name != "."
        && name != ".."
        && !name.chars().any(char::is_control)
}

fn valid_skill_id(value: &str) -> bool {
    value
        .strip_prefix("wc_skill_")
        .and_then(webcodex_core::compact::decode::<16>)
        .is_some()
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_catalog_revision(value: &str) -> bool {
    value
        .strip_prefix("wc_skillcat_")
        .and_then(webcodex_core::compact::decode::<32>)
        .is_some()
}

fn skill_id(project: &str, package_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.project-skill-id.v1\0");
    hasher.update(project.as_bytes());
    hasher.update(b"\0");
    hasher.update(SKILL_ROOT.as_bytes());
    hasher.update(b"/");
    hasher.update(package_name.as_bytes());
    format!(
        "wc_skill_{}",
        webcodex_core::compact::encode(&hasher.finalize()[..16])
    )
}

fn catalog_revision(
    skills: &[CatalogSkill],
    invalid_count: usize,
    diagnostics: &[Value],
    discovery_truncated: bool,
    sources: &[SkillCatalogSourceSummary],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.skill-catalog.v3\0");
    for skill in skills {
        for value in [
            skill.descriptor.skill_id.as_str(),
            skill.descriptor.name.as_str(),
            skill.descriptor.description.as_str(),
            skill.descriptor.source_scope,
            skill.descriptor.trust,
            skill.descriptor.definition_revision.as_str(),
            skill
                .descriptor
                .package_revision
                .as_deref()
                .unwrap_or_default(),
        ] {
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
    }
    hasher.update((invalid_count as u64).to_be_bytes());
    for diagnostic in diagnostics {
        if let Some(reason_code) = diagnostic.get("reason_code").and_then(Value::as_str) {
            hasher.update((reason_code.len() as u64).to_be_bytes());
            hasher.update(reason_code.as_bytes());
        }
    }
    hasher.update([u8::from(discovery_truncated)]);
    for source in sources {
        for value in [
            source.kind,
            source.status,
            source.root_hint.unwrap_or_default(),
            source.reason_code.unwrap_or_default(),
        ] {
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        hasher.update((source.skill_count as u64).to_be_bytes());
        hasher.update((source.invalid_count as u64).to_be_bytes());
        hasher.update([u8::from(source.discovery_truncated)]);
    }
    format!(
        "wc_skillcat_{}",
        webcodex_core::compact::encode(hasher.finalize())
    )
}

fn push_diagnostic(diagnostics: &mut Vec<Value>, reason_code: &str) {
    if diagnostics.len() < MAX_SKILL_INVALID_DIAGNOSTICS {
        diagnostics.push(json!({"reason_code": reason_code}));
    }
}

fn classify_skill_io_error(error: Option<&str>) -> SkillIoError {
    match error.unwrap_or_default() {
        "skill_file_not_found" => SkillIoError::NotFound,
        "skill_sensitive_path" => SkillIoError::SensitivePath,
        "skill_invalid_utf8" => SkillIoError::InvalidUtf8,
        "skill_file_too_large" => SkillIoError::TooLarge,
        "skill_path_invalid" | "skill_path_escape" => SkillIoError::InvalidPath,
        _ => SkillIoError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_parser_requires_explicit_bounded_metadata() {
        let parsed = parse_skill_metadata(
            "---\nname: demo\ndescription: 'Use demo safely'\nlicense: MIT\n---\n# body\nSECRET_BODY",
        )
        .unwrap();
        assert_eq!(parsed.name, "demo");
        assert_eq!(parsed.description, "Use demo safely");
        for invalid in [
            "# no frontmatter\nname: guessed",
            "---\ndescription: only desc\n---\nname in body",
            "---\nname: x\n---\nbody description",
        ] {
            assert!(parse_skill_metadata(invalid).is_err(), "{invalid}");
        }

        let block = parse_skill_metadata("---\nname: x\ndescription: |\n  block\n---").unwrap();
        assert_eq!(block.description, "block\n");
    }

    #[test]
    fn ids_are_locator_stable_and_project_scoped() {
        let a = skill_id("agent:a:demo", "foo");
        assert_eq!(a, skill_id("agent:a:demo", "foo"));
        assert_ne!(a, skill_id("agent:b:demo", "foo"));
        assert_ne!(a, skill_id("agent:a:demo", "bar"));
        assert!(valid_skill_id(&a));
        assert!(!a.contains(SKILL_ROOT));
    }

    #[test]
    fn exact_name_selection_fails_closed_when_catalog_discovery_is_truncated() {
        let skills = vec![CatalogSkill {
            descriptor: SkillDescriptor {
                skill_id: "wc_skill_AAAAAAAAAAAAAAAAAAAAAg".to_string(),
                name: "demo".to_string(),
                description: "demo".to_string(),
                definition_revision: "a".repeat(64),
                package_revision: None,
                source_scope: "project",
                trust: "project_content",
                name_conflict: false,
            },
            order_key: "demo".to_string(),
        }];
        let sources = vec![
            project_skill_source_summary(skills.len(), 0, true),
            runner_skill_source_summary("configured_runner_roots", 0, 0, false, false),
            runner_skill_source_summary("managed_runner_store", 0, 0, false, false),
        ];
        let catalog = SkillCatalog {
            catalog_revision: catalog_revision(&skills, 0, &[], true, &sources),
            skills,
            invalid_count: 0,
            diagnostics: Vec::new(),
            discovery_truncated: true,
            sources,
        };
        assert_eq!(
            exact_skill_name_matches(&catalog, "demo").unwrap_err(),
            "skill_catalog_truncated"
        );
    }

    #[test]
    fn catalog_projection_is_byte_bounded_and_continuable() {
        let mut skills = Vec::new();
        for index in 0..64usize {
            skills.push(CatalogSkill {
                descriptor: SkillDescriptor {
                    skill_id: format!(
                        "wc_skill_{}",
                        webcodex_core::compact::encode(&(index as u128).to_be_bytes()[0..])
                    ),
                    name: format!("skill-{index:02}"),
                    description: format!("descriptor-{index:02}-{}", "d".repeat(420)),
                    definition_revision: format!("{index:064x}"),
                    package_revision: None,
                    source_scope: "project",
                    trust: "project_content",
                    name_conflict: false,
                },
                order_key: format!("package-{index:02}"),
            });
        }
        let sources = vec![
            project_skill_source_summary(skills.len(), 0, false),
            runner_skill_source_summary("configured_runner_roots", 0, 0, false, false),
            runner_skill_source_summary("managed_runner_store", 0, 0, false, false),
        ];
        let catalog_revision = catalog_revision(&skills, 0, &[], false, &sources);
        let catalog = SkillCatalog {
            catalog_revision,
            skills,
            invalid_count: 0,
            diagnostics: Vec::new(),
            discovery_truncated: false,
            sources,
        };
        let page = catalog.page_value(
            "agent:test:project",
            None,
            0,
            MAX_SKILL_LIST_LIMIT,
            MAX_SKILL_SIDECAR_CATALOG_BYTES,
        );
        assert!(serde_json::to_vec(&page).unwrap().len() <= MAX_SKILL_SIDECAR_CATALOG_BYTES);
        assert_eq!(page["total_count"], 64);
        assert_eq!(page["truncated"], true);
        let next = page["next_offset"].as_u64().unwrap() as usize;
        assert!(next > 0 && next < 64);

        let explicit = catalog.page_value(
            "agent:test:project",
            None,
            next,
            MAX_SKILL_LIST_LIMIT,
            MAX_SKILL_CATALOG_RESULT_BYTES,
        );
        assert!(serde_json::to_vec(&explicit).unwrap().len() <= MAX_SKILL_CATALOG_RESULT_BYTES);
        assert_eq!(explicit["offset"], next);
    }

    #[test]
    fn catalog_projection_explains_empty_source_participation_and_revisions() {
        let skills = Vec::new();
        let unavailable_sources = vec![
            project_skill_source_summary(0, 0, false),
            runner_skill_source_summary("configured_runner_roots", 0, 0, false, false),
            runner_skill_source_summary("managed_runner_store", 0, 0, false, false),
        ];
        let available_sources = vec![
            project_skill_source_summary(0, 0, false),
            runner_skill_source_summary("configured_runner_roots", 0, 0, false, true),
            runner_skill_source_summary("managed_runner_store", 0, 0, false, true),
        ];
        assert_ne!(
            catalog_revision(&skills, 0, &[], false, &unavailable_sources),
            catalog_revision(&skills, 0, &[], false, &available_sources),
            "source availability must fence the catalog revision even when every source is empty"
        );
        let catalog = SkillCatalog {
            catalog_revision: catalog_revision(&skills, 0, &[], false, &available_sources),
            skills,
            invalid_count: 0,
            diagnostics: Vec::new(),
            discovery_truncated: false,
            sources: available_sources,
        };
        let page = catalog.page_value(
            "agent:test:empty-skills",
            None,
            0,
            DEFAULT_SKILL_LIST_LIMIT,
            MAX_SKILL_CATALOG_RESULT_BYTES,
        );
        assert_eq!(page["skills"], json!([]));
        assert_eq!(page["sources"].as_array().unwrap().len(), 3);
        assert_eq!(page["sources"][0]["kind"], "project");
        assert_eq!(page["sources"][0]["status"], "available");
        assert_eq!(page["sources"][0]["root_hint"], SKILL_ROOT);
        assert_eq!(page["sources"][0]["skill_count"], 0);
        assert_eq!(page["sources"][1]["kind"], "configured_runner_roots");
        assert_eq!(page["sources"][1]["status"], "available");
        assert_eq!(page["sources"][1]["skill_count"], 0);
        assert!(page["sources"][1].get("root_hint").is_none());
        assert_eq!(page["sources"][2]["kind"], "managed_runner_store");
        assert_eq!(page["sources"][2]["status"], "available");
        assert_eq!(page["sources"][2]["skill_count"], 0);
    }

    #[test]
    fn runner_skill_admission_errors_keep_runtime_fail_closed_and_management_store_semantics() {
        let unsupported = EnqueueRunnerSkillError::UnsupportedCapability {
            client_id: "runner-a".to_string(),
            capability: "skill_runtime",
        };
        assert_eq!(
            runner_skill_enqueue_error_kind(&unsupported, false),
            "skill_catalog_unavailable"
        );
        assert_eq!(
            runner_skill_enqueue_error_kind(&unsupported, true),
            "skill_store_capability_unavailable"
        );

        let changed = EnqueueRunnerSkillError::RunnerChanged {
            client_id: "runner-a".to_string(),
        };
        assert_eq!(
            runner_skill_enqueue_error_kind(&changed, false),
            "skill_catalog_unavailable"
        );
        assert_eq!(
            runner_skill_enqueue_error_kind(&changed, true),
            "skill_store_runner_changed"
        );

        let unavailable = EnqueueRunnerSkillError::DispatchUnavailable {
            message: "too many pending requests".to_string(),
        };
        assert_eq!(
            runner_skill_enqueue_error_kind(&unavailable, true),
            "skill_store_runner_unavailable"
        );
        assert_eq!(
            stable_skill_store_error(Some(
                "skill_capability_unavailable: exact Runner no longer advertises skill_management before dispatch"
            )),
            "skill_store_capability_unavailable"
        );
    }

    #[test]
    fn uncertain_skill_store_errors_have_same_key_reconciliation_path() {
        let assert_actionable = |result: &ToolResult| {
            let suggested = &result.output["suggested_call"];
            assert_eq!(suggested["tool"], "skill_versions");
            assert_eq!(
                suggested["arguments"],
                json!({"project": "agent:test:demo", "skill_key": "demo"})
            );
            assert!(crate::tool_runtime::ToolCall::from_tool_name(
                suggested["tool"].as_str().expect("suggested Skill tool"),
                suggested["arguments"].clone(),
            )
            .is_ok());
            assert!(result.output.get("recovery_tool").is_none());
            assert!(result.output.get("reconcile_with").is_none());
            assert!(result.output.get("recovery_kind").is_none());
            let mut projected = ToolResult::err_with_output("recovery", result.output.clone());
            crate::model_surface::project_tool_result_suggested_calls(
                "skill_install",
                &mut projected,
                &|target| crate::model_surface::suggested_tool_call_route(target, true),
            );
            let carrier = &projected.output["suggested_call"];
            assert_eq!(
                carrier["tool"],
                crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
            );
            assert_eq!(carrier["arguments"]["tool"], "skill_versions");
            assert_eq!(carrier["arguments"]["arguments"], suggested["arguments"]);
        };

        let unknown = skill_error_dynamic(
            "skill_store_outcome_unknown",
            "agent:test:demo",
            Some(json!({"skill_key": "demo"})),
            true,
        );
        assert_eq!(unknown.output["outcome_unknown"], true);
        assert_actionable(&unknown);
        assert_eq!(unknown.output["retry_same_idempotency_key"], true);
        assert!(!unknown.output.to_string().contains("new key"));

        assert!(uncertain_skill_store_error(
            "skill_store_replay_commit_failed"
        ));
        let commit_failed = skill_error_dynamic(
            "skill_store_replay_commit_failed",
            "agent:test:demo",
            Some(json!({"skill_key": "demo"})),
            true,
        );
        assert_eq!(commit_failed.output["outcome_unknown"], true);
        assert_actionable(&commit_failed);
        assert_eq!(commit_failed.output["retry_same_idempotency_key"], true);

        assert!(!uncertain_skill_store_error(
            "skill_store_replay_capacity_exceeded"
        ));
        let capacity = skill_error_dynamic(
            "skill_store_replay_capacity_exceeded",
            "agent:test:demo",
            Some(json!({"skill_key": "demo"})),
            false,
        );
        assert_eq!(capacity.output["outcome_unknown"], false);
        assert_eq!(capacity.output["state_changed"], false);
        assert!(capacity.output.get("recovery_kind").is_none());
        assert!(capacity.output.get("retry_same_idempotency_key").is_none());

        let claimed = skill_error_dynamic(
            "skill_install_reconcile_required",
            "agent:test:demo",
            Some(json!({"skill_key": "demo"})),
            false,
        );
        assert_eq!(claimed.output["outcome_unknown"], false);
        assert_actionable(&claimed);
        assert!(claimed.output.get("retry_same_idempotency_key").is_none());

        let family_only =
            skill_error_dynamic("skill_store_outcome_unknown", "agent:test:demo", None, true);
        assert_eq!(family_only.output["recovery_kind"], "reconcile");
        assert_eq!(family_only.output["reconcile_with"], "skill_versions");
        assert!(family_only.output.get("suggested_call").is_none());
        assert!(family_only.output.get("recovery_tool").is_none());
        assert_eq!(family_only.output["retry_same_idempotency_key"], true);
    }

    #[test]
    fn skill_resource_interpreter_is_runner_owned_and_extension_scoped() {
        assert_eq!(
            validate_skill_resource_interpreter("scripts/probe.py"),
            Ok(())
        );
        assert_eq!(
            validate_skill_resource_interpreter("scripts/probe.PY"),
            Ok(())
        );
        assert_eq!(
            validate_skill_resource_interpreter("scripts/probe.sh"),
            Ok(())
        );
        assert_eq!(
            validate_skill_resource_interpreter("scripts/probe.rb"),
            Err("skill_resource_interpreter_unsupported")
        );
    }

    #[test]
    fn resource_paths_are_package_relative_and_cross_platform_safe() {
        assert_eq!(
            validate_resource_path("references/foo.md").unwrap(),
            "references/foo.md"
        );
        for refused in [
            "../x",
            "references/../x",
            "/tmp/x",
            "C:\\tmp\\x",
            "a\\..\\x",
            ".env/../x",
            "a//b",
            "./x",
        ] {
            assert!(validate_resource_path(refused).is_err(), "{refused}");
        }
    }
}
