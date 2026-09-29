//! Typed Job inventory shared by model and Console projections.
//!
//! Select and bound records before presentation. This is the existing inventory
//! query (the Registry refreshes matching public Jobs), not the passive snapshot
//! reader used by Work Result/attention. Callers still own endpoint scope checks;
//! Registry visibility is applied before filters, ordering and limits.
use crate::auth::AuthContext;
use webcodex_core::runner_protocol::ShellJobInfo;

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

impl ToolRuntime {
    pub(crate) async fn query_job_inventory_for_auth(
        &self,
        limit: Option<usize>,
        status: Option<&str>,
        project: Option<&str>,
        session_id: Option<&str>,
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
