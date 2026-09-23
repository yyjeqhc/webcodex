import { useEffect, useState } from "react";
import type { ContinuationSummary } from "../../models/runtime-shell";
import { useShellText } from "../../i18n/runtime-shell";

export function ContinuationFacts({ value, observedAt }: { value: ContinuationSummary | null; observedAt?: number }) {
  const s = useShellText(); const [now, setNow] = useState(Date.now());
  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(timer); }, []);
  if (!value) return <p className="field-help">{s("No completed meaningful call is available in this bounded observation.")}</p>;
  const elapsed = value.elapsed_ms == null ? null : value.elapsed_ms + Math.max(0, now - (observedAt ?? now));
  const execution = value.execution === "completed" ? "Completed" : value.execution === "failed" ? "Failed" : value.execution === "cancelled" ? "Cancelled" : "Unknown";
  const response = value.response_handoff === "handler_returned" ? "Returned to HTTP framework" : value.response_handoff === "stream_started" ? "Response stream started" : "Response handoff not confirmed";
  return <section className="continuation-facts" aria-label={s("Last WebCodex call")}>
    <h3>{s("Last WebCodex call")} <code>{value.tool_name ?? s("Unknown")}</code></h3>
    <dl className="runtime-facts"><div><dt>{s("Execution")}</dt><dd>{s(execution)}</dd></div><div><dt>{s("Response")}</dt><dd>{s(response)}</dd></div>
      <div><dt>{s("Next meaningful WebCodex call")}</dt><dd>{s(value.next_meaningful_call === "observed" ? "Observed" : "Not observed")}</dd></div>
      <div><dt>{s("Elapsed")}</dt><dd>{elapsed == null ? "—" : `${Math.floor(elapsed / 1000)} s`}</dd></div>
      {value.service_ms != null && <div><dt>{s("Service time")}</dt><dd>{value.service_ms} ms</dd></div>}</dl>
    {value.response_handoff === "handler_returned" && value.execution === "completed" && value.next_meaningful_call === "not_observed" && <p>{s("WebCodex completed and returned this response to its HTTP framework; no later meaningful call has been observed in this Window.")}</p>}
    <p className="field-help">{s("The gap may include network, Tunnel delivery, Host scheduling, model inference, user actions or time WebCodex cannot observe. Handler return does not confirm Tunnel, MCP Host, or model receipt.")}</p>
    {value.history_partial && <p className="field-help">{s("History is partial")}</p>}
  </section>;
}
