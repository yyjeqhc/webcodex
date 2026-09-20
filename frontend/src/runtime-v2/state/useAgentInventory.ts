import { useEffect, useState } from "react";
import { fetchCommunicationAgents } from "../api/runtime.js";
import type { RuntimeV2Client } from "../api/client.js";

export type AgentInventoryState = {
  available: boolean | null;
  count: number | null;
};

export function useAgentInventory(client: RuntimeV2Client, enabled: boolean): AgentInventoryState {
  const [available, setAvailable] = useState<boolean | null>(null);
  const [count, setCount] = useState<number | null>(null);

  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    void fetchCommunicationAgents(client, controller.signal).then((response) => {
      if (controller.signal.aborted || !response) return;
      if (response.status === 403) {
        setAvailable(false);
        setCount(null);
        return;
      }
      if (!response.ok || !response.data) {
        setAvailable(null);
        return;
      }
      setAvailable(true);
      const payload = response.data as { agents?: unknown[]; returned?: number; total?: number };
      setCount(typeof payload.total === "number" ? payload.total :
        typeof payload.returned === "number" ? payload.returned :
          Array.isArray(payload.agents) ? payload.agents.length : 0);
    });
    return () => controller.abort();
  }, [client, enabled]);

  return { available, count };
}
