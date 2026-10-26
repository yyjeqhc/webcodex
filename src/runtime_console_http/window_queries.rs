//! Caller-authorized Window queries shared by Console endpoints.
use super::*;

/// Compare the server-authored attribution already present in this observation.
/// Live polling never locks the Store or infers identity from another request.
fn same_managed_oauth_window_principal(
    auth: &AuthContext,
    identity: Option<&(String, String)>,
) -> bool {
    auth.managed_oauth_window_identity().is_some_and(|caller| {
        identity.is_some_and(|(user, client)| caller == (user.as_str(), client.as_str()))
    })
}

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
        }) || same_managed_oauth_window_principal(
            auth,
            event.managed_oauth_identity.as_ref(),
        );
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
        }) && !same_managed_oauth_window_principal(auth, request.managed_oauth_identity.as_ref())
        {
            return false;
        }
    }
    active_window_request_visible_cached(runtime, auth, cache, request).await
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

pub(super) async fn visible_window_summary_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    window_key: &str,
    visibility_cache: &mut HashMap<String, bool>,
    project_filter: Option<&str>,
) -> Result<Option<RuntimeConsoleWindowSummary>, RuntimeConsoleError> {
    let project_ids = project_filter.map(|project| vec![project.to_owned()]);
    let (mut page, anchors) = super::window_inventory::inventory_for_auth_with_anchors(
        runtime,
        auth,
        Some(1),
        0,
        project_ids.as_deref(),
        Some(window_key),
        "",
    )
    .await?;
    // Keep ALL authorized historical and live anchors for the final response
    // recheck, including older Projects absent from last_project/activity page.
    visibility_cache.extend(anchors.into_iter().map(|project| (project, true)));
    Ok(page.windows.pop())
}

pub(super) async fn revalidate_window_projects(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    visibility_cache: &HashMap<String, bool>,
) -> Result<(), RuntimeConsoleError> {
    let project_ids = visibility_cache
        .iter()
        .filter_map(|(project, visible)| visible.then_some(project.clone()))
        .collect::<Vec<_>>();
    if !project_ids.is_empty() {
        super::window_inventory::authorize_selection(runtime, auth, &project_ids).await?;
    }
    Ok(())
}

pub(super) fn primary_window_activity_scan_limit(activity_limit: usize) -> usize {
    activity_limit
        .saturating_mul(PRIMARY_WINDOW_ACTIVITY_SCAN_MULTIPLIER)
        .max(PRIMARY_WINDOW_ACTIVITY_SCAN_FLOOR)
        .min(MAX_WINDOW_ACTIVITY_LIMIT)
}

#[cfg(test)]
pub(super) async fn windows_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    limit: Option<usize>,
    project_filter: Option<&str>,
) -> Result<RuntimeConsoleWindows, RuntimeConsoleError> {
    let selected_projects = project_filter.map(|project| vec![project.to_string()]);
    super::window_inventory::inventory_for_auth(
        runtime,
        auth,
        limit,
        0,
        selected_projects.as_deref(),
        None,
        "",
    )
    .await
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
    let phase_name = if input.detail_level == WindowDetailLevel::Primary {
        "console_window_primary"
    } else {
        "console_window_full"
    };
    let mut phase = super::store_read::Phase::start(phase_name);
    let result = window_for_auth_inner(runtime, auth, input).await;
    phase.finish(if result.is_ok() { "ok" } else { "error" });
    result
}

pub(super) async fn window_for_auth_inner(
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
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
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

    let activity_scan_limit = if primary_detail {
        primary_window_activity_scan_limit(activity_limit)
    } else if auth.is_admin_caller() {
        activity_limit
            .saturating_add(1)
            .min(MAX_WINDOW_ACTIVITY_LIMIT)
    } else {
        MAX_WINDOW_ACTIVITY_LIMIT
    };
    let session_scan_limit = if auth.is_admin_caller() {
        session_limit
            .saturating_add(1)
            .min(MAX_WINDOW_SESSION_LIMIT)
    } else {
        MAX_WINDOW_SESSION_LIMIT
    };
    let window_key = input.client_window_key.clone();
    let owned_principal = principal.clone();
    let admin = auth.is_admin_caller();
    let (mut summary_page, anchors, (durable_first_seen_at_ms, raw_activity, raw_sessions)) =
        super::window_inventory::inventory_and_read_for_auth(
            runtime,
            auth,
            Some(1),
            0,
            None,
            Some(&input.client_window_key),
            "",
            move |db| {
                let principal = window_principal_ref(&owned_principal);
                let first_seen = if admin {
                    db.get_window_activity_summary(&window_key, principal)?
                        .map(|summary| summary.first_seen_at_ms)
                } else {
                    None
                };
                let activity = super::store_read::read_events(
                    db,
                    &window_key,
                    principal,
                    activity_scan_limit,
                    cfg!(feature = "experimental-code-mode"),
                )?;
                let sessions = if primary_detail {
                    Vec::new()
                } else {
                    db.list_window_workflow_sessions(&window_key, principal, session_scan_limit)?
                };
                Ok((first_seen, activity, sessions))
            },
        )
        .await?;
    let summary = summary_page.windows.pop();
    let mut visibility_cache = anchors
        .into_iter()
        .map(|project| (project, true))
        .collect::<HashMap<_, _>>();
    let mut active_authorization = super::store_read::Phase::start("console_window_authorization");
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

    active_authorization.finish("ok");
    drop(active_authorization);
    let active_count = active_requests.len();
    active_requests.truncate(crate::tool_runtime::MAX_ACTIVE_REQUESTS_PER_WINDOW);

    let raw_activity_at_cap = raw_activity.len() == activity_scan_limit;
    let mut authorization = super::store_read::Phase::start("console_window_authorization");
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
    authorization.finish("ok");
    drop(authorization);
    let mut assembly = super::store_read::Phase::start("console_window_assembly");
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

    assembly.finish("ok");
    drop(assembly);
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
            visibility_cache.insert(project.to_owned(), true);
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

    // Authorization may have changed during any awaited/offloaded activity or
    // relation read. A stale summary must not leak an older Project timestamp.
    revalidate_window_projects(runtime, auth, &visibility_cache).await?;
    if summary.is_none()
        && active_requests.is_empty()
        && activity.is_empty()
        && linked_sessions.is_empty()
    {
        return Err(RuntimeConsoleError::NotFound);
    }
    let mut assembly = super::store_read::Phase::start("console_window_assembly");
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
    let result = RuntimeConsoleWindowDetail {
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
    };
    assembly.finish("ok");
    Ok(result)
}
