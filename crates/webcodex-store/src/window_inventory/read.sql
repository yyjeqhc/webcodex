WITH allowed(project) AS (SELECT value FROM json_each(:allowed)),
selected(project) AS (SELECT value FROM json_each(:projects)),
visible AS (
    SELECT c.* FROM window_inventory_cells c
    WHERE (:pk IS NULL OR (principal_kind=:pk AND principal_id=:pi))
        AND (:key IS NULL OR window_key=:key)
        AND (:projects IS NULL OR project IN selected)
        AND CASE WHEN project IS NOT NULL THEN project IN allowed
            WHEN :management THEN
                CASE WHEN EXISTS(SELECT 1 FROM json_each(anchors) WHERE value IS NOT NULL)
                     THEN EXISTS(SELECT 1 FROM json_each(anchors) WHERE value IN allowed)
                     ELSE principal_kind=:ck AND principal_id=:ci END
            ELSE json_array_length(anchors)=0 OR EXISTS(SELECT 1 FROM json_each(anchors) WHERE value IS NULL OR value IN allowed) END
), ranked AS (
    SELECT *, ROW_NUMBER() OVER(PARTITION BY window_key ORDER BY last_seen DESC,observed DESC,event_id DESC) AS newest,
        ROW_NUMBER() OVER(PARTITION BY window_key ORDER BY (project IS NOT NULL) DESC,last_seen DESC,observed DESC,event_id DESC) AS newest_project
    FROM visible
), summaries AS (
    SELECT window_key, MAX(CASE WHEN newest=1 THEN source END) AS source,
        MAX(CASE WHEN newest_project=1 THEN project END) AS last_project,
        MIN(first_seen) AS first_seen,MAX(last_seen) AS last_seen,
        MAX(last_tool) AS last_tool,MAX(last_meaningful) AS last_meaningful,
        MAX(CASE WHEN newest=1 THEN activity_name END) AS activity_name,
        MAX(CASE WHEN newest=1 THEN activity_status END) AS activity_status,
        MAX(CASE WHEN newest=1 THEN activity_meaningful END) AS activity_meaningful,
        SUM(gap_count) AS gap_count
    FROM ranked GROUP BY window_key
), relations AS (
    SELECT window_key,workflow_session_id,MAX(project) AS project FROM window_inventory_links
    WHERE (:pk IS NULL OR (principal_kind=:pk AND principal_id=:pi))
        AND window_key IN (SELECT window_key FROM summaries)
    GROUP BY window_key,workflow_session_id
), counts AS (
    SELECT window_key,COUNT(*) AS linked FROM relations
    WHERE (project IN allowed OR (project IS NULL AND NOT :management))
        AND (:projects IS NULL OR project IN selected)
    GROUP BY window_key
), live AS (
    SELECT json_extract(value,'$.client_window_key') AS window_key,
        json_extract(value,'$.source') AS source,json_extract(value,'$.last_project') AS last_project,
        json_extract(value,'$.first_seen_at_ms') AS first_seen,json_extract(value,'$.last_seen_at_ms') AS last_seen,
        json_extract(value,'$.last_activity_name') AS activity_name,
        json_extract(value,'$.last_activity_status') AS activity_status,
        json_extract(value,'$.last_activity_meaningful') AS activity_meaningful,
        json_extract(value,'$.active_count') AS active_count
    FROM json_each(:live)
), merged AS (
    SELECT s.window_key AS client_window_key,COALESCE(l.source,s.source) AS source,
        COALESCE(l.last_project,s.last_project) AS last_project,
        MIN(s.first_seen,COALESCE(l.first_seen,s.first_seen)) AS first_seen_at_ms,
        MAX(s.last_seen,COALESCE(l.last_seen,s.last_seen)) AS last_seen_at_ms,
        s.last_tool AS last_tool_call_at_ms,s.last_meaningful AS last_meaningful_activity_at_ms,
        COALESCE(l.activity_name,s.activity_name) AS last_activity_name,
        COALESCE(l.activity_status,s.activity_status) AS last_activity_status,
        COALESCE(l.activity_meaningful,s.activity_meaningful) AS last_activity_meaningful,
        COALESCE(l.active_count,0) AS active_count,COALESCE(c.linked,0) AS linked_session_count,s.gap_count AS recorder_gap_count
    FROM summaries s LEFT JOIN live l ON l.window_key=s.window_key LEFT JOIN counts c ON c.window_key=s.window_key
    UNION ALL SELECT l.window_key,l.source,l.last_project,l.first_seen,l.last_seen,NULL,NULL,
        l.activity_name,l.activity_status,l.activity_meaningful,l.active_count,0,0
    FROM live l WHERE NOT EXISTS(SELECT 1 FROM summaries s WHERE s.window_key=l.window_key)
), filtered AS (
    SELECT * FROM merged WHERE (:key IS NULL OR client_window_key=:key)
        AND (:query='' OR instr(lower(client_window_key),lower(:query))>0
            OR instr(lower(COALESCE(last_project,'')),lower(:query))>0
            OR instr(lower(COALESCE(last_activity_name,'')),lower(:query))>0)
)
SELECT *, COUNT(*) OVER() AS total FROM filtered ORDER BY last_seen_at_ms DESC, client_window_key ASC LIMIT :limit OFFSET :offset
