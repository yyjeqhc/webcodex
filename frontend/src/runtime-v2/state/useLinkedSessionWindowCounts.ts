import { useEffect, useState } from "react";
import { fetchSessionDetail } from "../api/sessions.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { WindowLinkedSession } from "../model/types.js";

const MAX_ENRICHMENT = 20;
const CONCURRENCY = 3;

export function useLinkedSessionWindowCounts(
  client: RuntimeV2Client,
  enabled: boolean,
  sessions: WindowLinkedSession[],
): Map<string, number | null> {
  const [counts, setCounts] = useState<Map<string, number | null>>(new Map());

  useEffect(() => {
    if (!enabled) return;
    const targets = sessions.filter((session) => Boolean(session.project)).slice(0, MAX_ENRICHMENT);
    if (!targets.length) {
      setCounts(new Map());
      return;
    }
    const controller = new AbortController();
    let cursor = 0;
    let running = 0;
    let disposed = false;
    const next = () => {
      while (!disposed && !controller.signal.aborted && running < CONCURRENCY && cursor < targets.length) {
        const session = targets[cursor++];
        running += 1;
        void fetchSessionDetail(client, session.project!, session.workflow_session_id, controller.signal, 1)
          .then((response) => {
            if (disposed || controller.signal.aborted) return;
            setCounts((existing) => {
              const updated = new Map(existing);
              updated.set(
                session.workflow_session_id,
                response?.ok && response.data ? response.data.linked_windows.length : null,
              );
              return updated;
            });
          })
          .finally(() => {
            running -= 1;
            next();
          });
      }
    };
    next();
    return () => {
      disposed = true;
      controller.abort();
    };
  }, [
    client,
    enabled,
    sessions.map((session) => session.workflow_session_id + ":" + (session.project || "")).join("|"),
  ]);

  return counts;
}
