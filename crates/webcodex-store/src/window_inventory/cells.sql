WITH source AS (
    SELECT e.*,
        CASE WHEN e.project IS NOT NULL THEN '[]' ELSE
            (SELECT json_group_array(project) FROM
                (SELECT DISTINCT project FROM action_event_workflow_links WHERE event_id=e.event_id ORDER BY project)) END AS anchors
    FROM action_events e
    WHERE __SELECTION__ AND e.client_window_key IS NOT NULL
        AND e.window_started_at_ms IS NOT NULL AND e.window_ended_at_ms IS NOT NULL
        AND COALESCE(e.operation,'') NOT IN ('present_work_result','work_result_state','work_result_send_message','changes_file_diff')
), scoped AS (
    SELECT *, json_array(principal_correlation_kind,principal_correlation_id,project,json(anchors)) AS scope_key FROM source
), ranked AS (
    SELECT *, ROW_NUMBER() OVER(PARTITION BY client_window_key,scope_key
        ORDER BY window_ended_at_ms DESC, COALESCE(request_observed_at_ms,window_started_at_ms) DESC,event_id DESC) AS position FROM scoped
)
INSERT INTO window_inventory_cells
SELECT client_window_key,scope_key,principal_correlation_kind,principal_correlation_id,project,anchors,
    COALESCE(MAX(CASE WHEN position=1 THEN client_window_source END),''),
    MIN(window_ended_at_ms),MAX(window_ended_at_ms),
    MAX(CASE WHEN action_name='toolsCall' THEN window_ended_at_ms END),
    MAX(CASE WHEN window_meaningful=1 THEN window_ended_at_ms END),
    SUM(CASE WHEN recorder_gap_session_id IS NOT NULL THEN 1 ELSE 0 END),
    MAX(CASE WHEN position=1 THEN COALESCE(operation,action_name) END),
    MAX(CASE WHEN position=1 THEN status END), MAX(CASE WHEN position=1 THEN window_meaningful END),
    MAX(CASE WHEN position=1 THEN COALESCE(request_observed_at_ms,window_started_at_ms) END),
    MAX(CASE WHEN position=1 THEN event_id END)
FROM ranked WHERE true GROUP BY client_window_key,scope_key
ON CONFLICT(window_key,scope_key) DO UPDATE SET
    first_seen=MIN(first_seen,excluded.first_seen),
    source=CASE WHEN (excluded.last_seen,excluded.observed,excluded.event_id)>(last_seen,observed,event_id) THEN excluded.source ELSE source END,
    activity_name=CASE WHEN (excluded.last_seen,excluded.observed,excluded.event_id)>(last_seen,observed,event_id) THEN excluded.activity_name ELSE activity_name END,
    activity_status=CASE WHEN (excluded.last_seen,excluded.observed,excluded.event_id)>(last_seen,observed,event_id) THEN excluded.activity_status ELSE activity_status END,
    activity_meaningful=CASE WHEN (excluded.last_seen,excluded.observed,excluded.event_id)>(last_seen,observed,event_id) THEN excluded.activity_meaningful ELSE activity_meaningful END,
    observed=CASE WHEN (excluded.last_seen,excluded.observed,excluded.event_id)>(last_seen,observed,event_id) THEN excluded.observed ELSE observed END,
    event_id=CASE WHEN (excluded.last_seen,excluded.observed,excluded.event_id)>(last_seen,observed,event_id) THEN excluded.event_id ELSE event_id END,
    last_seen=MAX(last_seen,excluded.last_seen),
    last_tool=COALESCE(MAX(last_tool,excluded.last_tool),last_tool,excluded.last_tool),
    last_meaningful=COALESCE(MAX(last_meaningful,excluded.last_meaningful),last_meaningful,excluded.last_meaningful),
    gap_count=gap_count+excluded.gap_count;
