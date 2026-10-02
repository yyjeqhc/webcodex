//! Pure startup presentation over already-authorized workflow observations.

use super::*;

#[derive(Deserialize)]
struct WorkOnProjectBriefProjection {
    session: WorkOnProjectSessionProjection,
    project: WorkOnProjectProjectProjection,
    project_resolution: ProjectResolutionMetadata,
    workspace: WorkOnProjectWorkspaceProjection,
    workflow: Value,
    instructions: WorkOnProjectInstructionsProjection,
    semantic_navigation: WorkOnProjectSemanticNavigationProjection,
    #[serde(default)]
    extensions: Option<Value>,
    #[serde(default)]
    coding_agent_providers: Vec<webcodex_core::coding_agent::CodingAgentProviderSummary>,
    repository: Value,
    continuation: WorkOnProjectContinuationProjection,
    blockers: Vec<String>,
    warnings: Vec<String>,
    startup_verdict: WorkOnProjectStartupVerdictProjection,
}

#[derive(Deserialize)]
struct WorkOnProjectSessionProjection {
    session_id: String,
    #[serde(default)]
    session_ref: Option<String>,
    continuation: String,
    execution_context: sessions::SessionExecutionContext,
}

#[derive(Deserialize)]
struct WorkOnProjectProjectProjection {
    resolved_id: String,
    #[serde(default)]
    project_ref: Option<String>,
    #[serde(default)]
    knowledge_association: Option<Value>,
}

#[derive(Deserialize)]
struct WorkOnProjectSemanticNavigationProjection {
    #[serde(default)]
    supported: bool,
    available: WorkOnProjectRequiredNullable<bool>,
    status: String,
    capability: WorkOnProjectRequiredNullable<String>,
    reason_code: WorkOnProjectRequiredNullable<String>,
}

#[derive(Deserialize)]
struct WorkOnProjectJobsProjection {
    active_count: u64,
    blocking_active_count: u64,
    nonblocking_active_count: u64,
    recovering_count: u64,
    terminal_pending_count: u64,
    latest_status: String,
}

#[derive(Deserialize, Serialize)]
#[serde(transparent)]
struct WorkOnProjectRequiredNullable<T>(Option<T>);

#[derive(Deserialize)]
struct WorkOnProjectWorkspaceProjection {
    status: String,
    git: WorkOnProjectGitProjection,
    git_available: WorkOnProjectRequiredNullable<bool>,
    branch: WorkOnProjectRequiredNullable<String>,
    head: WorkOnProjectRequiredNullable<String>,
    clean: WorkOnProjectRequiredNullable<bool>,
    conflicts: u64,
    #[serde(default)]
    upstream_status: Option<String>,
    #[serde(default)]
    upstream_reason_code: Option<String>,
    #[serde(default)]
    upstream: Option<String>,
    #[serde(default)]
    ahead: Option<u64>,
    #[serde(default)]
    behind: Option<u64>,
    #[serde(default)]
    changed_paths: Vec<String>,
    #[serde(default)]
    changed_paths_total: u64,
    #[serde(default)]
    changed_paths_truncated: bool,
}

#[derive(Deserialize, Serialize)]
struct WorkOnProjectGitProjection {
    status: String,
    reason_code: WorkOnProjectRequiredNullable<String>,
}

#[derive(Deserialize)]
struct WorkOnProjectInstructionsProjection {
    status: String,
    sources: Vec<WorkOnProjectInstructionSourceProjection>,
    #[serde(default)]
    changed_sources: Option<Vec<String>>,
    #[serde(default)]
    content_included: bool,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    total_chars: u64,
}

#[derive(Deserialize, Serialize)]
struct WorkOnProjectInstructionSourceProjection {
    source_scope: String,
    path: String,
    fingerprint: String,
    truncated: bool,
    headings: Vec<String>,
    content: WorkOnProjectRequiredNullable<String>,
    read_more: WorkOnProjectRequiredNullable<WorkOnProjectReadMoreProjection>,
}

#[derive(Deserialize, Serialize)]
struct WorkOnProjectReadMoreProjection {
    path: String,
    start_line: u64,
    limit: u64,
}

#[derive(Deserialize)]
struct WorkOnProjectContinuationProjection {
    suggested_next_actions: WorkOnProjectActionItemsProjection,
    jobs: WorkOnProjectJobsProjection,
}

#[derive(Deserialize)]
struct WorkOnProjectActionItemsProjection {
    items: Vec<String>,
}

#[derive(Deserialize)]
struct WorkOnProjectStartupVerdictProjection {
    status: String,
    blocking: bool,
    suggested_next_actions: Vec<String>,
}

fn sparse_work_on_project_instruction_source(
    source: WorkOnProjectInstructionSourceProjection,
) -> Value {
    let WorkOnProjectInstructionSourceProjection {
        source_scope,
        path,
        fingerprint,
        truncated,
        headings,
        content,
        read_more,
    } = source;
    let mut projected = json!({
        "source_scope": source_scope,
        "path": path,
        "fingerprint": fingerprint,
    });
    if truncated {
        projected["truncated"] = json!(true);
    }
    if let Some(content) = content.0 {
        if !headings.is_empty() {
            projected["headings"] = json!(headings);
        }
        projected["content"] = json!(content);
        if let Some(read_more) = read_more.0 {
            projected["read_more"] = json!(read_more);
        }
    }
    projected
}

fn sparse_work_on_project_workspace(workspace: WorkOnProjectWorkspaceProjection) -> Value {
    let WorkOnProjectWorkspaceProjection {
        status,
        git,
        git_available,
        branch,
        head,
        clean,
        conflicts,
        upstream_status,
        upstream_reason_code,
        upstream,
        ahead,
        behind,
        changed_paths,
        changed_paths_total,
        changed_paths_truncated,
    } = workspace;
    let status_unavailable = status == "unavailable";
    let mut projected = json!({
        "status": status,
        "git": {
            "status": git.status,
            "reason_code": git.reason_code.0,
        },
    });
    if git_available.0 == Some(false) {
        projected["git_available"] = json!(false);
    }
    if let Some(branch) = branch.0 {
        projected["branch"] = json!(branch);
    }
    if let Some(head) = head.0 {
        projected["head"] = json!(head);
    }
    if let Some(upstream_status) = upstream_status.filter(|status| status != "unobserved") {
        projected["upstream_status"] = json!(upstream_status);
    }
    if let Some(upstream_reason_code) = upstream_reason_code {
        projected["upstream_reason_code"] = json!(upstream_reason_code);
    }
    if let Some(upstream) = upstream {
        projected["upstream"] = json!(upstream);
    }
    if let Some(ahead) = ahead {
        projected["ahead"] = json!(ahead);
    }
    if let Some(behind) = behind {
        projected["behind"] = json!(behind);
    }
    if !changed_paths.is_empty() {
        projected["changed_paths"] = json!(changed_paths);
        projected["changed_paths_total"] = json!(changed_paths_total.max(
            projected["changed_paths"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0) as u64
        ));
        if changed_paths_truncated {
            projected["changed_paths_truncated"] = json!(true);
        }
    }
    if status_unavailable {
        if let Some(clean) = clean.0 {
            projected["clean"] = json!(clean);
        }
    }
    if conflicts > 0 {
        projected["conflicts"] = json!(conflicts);
    }
    projected
}

fn sparse_work_on_project_jobs(jobs: WorkOnProjectJobsProjection) -> Option<Value> {
    let mut projected = json!({});
    for (key, count) in [
        ("active_count", jobs.active_count),
        ("blocking_active_count", jobs.blocking_active_count),
        ("nonblocking_active_count", jobs.nonblocking_active_count),
        ("recovering_count", jobs.recovering_count),
        ("terminal_pending_count", jobs.terminal_pending_count),
    ] {
        if count > 0 {
            projected[key] = json!(count);
        }
    }
    if jobs.latest_status != "not_observed" {
        projected["latest_status"] = json!(jobs.latest_status);
    }
    projected.as_object().filter(|object| !object.is_empty())?;
    Some(projected)
}

fn is_default_work_on_project_resolution(
    resolution: &ProjectResolutionMetadata,
    resolved_project: &str,
) -> bool {
    resolution.source == "project"
        && resolution.outcome == "resolved_existing_project"
        && resolution.resolved_project == resolved_project
        && !resolution.registered
}

fn is_default_work_on_project_repository(repository: &Value) -> bool {
    repository.as_object().is_some_and(|object| {
        object.len() == 2
            && repository.get("status").and_then(Value::as_str) == Some("unavailable")
            && repository.get("reason_code").and_then(Value::as_str)
                == Some(REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON)
    })
}

/// Convert a successful canonical coding-startup result into the compact
/// `work_on_project` contract. The delegated engine may already have changed
/// Session state, so protocol drift fails closed with `state_changed=true`.
#[cfg(test)]
pub(crate) fn project_work_on_project_output(project: String, output: Value) -> ToolResult {
    project_work_on_project_output_inner(project, output, CodingGuidanceProfile::Direct, None)
}

#[cfg(test)]
pub(crate) fn project_work_on_project_output_with_correlation_for_test(
    project: String,
    output: Value,
    correlation: &mut ToolCallCorrelation,
) -> ToolResult {
    project_work_on_project_output_inner(
        project,
        output,
        CodingGuidanceProfile::Direct,
        Some(correlation),
    )
}

pub(super) fn project_work_on_project_output_inner(
    project: String,
    output: Value,
    guidance_profile: CodingGuidanceProfile,
    correlation: Option<&mut ToolCallCorrelation>,
) -> ToolResult {
    let permission = output.get("permission").cloned();
    let Some(brief) = startup_brief_from_output(&output) else {
        return work_on_project_projection_failed(
            "output",
            "complete startup brief object",
            "non-object",
            None,
        );
    };
    let projection = match serde_json::from_value::<WorkOnProjectBriefProjection>(brief.clone()) {
        Ok(projection) => projection,
        Err(error) => {
            return work_on_project_projection_failed(
                "output",
                "complete typed startup brief",
                "missing or wrongly typed field",
                Some(error.to_string()),
            )
        }
    };
    if !sessions::is_valid_session_id(&projection.session.session_id) {
        return work_on_project_projection_failed(
            "session.session_id",
            "valid wc_sess_* string",
            "invalid string",
            None,
        );
    }
    if !matches!(
        projection.session.continuation.as_str(),
        "created" | "continued" | "resumed_explicitly"
    ) {
        return work_on_project_projection_failed(
            "session.continuation",
            "created, continued, or resumed_explicitly",
            "unsupported string",
            None,
        );
    }
    if !matches!(
        projection.workspace.status.as_str(),
        "available" | "clean" | "dirty" | "blocked" | "unavailable"
    ) {
        return work_on_project_projection_failed(
            "workspace.status",
            "available, clean, dirty, blocked, or unavailable",
            "unsupported string",
            None,
        );
    }
    if !matches!(
        projection.instructions.status.as_str(),
        "loaded" | "reused" | "changed" | "not_found" | "unavailable"
    ) {
        return work_on_project_projection_failed(
            "instructions.status",
            "loaded, reused, changed, not_found, or unavailable",
            "unsupported string",
            None,
        );
    }
    if projection.instructions.sources.len()
        > webcodex_core::runner_instruction::RUNNER_INSTRUCTION_RESPONSE_MAX_FILES
            + crate::tool_runtime::project_instructions::INSTRUCTION_CANDIDATE_PATHS.len()
    {
        return work_on_project_projection_failed(
            "instructions.sources",
            "at most 21 source objects",
            "invalid array contents",
            None,
        );
    }
    if projection.workflow != builtin_coding_workflow_projection(guidance_profile) {
        return work_on_project_projection_failed(
            "workflow",
            "canonical built-in coding workflow contract",
            "non-canonical workflow projection",
            None,
        );
    }

    if let Some(correlation) = correlation {
        correlation.resolved_project = Some(projection.project.resolved_id.clone());
        correlation.add_workflow_session(WorkflowSessionCorrelation {
            session_id: projection.session.session_id.clone(),
            project: Some(projection.project.resolved_id.clone()),
            relation: WorkflowSessionCorrelationRelation::WorkOnProject,
        });
    }

    let suggested_next_actions = if projection.startup_verdict.suggested_next_actions.is_empty() {
        projection.continuation.suggested_next_actions.items
    } else {
        projection.startup_verdict.suggested_next_actions
    };
    let generic_begin_action_only = suggested_next_actions.len() == 1
        && suggested_next_actions[0] == "begin the requested coding task";
    let project_resolution_is_default = is_default_work_on_project_resolution(
        &projection.project_resolution,
        &projection.project.resolved_id,
    );
    let worktree = projection.project_resolution.worktree.clone();
    let repository_is_default = is_default_work_on_project_repository(&projection.repository);
    let execution_context_is_empty = projection.session.execution_context.is_empty();
    let jobs = sparse_work_on_project_jobs(projection.continuation.jobs);
    let workspace = sparse_work_on_project_workspace(projection.workspace);

    let WorkOnProjectInstructionsProjection {
        status,
        sources,
        changed_sources,
        content_included,
        truncated,
        total_chars,
    } = projection.instructions;
    let mut instructions = json!({
        "status": status,
        "sources": sources
            .into_iter()
            .map(sparse_work_on_project_instruction_source)
            .collect::<Vec<_>>(),
    });
    if changed_sources
        .as_ref()
        .is_some_and(|sources| !sources.is_empty())
    {
        instructions["changed_sources"] = json!(changed_sources);
    }
    if content_included {
        instructions["content_included"] = json!(true);
    }
    if truncated {
        instructions["truncated"] = json!(true);
        if total_chars > 0 {
            instructions["total_chars"] = json!(total_chars);
        }
    }

    let semantic_navigation = json!({
        "supported": projection.semantic_navigation.supported,
        "available": projection.semantic_navigation.available,
        "status": projection.semantic_navigation.status,
        "capability": projection.semantic_navigation.capability,
        "reason_code": projection.semantic_navigation.reason_code,
    });
    let mut result = ToolResult::ok(json!({
        "session_id": projection.session.session_id,
        "project": project,
        "resolved_project": projection.project.resolved_id,
        "continuation": projection.session.continuation,
        "workspace": workspace,
        "instructions": instructions,
        "semantic_navigation": semantic_navigation,
    }));
    if let Some(knowledge_association) = projection.project.knowledge_association {
        result.output["knowledge_association"] = knowledge_association;
    }
    if let Some(session_ref) = projection.session.session_ref {
        result.output["session_ref"] = json!(session_ref);
    }
    if let Some(project_ref) = projection.project.project_ref {
        result.output["project_ref"] = json!(project_ref);
    }
    if let Some(extensions) = projection.extensions {
        result.output["extensions"] = extensions;
    }
    if !projection.coding_agent_providers.is_empty() {
        result.output["coding_agent_providers"] = json!(projection.coding_agent_providers);
    }
    if !project_resolution_is_default {
        let mut project_resolution = json!(projection.project_resolution);
        if let Some(project_resolution) = project_resolution.as_object_mut() {
            project_resolution.remove("worktree");
        }
        result.output["project_resolution"] = project_resolution;
    }
    if let Some(worktree) = worktree {
        result.output["worktree"] = json!(worktree);
    }
    if !execution_context_is_empty {
        result.output["execution_context"] = json!(projection.session.execution_context);
    }
    if projection.startup_verdict.status != "pass" || projection.startup_verdict.blocking {
        result.output["readiness"] = json!({
            "status": projection.startup_verdict.status,
            "blocking": projection.startup_verdict.blocking,
        });
    }
    if !repository_is_default {
        result.output["repository"] = projection.repository;
    }
    if let Some(jobs) = jobs {
        result.output["jobs"] = jobs;
    }
    if !projection.blockers.is_empty() {
        result.output["blockers"] = json!(projection.blockers);
    }
    if !projection.warnings.is_empty() {
        result.output["warnings"] = json!(projection.warnings);
    }
    if !suggested_next_actions.is_empty() && !generic_begin_action_only {
        result.output["suggested_next_actions"] = json!(suggested_next_actions);
    }
    if let Some(permission) = permission {
        result.output["permission"] = permission;
    }
    result
}

fn work_on_project_projection_failed(
    field: &str,
    expected: &str,
    actual: &str,
    detail: Option<String>,
) -> ToolResult {
    ToolResult::err_with_output(
        format!("work_on_project projection failed: {field} expected {expected}, got {actual}"),
        json!({
            "error_kind": "work_on_project_projection_failed",
            "failure_kind": "work_on_project_projection_failed",
            "underlying_tool": "work_on_project",
            "field": field,
            "expected": expected,
            "actual": actual,
            "detail": detail,
            "state_changed": true,
        }),
    )
}

pub(super) fn resolved_project_payload(resolved: &ResolvedProject) -> Value {
    json!({
        "input": resolved.input.clone(),
        "id": resolved.resolved_id.clone(),
        "path": resolved.config.path.clone(),
        "client_id": resolved.config.client_id.clone(),
        "allow_patch": resolved.config.allow_patch,
    })
}

pub(super) fn rules_summary(snapshot: Option<&ProjectInstructionsSnapshot>) -> Value {
    let Some(snapshot) = snapshot else {
        return Value::Null;
    };
    let sources: Vec<Value> = snapshot.files.iter().map(rule_source_summary).collect();
    json!({
        "present": snapshot.loaded,
        "loaded": snapshot.loaded,
        "sources": sources,
        "candidate_paths": snapshot.candidate_paths.clone(),
        "total_chars": snapshot.total_chars,
        "max_total_chars": snapshot.max_total_chars,
        "truncated": snapshot.truncated,
        "scan_complete": snapshot.scan_complete,
        "summary": if snapshot.loaded {
            "deterministic instruction source summary; read listed sources for full content"
        } else {
            "no project instruction source loaded from the fixed candidate list"
        },
        "note": snapshot.note.clone(),
    })
}

fn rule_source_summary(file: &ProjectInstructionFile) -> Value {
    json!({
        "path": file.path.clone(),
        "fingerprint": file.fingerprint.clone(),
        "chars": file.chars,
        "total_lines": file.total_lines,
        "start_line": file.start_line,
        "limit": file.limit,
        "truncated": file.truncated,
        "read_more": file.read_more.clone(),
        "headings": extract_headings(&file.content),
        "first_lines": extract_first_lines(&file.content),
    })
}

fn extract_headings(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .take(RULES_MAX_HEADINGS)
        .map(bound_line)
        .collect()
}

fn extract_first_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(RULES_MAX_FIRST_LINES)
        .map(bound_line)
        .collect()
}

fn bound_line(line: &str) -> String {
    let mut out = String::new();
    for ch in line.chars().take(RULES_MAX_LINE_CHARS) {
        out.push(ch);
    }
    out
}

/// Full default startup recommended flow. Reuses the shared
/// `TOOL_RECOMMENDED_FLOWS` group definitions so top-level startup guidance
/// does not drift from `read_tool_manifest.recommended_flows`.
pub(super) fn recommended_flow_payload() -> Value {
    recommended_flow_groups(None)
}

/// Project top-level `recommended_flow` onto tools present in the embedded
/// `read_tool_manifest`. Group keys stay fixed; empty groups are allowed.
pub(super) fn recommended_flow_payload_for_manifest_tools(manifest: &Value) -> Value {
    let visible: HashSet<&str> = manifest
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect();
    recommended_flow_groups(Some(&visible))
}

fn recommended_flow_groups(visible: Option<&HashSet<&str>>) -> Value {
    const GROUPS: &[&str] = &["inspect", "edit", "validate", "review", "handoff"];
    let mut map = serde_json::Map::new();
    for group in GROUPS {
        let tools = model_visible_recommended_flows()
            .find(|flow| flow.name == *group)
            .map(|flow| {
                let mut seen = HashSet::new();
                flow.tools
                    .iter()
                    .copied()
                    .filter(|tool| {
                        let allowed = match visible {
                            Some(set) => set.contains(*tool),
                            None => true,
                        };
                        allowed && seen.insert(*tool)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        map.insert((*group).to_string(), json!(tools));
    }
    Value::Object(map)
}

pub(super) fn workspace_payload_from_show_changes(show_changes: &Value) -> Value {
    let counts = show_changes
        .get("counts")
        .cloned()
        .unwrap_or_else(|| json!({}));
    json!({
        "clean": show_changes.get("clean").cloned().unwrap_or(Value::Null),
        "git_available": show_changes
            .get("git_available")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "non_git_project": show_changes
            .get("non_git_project")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "branch": show_changes.get("branch").cloned().unwrap_or(Value::Null),
        "head": show_changes.get("head").cloned().unwrap_or(Value::Null),
        "changed_files_count": changed_files_count_from_counts(&counts),
        "counts": counts,
        "warnings": show_changes
            .get("warnings")
            .cloned()
            .unwrap_or_else(|| json!([])),
    })
}

/// Map startup `git` summary fields into the workspace warning shape.
pub(super) fn workspace_payload_from_git_summary(git: &Value) -> Value {
    let counts = git.get("counts").cloned().unwrap_or_else(|| json!({}));
    json!({
        "clean": git.get("clean").cloned().unwrap_or(Value::Null),
        "non_git_project": git
            .get("non_git_project")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "git_available": git
            .get("available")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "changed_files_count": git
            .get("changed_files_count")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| changed_files_count_from_counts(&counts)),
        "counts": counts,
    })
}

pub(super) fn startup_verdict(
    output: &Value,
    active_jobs: &Value,
    owning_runner_available: Option<bool>,
    runtime_status_call_failed: bool,
    tool_manifest_requested: bool,
) -> Value {
    let mut checks = Vec::new();
    let mut actions: Vec<String> = Vec::new();

    push_startup_check(
        &mut checks,
        "get_runtime_status",
        runtime_status_check(output, runtime_status_call_failed),
    );
    push_startup_check(&mut checks, "workspace", workspace_check(output));
    push_startup_check(&mut checks, "jobs", startup_jobs_check(active_jobs));
    push_startup_check(
        &mut checks,
        "agent",
        startup_agent_check(output, owning_runner_available),
    );
    push_startup_check(
        &mut checks,
        "read_tool_manifest",
        startup_tool_manifest_check(output, tool_manifest_requested),
    );

    for check in &checks {
        match check.get("reason").and_then(Value::as_str) {
            Some("runtime_status_call_failed") => {
                push_unique_action(&mut actions, "inspect get_runtime_status directly")
            }
            Some("workspace_dirty") => push_unique_action(
                &mut actions,
                "inspect existing worktree changes with read_workspace_changes and preserve them while editing",
            ),
            Some("workspace_conflicts") => push_unique_action(
                &mut actions,
                "review merge/rebase conflicts carefully; do not reset or overwrite conflict markers unless resolving them",
            ),
            Some("active_jobs_present") | Some("blocking_active_jobs") => {
                push_unique_action(&mut actions, "inspect active jobs before proceeding")
            }
            Some("agent_offline") => {
                push_unique_action(&mut actions, "check Runner connectivity with list_runners")
            }
            Some("tool_manifest_not_requested") => push_unique_action(
                &mut actions,
                "request read_tool_manifest if workflow discovery is needed",
            ),
            Some("truncated_by_limit") => push_unique_action(
                &mut actions,
                "continue with the bounded read_tool_manifest or request a focused category",
            ),
            Some("tool_manifest_unavailable") => {
                push_unique_action(&mut actions, "inspect read_tool_manifest directly")
            }
            _ => {}
        }
    }

    if actions.is_empty() {
        actions.push("proceed with the coding task using the explicit session_id".to_string());
    }
    let status = aggregate_startup_status(&checks);
    json!({
        "status": status,
        "blocking": status == "fail",
        "checks": checks,
        "suggested_next_actions": actions,
    })
}

fn runtime_status_check(
    output: &Value,
    runtime_status_call_failed: bool,
) -> (&'static str, Option<&'static str>) {
    if runtime_status_call_failed {
        return ("fail", Some("runtime_status_call_failed"));
    }
    let runtime_status = output.get("runtime_status").unwrap_or(&Value::Null);
    if !runtime_status.is_object() {
        return ("fail", Some("runtime_status_unavailable"));
    }
    match runtime_status
        .pointer("/tools/count")
        .and_then(Value::as_u64)
    {
        Some(count) if count > 0 => ("pass", None),
        Some(_) => ("fail", Some("tool_count_zero")),
        None => ("warn", Some("tool_count_unknown")),
    }
}

fn workspace_check(output: &Value) -> (&'static str, Option<&'static str>) {
    let git = output.get("git").unwrap_or(&Value::Null);
    if git
        .get("non_git_project")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return ("pass", Some("non_git_project"));
    }
    if git.get("available").and_then(Value::as_bool) == Some(false) {
        return ("warn", Some("git_unavailable"));
    }
    // Ordinary tracked/staged/untracked edits are expected development state.
    // An unresolved merge/rebase conflict is a deterministic blocker until it
    // is resolved; the session itself remains usable for inspection and repair.
    let conflicted = git
        .pointer("/counts/conflicted")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if conflicted > 0 {
        return ("fail", Some("workspace_conflicts"));
    }
    match git.get("clean").and_then(Value::as_bool) {
        Some(true) => ("pass", None),
        Some(false) => ("warn", Some("workspace_dirty")),
        None => ("warn", Some("workspace_unknown")),
    }
}

fn startup_jobs_check(jobs: &Value) -> (&'static str, Option<&'static str>) {
    if jobs
        .get("blocking_active_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        return ("fail", Some("blocking_active_jobs"));
    }
    match jobs.get("active_count").and_then(Value::as_u64) {
        Some(0) => ("pass", None),
        Some(_) => ("warn", Some("active_jobs_present")),
        None => ("warn", Some("jobs_unknown")),
    }
}

pub(super) fn startup_agent_check(
    _output: &Value,
    owning_runner_available: Option<bool>,
) -> (&'static str, Option<&'static str>) {
    match owning_runner_available {
        Some(false) => ("fail", Some("agent_offline")),
        Some(true) => ("pass", None),
        None => ("warn", Some("agent_health_unknown")),
    }
}

/// Reuse the already-authorized startup observation, selecting only the Project's
/// owning Runner. Never combine fleet inventories or choose a default provider.
pub(crate) fn project_coding_agent_providers(
    client_id: &str,
    runtime_status: &Value,
) -> Vec<webcodex_core::coding_agent::CodingAgentProviderSummary> {
    runtime_status
        .pointer("/runners/clients")
        .and_then(Value::as_array)
        .and_then(|clients| {
            clients.iter().find(|client| {
                client.get("client_id").and_then(Value::as_str) == Some(client_id)
                    && client.get("connected").and_then(Value::as_bool) == Some(true)
            })
        })
        .and_then(|client| client.get("coding_agent_providers"))
        .and_then(|providers| {
            serde_json::from_value::<Vec<webcodex_core::coding_agent::CodingAgentProviderSummary>>(
                providers.clone(),
            )
            .ok()
        })
        .filter(|providers| {
            providers.len() <= webcodex_core::coding_agent::CODING_AGENT_MAX_PROVIDERS
        })
        .unwrap_or_default()
}

pub(super) fn owning_runner_available(
    resolved: &ResolvedProject,
    runtime_status: &Value,
    runtime_status_call_failed: bool,
) -> Option<bool> {
    if runtime_status_call_failed {
        return None;
    }
    Some(
        runtime_status
            .pointer("/runners/clients")
            .and_then(Value::as_array)
            .and_then(|clients| {
                clients.iter().find(|client| {
                    client.get("client_id").and_then(Value::as_str)
                        == Some(resolved.config.client_id.as_str())
                })
            })
            .and_then(|client| client.get("status").and_then(Value::as_str))
            == Some("online"),
    )
}

fn startup_tool_manifest_check(
    output: &Value,
    tool_manifest_requested: bool,
) -> (&'static str, Option<&'static str>) {
    if !tool_manifest_requested {
        return ("warn", Some("tool_manifest_not_requested"));
    }
    let Some(manifest) = output.get("tool_manifest") else {
        return ("fail", Some("tool_manifest_unavailable"));
    };
    if !manifest.is_object() {
        return ("fail", Some("tool_manifest_unavailable"));
    }
    if manifest
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        if manifest.get("truncation_reason").and_then(Value::as_str) == Some("limit") {
            return ("warn", Some("truncated_by_limit"));
        }
        return ("warn", Some("tool_manifest_truncated"));
    }
    ("pass", None)
}

fn push_startup_check(
    checks: &mut Vec<Value>,
    name: &'static str,
    (status, reason): (&'static str, Option<&'static str>),
) {
    let mut check = json!({
        "name": name,
        "status": status,
    });
    if let Some(reason) = reason {
        check["reason"] = json!(reason);
    }
    checks.push(check);
}

fn aggregate_startup_status(checks: &[Value]) -> &'static str {
    if checks
        .iter()
        .any(|check| check.get("status").and_then(Value::as_str) == Some("fail"))
    {
        "fail"
    } else if checks
        .iter()
        .any(|check| check.get("status").and_then(Value::as_str) == Some("warn"))
    {
        "warn"
    } else {
        "pass"
    }
}

fn push_unique_action(actions: &mut Vec<String>, action: &str) {
    if !actions.iter().any(|existing| existing == action) {
        actions.push(action.to_string());
    }
}

pub(super) fn changed_files_count_from_counts(counts: &Value) -> u64 {
    [
        "modified",
        "added",
        "deleted",
        "renamed",
        "copied",
        "untracked",
        "conflicted",
    ]
    .iter()
    .map(|key| counts.get(*key).and_then(Value::as_u64).unwrap_or(0))
    .sum()
}

const RULES_MAX_HEADINGS: usize = 8;
const RULES_MAX_FIRST_LINES: usize = 5;
const RULES_MAX_LINE_CHARS: usize = 180;

pub(super) fn append_workspace_warnings(workspace: &Value, warnings: &mut Vec<Value>) {
    if workspace.get("clean").and_then(Value::as_bool) == Some(false) {
        let conflicted = workspace
            .pointer("/counts/conflicted")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let message = if conflicted > 0 {
            "workspace has merge/rebase conflicts; inspect and preserve existing worktree state"
        } else {
            "workspace has existing tracked or untracked changes; inspect and preserve them while editing"
        };
        warnings.push(json!({
            "kind": "dirty_worktree",
            "changed_files_count": workspace
                .get("changed_files_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "conflicted": conflicted,
            "message": message,
        }));
    }
    if !workspace
        .get("git_available")
        .and_then(Value::as_bool)
        .unwrap_or(true)
        && !workspace
            .get("non_git_project")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        warnings.push(json!({
            "kind": "git_unavailable",
            "message": "git-backed workspace inspection unavailable",
        }));
    }
}
