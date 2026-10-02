//! Set-oriented Window inventory and bounded live observations. Store partitions
//! retain evidence, never grant authority; all Project anchors are reauthorized
//! with the canonical resolver on every HTTP request.
use super::window_queries::{
    console_active_window_request_visible_cached, window_principal_filter, window_principal_ref,
    window_summary_internal_tool,
};
use super::{
    authorize_exact_project, require_runtime_read, valid_window_key, AuthContext,
    RuntimeConsoleError, RuntimeConsoleWindowSummary, RuntimeConsoleWindowVisibility,
    RuntimeConsoleWindowVisibilityScope, RuntimeConsoleWindows, ToolRuntime,
    WindowInventoryProjection, WindowsInput, DEFAULT_WINDOW_LIMIT, MAX_WINDOW_LIMIT,
};
use std::collections::HashMap;
use webcodex_store::{WindowInventoryQuery, WindowInventoryRow};

pub(super) async fn query_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    mut input: WindowsInput,
) -> Result<RuntimeConsoleWindows, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    if (input.project.is_some() && input.projects.is_some())
        || input
            .projects
            .as_ref()
            .is_some_and(|ids| ids.is_empty() || ids.len() > 128)
        || input
            .client_window_key
            .as_deref()
            .is_some_and(|key| !valid_window_key(key))
        || input.query.len() > 256
        || input.query.chars().any(char::is_control)
    {
        return Err(RuntimeConsoleError::Invalid);
    }
    let mut selected = input
        .projects
        .take()
        .or_else(|| input.project.take().map(|id| vec![id]));
    if let Some(ids) = &mut selected {
        ids.sort();
        ids.dedup();
        for id in ids {
            authorize_exact_project(runtime, auth, id).await?;
        }
    }
    if input.projection == WindowInventoryProjection::Inventory {
        return inventory_for_auth(
            runtime,
            auth,
            input.limit,
            input.offset,
            selected.as_deref(),
            input.client_window_key.as_deref(),
            &input.query,
        )
        .await;
    }
    // This path performs NO SQLite access, even with a large historical database.
    let mut live = live_for_auth(
        runtime,
        auth,
        selected.as_deref(),
        input.client_window_key.as_deref(),
    )
    .await?;
    let needle = input.query.to_lowercase();
    live.retain(|row| {
        needle.is_empty()
            || row.client_window_key.to_lowercase().contains(&needle)
            || row
                .last_project
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&needle)
            || row
                .last_activity_name
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&needle)
    });
    live.sort_by(|a, b| {
        b.last_seen_at_ms
            .cmp(&a.last_seen_at_ms)
            .then_with(|| a.client_window_key.cmp(&b.client_window_key))
    });
    let total = live.len();
    let rows: Vec<_> = live
        .into_iter()
        .skip(input.offset)
        .take(
            input
                .limit
                .unwrap_or(MAX_WINDOW_LIMIT)
                .clamp(1, MAX_WINDOW_LIMIT),
        )
        .map(summary)
        .collect();
    let next = input.offset.saturating_add(rows.len());
    let principal = window_principal_filter(auth)?;
    Ok(RuntimeConsoleWindows {
        returned: rows.len(),
        windows: rows,
        total,
        truncated: next < total,
        next_offset: (next < total).then_some(next),
        visibility: RuntimeConsoleWindowVisibility {
            scope: if principal.is_none() {
                RuntimeConsoleWindowVisibilityScope::Global
            } else {
                RuntimeConsoleWindowVisibilityScope::Principal
            },
        },
    })
}

pub(super) fn summary(row: WindowInventoryRow) -> RuntimeConsoleWindowSummary {
    RuntimeConsoleWindowSummary {
        client_window_key: row.client_window_key,
        source: row.source,
        last_project: row.last_project,
        first_seen_at_ms: Some(row.first_seen_at_ms),
        last_seen_at_ms: row.last_seen_at_ms,
        last_tool_call_at_ms: row.last_tool_call_at_ms,
        last_meaningful_activity_at_ms: row.last_meaningful_activity_at_ms,
        last_activity_name: row.last_activity_name,
        last_activity_status: row.last_activity_status,
        last_activity_meaningful: row.last_activity_meaningful,
        active_count: row.active_count,
        linked_session_count: row.linked_session_count,
        recorder_gap_count: row.recorder_gap_count,
    }
}

pub(super) async fn live_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    projects: Option<&[String]>,
    key: Option<&str>,
) -> Result<Vec<WindowInventoryRow>, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
    let caller = crate::tool_runtime::runtime_observation_principal(Some(auth)).ok();
    let mut cache = HashMap::new();
    let mut rows = Vec::new();
    for window in runtime.window_activity.active_windows(principal_ref) {
        if key.is_some_and(|key| key != window.client_window_key) {
            continue;
        }
        let mut row: Option<WindowInventoryRow> = None;
        for request in runtime
            .window_activity
            .list_for_window(&window.client_window_key, principal_ref)
        {
            if window_summary_internal_tool(request.tool_name.as_deref())
                || projects.is_some_and(|projects| {
                    !request
                        .project
                        .as_ref()
                        .is_some_and(|project| projects.contains(project))
                })
                || !console_active_window_request_visible_cached(
                    runtime,
                    auth,
                    principal_ref,
                    window_principal_ref(&caller),
                    &mut cache,
                    &request,
                )
                .await
            {
                continue;
            }
            let entry = row.get_or_insert_with(|| WindowInventoryRow {
                client_window_key: window.client_window_key.clone(),
                source: request.client_window_source.clone(),
                last_project: None,
                first_seen_at_ms: request.started_at_ms,
                last_seen_at_ms: i64::MIN,
                last_tool_call_at_ms: None,
                last_meaningful_activity_at_ms: None,
                last_activity_name: None,
                last_activity_status: Some("running".into()),
                last_activity_meaningful: None,
                active_count: 0,
                linked_session_count: 0,
                recorder_gap_count: 0,
            });
            entry.active_count += 1;
            entry.first_seen_at_ms = entry.first_seen_at_ms.min(request.started_at_ms);
            if request.started_at_ms >= entry.last_seen_at_ms {
                entry.last_seen_at_ms = request.started_at_ms;
                entry.source = request.client_window_source.clone();
                if request.project.is_some() {
                    entry.last_project = request.project.clone();
                }
                entry.last_activity_name =
                    request.tool_name.clone().or(Some(request.method.clone()));
                entry.last_activity_meaningful = Some(request.is_meaningful());
            }
        }
        if let Some(row) = row {
            rows.push(row);
        }
    }
    Ok(rows)
}

pub(super) async fn inventory_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    limit: Option<usize>,
    offset: usize,
    projects: Option<&[String]>,
    key: Option<&str>,
    query: &str,
) -> Result<RuntimeConsoleWindows, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    if let Some(projects) = projects {
        for project in projects {
            authorize_exact_project(runtime, auth, project).await?;
        }
    }
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
    let caller = crate::tool_runtime::runtime_observation_principal(Some(auth)).ok();
    let anchors = match projects {
        Some(projects) => projects.to_vec(),
        None => db
            .window_inventory_project_anchors(principal_ref)
            .map_err(|_| RuntimeConsoleError::Internal)?,
    };
    let mut allowed = Vec::new();
    for project in anchors {
        if runtime.exact_project_visible_to_auth(auth, &project).await {
            allowed.push(project);
        }
    }
    let live = live_for_auth(runtime, auth, projects, key).await?;
    let page = db
        .read_window_inventory(WindowInventoryQuery {
            principal: principal_ref,
            caller: window_principal_ref(&caller),
            management: principal.is_none() && !auth.is_admin_caller(),
            visible_projects: &allowed,
            projects,
            window_key: key,
            query,
            live: &live,
            offset,
            limit: limit
                .unwrap_or(DEFAULT_WINDOW_LIMIT)
                .clamp(1, MAX_WINDOW_LIMIT),
        })
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let returned = page.rows.len();
    Ok(RuntimeConsoleWindows {
        next_offset: (offset.saturating_add(returned) < page.total)
            .then_some(offset.saturating_add(returned)),
        windows: page.rows.into_iter().map(summary).collect(),
        returned,
        total: page.total,
        truncated: offset.saturating_add(returned) < page.total,
        visibility: RuntimeConsoleWindowVisibility {
            scope: if principal.is_none() {
                RuntimeConsoleWindowVisibilityScope::Global
            } else {
                RuntimeConsoleWindowVisibilityScope::Principal
            },
        },
    })
}
