//! Privacy-bounded telemetry projection for model-visible runtime tool calls.
//!
//! This module owns no persistence. The shared tool kernel measures invocation
//! latency and transports finalize the record from the exact model-facing
//! `ToolResult` projection before attaching it to the existing Action Audit row.

pub(crate) mod job_convergence;

use super::edit_tool_telemetry::{edit_tool_surface, EditToolSurface};
use super::tool_definition::model_visible_tool_definitions;
use super::{ToolResult, RECOVERY_KIND_VALUES};
use crate::json_measurement::serialized_json_len;
use crate::mcp_host::McpHostRuntimePolicy;
use serde::Serialize;
use serde_json::Value;
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;
use webcodex_tool_contracts::tool_inputs::CodingGuidanceProfile;

const MAX_STRUCTURED_KIND_BYTES: usize = 64;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WorkOnProjectSource {
    Project,
    Path,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WorkOnProjectMode {
    Checkout,
    Worktree,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WorkOnProjectGuidanceProfile {
    Direct,
    HostCodeMode,
    CodeMode,
    Invalid,
}

impl WorkOnProjectGuidanceProfile {
    fn explicit_request(self) -> Option<CodingGuidanceProfile> {
        match self {
            Self::Direct => Some(CodingGuidanceProfile::Direct),
            Self::HostCodeMode => Some(CodingGuidanceProfile::HostCodeMode),
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeMode => Some(CodingGuidanceProfile::CodeMode),
            #[cfg(not(feature = "experimental-code-mode"))]
            Self::CodeMode => None,
            Self::Invalid => None,
        }
    }

    fn from_effective(profile: CodingGuidanceProfile) -> Self {
        match profile {
            CodingGuidanceProfile::Direct => Self::Direct,
            CodingGuidanceProfile::HostCodeMode => Self::HostCodeMode,
            #[cfg(feature = "experimental-code-mode")]
            CodingGuidanceProfile::CodeMode => Self::CodeMode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct WorkOnProjectErgonomicsFacts {
    resume_requested: bool,
    source: WorkOnProjectSource,
    mode: WorkOnProjectMode,
    mode_explicit: bool,
    base_ref_present: bool,
    guidance_profile: WorkOnProjectGuidanceProfile,
    guidance_profile_explicit: bool,
    include_extension_catalog: Option<bool>,
    include_extension_catalog_explicit: bool,
}

#[derive(Debug)]
pub(crate) struct ModelErgonomicsTimer {
    tool_name: &'static str,
    tool_category: &'static str,
    started: Instant,
    finish_summary_only: Option<bool>,
    work_on_project: Option<WorkOnProjectErgonomicsFacts>,
    bulk_exact_requested: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ModelErgonomicsCompletion {
    tool_name: &'static str,
    tool_category: &'static str,
    duration_ms: u64,
    finish_summary_only: Option<bool>,
    work_on_project: Option<WorkOnProjectErgonomicsFacts>,
    bulk_exact_requested: bool,
    pub(crate) job_convergence: Option<job_convergence::JobConvergenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ModelErgonomicsRecord {
    pub(crate) schema_version: u8,
    pub(crate) tool_name: &'static str,
    pub(crate) tool_category: &'static str,
    pub(crate) success: bool,
    pub(crate) duration_ms: u64,
    pub(crate) serialized_result_bytes: Option<u64>,
    pub(crate) error_kind: Option<String>,
    pub(crate) failure_kind: Option<String>,
    pub(crate) recovery_kind: Option<String>,
    pub(crate) execution_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) finish_summary_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) edit_surface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) edit_outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) edit_conflict_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bulk_exact_outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bulk_exact_match_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) work_on_project: Option<WorkOnProjectErgonomicsFacts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) job_convergence: Option<job_convergence::JobConvergenceRecord>,
}

impl ModelErgonomicsRecord {
    pub(crate) fn outcome_class(&self) -> &'static str {
        if self.execution_state.as_deref() == Some("outcome_unknown")
            || self.error_kind.as_deref() == Some("dispatch_hard_timeout")
        {
            "unknown"
        } else if self.success {
            "success"
        } else {
            "failure"
        }
    }
}

impl ModelErgonomicsTimer {
    pub(crate) fn resolve_work_on_project_guidance_profile(
        &mut self,
        policy: McpHostRuntimePolicy,
        mcp_transport: bool,
    ) {
        let Some(facts) = self.work_on_project.as_mut() else {
            return;
        };
        if facts.guidance_profile == WorkOnProjectGuidanceProfile::Invalid {
            return;
        }
        let requested = facts
            .guidance_profile_explicit
            .then(|| facts.guidance_profile.explicit_request())
            .flatten();
        facts.guidance_profile = WorkOnProjectGuidanceProfile::from_effective(
            policy.effective_guidance_profile(requested, mcp_transport),
        );
    }

    pub(crate) fn start(tool_name: &str) -> Option<Self> {
        Self::start_with_arguments(tool_name, &Value::Null)
    }

    pub(crate) fn start_with_arguments(tool_name: &str, arguments: &Value) -> Option<Self> {
        let definition =
            model_visible_tool_definitions().find(|definition| definition.name == tool_name)?;
        let finish_summary_only = (tool_name == "finish_coding_task").then(|| {
            arguments
                .get("summary_only")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
        let work_on_project = work_on_project_facts(tool_name, arguments);
        let bulk_exact_requested = tool_name == "apply_text_edits"
            && arguments
                .get("changes")
                .and_then(Value::as_array)
                .is_some_and(|changes| {
                    changes.iter().any(|change| {
                        change
                            .get("edits")
                            .and_then(Value::as_array)
                            .is_some_and(|edits| {
                                edits
                                    .iter()
                                    .any(|edit| edit.get("expected_match_count").is_some())
                            })
                    })
                });
        Some(Self {
            tool_name: definition.name,
            tool_category: definition.category,
            started: Instant::now(),

            finish_summary_only,
            work_on_project,
            bulk_exact_requested,
        })
    }

    pub(crate) fn finish(self) -> ModelErgonomicsCompletion {
        let elapsed = self.started.elapsed();
        ModelErgonomicsCompletion {
            tool_name: self.tool_name,
            tool_category: self.tool_category,
            duration_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,

            finish_summary_only: self.finish_summary_only,
            work_on_project: self.work_on_project,
            bulk_exact_requested: self.bulk_exact_requested,
            job_convergence: None,
        }
    }

    #[cfg(test)]
    fn finish_after(self, elapsed: Duration) -> ModelErgonomicsCompletion {
        ModelErgonomicsCompletion {
            tool_name: self.tool_name,
            tool_category: self.tool_category,
            duration_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,

            finish_summary_only: self.finish_summary_only,
            work_on_project: self.work_on_project,
            bulk_exact_requested: self.bulk_exact_requested,
            job_convergence: None,
        }
    }
}

impl ModelErgonomicsCompletion {
    /// Finalize telemetry from the exact `ToolResult` value rendered by the API
    /// transport. Serialization failure drops telemetry rather than affecting
    /// the tool outcome.
    pub(crate) fn record_for_tool_result(
        &self,
        result: &ToolResult,
    ) -> Option<ModelErgonomicsRecord> {
        let serialized_result_bytes = serialized_json_len(result).ok()?;
        Some(self.record_from_parts(
            result.success,
            &result.output,
            Some(serialized_result_bytes),
        ))
    }

    /// Finalize telemetry from MCP `structuredContent`, which is the final
    /// model-facing ToolResult projection after MCP-only image/resource framing.
    /// The MCP content blocks and JSON-RPC envelope are intentionally excluded.
    pub(crate) fn record_for_structured_content(
        &self,
        structured_content: &Value,
    ) -> Option<ModelErgonomicsRecord> {
        let success = structured_content.get("success")?.as_bool()?;
        let output = structured_content.get("output")?;
        let serialized_result_bytes = serialized_json_len(structured_content).ok()?;
        Some(self.record_from_parts(success, output, Some(serialized_result_bytes)))
    }

    /// Record an invocation after a model-visible tool identity exists but no
    /// ToolResult is available. The byte field is intentionally null rather than
    /// measuring a transport-specific error envelope.
    pub(crate) fn record_for_pre_result_failure(
        &self,
        error_kind: &'static str,
    ) -> ModelErgonomicsRecord {
        let mut record = self.record_from_parts(false, &Value::Null, None);
        record.error_kind = Some(error_kind.to_string());
        // The MCP outer hard timeout fires after dispatch and explicitly leaves
        // terminal tool state unknown. A structured/patch edit therefore cannot be
        // projected as a definite rejection merely because no ToolResult was
        // available to classify.
        if error_kind == "dispatch_hard_timeout"
            && record.edit_surface.as_deref() == Some("structured_or_patch")
        {
            record.edit_outcome = Some("uncertain".to_string());
        }
        record
    }

    fn record_from_parts(
        &self,
        success: bool,
        output: &Value,
        serialized_result_bytes: Option<usize>,
    ) -> ModelErgonomicsRecord {
        let (error_kind, failure_kind, recovery_kind) = if success {
            (None, None, None)
        } else {
            (
                structured_kind(output, "error_kind"),
                structured_kind(output, "failure_kind"),
                recovery_kind(output),
            )
        };
        let edit = edit_facts(self.tool_name, success, output);
        let edit_uncertain = edit.outcome.as_deref() == Some("uncertain");
        ModelErgonomicsRecord {
            schema_version: 10,
            tool_name: self.tool_name,
            tool_category: self.tool_category,
            success,
            duration_ms: self.duration_ms,
            serialized_result_bytes: serialized_result_bytes
                .map(|bytes| bytes.min(u64::MAX as usize) as u64),
            error_kind,
            failure_kind,
            recovery_kind,
            execution_state: execution_state(output),
            finish_summary_only: self.finish_summary_only,
            edit_surface: edit.surface,
            edit_outcome: edit.outcome,
            edit_conflict_kind: edit.conflict_kind,
            bulk_exact_outcome: self.bulk_exact_requested.then(|| {
                if success {
                    if output.get("dry_run").and_then(Value::as_bool) == Some(true) {
                        "dry_run"
                    } else {
                        "success"
                    }
                } else if edit_uncertain {
                    "uncertain"
                } else {
                    "rejection"
                }
                .to_string()
            }),
            bulk_exact_match_total: if self.bulk_exact_requested {
                if success {
                    Some(
                        output
                            .get("files")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .flat_map(|file| {
                                file.get("edits")
                                    .and_then(Value::as_array)
                                    .into_iter()
                                    .flatten()
                            })
                            .filter_map(|edit| {
                                edit.get("expected_match_count")
                                    .and_then(Value::as_u64)
                                    .and_then(|_| edit.get("match_count").and_then(Value::as_u64))
                            })
                            .sum::<u64>()
                            .min(327_680),
                    )
                } else {
                    output
                        .get("actual_match_count")
                        .and_then(Value::as_u64)
                        .map(|count| count.min(327_680))
                }
            } else {
                None
            },
            work_on_project: self.work_on_project,
            job_convergence: self.job_convergence.clone().or_else(|| {
                matches!(self.tool_name, "wait_for_job_terminal" | "observe_jobs").then(|| {
                    job_convergence::JobConvergenceRecord {
                        wait_for_job_terminal_count: u8::from(
                            self.tool_name == "wait_for_job_terminal",
                        ),
                        ..Default::default()
                    }
                })
            }),
        }
    }
}

fn work_on_project_facts(
    tool_name: &str,
    arguments: &Value,
) -> Option<WorkOnProjectErgonomicsFacts> {
    if tool_name != "work_on_project" {
        return None;
    }
    let object = arguments.as_object()?;
    let source = match (
        object.get("project"),
        object.get("client_id"),
        object.get("path"),
    ) {
        (Some(Value::String(project)), None, None) if !project.is_empty() => {
            WorkOnProjectSource::Project
        }
        (None, Some(Value::String(client_id)), Some(Value::String(path)))
            if !client_id.is_empty() && !path.is_empty() =>
        {
            WorkOnProjectSource::Path
        }
        _ => WorkOnProjectSource::Invalid,
    };
    let mode_explicit = object.contains_key("mode");
    let mode = match object.get("mode") {
        None => WorkOnProjectMode::Checkout,
        Some(Value::String(mode)) if mode == "checkout" => WorkOnProjectMode::Checkout,
        Some(Value::String(mode)) if mode == "worktree" => WorkOnProjectMode::Worktree,
        _ => WorkOnProjectMode::Invalid,
    };
    let guidance_profile_explicit = object.contains_key("guidance_profile");
    let guidance_profile = match object.get("guidance_profile") {
        None => WorkOnProjectGuidanceProfile::Direct,
        Some(Value::String(profile)) if profile == "direct" => WorkOnProjectGuidanceProfile::Direct,
        Some(Value::String(profile)) if profile == "host_code_mode" => {
            WorkOnProjectGuidanceProfile::HostCodeMode
        }
        Some(Value::String(profile))
            if cfg!(feature = "experimental-code-mode") && profile == "code_mode" =>
        {
            WorkOnProjectGuidanceProfile::CodeMode
        }
        _ => WorkOnProjectGuidanceProfile::Invalid,
    };
    let (include_extension_catalog, include_extension_catalog_explicit) =
        effective_default_true_boolean(object, "include_extension_catalog");
    Some(WorkOnProjectErgonomicsFacts {
        resume_requested: object.contains_key("session_id"),
        source,
        mode,
        mode_explicit,
        base_ref_present: object.contains_key("base_ref"),
        guidance_profile,
        guidance_profile_explicit,
        include_extension_catalog,
        include_extension_catalog_explicit,
    })
}

fn effective_default_true_boolean(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> (Option<bool>, bool) {
    match object.get(field) {
        None => (Some(true), false),
        Some(value) => (value.as_bool(), true),
    }
}

#[derive(Debug)]
struct EditFacts {
    surface: Option<String>,
    outcome: Option<String>,
    conflict_kind: Option<String>,
}

fn edit_facts(tool_name: &str, success: bool, output: &Value) -> EditFacts {
    let surface = edit_tool_surface(tool_name).map(|surface| surface.as_str().to_string());
    let mut facts = EditFacts {
        surface,
        outcome: None,
        conflict_kind: None,
    };
    match edit_tool_surface(tool_name) {
        Some(EditToolSurface::StructuredOrPatch)
            if matches!(tool_name, "apply_text_edits" | "apply_patch") =>
        {
            facts.conflict_kind = edit_conflict_kind(output);
            facts.outcome = if success {
                match (
                    output.get("dry_run").and_then(Value::as_bool),
                    output.get("changed").and_then(Value::as_bool),
                    output.get("would_change").and_then(Value::as_bool),
                ) {
                    (Some(true), _, Some(true)) => Some("dry_run_would_change".to_string()),
                    (Some(true), _, Some(false)) => Some("dry_run_no_change".to_string()),
                    (Some(false), Some(true), _) => Some("applied".to_string()),
                    (Some(false), Some(false), _) => Some("no_change".to_string()),
                    _ => None,
                }
            } else if output.get("execution_state").and_then(Value::as_str)
                == Some("outcome_unknown")
                || output.get("error_kind").and_then(Value::as_str) == Some("outcome_unknown")
                || output.get("rollback_complete").and_then(Value::as_bool) == Some(false)
                || output.get("changed").and_then(Value::as_bool) == Some(true)
            {
                Some("uncertain".to_string())
            } else if facts.conflict_kind.is_some() {
                Some("conflict".to_string())
            } else {
                Some("rejected".to_string())
            };
        }
        Some(EditToolSurface::StructuredOrPatch) if tool_name == "apply_unified_diff" => {
            let error_kind = output.get("error_kind").and_then(Value::as_str);
            facts.outcome = match (
                output.get("applied").and_then(Value::as_bool),
                output.get("policy_blocked").and_then(Value::as_bool),
                output.get("can_apply").and_then(Value::as_bool),
                error_kind,
            ) {
                (Some(true), _, _, _) => Some("applied".to_string()),
                (_, Some(true), _, _) => Some("policy_blocked".to_string()),
                (_, _, Some(false), Some("not_applicable")) => Some("not_applicable".to_string()),
                (
                    _,
                    _,
                    _,
                    Some("unsupported_diff_format" | "invalid_unified_diff" | "diff_too_large"),
                ) => Some("malformed".to_string()),
                (_, _, _, Some("outcome_unknown")) => Some("uncertain".to_string()),
                (_, _, Some(true), Some("apply_failed" | "apply_not_started")) => {
                    Some("apply_failed".to_string())
                }
                _ if !success => Some("rejected".to_string()),
                _ => None,
            };
        }
        _ => {}
    }
    facts
}

fn edit_conflict_kind(output: &Value) -> Option<String> {
    let value = output.get("error_kind").and_then(Value::as_str)?;
    matches!(
        value,
        "multiple_matches"
            | "match_count_mismatch"
            | "match_not_found"
            | "occurrence_out_of_range"
            | "occurrence_outside_line_scope"
            | "overlapping_edits"
            | "stale_file_revision"
    )
    .then(|| value.to_string())
}

fn structured_kind(output: &Value, field: &str) -> Option<String> {
    let value = output.get(field)?.as_str()?.trim();
    if value.is_empty()
        || value.len() > MAX_STRUCTURED_KIND_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    Some(value.to_string())
}

fn recovery_kind(output: &Value) -> Option<String> {
    let value = output.get("recovery_kind")?.as_str()?;
    RECOVERY_KIND_VALUES
        .contains(&value)
        .then(|| value.to_string())
}

fn execution_state(output: &Value) -> Option<String> {
    let value = output.get("execution_state")?.as_str()?;
    matches!(
        value,
        "not_started"
            | "pending"
            | "running"
            | "started"
            | "outcome_unknown"
            | "completed"
            | "cancelled"
            | "timed_out"
    )
    .then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn completion(tool_name: &str, duration_ms: u64) -> ModelErgonomicsCompletion {
        ModelErgonomicsTimer::start(tool_name)
            .expect("model-visible tool")
            .finish_after(Duration::from_millis(duration_ms))
    }

    #[test]
    fn bulk_exact_metrics_record_only_bounded_counts_and_outcomes() {
        let args = json!({"changes":[{"path":"private.rs","edits":[{"kind":"replace_exact","old_text":"SECRET_OLD","new_text":"SECRET_NEW","expected_match_count":2}]}]});
        let completion = ModelErgonomicsTimer::start_with_arguments("apply_text_edits", &args)
            .unwrap()
            .finish();
        let dry = completion
            .record_for_tool_result(&ToolResult::ok(json!({
                "dry_run":true,"changed":false,"would_change":true,
                "files":[{"edits":[{"expected_match_count":2,"match_count":2}]}]
            })))
            .unwrap();
        assert_eq!(dry.bulk_exact_outcome.as_deref(), Some("dry_run"));
        assert_eq!(dry.bulk_exact_match_total, Some(2));
        let reject = completion.record_for_tool_result(&ToolResult::err_with_output(
            "count mismatch", json!({"error_kind":"match_count_mismatch","actual_match_count":1,"state_changed":false,"execution_state":"not_started"})
        )).unwrap();
        assert_eq!(reject.bulk_exact_outcome.as_deref(), Some("rejection"));
        assert_eq!(reject.bulk_exact_match_total, Some(1));
        assert_eq!(
            reject.edit_conflict_kind.as_deref(),
            Some("match_count_mismatch")
        );
        let serialized = serde_json::to_string(&reject).unwrap();
        assert!(!serialized.contains("SECRET_OLD"));
        assert!(!serialized.contains("private.rs"));
    }

    fn work_on_project_record(arguments: Value) -> ModelErgonomicsRecord {
        ModelErgonomicsTimer::start_with_arguments("work_on_project", &arguments)
            .expect("work_on_project telemetry")
            .finish_after(Duration::ZERO)
            .record_for_tool_result(&ToolResult::ok(json!({})))
            .expect("serializable telemetry")
    }

    #[test]
    fn omitted_work_on_project_profile_records_effective_mcp_host_profile() {
        let mut timer = ModelErgonomicsTimer::start_with_arguments(
            "work_on_project",
            &json!({"project":"agent:private:project","instruction":"private instruction"}),
        )
        .unwrap();
        timer.resolve_work_on_project_guidance_profile(
            crate::mcp_host::McpHostConfig {
                profile: crate::mcp_host::McpHostProfile::HostCodeMode,
                host_budget_secs: None,
            }
            .runtime_policy(),
            true,
        );
        let record = timer
            .finish_after(Duration::ZERO)
            .record_for_tool_result(&ToolResult::ok(json!({})))
            .unwrap();
        let facts = record.work_on_project.unwrap();
        assert_eq!(
            facts.guidance_profile,
            WorkOnProjectGuidanceProfile::HostCodeMode
        );
        assert!(!facts.guidance_profile_explicit);

        let mut explicit = ModelErgonomicsTimer::start_with_arguments(
            "work_on_project",
            &json!({"project":"agent:private:project","instruction":"private instruction","guidance_profile":"direct"}),
        )
        .unwrap();
        explicit.resolve_work_on_project_guidance_profile(
            crate::mcp_host::McpHostConfig {
                profile: crate::mcp_host::McpHostProfile::HostCodeMode,
                host_budget_secs: None,
            }
            .runtime_policy(),
            true,
        );
        let facts = explicit
            .finish_after(Duration::ZERO)
            .record_for_tool_result(&ToolResult::ok(json!({})))
            .unwrap()
            .work_on_project
            .unwrap();
        assert_eq!(facts.guidance_profile, WorkOnProjectGuidanceProfile::Direct);
        assert!(facts.guidance_profile_explicit);
    }

    #[test]
    fn success_record_uses_exact_utf8_tool_result_bytes() {
        let result = ToolResult::ok(json!({"text": "中文", "count": 2}));
        let record = completion("tool_manifest", 7)
            .record_for_tool_result(&result)
            .unwrap();
        let expected = serde_json::to_vec(&result).unwrap().len() as u64;
        let chars = serde_json::to_string(&result).unwrap().chars().count() as u64;
        assert_eq!(record.serialized_result_bytes, Some(expected));
        assert!(
            expected > chars,
            "UTF-8 multibyte text must count bytes, not chars"
        );
        assert_eq!(record.duration_ms, 7);
        assert!(record.success);
        assert_eq!(record.tool_name, "tool_manifest");
        assert_eq!(record.tool_category, "runtime");
        assert_eq!(record.error_kind, None);
        assert_eq!(record.failure_kind, None);
        assert_eq!(record.recovery_kind, None);
    }

    #[test]
    fn non_work_on_project_omits_bootstrap_preference_facts() {
        let record = completion("tool_manifest", 0)
            .record_for_tool_result(&ToolResult::ok(json!({})))
            .unwrap();
        assert_eq!(record.schema_version, 10);
        assert_eq!(record.work_on_project, None);
        assert!(!serde_json::to_string(&record)
            .unwrap()
            .contains("work_on_project"));
    }

    #[test]
    fn work_on_project_fresh_defaults_are_queryable_without_raw_values() {
        let record = work_on_project_record(json!({
            "project": "agent:private:project",
            "instruction": "private instruction"
        }));
        let facts = record.work_on_project.expect("work_on_project facts");
        assert!(!facts.resume_requested);
        assert_eq!(facts.source, WorkOnProjectSource::Project);
        assert_eq!(facts.mode, WorkOnProjectMode::Checkout);
        assert!(!facts.mode_explicit);
        assert!(!facts.base_ref_present);
        assert_eq!(facts.guidance_profile, WorkOnProjectGuidanceProfile::Direct);
        assert!(!facts.guidance_profile_explicit);
        assert_eq!(facts.include_extension_catalog, Some(true));
        assert!(!facts.include_extension_catalog_explicit);
    }

    #[test]
    fn work_on_project_explicit_resume_and_remaining_preferences_are_queryable() {
        let record = work_on_project_record(json!({
            "project": "agent:private:project",
            "instruction": "private instruction",
            "session_id": "wc_sess_private",
            "guidance_profile": "direct",
            "include_extension_catalog": false
        }));
        let facts = record.work_on_project.expect("work_on_project facts");
        assert!(facts.resume_requested);
        assert_eq!(facts.guidance_profile, WorkOnProjectGuidanceProfile::Direct);
        assert!(facts.guidance_profile_explicit);
        assert_eq!(facts.include_extension_catalog, Some(false));
        assert!(facts.include_extension_catalog_explicit);
    }

    #[test]
    fn work_on_project_host_code_mode_profile_is_not_feature_gated() {
        let record = work_on_project_record(json!({
            "project": "agent:private:project",
            "instruction": "private instruction",
            "guidance_profile": "host_code_mode"
        }));
        let facts = record.work_on_project.expect("work_on_project facts");
        assert_eq!(
            facts.guidance_profile,
            WorkOnProjectGuidanceProfile::HostCodeMode
        );
        assert!(facts.guidance_profile_explicit);
    }

    #[test]
    fn work_on_project_code_mode_profile_telemetry_matches_compiled_availability() {
        let record = work_on_project_record(json!({
            "project": "agent:private:project",
            "instruction": "private instruction",
            "guidance_profile": "code_mode"
        }));
        let facts = record.work_on_project.expect("work_on_project facts");
        assert_eq!(
            facts.guidance_profile,
            if cfg!(feature = "experimental-code-mode") {
                WorkOnProjectGuidanceProfile::CodeMode
            } else {
                WorkOnProjectGuidanceProfile::Invalid
            }
        );
        assert!(facts.guidance_profile_explicit);
    }

    #[test]
    fn work_on_project_path_worktree_records_only_closed_preferences() {
        let record = work_on_project_record(json!({
            "client_id": "private-client",
            "path": "/private/path",
            "mode": "worktree",
            "base_ref": "private/base-ref",
            "instruction": "private instruction"
        }));
        let facts = record.work_on_project.expect("work_on_project facts");
        assert_eq!(facts.source, WorkOnProjectSource::Path);
        assert_eq!(facts.mode, WorkOnProjectMode::Worktree);
        assert!(facts.mode_explicit);
        assert!(facts.base_ref_present);
    }

    #[test]
    fn work_on_project_malformed_values_fail_closed_without_guessing_effective_booleans() {
        let record = work_on_project_record(json!({
            "project": 42,
            "instruction": "private instruction",
            "session_id": 7,
            "mode": 9,
            "base_ref": {"private": true},
            "guidance_profile": {"invalid": true},
            "include_extension_catalog": []
        }));
        let facts = record.work_on_project.expect("work_on_project facts");
        assert!(facts.resume_requested);
        assert_eq!(facts.source, WorkOnProjectSource::Invalid);
        assert_eq!(facts.mode, WorkOnProjectMode::Invalid);
        assert!(facts.mode_explicit);
        assert!(facts.base_ref_present);
        assert_eq!(
            facts.guidance_profile,
            WorkOnProjectGuidanceProfile::Invalid
        );
        assert!(facts.guidance_profile_explicit);
        assert_eq!(facts.include_extension_catalog, None);
        assert!(facts.include_extension_catalog_explicit);
    }

    #[test]
    fn work_on_project_telemetry_never_serializes_private_request_bodies() {
        let sentinels = [
            "PRIVATE_INSTRUCTION_SENTINEL",
            "PRIVATE_PROJECT_SENTINEL",
            "PRIVATE_CLIENT_SENTINEL",
            "PRIVATE_PATH_SENTINEL",
            "PRIVATE_SESSION_SENTINEL",
            "PRIVATE_BASE_REF_SENTINEL",
        ];
        let record = work_on_project_record(json!({
            "instruction": sentinels[0],
            "project": sentinels[1],
            "client_id": sentinels[2],
            "path": sentinels[3],
            "session_id": sentinels[4],
            "base_ref": sentinels[5],
            "mode": "worktree",
            "include_extension_catalog": false
        }));
        let serialized = serde_json::to_string(&record).unwrap();
        for sentinel in sentinels {
            assert!(
                !serialized.contains(sentinel),
                "work_on_project telemetry leaked {sentinel}: {serialized}"
            );
        }
    }

    #[test]
    fn failure_record_consumes_only_structured_recovery_fields() {
        let private = "PRIVATE error prose /tmp/secret.rs token=abc search needle";
        let result = ToolResult::err_with_output(
            private,
            json!({
                "error_kind": "stale_surface",
                "failure_kind": "not_started",
                "recovery_kind": "reobserve",
                "execution_state": "not_started",
                "command": private,
                "path": "/tmp/secret.rs",
                "query": "search needle",
                "message": private
            }),
        );
        let record = completion("computer_control", 3)
            .record_for_tool_result(&result)
            .unwrap();
        assert!(!record.success);
        assert_eq!(record.error_kind.as_deref(), Some("stale_surface"));
        assert_eq!(record.failure_kind.as_deref(), Some("not_started"));
        assert_eq!(record.recovery_kind.as_deref(), Some("reobserve"));
        assert_eq!(record.execution_state.as_deref(), Some("not_started"));
        let telemetry = serde_json::to_string(&record).unwrap();
        for forbidden in [private, "/tmp/secret.rs", "search needle", "token=abc"] {
            assert!(
                !telemetry.contains(forbidden),
                "telemetry leaked {forbidden}: {telemetry}"
            );
        }
    }

    #[test]
    fn success_never_invents_failure_or_recovery_metadata() {
        let result = ToolResult::ok(json!({
            "error_kind": "stale_surface",
            "failure_kind": "outcome_unknown",
            "recovery_kind": "retry_same"
        }));
        let record = completion("tool_manifest", 0)
            .record_for_tool_result(&result)
            .unwrap();
        assert_eq!(record.error_kind, None);
        assert_eq!(record.failure_kind, None);
        assert_eq!(record.recovery_kind, None);
    }

    #[test]
    fn invalid_structured_kinds_are_not_promoted_to_telemetry() {
        let result = ToolResult::err_with_output(
            "private prose",
            json!({
                "error_kind": "PRIVATE arbitrary text / path",
                "failure_kind": "x".repeat(MAX_STRUCTURED_KIND_BYTES + 1),
                "recovery_kind": "blind_retry",
                "execution_state": "maybe"
            }),
        );
        let record = completion("tool_manifest", 0)
            .record_for_tool_result(&result)
            .unwrap();
        assert_eq!(record.error_kind, None);
        assert_eq!(record.failure_kind, None);
        assert_eq!(record.recovery_kind, None);
        assert_eq!(record.execution_state, None);
    }

    #[test]
    fn pre_result_failure_counts_invocation_without_fabricating_tool_result_bytes() {
        let record = completion("read_files", 2).record_for_pre_result_failure("invalid_arguments");
        assert!(!record.success);
        assert_eq!(record.error_kind.as_deref(), Some("invalid_arguments"));
        assert_eq!(record.serialized_result_bytes, None);
        assert_eq!(record.failure_kind, None);
        assert_eq!(record.recovery_kind, None);
        assert_eq!(record.execution_state, None);
    }

    #[test]
    fn structured_or_patch_edit_pre_result_hard_timeout_is_uncertain_not_rejected() {
        for tool in ["apply_text_edits", "apply_patch", "apply_unified_diff"] {
            let record = completion(tool, 0).record_for_pre_result_failure("dispatch_hard_timeout");
            assert!(!record.success);
            assert_eq!(record.error_kind.as_deref(), Some("dispatch_hard_timeout"));
            assert_eq!(record.outcome_class(), "unknown");
            assert_eq!(record.serialized_result_bytes, None);
            assert_eq!(record.edit_surface.as_deref(), Some("structured_or_patch"));
            assert_eq!(record.edit_outcome.as_deref(), Some("uncertain"));
            assert_eq!(record.edit_conflict_kind, None);
        }

        for error_kind in ["invalid_arguments", "insufficient_scope"] {
            let record =
                completion("apply_text_edits", 0).record_for_pre_result_failure(error_kind);
            assert!(!record.success);
            assert_eq!(record.error_kind.as_deref(), Some(error_kind));
            assert_eq!(record.outcome_class(), "failure");
            assert_eq!(record.serialized_result_bytes, None);
            assert_eq!(record.edit_surface.as_deref(), Some("structured_or_patch"));
            assert_eq!(record.edit_outcome.as_deref(), Some("rejected"));
            assert_eq!(record.edit_conflict_kind, None);
        }
    }

    #[test]
    fn edit_outcomes_are_bounded_and_authoritative() {
        let text_cases = [
            (
                true,
                json!({"dry_run": false, "changed": true}),
                Some("applied"),
                None,
            ),
            (
                true,
                json!({"dry_run": true, "would_change": true}),
                Some("dry_run_would_change"),
                None,
            ),
            (
                true,
                json!({"dry_run": true, "would_change": false}),
                Some("dry_run_no_change"),
                None,
            ),
            (
                true,
                json!({"dry_run": false, "changed": false}),
                Some("no_change"),
                None,
            ),
            (
                false,
                json!({"error_kind": "multiple_matches"}),
                Some("conflict"),
                Some("multiple_matches"),
            ),
            (
                false,
                json!({"error_kind": "stale_file_revision"}),
                Some("conflict"),
                Some("stale_file_revision"),
            ),
            (
                false,
                json!({"rollback_complete": false, "changed": true, "error_kind": "multiple_matches"}),
                Some("uncertain"),
                Some("multiple_matches"),
            ),
        ];
        for (success, output, outcome, conflict_kind) in text_cases {
            let result = if success {
                ToolResult::ok(output)
            } else {
                ToolResult::err_with_output("private", output)
            };
            let record = completion("apply_text_edits", 0)
                .record_for_tool_result(&result)
                .unwrap();
            assert_eq!(record.schema_version, 10);
            assert_eq!(record.edit_surface.as_deref(), Some("structured_or_patch"));
            assert_eq!(record.edit_outcome.as_deref(), outcome);
            assert_eq!(record.edit_conflict_kind.as_deref(), conflict_kind);
        }

        let unified_diff_cases = [
            (
                true,
                json!({"applied": true, "can_apply": true, "policy_blocked": false, "error_kind": null}),
                Some("applied"),
            ),
            (
                true,
                json!({"applied": false, "can_apply": false, "policy_blocked": false, "error_kind": "not_applicable"}),
                Some("not_applicable"),
            ),
            (
                true,
                json!({"applied": false, "can_apply": false, "policy_blocked": true, "error_kind": "policy_blocked"}),
                Some("policy_blocked"),
            ),
            (
                false,
                json!({"applied": false, "can_apply": null, "policy_blocked": false, "error_kind": "unsupported_diff_format"}),
                Some("malformed"),
            ),
            (
                false,
                json!({"applied": null, "can_apply": true, "policy_blocked": false, "error_kind": "outcome_unknown"}),
                Some("uncertain"),
            ),
            (
                false,
                json!({"applied": false, "can_apply": true, "policy_blocked": false, "error_kind": "apply_failed"}),
                Some("apply_failed"),
            ),
            (
                false,
                json!({"applied": false, "can_apply": null, "policy_blocked": false, "error_kind": "project_unavailable"}),
                Some("rejected"),
            ),
        ];
        for (success, output, outcome) in unified_diff_cases {
            let result = if success {
                ToolResult::ok(output)
            } else {
                ToolResult::err_with_output("private", output)
            };
            let record = completion("apply_unified_diff", 0)
                .record_for_tool_result(&result)
                .unwrap();
            assert_eq!(record.edit_surface.as_deref(), Some("structured_or_patch"));
            assert_eq!(record.edit_outcome.as_deref(), outcome);
            assert_eq!(record.edit_conflict_kind, None);
        }
    }

    #[test]
    fn edit_telemetry_omits_unknown_labels_and_private_content() {
        let private = "PRIVATE /tmp/secret.rs patch-token old_text new_text";
        let result = ToolResult::err_with_output(
            private,
            json!({
                "changed": false,
                "conflict_recovery": {"conflict_kind": private},
                "path": private,
                "patch": private,
                "error": private
            }),
        );
        let record = completion("apply_text_edits", 0)
            .record_for_tool_result(&result)
            .unwrap();
        assert_eq!(record.edit_surface.as_deref(), Some("structured_or_patch"));
        assert_eq!(record.edit_outcome.as_deref(), Some("rejected"));
        assert_eq!(record.edit_conflict_kind, None);
        let serialized = serde_json::to_string(&record).unwrap();
        assert!(!serialized.contains(private));

        for tool in ["read_files", "tool_manifest"] {
            let record = completion(tool, 0)
                .record_for_tool_result(&ToolResult::ok(json!({"changed": true})))
                .unwrap();
            assert_eq!(record.edit_surface, None);
            assert_eq!(record.edit_outcome, None);
            assert_eq!(record.edit_conflict_kind, None);
        }

        for tool in ["write_project_file"] {
            let record = completion(tool, 0)
                .record_for_tool_result(&ToolResult::ok(json!({"changed": true})))
                .unwrap();
            assert_eq!(record.edit_surface.as_deref(), Some("whole_file"));
            assert_eq!(record.edit_outcome, None);
            assert_eq!(record.edit_conflict_kind, None);
        }
    }

    #[test]
    fn finish_summary_only_is_taken_from_request_metadata_without_body_capture() {
        for (arguments, expected) in [(json!({"summary_only": true}), true), (json!({}), false)] {
            let record =
                ModelErgonomicsTimer::start_with_arguments("finish_coding_task", &arguments)
                    .unwrap()
                    .finish_after(Duration::ZERO)
                    .record_for_tool_result(&ToolResult::ok(json!({"private_body": "do-not-copy"})))
                    .unwrap();
            assert_eq!(record.schema_version, 10);
            assert_eq!(record.finish_summary_only, Some(expected));
            assert!(record.serialized_result_bytes.is_some());
            let serialized = serde_json::to_string(&record).unwrap();
            assert!(!serialized.contains("do-not-copy"));
            assert!(!serialized.contains("session_id"));
            assert!(!serialized.contains("ack_session_context_revision"));
        }
    }

    #[test]
    fn retired_and_internal_tools_do_not_start_generic_model_usage_telemetry() {
        assert!(ModelErgonomicsTimer::start("start_coding_task").is_none());
        assert!(ModelErgonomicsTimer::start("definitely_internal_helper").is_none());
    }

    #[test]
    fn every_registered_model_visible_tool_has_generic_telemetry_identity() {
        let specs = super::super::registered_tool_specs();
        assert!(!specs.is_empty());
        for spec in specs {
            let timer = ModelErgonomicsTimer::start(&spec.name)
                .unwrap_or_else(|| panic!("{} bypasses generic telemetry identity", spec.name));
            assert_ne!(
                timer.tool_category, "other",
                "{} has no bounded category",
                spec.name
            );
        }
    }
}
