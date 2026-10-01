//! Caller-authorized Window queries shared by Console endpoints.
use super::*;

pub(super) fn window_principal_filter(
    auth: &AuthContext,
) -> Result<Option<(String, String)>, RuntimeConsoleError> {
    // Runtime Console is an operator surface. A caller with runtime:read should
    // discover Window candidates across credentials and then have every projected
    // event re-authorized through current Project visibility. Restrict only an
    // explicitly Project-scoped model credential to its own durable principal;
    // that narrow credential must never become a cross-Project observation key.
    if !auth.is_project_scoped_model_subject() {
        return Ok(None);
    }
    crate::tool_runtime::runtime_observation_principal(Some(auth))
        .map(Some)
        .map_err(|_| RuntimeConsoleError::Internal)
}

pub(super) fn window_principal_ref(principal: &Option<(String, String)>) -> Option<(&str, &str)> {
    principal
        .as_ref()
        .map(|(kind, id)| (kind.as_str(), id.as_str()))
}

pub(super) fn valid_window_key(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) async fn window_event_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    cache: &mut HashMap<String, bool>,
    event: &webcodex_store::models::WindowActivityEventRecord,
) -> bool {
    crate::tool_runtime::window_activity::window_event_visible_cached(runtime, auth, cache, event)
        .await
}

pub(super) async fn active_window_request_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    cache: &mut HashMap<String, bool>,
    request: &crate::tool_runtime::ActiveWindowRequest,
) -> bool {
    crate::tool_runtime::window_activity::active_window_request_visible_cached(
        runtime, auth, cache, request,
    )
    .await
}

pub(super) async fn console_window_event_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    discovery_principal: Option<(&str, &str)>,
    caller_principal: Option<(&str, &str)>,
    cache: &mut HashMap<String, bool>,
    event: &webcodex_store::models::WindowActivityEventRecord,
) -> bool {
    if discovery_principal.is_none() && !auth.is_admin_caller() {
        if let Some(project) = event.project.as_deref() {
            return window_project_visible_cached(runtime, auth, cache, Some(project)).await;
        }
        let mut has_project_anchor = false;
        for link in &event.workflow_links {
            if let Some(project) = link.project.as_deref() {
                has_project_anchor = true;
                if window_project_visible_cached(runtime, auth, cache, Some(project)).await {
                    return true;
                }
            }
        }
        if has_project_anchor {
            return false;
        }
        return caller_principal.is_some_and(|(kind, id)| {
            event.principal_correlation_kind.as_deref() == Some(kind)
                && event.principal_correlation_id.as_deref() == Some(id)
        });
    }
    window_event_visible_cached(runtime, auth, cache, event).await
}

pub(super) async fn console_active_window_request_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    discovery_principal: Option<(&str, &str)>,
    caller_principal: Option<(&str, &str)>,
    cache: &mut HashMap<String, bool>,
    request: &crate::tool_runtime::ActiveWindowRequest,
) -> bool {
    if discovery_principal.is_none() && !auth.is_admin_caller() && request.project.is_none() {
        if !caller_principal.is_some_and(|principal| {
            crate::tool_runtime::window_activity::active_window_request_matches_principal(
                request, principal,
            )
        }) {
            return false;
        }
    }
    active_window_request_visible_cached(runtime, auth, cache, request).await
}

pub(super) async fn console_window_project_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    principal: Option<(&str, &str)>,
    cache: &mut HashMap<String, bool>,
    project: Option<&str>,
) -> bool {
    if principal.is_none() && !auth.is_admin_caller() && project.is_none() {
        return false;
    }
    window_project_visible_cached(runtime, auth, cache, project).await
}

pub(super) async fn window_project_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    cache: &mut HashMap<String, bool>,
    project: Option<&str>,
) -> bool {
    crate::tool_runtime::window_activity::window_project_visible_cached(
        runtime, auth, cache, project,
    )
    .await
}

pub(super) fn window_summary_internal_tool(tool: Option<&str>) -> bool {
    matches!(
        tool,
        Some(
            "present_work_result"
                | "get_work_result_state"
                | "send_work_result_message"
                | "read_changed_file_diff"
        )
    )
}

pub(super) async fn visible_window_summary_for_auth_bounded(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    principal: Option<(&str, &str)>,
    window_key: &str,
    visibility_cache: &mut HashMap<String, bool>,
    project_filter: Option<&str>,
    activity_scan_limit: usize,
    include_relation_count: bool,
) -> Result<Option<RuntimeConsoleWindowSummary>, RuntimeConsoleError> {
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    // Summary fields do not consume Code Mode composition or audit summary JSON.
    // Keep full composition hydration on the selected Window detail path only.
    let events = db.list_window_activity_events(window_key, principal, activity_scan_limit);
    let events = events.map_err(|_| RuntimeConsoleError::Internal)?;
    let caller_principal = if principal.is_none() && !auth.is_admin_caller() {
        crate::tool_runtime::runtime_observation_principal(Some(auth)).ok()
    } else {
        None
    };
    let caller_principal_ref = window_principal_ref(&caller_principal);
    let mut source = None;
    let mut last_seen_at_ms = None;
    let mut first_seen_at_ms = None;
    let mut last_tool_call_at_ms = None;
    let mut last_meaningful_activity_at_ms = None;
    let mut recorder_gap_count = 0usize;
    let mut last_project = None;
    let mut project_observed_at = i64::MIN;
    let mut last_activity_name = None;
    let mut last_activity_status = None;
    let mut last_activity_meaningful = None;
    let mut last_activity_observed_at = i64::MIN;
    let mut latest_active_started_at = i64::MIN;
    for event in events {
        if !console_window_event_visible_cached(
            runtime,
            auth,
            principal,
            caller_principal_ref,
            visibility_cache,
            &event,
        )
        .await
            || project_filter.is_some_and(|project| event.project.as_deref() != Some(project))
        {
            continue;
        }
        if window_summary_internal_tool(event.operation.as_deref()) {
            continue;
        }
        source = Some(event.client_window_source.clone());
        last_seen_at_ms = Some(last_seen_at_ms.unwrap_or(i64::MIN).max(event.ended_at_ms));
        first_seen_at_ms = Some(first_seen_at_ms.unwrap_or(i64::MAX).min(event.ended_at_ms));
        if event.action_name == "toolsCall" {
            last_tool_call_at_ms = Some(
                last_tool_call_at_ms
                    .unwrap_or(i64::MIN)
                    .max(event.ended_at_ms),
            );
        }
        if event.project.is_some() && event.ended_at_ms > project_observed_at {
            last_project = event.project.clone();
            project_observed_at = event.ended_at_ms;
        }
        if event.ended_at_ms > last_activity_observed_at {
            last_activity_name = event
                .operation
                .clone()
                .or_else(|| Some(event.action_name.clone()));
            last_activity_status = Some(event.status.clone());
            last_activity_meaningful = Some(event.meaningful);
            last_activity_observed_at = event.ended_at_ms;
        }
        if event.meaningful {
            last_meaningful_activity_at_ms = Some(
                last_meaningful_activity_at_ms
                    .unwrap_or(i64::MIN)
                    .max(event.ended_at_ms),
            );
        }
        if event.recorder_gap_session_id.is_some() {
            recorder_gap_count = recorder_gap_count.saturating_add(1);
        }
    }

    // Session-link history has its own bounded relation query. Do not derive
    // the cardinality only from the latest activity-page events: a busy Window
    // may have >500 later calls while an older authoritative Session relation
    // remains part of its many-to-many history.
    let mut linked_session_count = 0usize;
    if include_relation_count {
        let relation_rows = db
            .list_window_workflow_sessions(window_key, principal, MAX_WINDOW_SESSION_LIMIT)
            .map_err(|_| RuntimeConsoleError::Internal)?;
        for link in relation_rows {
            if project_filter.is_some_and(|project| link.project.as_deref() != Some(project)) {
                continue;
            }
            if console_window_project_visible_cached(
                runtime,
                auth,
                principal,
                visibility_cache,
                link.project.as_deref(),
            )
            .await
            {
                linked_session_count = linked_session_count.saturating_add(1);
            }
        }
    }

    let mut active_count = 0usize;
    for request in runtime
        .window_activity
        .list_for_window(window_key, principal)
    {
        if !console_active_window_request_visible_cached(
            runtime,
            auth,
            principal,
            caller_principal_ref,
            visibility_cache,
            &request,
        )
        .await
            || project_filter.is_some_and(|project| request.project.as_deref() != Some(project))
        {
            continue;
        }
        if window_summary_internal_tool(request.tool_name.as_deref()) {
            continue;
        }
        source = Some(request.client_window_source.clone());
        last_seen_at_ms = Some(
            last_seen_at_ms
                .unwrap_or(i64::MIN)
                .max(request.started_at_ms),
        );
        first_seen_at_ms = Some(
            first_seen_at_ms
                .unwrap_or(i64::MAX)
                .min(request.started_at_ms),
        );
        if request.started_at_ms >= latest_active_started_at {
            if request.project.is_some() {
                last_project = request.project.clone();
            }
            last_activity_name = request
                .tool_name
                .clone()
                .or_else(|| Some(request.method.clone()));
            last_activity_status = Some("running".to_string());
            last_activity_meaningful = Some(request.is_meaningful());
            latest_active_started_at = request.started_at_ms;
        }
        active_count = active_count.saturating_add(1);
    }

    let Some(last_seen_at_ms) = last_seen_at_ms else {
        return Ok(None);
    };
    Ok(Some(RuntimeConsoleWindowSummary {
        client_window_key: window_key.to_string(),
        last_project,
        source: source.unwrap_or_default(),
        last_seen_at_ms,
        first_seen_at_ms: if include_relation_count {
            first_seen_at_ms
        } else {
            None
        },
        last_tool_call_at_ms,
        last_meaningful_activity_at_ms,
        last_activity_name,
        last_activity_status,
        last_activity_meaningful,
        active_count,
        linked_session_count,
        recorder_gap_count,
    }))
}

pub(super) async fn visible_window_summary_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    principal: Option<(&str, &str)>,
    window_key: &str,
    visibility_cache: &mut HashMap<String, bool>,
    project_filter: Option<&str>,
) -> Result<Option<RuntimeConsoleWindowSummary>, RuntimeConsoleError> {
    visible_window_summary_for_auth_bounded(
        runtime,
        auth,
        principal,
        window_key,
        visibility_cache,
        project_filter,
        MAX_WINDOW_ACTIVITY_LIMIT,
        true,
    )
    .await
}

pub(super) fn primary_window_activity_scan_limit(activity_limit: usize) -> usize {
    activity_limit
        .saturating_mul(PRIMARY_WINDOW_ACTIVITY_SCAN_MULTIPLIER)
        .max(PRIMARY_WINDOW_ACTIVITY_SCAN_FLOOR)
        .min(MAX_WINDOW_ACTIVITY_LIMIT)
}

pub(super) async fn windows_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    limit: Option<usize>,
    project_filter: Option<&str>,
) -> Result<RuntimeConsoleWindows, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    if let Some(project) = project_filter {
        authorize_exact_project(runtime, auth, project).await?;
    }
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
    let limit = limit
        .unwrap_or(DEFAULT_WINDOW_LIMIT)
        .clamp(1, MAX_WINDOW_LIMIT);
    let durable = db
        .list_window_activity_summaries(principal_ref, MAX_WINDOW_LIMIT)
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let active = runtime.window_activity.active_windows(principal_ref);
    let mut by_key = BTreeMap::<String, RuntimeConsoleWindowSummary>::new();
    let total;
    let source_truncated;
    if auth.is_admin_caller() && project_filter.is_none() {
        let durable_total = db
            .count_window_activity_summaries(principal_ref)
            .map_err(|_| RuntimeConsoleError::Internal)?;
        for summary in durable {
            by_key.insert(
                summary.client_window_key.clone(),
                RuntimeConsoleWindowSummary {
                    client_window_key: summary.client_window_key,
                    last_project: None,
                    source: summary.client_window_source,
                    last_seen_at_ms: summary.last_seen_at_ms,
                    first_seen_at_ms: Some(summary.first_seen_at_ms),
                    last_tool_call_at_ms: summary.last_tool_call_at_ms,
                    last_meaningful_activity_at_ms: summary.last_meaningful_activity_at_ms,
                    last_activity_name: None,
                    last_activity_status: None,
                    last_activity_meaningful: None,
                    active_count: 0,
                    linked_session_count: summary.linked_session_count,
                    recorder_gap_count: summary.recorder_gap_count,
                },
            );
        }
        let mut active_only = 0usize;
        for live in active {
            if let Some(existing) = by_key.get_mut(&live.client_window_key) {
                existing.active_count = live.active_count;
                existing.last_seen_at_ms = existing.last_seen_at_ms.max(live.last_started_at_ms);
            } else if let Some(summary) = db
                .get_window_activity_summary(&live.client_window_key, principal_ref)
                .map_err(|_| RuntimeConsoleError::Internal)?
            {
                // A live Window can be older than the bounded durable page.
                // It is already included in durable_total and retains its history.
                by_key.insert(
                    live.client_window_key.clone(),
                    RuntimeConsoleWindowSummary {
                        client_window_key: live.client_window_key,
                        last_project: None,
                        source: live.client_window_source,
                        last_seen_at_ms: summary.last_seen_at_ms.max(live.last_started_at_ms),
                        first_seen_at_ms: Some(summary.first_seen_at_ms),
                        last_tool_call_at_ms: summary.last_tool_call_at_ms,
                        last_meaningful_activity_at_ms: summary.last_meaningful_activity_at_ms,
                        last_activity_name: None,
                        last_activity_status: None,
                        last_activity_meaningful: None,
                        active_count: live.active_count,
                        linked_session_count: summary.linked_session_count,
                        recorder_gap_count: summary.recorder_gap_count,
                    },
                );
            } else {
                active_only = active_only.saturating_add(1);
                by_key.insert(
                    live.client_window_key.clone(),
                    RuntimeConsoleWindowSummary {
                        client_window_key: live.client_window_key,
                        last_project: None,
                        source: live.client_window_source,
                        last_seen_at_ms: live.last_started_at_ms,
                        first_seen_at_ms: Some(live.last_started_at_ms),
                        last_tool_call_at_ms: None,
                        last_meaningful_activity_at_ms: None,
                        last_activity_name: None,
                        last_activity_status: Some("running".to_string()),
                        last_activity_meaningful: None,
                        active_count: live.active_count,
                        linked_session_count: 0,
                        recorder_gap_count: 0,
                    },
                );
            }
        }
        total = durable_total.saturating_add(active_only);
        source_truncated = durable_total > MAX_WINDOW_LIMIT;
    } else {
        // Principal equality is necessary but not sufficient: a historical
        // Project grant may have been revoked after the event was written. Use
        // the principal-filtered rows only as bounded candidate keys, then
        // re-project every visible field through current canonical Project
        // authority. This keeps a known Window hash from becoming an existence
        // or timestamp oracle for an inaccessible Project.
        let mut candidate_keys = durable
            .iter()
            .map(|summary| summary.client_window_key.clone())
            .collect::<std::collections::BTreeSet<_>>();
        candidate_keys.extend(
            active
                .iter()
                .map(|summary| summary.client_window_key.clone()),
        );
        let mut visibility_cache = HashMap::new();
        for window_key in candidate_keys {
            if let Some(summary) = visible_window_summary_for_auth(
                runtime,
                auth,
                principal_ref,
                &window_key,
                &mut visibility_cache,
                project_filter,
            )
            .await?
            {
                by_key.insert(window_key, summary);
            }
        }
        total = by_key.len();
        // Candidate discovery itself is bounded. Conservatively report
        // truncation whenever that principal-filtered candidate scan reaches
        // the hard cap; hidden rows are never identified or counted in the
        // response, but an older currently-visible Window may exist beyond it.
        source_truncated = durable.len() == MAX_WINDOW_LIMIT;
    }
    let mut window_rows = by_key.into_values().collect::<Vec<_>>();
    window_rows.sort_by(|left, right| {
        right
            .last_seen_at_ms
            .cmp(&left.last_seen_at_ms)
            .then_with(|| left.client_window_key.cmp(&right.client_window_key))
    });
    window_rows.truncate(limit);
    if auth.is_admin_caller() && project_filter.is_none() {
        let mut visibility_cache = HashMap::new();
        for row in &mut window_rows {
            if let Some(observed) = visible_window_summary_for_auth(
                runtime,
                auth,
                principal_ref,
                &row.client_window_key,
                &mut visibility_cache,
                None,
            )
            .await?
            {
                row.last_project = observed.last_project;
                row.last_activity_name = observed.last_activity_name;
                row.last_activity_status = observed.last_activity_status;
                row.last_activity_meaningful = observed.last_activity_meaningful;
                row.active_count = observed.active_count;
                row.last_seen_at_ms = observed.last_seen_at_ms;
            }
        }
    }
    let visibility = RuntimeConsoleWindowVisibility {
        scope: if principal.is_none() {
            RuntimeConsoleWindowVisibilityScope::Global
        } else {
            RuntimeConsoleWindowVisibilityScope::Principal
        },
    };
    Ok(RuntimeConsoleWindows {
        returned: window_rows.len(),
        truncated: source_truncated || total > window_rows.len(),
        total,
        windows: window_rows,
        visibility,
    })
}

pub(super) async fn active_window_count_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
) -> Result<usize, RuntimeConsoleError> {
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
    let caller_principal = if principal.is_none() && !auth.is_admin_caller() {
        crate::tool_runtime::runtime_observation_principal(Some(auth)).ok()
    } else {
        None
    };
    let caller_principal_ref = window_principal_ref(&caller_principal);
    let mut visibility_cache = HashMap::new();
    let mut visible_windows = 0usize;

    for active_window in runtime.window_activity.active_windows(principal_ref) {
        let mut visible = false;
        for request in runtime
            .window_activity
            .list_for_window(&active_window.client_window_key, principal_ref)
        {
            if console_active_window_request_visible_cached(
                runtime,
                auth,
                principal_ref,
                caller_principal_ref,
                &mut visibility_cache,
                &request,
            )
            .await
            {
                visible = true;
                break;
            }
        }
        if visible {
            visible_windows = visible_windows.saturating_add(1);
        }
    }
    Ok(visible_windows)
}

pub(super) async fn window_jobs_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    activity: &[RuntimeConsoleWindowActivity],
) -> (Vec<RuntimeConsoleWindowJob>, bool) {
    let mut seen = HashSet::new();
    let mut job_ids = Vec::new();
    for event in activity {
        for job_id in event
            .async_job_id
            .iter()
            .chain(event.observed_job_ids.iter())
        {
            if seen.insert(job_id.clone()) {
                job_ids.push(job_id.clone());
            }
        }
    }
    let truncated = job_ids.len() > MAX_WINDOW_JOB_LIMIT;
    job_ids.truncate(MAX_WINDOW_JOB_LIMIT);

    let access = crate::runner_http::runner_access_from_auth(Some(auth));
    let mut jobs = Vec::new();
    for job_id in job_ids {
        let Ok(job) = runtime
            .runner_registry
            .get_job_for_auth(access.as_ref(), &job_id)
            .await
        else {
            continue;
        };
        let terminal =
            RunnerJobLifecycle::from_wire(&job.status).is_ok_and(RunnerJobLifecycle::is_terminal);
        jobs.push(RuntimeConsoleWindowJob {
            job_id: job.job_id,
            status: job.status.clone(),
            active: webcodex_runner_registry::job_status_is_active(&job.status),
            terminal,
            started_at: job.started_at,
            ended_at: job.ended_at,
            duration_ms: job.duration_ms,
            elapsed_secs: job.elapsed_secs,
        });
    }
    (jobs, truncated)
}

pub(super) async fn window_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    input: WindowInput,
) -> Result<RuntimeConsoleWindowDetail, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    if input.client_window_key.chars().count() > MAX_WINDOW_KEY_CHARS
        || !valid_window_key(&input.client_window_key)
    {
        return Err(RuntimeConsoleError::Invalid);
    }
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
    let durable_first_seen_at_ms = if auth.is_admin_caller() {
        db.get_window_activity_summary(&input.client_window_key, principal_ref)
            .map_err(|_| RuntimeConsoleError::Internal)?
            .map(|summary| summary.first_seen_at_ms)
    } else {
        None
    };
    let caller_principal = if principal.is_none() && !auth.is_admin_caller() {
        crate::tool_runtime::runtime_observation_principal(Some(auth)).ok()
    } else {
        None
    };
    let caller_principal_ref = window_principal_ref(&caller_principal);
    let detail_level = input.detail_level;
    let primary_detail = detail_level == WindowDetailLevel::Primary;
    let activity_limit = input
        .activity_limit
        .unwrap_or(DEFAULT_WINDOW_ACTIVITY_LIMIT)
        .clamp(1, MAX_WINDOW_ACTIVITY_LIMIT);
    let session_limit = input
        .session_limit
        .unwrap_or(DEFAULT_WINDOW_SESSION_LIMIT)
        .clamp(1, MAX_WINDOW_SESSION_LIMIT);

    let mut visibility_cache = HashMap::new();
    let summary = if primary_detail {
        let primary_scan_limit = primary_window_activity_scan_limit(activity_limit);
        let quick = visible_window_summary_for_auth_bounded(
            runtime,
            auth,
            principal_ref,
            &input.client_window_key,
            &mut visibility_cache,
            None,
            primary_scan_limit,
            false,
        )
        .await?;
        if quick.is_some() {
            quick
        } else {
            // A bounded primary scan can land entirely on currently-hidden Project
            // history. Fall back to the canonical full authority scan rather than
            // turning a performance optimization into a false 404/existence signal.
            visible_window_summary_for_auth(
                runtime,
                auth,
                principal_ref,
                &input.client_window_key,
                &mut visibility_cache,
                None,
            )
            .await?
        }
    } else {
        visible_window_summary_for_auth(
            runtime,
            auth,
            principal_ref,
            &input.client_window_key,
            &mut visibility_cache,
            None,
        )
        .await?
    };
    let mut active_requests = Vec::new();
    let now_ms = chrono::Utc::now().timestamp_millis();
    for request in runtime
        .window_activity
        .list_for_window(&input.client_window_key, principal_ref)
    {
        if !console_active_window_request_visible_cached(
            runtime,
            auth,
            principal_ref,
            caller_principal_ref,
            &mut visibility_cache,
            &request,
        )
        .await
        {
            continue;
        }
        active_requests.push(RuntimeConsoleActiveWindowRequest {
            server_trace_id: request.server_trace_id,
            method: request.method,
            tool_name: request.tool_name,
            project: request.project,
            started_at_ms: request.started_at_ms,
            elapsed_ms: now_ms.saturating_sub(request.started_at_ms),
        });
    }

    let active_count = active_requests.len();
    active_requests.truncate(crate::tool_runtime::MAX_ACTIVE_REQUESTS_PER_WINDOW);

    let activity_scan_limit = if primary_detail {
        primary_window_activity_scan_limit(activity_limit)
    } else if auth.is_admin_caller() {
        activity_limit
            .saturating_add(1)
            .min(MAX_WINDOW_ACTIVITY_LIMIT)
    } else {
        MAX_WINDOW_ACTIVITY_LIMIT
    };
    #[cfg(feature = "experimental-code-mode")]
    let raw_activity = db.list_window_activity_events_with_code_mode_composition(
        &input.client_window_key,
        principal_ref,
        activity_scan_limit,
    );
    #[cfg(not(feature = "experimental-code-mode"))]
    let raw_activity = db.list_window_activity_events(
        &input.client_window_key,
        principal_ref,
        activity_scan_limit,
    );
    let raw_activity = raw_activity.map_err(|_| RuntimeConsoleError::Internal)?;
    let raw_activity_at_cap = raw_activity.len() == activity_scan_limit;
    let mut activity_visible = Vec::with_capacity(raw_activity.len());
    for event in &raw_activity {
        activity_visible.push(
            console_window_event_visible_cached(
                runtime,
                auth,
                principal_ref,
                caller_principal_ref,
                &mut visibility_cache,
                event,
            )
            .await,
        );
    }
    let timing = project_window_loop_timings(&raw_activity, &activity_visible);
    let mut activity = Vec::new();
    for ((event, visible), timing) in raw_activity.into_iter().zip(activity_visible).zip(timing) {
        if !visible {
            continue;
        }
        activity.push(
            project_visible_window_activity(runtime, auth, &mut visibility_cache, event, timing)
                .await,
        );
    }
    let activity_truncated = activity.len() > activity_limit || raw_activity_at_cap;
    activity.truncate(activity_limit);

    let (jobs, jobs_truncated) = if primary_detail {
        (Vec::new(), false)
    } else {
        window_jobs_for_auth(runtime, auth, &activity).await
    };

    let (linked_sessions, sessions_truncated) = if primary_detail {
        // Activity rows already carry exact Workflow Session links, which is
        // sufficient for immediate Session chips/filtering. Titles/lifecycle and
        // canonical relation history are hydrated by the full follow-up request.
        (Vec::new(), false)
    } else {
        let session_scan_limit = if auth.is_admin_caller() {
            session_limit
                .saturating_add(1)
                .min(MAX_WINDOW_SESSION_LIMIT)
        } else {
            MAX_WINDOW_SESSION_LIMIT
        };
        let raw_sessions = db
            .list_window_workflow_sessions(
                &input.client_window_key,
                principal_ref,
                session_scan_limit,
            )
            .map_err(|_| RuntimeConsoleError::Internal)?;
        let raw_sessions_at_cap = raw_sessions.len() == session_scan_limit;
        let mut linked_sessions = Vec::new();
        for link in raw_sessions {
            let Some(project) = link.project.as_deref() else {
                continue;
            };
            if authorize_exact_project(runtime, auth, project)
                .await
                .is_err()
            {
                continue;
            }
            let detail = runtime.workflow_session_console_detail(
                project,
                &link.workflow_session_id,
                Some(1),
            );
            linked_sessions.push(RuntimeConsoleWindowSession {
                workflow_session_id: link.workflow_session_id,
                project: link.project,
                first_linked_at_ms: link.first_linked_at_ms,
                last_linked_at_ms: link.last_linked_at_ms,
                relations: link.relations,
                relation_count: link.relation_count,
                title: detail.as_ref().map(|detail| detail.title.clone()),
                lifecycle: detail.as_ref().map(|detail| detail.lifecycle.clone()),
            });
        }
        let sessions_truncated = linked_sessions.len() > session_limit || raw_sessions_at_cap;
        linked_sessions.truncate(session_limit);
        (linked_sessions, sessions_truncated)
    };

    if summary.is_none()
        && active_requests.is_empty()
        && activity.is_empty()
        && linked_sessions.is_empty()
    {
        return Err(RuntimeConsoleError::NotFound);
    }
    let active_last_seen = active_requests
        .iter()
        .map(|request| request.started_at_ms)
        .max();
    let last_seen_at_ms = summary
        .as_ref()
        .map(|summary| summary.last_seen_at_ms)
        .into_iter()
        .chain(active_last_seen)
        .chain(linked_sessions.iter().map(|link| link.last_linked_at_ms))
        .max()
        .unwrap_or(0);
    let source = summary
        .as_ref()
        .map(|summary| summary.source.clone())
        .unwrap_or_else(|| "openai-session".to_string());
    Ok(RuntimeConsoleWindowDetail {
        client_window_key: input.client_window_key,
        detail_level,
        source,
        last_seen_at_ms,
        first_seen_at_ms: durable_first_seen_at_ms.or_else(|| {
            summary
                .as_ref()
                .and_then(|summary| summary.first_seen_at_ms)
        }),
        last_tool_call_at_ms: summary
            .as_ref()
            .and_then(|summary| summary.last_tool_call_at_ms),
        last_meaningful_activity_at_ms: summary
            .as_ref()
            .and_then(|summary| summary.last_meaningful_activity_at_ms),
        active_count,
        active_requests,
        sessions_returned: linked_sessions.len(),
        sessions_truncated,
        linked_sessions,
        activity_returned: activity.len(),
        activity_truncated,
        jobs,
        jobs_truncated,
        activity,
        visibility: RuntimeConsoleWindowVisibility {
            scope: if principal.is_none() {
                RuntimeConsoleWindowVisibilityScope::Global
            } else {
                RuntimeConsoleWindowVisibilityScope::Principal
            },
        },
    })
}
