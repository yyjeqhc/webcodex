-- Restrict to the indexed principal/Project/recent interval BEFORE grouping.
-- Only compact covered columns are read; message/event bodies are never loaded.
WITH recent AS MATERIALIZED (
    SELECT e.client_window_key, MAX(e.client_window_source) AS source,
           MAX(e.window_ended_at_ms) AS last_activity
    FROM action_events e
    WHERE e.client_window_key IS NOT NULL AND e.client_window_key <> ?1
      AND e.principal_correlation_kind = ?2 AND e.principal_correlation_id = ?3
      AND e.project = ?4 AND e.window_meaningful = 1 AND e.window_ended_at_ms >= ?5
    GROUP BY e.client_window_key
)
SELECT r.client_window_key, r.source, r.last_activity FROM recent r
WHERE NOT EXISTS (
    SELECT 1 FROM window_peer_discoveries d
    WHERE d.principal_kind=?2 AND d.principal_id=?3
      AND d.observer_window_key=?1 AND d.peer_window_key=r.client_window_key AND d.project=?4
)
ORDER BY r.last_activity DESC, r.client_window_key ASC LIMIT ?6
