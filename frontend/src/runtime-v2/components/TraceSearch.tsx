import { useEffect, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { TracePage, TraceQuery } from "../api/traces.js";
import { useTraceRead } from "../state/useTraceRead.js";
import { absoluteTime, shortId } from "../model/format.js";
import { TraceCallDetails } from "./TraceCallDetails.js";

function localDate(value: number): string {
  const date = new Date(value);
  return new Date(value - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 19);
}

export function TraceSearch({ client, language, onSelectWindow, title = "Find calls by time or Project" }: {
  title?: string;
  client: RuntimeV2Client; language: RuntimeLanguage; onSelectWindow: (key: string) => void;
}) {
  const t = (value: string) => translate(value, language);
  const [project, setProject] = useState("");
  const [tool, setTool] = useState("");
  const [since, setSince] = useState(() => localDate(Date.now() - 86_400_000));
  const [until, setUntil] = useState(() => localDate(Date.now()));
  const [includePassive, setIncludePassive] = useState(false);
  const [page, setPage] = useState<TracePage | null>(null);
  const [invalid, setInvalid] = useState(false);
  const request = useTraceRead(client);
  useEffect(() => { setPage(null); }, [client]);
  const search = async (query?: TraceQuery, offset = 0) => {
    setInvalid(false);
    const from = new Date(since).getTime(); const to = new Date(until).getTime();
    if (!query && (!Number.isFinite(from) || !Number.isFinite(to) || from > to || to - from > 31 * 86_400_000)) {
      request.cancel(); setPage(null); setInvalid(true); return;
    }
    setPage(null);
    const value = await request.read({ query: query ?? {
      since_ms: from, until_ms: to, ...(project.trim() ? { project: project.trim() } : {}),
      ...(tool.trim() ? { tool_name: tool.trim() } : {}), include_nonmeaningful: includePassive,
    }, offset, limit: 20 });
    if (value) setPage(value);
  };
  return <details className="trace-search">
    <summary>{t(title)}</summary>
    <p className="inventory-note">{t("Administrator diagnostics. No Window hash is required. Queries run only on request.")}</p>
    <form onSubmit={event => { event.preventDefault(); void search(); }}>
      <label>{t("Exact Project ID")}<input value={project} onChange={event => setProject(event.target.value)} placeholder="agent:special:project" /></label>
      <label>{t("Exact tool name")}<input value={tool} onChange={event => setTool(event.target.value)} placeholder="work_on_project" /></label>
      <label>{t("From")}<input type="datetime-local" step="1" value={since} onChange={event => setSince(event.target.value)} /></label>
      <label>{t("Until")}<input type="datetime-local" step="1" value={until} onChange={event => setUntil(event.target.value)} /></label>
      <label className="trace-passive"><input type="checkbox" checked={includePassive} onChange={event => setIncludePassive(event.target.checked)} />{t("Include passive calls")}</label>
      <button type="submit" className="text-button">{t("Search retained calls")}</button>
    </form>
    {invalid && <p role="alert">{t("Choose an ordered time range of at most 31 days.")}</p>}
    {request.pending && <p role="status">{t("Loading diagnostics…")}</p>}
    {request.error && <p role="alert">{t(request.error)}</p>}
    {page && <div className="trace-search-results">
      <p className="inventory-note">{t("Showing retained calls in this page, not a complete Window history.")}</p>
      {page.calls?.map(call => <article key={call.trace_ref} className="trace-search-call">
        <strong>{call.tool_name}</strong><time>{absoluteTime(call.observed_at_ms)}</time>
        <small>{call.project}</small>
        {call.window_key ? <button type="button" className="text-button" onClick={() => onSelectWindow(call.window_key!)}>{t("Open Window")} · {shortId(call.window_key, 10, 4)}</button> : <span>{t("Window identity unavailable")}</span>}
        <TraceCallDetails client={client} traceRef={call.trace_ref} language={language} />
      </article>)}
      {!page.calls?.length && <p>{t("No retained calls match these filters.")}</p>}
      {page.next_offset != null && <button type="button" className="text-button" disabled={request.pending} onClick={() => void search(page.query, page.next_offset!)}>{t("Next call page")}</button>}
    </div>}
  </details>;
}
