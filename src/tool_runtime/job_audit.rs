//! Content-free bounded Job correlation for ActionAudit.
//!
//! This projection is derived only from Server-held canonical Job state. It is
//! never model-facing, never execution authority, and never performs Runner I/O.

use super::{ToolCallCorrelation, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde::Serialize;
use serde_json::{json, Value};
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;
use webcodex_core::runner_protocol::{ShellJobActivity, ShellJobInfo};
use webcodex_core::workflow_session_contract::ExecutionPurpose;

pub(crate) const MAX_JOB_AUDIT_ITEMS: usize = 8;
const MAX_JOB_AUDIT_ELAPSED_SECS: u64 = 7 * 24 * 60 * 60;
const MAX_AUDIT_CLIENT_ID_CHARS: usize = 128;
const MAX_AUDIT_PROJECT_ID_CHARS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct JobAuditItem {
    pub(crate) job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) execution_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) purpose: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) validation_tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) validation_adapter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) elapsed_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) activity: Option<ShellJobActivity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct JobAuditTrace {
    pub(crate) requested_jobs: usize,
    pub(crate) metadata_unavailable_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) wait_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) wait_outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) waited_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ready_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pending_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) returned_count: Option<usize>,
    pub(crate) jobs: Vec<JobAuditItem>,
}

fn bounded_identity(value: &str, max_chars: usize) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control))
    .then(|| value.to_string())
}

fn safe_execution_source(job: &ShellJobInfo) -> Option<String> {
    if let Some(metadata) = job
        .structured_execution
        .as_ref()
        .filter(|metadata| metadata.is_valid())
    {
        return Some(metadata.execution_source.clone());
    }
    if let Some(validation) = job
        .validation
        .as_ref()
        .filter(|metadata| metadata.is_valid())
    {
        return Some(validation.tool.clone());
    }
    match job.kind.as_str() {
        "run_shell" | "run_job" | "shell" | "cargo_fmt" | "cargo_check" | "cargo_test"
        | "go_test" | "project_validate" | "project_build" => Some(job.kind.clone()),
        _ => None,
    }
}

fn safe_kind(job: &ShellJobInfo, execution_source: Option<&str>) -> Option<String> {
    if let Some(validation) = job
        .validation
        .as_ref()
        .filter(|metadata| metadata.is_valid())
    {
        return match validation.kind.as_str() {
            "format" | "check" | "test" => Some(validation.kind.clone()),
            _ => None,
        };
    }
    match execution_source {
        Some("run_process" | "run_detached_process" | "run_process_interactive") => {
            Some("process".to_string())
        }
        Some("run_script") => Some("script".to_string()),
        Some("project_build") => Some("build".to_string()),
        Some("run_shell" | "run_job" | "shell") => Some("shell".to_string()),
        Some("cargo_fmt") => Some("format".to_string()),
        Some("cargo_check") => Some("check".to_string()),
        Some("cargo_test" | "go_test") => Some("test".to_string()),
        _ => None,
    }
}

fn safe_status(status: &str) -> Option<String> {
    (status == "recovering" || RunnerJobLifecycle::from_wire(status).is_ok())
        .then(|| status.to_string())
}

fn safe_purpose(value: Option<&str>) -> Option<String> {
    ExecutionPurpose::parse(value?).map(|purpose| purpose.as_str().to_string())
}

fn job_audit_item(job: &ShellJobInfo) -> JobAuditItem {
    let execution_source = safe_execution_source(job);
    let validation = job
        .validation
        .as_ref()
        .filter(|metadata| metadata.is_valid());
    JobAuditItem {
        job_id: job.job_id.clone(),
        client_id: bounded_identity(&job.client_id, MAX_AUDIT_CLIENT_ID_CHARS),
        project: job
            .project_id
            .as_deref()
            .and_then(|project| bounded_identity(project, MAX_AUDIT_PROJECT_ID_CHARS)),
        kind: safe_kind(job, execution_source.as_deref()),
        execution_source,
        purpose: safe_purpose(job.purpose.as_deref()),
        validation_tool: validation.map(|metadata| metadata.tool.clone()),
        validation_adapter: validation.map(|metadata| metadata.adapter.clone()),
        status: safe_status(&job.status),
        elapsed_secs: job
            .elapsed_secs
            .map(|elapsed| elapsed.min(MAX_JOB_AUDIT_ELAPSED_SECS)),
        activity: job.activity.filter(|activity| activity.is_canonical()),
    }
}

fn safe_job_id(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|job_id| webcodex_core::workflow_session_contract::is_safe_job_id(job_id))
        .map(str::to_string)
}

fn push_unique_job_id(ids: &mut Vec<String>, value: Option<&Value>) {
    if ids.len() >= MAX_JOB_AUDIT_ITEMS {
        return;
    }
    if let Some(id) = safe_job_id(value) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
}

pub(crate) fn execution_job_id_for_audit(
    tool_name: Option<&str>,
    output: &Value,
) -> Option<String> {
    let tool_name = tool_name?;
    if !matches!(
        tool_name,
        "run_job"
            | "run_shell"
            | "run_process"
            | "run_detached_process"
            | "run_script"
            | "run_skill_resource"
            | "project_build"
            | "project_validate"
            | "cargo_fmt"
            | "cargo_check"
            | "cargo_test"
            | "go_test"
    ) {
        return None;
    }
    if let Some(job_id) = safe_job_id(output.get("job_id")) {
        let state = output.get("execution_state").and_then(Value::as_str);
        let promoted = output
            .get("promoted_to_job")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if promoted
            || tool_name == "run_job"
            || matches!(state, Some("queued" | "started" | "running" | "pending"))
        {
            return Some(job_id);
        }
    }
    if !matches!(
        output.get("execution_state").and_then(Value::as_str),
        Some("queued" | "started" | "running" | "pending")
    ) || output.pointer("/continuation/tool").and_then(Value::as_str) != Some("observe_jobs")
    {
        return None;
    }
    let items = output
        .pointer("/continuation/arguments/items")
        .and_then(Value::as_array)?;
    (items.len() == 1)
        .then(|| safe_job_id(items[0].get("job_id")))
        .flatten()
}

pub(crate) fn observed_job_ids_for_audit(tool_name: Option<&str>, output: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    match tool_name {
        Some("wait_for_job_readiness") => {
            for value in output
                .get("ready")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|item| item.get("job_id"))
                .chain(
                    output
                        .get("pending_job_ids")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten(),
                )
            {
                push_unique_job_id(&mut ids, Some(value));
            }
        }
        Some("observe_jobs") => {
            if let Some(items) = output.get("items").and_then(Value::as_array) {
                for item in items.iter().take(MAX_JOB_AUDIT_ITEMS) {
                    let nested = item.get("output").filter(|value| value.is_object());
                    push_unique_job_id(
                        &mut ids,
                        item.get("job_id")
                            .or_else(|| nested.and_then(|output| output.get("job_id"))),
                    );
                }
            }
        }
        _ => {}
    }
    ids
}

pub(crate) fn action_audit_job_ids(tool_name: Option<&str>, output: &Value) -> Option<Value> {
    if let Some(job_id) = execution_job_id_for_audit(tool_name, output) {
        return Some(json!({"async_job_id": job_id}));
    }
    let observed = observed_job_ids_for_audit(tool_name, output);
    (!observed.is_empty()).then(|| json!({"observed_job_ids": observed}))
}

fn closed_wait_state(output: &Value) -> Option<String> {
    match output.get("wait_state").and_then(Value::as_str) {
        Some(value @ ("ready" | "deadline")) => Some(value.to_string()),
        _ => None,
    }
}

fn closed_wait_outcome(output: &Value) -> Option<String> {
    match output.pointer("/wait/outcome").and_then(Value::as_str) {
        Some(value @ ("immediate" | "updated" | "terminal" | "item_error" | "timeout")) => {
            Some(value.to_string())
        }
        _ => None,
    }
}

fn build_job_audit_trace(
    tool_name: &str,
    output: &Value,
    requested_ids: usize,
    jobs: Vec<ShellJobInfo>,
) -> JobAuditTrace {
    let jobs = jobs
        .iter()
        .take(MAX_JOB_AUDIT_ITEMS)
        .map(job_audit_item)
        .collect::<Vec<_>>();
    let requested_jobs = match tool_name {
        "observe_jobs" => output
            .get("requested_count")
            .and_then(Value::as_u64)
            .map(|count| count.min(MAX_JOB_AUDIT_ITEMS as u64) as usize)
            .unwrap_or(requested_ids),
        _ => requested_ids,
    }
    .min(MAX_JOB_AUDIT_ITEMS);
    let metadata_unavailable_count = requested_ids.saturating_sub(jobs.len());
    JobAuditTrace {
        requested_jobs,
        metadata_unavailable_count,
        wait_state: (tool_name == "wait_for_job_readiness")
            .then(|| closed_wait_state(output))
            .flatten(),
        wait_outcome: (tool_name == "observe_jobs")
            .then(|| closed_wait_outcome(output))
            .flatten(),
        waited_ms: match tool_name {
            "wait_for_job_readiness" => output.get("waited_ms").and_then(Value::as_u64),
            "observe_jobs" => output.pointer("/wait/waited_ms").and_then(Value::as_u64),
            _ => None,
        },
        ready_count: (tool_name == "wait_for_job_readiness")
            .then(|| output.get("ready").and_then(Value::as_array).map(Vec::len))
            .flatten(),
        pending_count: (tool_name == "wait_for_job_readiness")
            .then(|| {
                output
                    .get("pending_job_ids")
                    .and_then(Value::as_array)
                    .map(Vec::len)
            })
            .flatten(),
        returned_count: (tool_name == "observe_jobs")
            .then(|| {
                output
                    .get("returned_count")
                    .and_then(Value::as_u64)
                    .map(|count| count.min(MAX_JOB_AUDIT_ITEMS as u64) as usize)
            })
            .flatten(),
        jobs,
    }
}

impl ToolRuntime {
    pub(crate) async fn capture_job_audit_trace(
        &self,
        correlation: &mut ToolCallCorrelation,
        tool_name: &str,
        result: &ToolResult,
        auth: Option<&AuthContext>,
    ) {
        if !result.success || !matches!(tool_name, "wait_for_job_readiness" | "observe_jobs") {
            return;
        }
        let ids = observed_job_ids_for_audit(Some(tool_name), &result.output);
        let access = crate::runner_http::runner_access_from_auth(auth);
        let refs = ids.iter().map(String::as_str).collect::<Vec<_>>();
        let jobs = self
            .runner_registry
            .job_observability_views_for_auth(access.as_ref(), &refs)
            .await;
        correlation.job_audit = Some(build_job_audit_trace(
            tool_name,
            &result.output,
            ids.len(),
            jobs,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(value: Value) -> ShellJobInfo {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn pending_handoff_extracts_only_canonical_job_identity() {
        let pending = json!({
            "execution_state": "pending",
            "continuation": {
                "tool": "observe_jobs",
                "arguments": {"items": [{"job_id": "wc_job_safe_123"}]}
            }
        });
        assert_eq!(
            execution_job_id_for_audit(Some("run_process"), &pending).as_deref(),
            Some("wc_job_safe_123")
        );
        let mut poisoned = pending.clone();
        poisoned["continuation"]["arguments"]["items"][0]["job_id"] = json!("../unsafe");
        assert!(execution_job_id_for_audit(Some("run_process"), &poisoned).is_none());
    }

    #[test]
    fn job_projection_is_content_free_and_classifies_structured_validation() {
        let job = job(json!({
            "job_id":"wc_job_private_123",
            "client_id":"runner-a",
            "kind":"shell",
            "project_id":"agent:runner-a:repo",
            "cwd":"/private/path",
            "purpose":"test",
            "command_preview":"TOKEN=very-secret cargo test",
            "status":"running",
            "created_at":1,
            "elapsed_secs":287,
            "structured_execution":{
                "execution_source":"run_process",
                "arg_count":3,
                "stdin_present":false,
                "validation_identity":"assertion:0123456789abcdef01234567",
                "assertion_name":"PRIVATE ASSERTION",
                "validation_tool":"cargo_test"
            },
            "validation":{
                "tool":"cargo_test",
                "kind":"test",
                "steps":[{"name":"test","program":"cargo","args":["test"],"env":[]}],
                "effective_timeout_secs":600,
                "sync_wait_secs":10,
                "adapter":"cargo_test",
                "validation_target_id":"target:0123456789abcdef01234567"
            },
            "activity":{"state":"working","phase":"validation_test","source":"validation_plan"},
            "recovered_after_server_restart":false,
            "stdout_log_truncated":false,
            "stderr_log_truncated":false
        }));
        let item = job_audit_item(&job);
        let encoded = serde_json::to_string(&item).unwrap();
        assert_eq!(item.execution_source.as_deref(), Some("run_process"));
        assert_eq!(item.kind.as_deref(), Some("test"));
        assert_eq!(item.purpose.as_deref(), Some("test"));
        assert_eq!(item.validation_tool.as_deref(), Some("cargo_test"));
        assert_eq!(item.validation_adapter.as_deref(), Some("cargo_test"));
        for secret in [
            "very-secret",
            "PRIVATE",
            "/private/path",
            "0123456789abcdef01234567",
            "command_preview",
            "cwd",
        ] {
            assert!(!encoded.contains(secret), "{secret} leaked: {encoded}");
        }
    }

    #[test]
    fn generic_shell_and_arbitrary_purpose_remain_coarse() {
        let job = job(json!({
            "job_id":"wc_job_shell_123",
            "client_id":"runner-a",
            "kind":"run_shell",
            "purpose":"PRIVATE arbitrary purpose",
            "command_preview":"echo PRIVATE_COMMAND",
            "status":"running",
            "created_at":1,
            "recovered_after_server_restart":false,
            "stdout_log_truncated":false,
            "stderr_log_truncated":false
        }));
        let item = job_audit_item(&job);
        assert_eq!(item.execution_source.as_deref(), Some("run_shell"));
        assert_eq!(item.kind.as_deref(), Some("shell"));
        assert!(item.purpose.is_none());
        assert!(!serde_json::to_string(&item).unwrap().contains("PRIVATE"));
    }

    #[tokio::test]
    async fn missing_job_metadata_is_fail_soft_and_preserves_readiness_semantics() {
        let runtime = ToolRuntime::new_for_tests();
        let result = ToolResult::ok(json!({
            "wait_state":"deadline",
            "mode":"all",
            "waited_ms":45_000,
            "ready":[],
            "pending_job_ids":["wc_job_missing_123"]
        }));
        let before_success = result.success;
        let before_output = result.output.clone();
        let mut correlation = ToolCallCorrelation::default();
        runtime
            .capture_job_audit_trace(&mut correlation, "wait_for_job_readiness", &result, None)
            .await;
        assert_eq!(result.success, before_success);
        assert_eq!(
            result.output, before_output,
            "audit enrichment must not mutate ToolResult"
        );
        let trace = correlation.job_audit.expect("bounded audit trace");
        assert_eq!(trace.requested_jobs, 1);
        assert_eq!(trace.metadata_unavailable_count, 1);
        assert_eq!(trace.wait_state.as_deref(), Some("deadline"));
        assert_eq!(trace.waited_ms, Some(45_000));
        assert_eq!(trace.ready_count, Some(0));
        assert_eq!(trace.pending_count, Some(1));
        assert!(trace.jobs.is_empty());
    }

    #[test]
    fn observed_job_id_projection_is_deduplicated_and_bounded() {
        let items = (0..20)
            .map(|index| json!({"job_id": format!("wc_job_{index:02}_safe")}))
            .collect::<Vec<_>>();
        let ids = observed_job_ids_for_audit(Some("observe_jobs"), &json!({"items": items}));
        assert_eq!(ids.len(), MAX_JOB_AUDIT_ITEMS);
    }
}
