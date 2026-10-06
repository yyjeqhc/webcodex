-- The LIMIT is inside the source subquery: filtering one Window must not scan
-- its entire permanent transcript. History is not a pending delivery queue.
SELECT message_id, 'operator', 'inbound', NULL, message,
       created_at_ms, NULL, kind, priority, context_session_id, context_project,
       requires_ack, first_projected_at_ms, first_ack_observed_at_ms
FROM (
    SELECT * FROM window_operator_messages
    WHERE principal_kind = ?1 AND principal_id = ?2
    ORDER BY created_at_ms DESC, message_id DESC
    LIMIT 512
) AS recent_operator_messages
WHERE recipient_window_key = ?3 AND first_ack_observed_at_ms IS NULL
  AND (first_projected_at_ms IS NULL OR requires_ack = 1)
ORDER BY (first_projected_at_ms IS NULL) DESC, created_at_ms, message_id
LIMIT ?4
