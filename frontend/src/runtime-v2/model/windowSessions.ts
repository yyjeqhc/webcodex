import type { WindowDetail, WindowLinkedSession } from "./types.js";

function cloneSession(session: WindowLinkedSession): WindowLinkedSession {
  return {
    ...session,
    relations: session.relations.slice(),
  };
}

/**
 * Exact Window↔Session catalog for UI navigation.
 * The dedicated relation inventory is authoritative and can outlive retained
 * activity; retained ActionAudit links supplement it when the bounded relation
 * inventory omits a still-visible exact relation.
 */
export function windowSessionCatalog(detail: WindowDetail): WindowLinkedSession[] {
  const byId = new Map(
    detail.linked_sessions.map((session) => [session.workflow_session_id, cloneSession(session)]),
  );

  for (const activity of detail.activity) {
    for (const link of activity.workflow_sessions) {
      const sessionId = link.workflow_session_id;
      const observedAt = activity.ended_at_ms;
      const project = link.project || activity.project;
      const existing = byId.get(sessionId);
      if (existing) {
        existing.first_linked_at_ms = Math.min(existing.first_linked_at_ms, observedAt);
        existing.last_linked_at_ms = Math.max(existing.last_linked_at_ms, observedAt);
        if (!existing.project && project) existing.project = project;
        if (!existing.relations.includes(link.relation)) {
          existing.relations.push(link.relation);
          existing.relation_count = Math.max(existing.relation_count, existing.relations.length);
        }
        continue;
      }
      byId.set(sessionId, {
        workflow_session_id: sessionId,
        project,
        first_linked_at_ms: observedAt,
        last_linked_at_ms: observedAt,
        relations: [link.relation],
        relation_count: 1,
      });
    }
  }

  return [...byId.values()].sort((left, right) =>
    left.first_linked_at_ms - right.first_linked_at_ms ||
    left.workflow_session_id.localeCompare(right.workflow_session_id));
}
