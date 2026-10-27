import { useEffect, useState, type ReactNode } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { TraceEvent, TracePage } from "../api/traces.js";
import { useTraceRead } from "../state/useTraceRead.js";
import { CopyIdentity } from "./ui/CopyIdentity.js";

const MAX_DISPLAYED_TRACE_EVENTS = 240;
type Props = {
  client: RuntimeV2Client; traceRef: string; language: RuntimeLanguage;
  expanded?: boolean; onExpandedChange?: (open: boolean) => void;
};

export function TraceCallDetails(props: Props) {
  // Exact identity changes discard both the pending read and cached content.
  return <TraceDisclosure key={props.traceRef} {...props} />;
}

function TraceDisclosure({ client, traceRef, language, expanded, onExpandedChange }: Props) {
  const [localOpen, setLocalOpen] = useState(false);
  const open = expanded ?? localOpen;
  const [visited, setVisited] = useState(open);
  useEffect(() => { if (open) setVisited(true); }, [open]);
  return <div className="trace-call-details" onClick={event => event.stopPropagation()}>
    <button type="button" className="text-button" aria-expanded={open} onClick={() => {
      setLocalOpen(!open);
      onExpandedChange?.(!open);
    }}>{translate(open ? "Hide call diagnostics" : "Inspect call diagnostics", language)}</button>
    {(visited || open) && <TraceCallPanel client={client} traceRef={traceRef} language={language} open={open} />}
  </div>;
}

function TraceJson({ title, value, initiallyOpen = false, children }: {
  title: string; value: unknown; initiallyOpen?: boolean; children?: ReactNode;
}) {
  const [open, setOpen] = useState(initiallyOpen);
  return <details className="trace-event" open={open} onToggle={event => setOpen(event.currentTarget.open)}>
    <summary>{title}</summary>
    {open && <><pre>{JSON.stringify(value, null, 2)}</pre>{children}</>}
  </details>;
}

function TraceCallPanel({ client, traceRef, language, open }: Props & { open: boolean }) {
  const t = (value: string) => translate(value, language);
  const request = useTraceRead(client);
  const [page, setPage] = useState<TracePage | null>(null);
  const [events, setEvents] = useState<TraceEvent[]>([]);
  const [payload, setPayload] = useState<TracePage | null>(null);
  useEffect(() => { setPage(null); setEvents([]); setPayload(null); }, [client]);
  const load = async (offset = 0) => {
    const next = await request.read({ trace_ref: traceRef, offset, limit: 12 });
    if (next === undefined) return;
    if (!next) { setPage(null); setEvents([]); setPayload(null); return; }
    setPage(next);
    setEvents(previous => (offset ? [...previous, ...(next.events || [])] : next.events || []).slice(0, MAX_DISPLAYED_TRACE_EVENTS));
    setPayload(null);
  };
  // Only an explicit open triggers a read. Failures never schedule a retry.
  // Keeping this small owner after first opening preserves cached pages on close.
  useEffect(() => { if (open && !page && !request.pending) void load(); }, [open]);
  if (!open) return null;
  return <section aria-label={t("Call diagnostics")}>
    <CopyIdentity value={traceRef} label={t("Trace")} language={language} />
    <p className="inventory-note">{t("Observed Server evidence only; not proof of delivery or model reading.")}</p>
    <button type="button" className="text-button" disabled={request.pending} onClick={() => void load()}>{t("Refresh diagnostics")}</button>
    {request.pending && <p role="status">{t("Loading diagnostics…")}</p>}
    {request.error && <p role="alert">{t(request.error)}</p>}
    {page?.status === "unavailable" && <p className="inventory-note">{t("Capture is not retained; it may be disabled, dropped, expired or evicted.")}</p>}
    {page?.status === "available" && <p className="inventory-note">{t("Captured mode")}: {page.trace_mode} · {t("Current capture")}: {page.capture_mode}</p>}
    {page?.coverage === "incomplete" && <p role="alert">{t("Trace is incomplete; captured loss markers report missing events.")} · {page.known_dropped_events}</p>}
    {page?.status === "available" && <p className="inventory-note">{t("Capture completeness is unverified, even when a Handler finish is present.")}</p>}
    {page?.capture_health && <TraceJson title={t("Capture health (process-wide)")} value={page.capture_health} />}
    {events.map((event, index) => <TraceJson key={index} title={event.phase || event.event || t("Event")}
      initiallyOpen={Boolean(event.diagnostic)} value={event.diagnostic ?? event}>
      {typeof event.payload_index === "number" && <button type="button" className="text-button" disabled={request.pending} onClick={async () => {
        setPayload(null);
        const value = await request.read({ trace_ref: traceRef, payload_index: event.payload_index });
        if (value) setPayload(value);
      }}>{t("Read retained full payload")} · {event.payload_bytes ?? "?"} B</button>}
    </TraceJson>)}
    {page?.next_offset != null && events.length < MAX_DISPLAYED_TRACE_EVENTS && <button type="button" className="text-button" disabled={request.pending} onClick={() => void load(page.next_offset!)}>{t("More trace events")}</button>}
    {events.length >= MAX_DISPLAYED_TRACE_EVENTS && <p className="inventory-note">{t("Diagnostic view limit reached. Use the exact trace reader for more.")}</p>}
    {payload && <TraceJson title={t("Retained full payload")} initiallyOpen value={payload.payload_available ? payload.payload : payload} />}
  </section>;
}
