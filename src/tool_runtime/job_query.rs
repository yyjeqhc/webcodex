//! Typed Job inventory and status shared by projections and validation evidence.
//!
//! Select and bound records before presentation. This is the existing inventory
//! query (the Registry refreshes matching public Jobs), not the passive snapshot
//! reader used by Work Result/attention. Callers still own endpoint scope checks;
//! Registry visibility is applied before filters, ordering and limits.
use super::jobs::{
    agent_log_stream_incomplete, annotate_project_validation, validation_job_result_with_policy,
    JobValidationResult,
};
use crate::auth::AuthContext;
use serde_json::json;
use webcodex_core::runner_protocol::ShellJobInfo;
use webcodex_core::workflow_session_contract::is_validation_like_execution_purpose;

use super::{ToolResult, ToolRuntime};

/// Internal records deliberately do not implement Serialize: adapters must use
/// their own bounded allowlist, never expose command previews/private metadata.
pub(crate) struct JobInventoryPage {
    pub(crate) jobs: Vec<ShellJobInfo>,
    pub(crate) matched_count: usize,
}

impl JobInventoryPage {
    pub(crate) fn truncated(&self) -> bool {
        self.matched_count > self.jobs.len()
    }
}

fn normalize_filter<'a>(
    value: Option<&'a str>,
    max_chars: usize,
    error_kind: &str,
    message: &str,
) -> Result<Option<&'a str>, ToolResult> {
    value
        .map(|value| {
            let value = value.trim();
            if value.is_empty() || value.chars().count() > max_chars {
                Err(super::jobs::invalid_job_observation_result(
                    error_kind,
                    message.to_string(),
                ))
            } else {
                Ok(value)
            }
        })
        .transpose()
}

/// Not serializable: callers project their own allowlist instead of exposing
/// private Job metadata. The validation verdict remains typed until projection.
pub(super) struct JobStatusObservation {
    pub(super) job: ShellJobInfo,
    pub(super) validation: Option<JobValidationResult>,
}

impl ToolRuntime {
    /// Refresh the same caller-visible Job and its bounded validation evidence.
    /// Tool output formatting is downstream; reconciliation consumes typed facts.
    pub(super) async fn query_job_status_for_auth(
        &self,
        job_id: &str,
        auth: Option<&AuthContext>,
    ) -> Result<JobStatusObservation, String> {
        let job = self
            .runner_registry
            .get_job_for_auth(
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
                job_id,
            )
            .await?;
        let status = job.status.as_str();
        let validation_metadata = job.validation.as_ref();
        let tool = validation_metadata.map(|metadata| metadata.adapter.as_str());
        let kind = validation_metadata.map(|metadata| metadata.kind.as_str());
        if tool.is_some() {
            let logs = self
                .runner_registry
                .job_log_for_auth(
                    crate::runner_http::runner_access_from_auth(auth).as_ref(),
                    job_id,
                    None,
                    None,
                    Some(500),
                    None,
                    None,
                )
                .await;
            let (stdout, stderr, truncated) = match logs {
                Ok((logged_job, stdout, stderr, next_stdout_line, next_stderr_line, _)) => {
                    let stdout = stdout.unwrap_or_default();
                    let stderr = stderr.unwrap_or_default();
                    let truncated = agent_log_stream_incomplete(
                        logged_job.stdout_log_truncated,
                        logged_job.stdout_retained_from_line,
                        stdout.lines().count(),
                        next_stdout_line,
                    ) || agent_log_stream_incomplete(
                        logged_job.stderr_log_truncated,
                        logged_job.stderr_retained_from_line,
                        stderr.lines().count(),
                        next_stderr_line,
                    );
                    (stdout, stderr, truncated)
                }
                Err(_) => (String::new(), String::new(), true),
            };
            if let Some(mut validation) = validation_job_result_with_policy(
                tool,
                kind,
                status,
                job.exit_code.map(i64::from),
                &stdout,
                &stderr,
                truncated,
                job.test_count_evidence.as_ref(),
                validation_metadata.and_then(|metadata| metadata.minimum_tests),
                validation_metadata.and_then(|metadata| metadata.require_tests),
                validation_metadata.and_then(|metadata| metadata.no_run),
            ) {
                let details = validation.details_mut();
                if let Some(target_id) = validation_metadata
                    .and_then(|metadata| metadata.validation_target_id.as_deref())
                {
                    details["validation_target_id"] = json!(target_id);
                }
                details["source_state"] = json!(self.validation_sources.observe(
                    job.project_id.as_deref().unwrap_or_default(),
                    validation_metadata.and_then(|metadata| metadata.source_fence.as_ref()),
                ));
                annotate_project_validation(details, validation_metadata);
                return Ok(JobStatusObservation {
                    job,
                    validation: Some(validation),
                });
            }
        }
        Ok(JobStatusObservation {
            job,
            validation: None,
        })
    }

    /// Keep the complete authorized candidate set: absence drives retention
    /// marker eviction, so model inventory pagination must never apply here.
    pub(crate) async fn validation_job_candidates_for_sessions(
        &self,
        project: &str,
        session_ids: &[String],
        auth: Option<&AuthContext>,
    ) -> std::collections::HashMap<String, Vec<ShellJobInfo>> {
        let requested = session_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        let mut grouped = std::collections::HashMap::<String, Vec<ShellJobInfo>>::new();
        if requested.is_empty() {
            return grouped;
        }
        for job in self
            .runner_registry
            .list_all_jobs_for_auth(crate::runner_http::runner_access_from_auth(auth).as_ref())
            .await
        {
            let Some(session_id) = job.session_id.as_deref() else {
                continue;
            };
            let generic_validation = job
                .structured_execution
                .as_ref()
                .and_then(|metadata| metadata.validation_identity.as_deref())
                .is_some()
                && job
                    .purpose
                    .as_deref()
                    .is_some_and(is_validation_like_execution_purpose);
            if job.project_id.as_deref() == Some(project)
                && requested.contains(session_id)
                && (job.validation.is_some() || generic_validation)
            {
                grouped.entry(session_id.to_string()).or_default().push(job);
            }
        }
        for jobs in grouped.values_mut() {
            jobs.sort_by(|a, b| {
                b.created_at
                    .cmp(&a.created_at)
                    .then_with(|| a.job_id.cmp(&b.job_id))
            });
        }
        grouped
    }

    pub(crate) async fn query_job_inventory_for_auth(
        &self,
        limit: Option<usize>,
        status: Option<&str>,
        project: Option<&str>,
        session_id: Option<&str>,
        client_id: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> Result<JobInventoryPage, ToolResult> {
        let max = limit.unwrap_or(20).clamp(1, 100);
        let status = status.map(str::trim).filter(|value| !value.is_empty());
        let project = normalize_filter(
            project,
            512,
            "invalid_project_filter",
            "invalid_project_filter: project must contain 1..=512 characters",
        )?;
        let session_id = normalize_filter(
            session_id,
            128,
            "invalid_session_filter",
            "invalid_session_filter: session_id must contain 1..=128 characters",
        )?;
        let mut jobs = self
            .runner_registry
            .list_jobs_for_auth_filtered(
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
                client_id,
                project,
                session_id,
            )
            .await;
        jobs.retain(|job| status.is_none_or(|status| status == job.status));
        jobs.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.job_id.cmp(&b.job_id))
        });
        let matched_count = jobs.len();
        jobs.truncate(max);
        Ok(JobInventoryPage {
            jobs,
            matched_count,
        })
    }
}
