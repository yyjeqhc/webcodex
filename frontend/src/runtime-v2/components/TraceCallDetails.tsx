import { useEffect, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { TraceEvent, TracePage } from "../api/traces.js";
import { useTraceRead } from "../state/useTraceRead.js";
import { CopyIdentity } from "./ui/CopyIdentity.js";

const MAX_DISPLAYED_TRACE_EVENTS = 240;

export function TraceCallDetails({ client, traceRef, language }: {
  client: RuntimeV2Client; traceRef: string; language: RuntimeLanguage;
}) {
  // Changing exact identity unmounts the old request and its cached content.
  return <TraceCallPanel key={traceRef} client={client} traceRef={traceRef} language={language} />;
}

function TraceCallPanel({ client, traceRef, language }: {
  client: RuntimeV2Client; traceRef: string; language: RuntimeLanguage;
}) {
  const t = (value: string) => translate(value, language);
  const request = useTraceRead(client);
  const [open, setOpen] = useState(false);
  const [page, setPage] = useState<TracePage | null>(null);
  const [events, setEvents] = useState<TraceEvent[]>([]);
  const [payload, setPayload] = useState<TracePage | null>(null);
  useEffect(() => { setPage(null); setEvents([]); setPayload(null); }, [client]);
  const load = async (offset = 0) => {
    const next = await request.read({ trace_ref: traceRef, offset, limit: 12 });
    if (next === undefined) return; // Superseded/aborted reads cannot clear newer evidence.
    if (!next) { setPage(null); setEvents([]); setPayload(null); return; }
    setPage(next);
    setEvents(previous => {
      const combined = offset ? [...previous, ...(next.events || [])] : next.events || [];
      return combined.slice(0, MAX_DISPLAYED_TRACE_EVENTS);
    });
    setPayload(null);
  };
  const toggle = () => {
    setOpen(!open);
    if (!open && !page && !request.pending) void load();
  };
  return <div className="trace-call-details" onClick={event => event.stopPropagation()}>
    <button type="button" className="text-button" aria-expanded={open} onClick={toggle}>
      {t(open ? "Hide call diagnostics" : "Inspect call diagnostics")}
    </button>
    {open && <section aria-label={t("Call diagnostics")}>
      <CopyIdentity value={traceRef} label={t("Trace")} language={language} />
      <p className="inventory-note">{t("Observed Server evidence only; not proof of delivery or model reading.")}</p>
      <button type="button" className="text-button" disabled={request.pending} onClick={() => void load()}>{t("Refresh diagnostics")}</button>
      {request.pending && <p role="status">{t("Loading diagnostics…")}</p>}
      {request.error && <p role="alert">{t(request.error)}</p>}
      {page?.status === "unavailable" && <p className="inventory-note">{t("Capture is not retained; it may be disabled, dropped, expired or evicted.")}</p>}
      {page?.status === "available" && <p className="inventory-note">{t("Captured mode")}: {page.trace_mode} · {t("Current capture")}: {page.capture_mode}</p>}
      {page?.capture_health && <details className="trace-event"><summary>{t("Capture health (process-wide)")}</summary><pre>{JSON.stringify(page.capture_health, null, 2)}</pre></details>}
      {events.map((event, index) => <details key={index} className="trace-event" open={Boolean(event.diagnostic)}>
        <summary>{event.phase || event.event || t("Event")}</summary>
        <pre>{JSON.stringify(event.diagnostic ?? event, null, 2)}</pre>
        {typeof event.payload_index === "number" && <button type="button" className="text-button" disabled={request.pending} onClick={async () => {
          setPayload(null);
          const value = await request.read({ trace_ref: traceRef, payload_index: event.payload_index });
          if (value) setPayload(value);
        }}>{t("Read retained full payload")} · {event.payload_bytes ?? "?"} B</button>}
      </details>)}
      {page?.next_offset != null && events.length < MAX_DISPLAYED_TRACE_EVENTS && <button type="button" className="text-button" disabled={request.pending} onClick={() => void load(page.next_offset!)}>{t("More trace events")}</button>}
      {events.length >= MAX_DISPLAYED_TRACE_EVENTS && <p className="inventory-note">{t("Diagnostic view limit reached. Use the exact trace reader for more.")}</p>}
      {payload && <details className="trace-event" open><summary>{t("Retained full payload")}</summary>
        <pre>{JSON.stringify(payload.payload_available ? payload.payload : payload, null, 2)}</pre>
      </details>}
    </section>}
  </div>;
}
