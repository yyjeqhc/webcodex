import { createReadCache } from "./read-cache.mjs";

// Detail requests belong to exact Window/Project/trace identities, not the first
// DOM node that asked. A refresh may replace that node while sharing the read.
export function createActivityDetails({ read, scope, alive, validate, render }) {
  const cache = createReadCache();
  return Object.freeze({
    async load(row, root, visible) {
      if (!alive() || !visible()) return;
      const trace = row.server_trace_id;
      if (!trace || row.running) {
        root.textContent = row.running ? "Live call · detailed timing is available after completion." : "No trace identity is available for this retained event.";
        root.className = "activity-detail activity-detail-state";
        return;
      }
      const selected = scope();
      const current = () => alive() && visible() && selected.project === scope().project && selected.window === scope().window;
      const key = JSON.stringify([selected.project, selected.window, trace]);
      const display = value => {
        if (!current()) return;
        root.className = "activity-detail"; render(root, value);
      };
      const cached = cache.peek(key);
      if (cached) { display(cached); return; }
      root.className = "activity-detail activity-detail-state";
      root.textContent = "Loading details…";
      try {
        const detail = await cache.read(key, async validGeneration => {
          const value = await read(selected.project, trace);
          if (!validGeneration()) throw new Error("Read view replaced");
          if (!validate(value) || value.server_trace_id !== trace) throw new Error("Invalid activity detail");
          return value;
        });
        display(detail);
      } catch (_) {
        if (current()) root.textContent = "Details unavailable.";
      }
    },
    dispose: cache.dispose,
  });
}
