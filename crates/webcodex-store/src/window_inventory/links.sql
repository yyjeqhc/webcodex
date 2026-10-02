INSERT INTO window_inventory_links
SELECT e.client_window_key,json_array(e.principal_correlation_kind,e.principal_correlation_id),
    e.principal_correlation_kind,e.principal_correlation_id,l.workflow_session_id,MAX(l.project),MAX(l.linked_at_ms)
FROM action_events e JOIN action_event_workflow_links l ON l.event_id=e.event_id
WHERE __SELECTION__ AND e.client_window_key IS NOT NULL
GROUP BY e.client_window_key,e.principal_correlation_kind,e.principal_correlation_id,l.workflow_session_id
ON CONFLICT(window_key,principal_key,workflow_session_id) DO UPDATE SET
    project=COALESCE(MAX(project,excluded.project),project,excluded.project),
    last_linked=MAX(last_linked,excluded.last_linked);
