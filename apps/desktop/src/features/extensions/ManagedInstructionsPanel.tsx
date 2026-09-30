import { useEffect, useRef, useState } from "react";
import { Button, Textarea } from "@mantine/core";
import { desktopApi } from "../../lib/desktop-api";
import { normalizeDesktopError } from "../../i18n/presentation";
import { useProduct } from "../../i18n/product";
import { useInstructionsText } from "../../i18n/instructions";
import type { DesktopError, DesktopState, RunnerSettings } from "../../models/topology";
import type { ManagedInstructionsSnapshot } from "../../models/managed-instructions";
import { sameProjectPath } from "../workspace/WorkspaceContext";

export function ManagedInstructionsPanel({ active, settings, disabled, onState, onEnabled }: {
  active: boolean; settings: RunnerSettings | null; disabled: boolean;
  onState: (state: DesktopState) => void; onEnabled: () => void;
}) {
  const text = useInstructionsText(); const p = useProduct();
  const [file, setFile] = useState<ManagedInstructionsSnapshot | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [notice, setNotice] = useState<"saved" | "applied" | null>(null);
  const [discard, setDiscard] = useState(false);
  const sequence = useRef(0); const attempted = useRef(false);
  useEffect(() => () => { sequence.current += 1; attempted.current = false; }, []);
  const dirty = file !== null && draft !== file.content;
  const bytes = new TextEncoder().encode(draft).length;
  const tooLarge = bytes > 1024 * 1024;
  // Presentation only. Enable always passes the exact saved Runner target and
  // settings fence; this path comparison cannot authorize a file write.
  const enabled = !!file && !!settings?.paths.instruction_files.some(path => sameProjectPath(path, file.path));
  const blocked = busy || disabled;
  const load = async () => {
    const id = ++sequence.current; setBusy(true); setError(null); setNotice(null); setDiscard(false);
    try {
      const next = await desktopApi.managedInstructionsRead();
      if (sequence.current !== id) return;
      setFile(next); setDraft(next.content);
    } catch (value) { if (sequence.current === id) setError(normalizeDesktopError(value)); }
    finally { if (sequence.current === id) setBusy(false); }
  };
  useEffect(() => {
    if (active && !attempted.current) { attempted.current = true; void load(); }
  }, [active]); // Do not overwrite a local draft on Project/catalog/Runner refresh.
  const save = async () => {
    if (!file || blocked || tooLarge) return;
    const id = ++sequence.current; setBusy(true); setError(null); setNotice(null);
    try {
      const next = await desktopApi.managedInstructionsSave(file.revision, draft);
      if (sequence.current !== id) return;
      setFile(next); setDraft(next.content); setNotice("saved");
    } catch (value) { if (sequence.current === id) setError(normalizeDesktopError(value)); }
    finally { if (sequence.current === id) setBusy(false); }
  };
  const enable = async () => {
    if (!file || !settings || blocked || dirty) return;
    const id = ++sequence.current; setBusy(true); setError(null); setNotice(null);
    try {
      const nextState = await desktopApi.managedInstructionsEnable(settings.target, settings.paths, file.revision);
      if (sequence.current !== id) return;
      onState(nextState); onEnabled(); setNotice("applied");
      const nextFile = await desktopApi.managedInstructionsRead();
      if (sequence.current === id) { setFile(nextFile); setDraft(nextFile.content); }
    } catch (value) {
      if (sequence.current === id) {
        setError(normalizeDesktopError(value)); onEnabled();
        // A failed config apply may still have created the managed file. Read
        // that file once, not another enable/save/reload. No user draft is lost.
        try { const next = await desktopApi.managedInstructionsRead(); if (sequence.current === id) setFile(next); } catch { /* preserve last observation */ }
      }
    } finally { if (sequence.current === id) setBusy(false); }
  };
  return <section hidden={!active} className="settings-section managed-instructions" aria-labelledby="managed-instructions-title">
    <h2 id="managed-instructions-title">{text("title")}</h2>
    <p>{text("help")}</p>
    {file && <>
      <p>{enabled ? text("configured") : text("disabled")}</p>
      <Textarea label={text("content")} id="managed-global-instructions" value={draft} onChange={event => { setDraft(event.currentTarget.value); setNotice(null); }} rows={8} disabled={blocked} spellCheck={false} autosize={false} />
      <p className="muted-text">{text("limit")} ({bytes.toLocaleString()} B)</p>
      {dirty && <p role="status">{text("unsaved")}</p>}
      {tooLarge && <p role="alert">{text("tooLarge")}</p>}
      <div className="extension-toolbar">
        <Button type="button" onClick={() => void save()} disabled={blocked || tooLarge || (file.exists && !dirty)}>{text("save")}</Button>
        {!enabled && <Button type="button" variant="default" onClick={() => void enable()} disabled={blocked || !settings || dirty}>{text("enable")}</Button>}
      </div>
      {!enabled && dirty && <p>{text("saveFirst")}</p>}
      {!settings && <p>{text("configure")}</p>}
      <details><summary>{p("details")}</summary><code>{file.path}</code></details>
    </>}
    <Button type="button" variant="subtle" disabled={blocked} onClick={() => dirty ? setDiscard(true) : void load()}>{text("reload")}</Button>
    {discard && <div className="workspace-notice"><p>{text("unsaved")}</p><Button type="button" variant="default" onClick={() => void load()}>{text("discard")}</Button><Button type="button" variant="subtle" onClick={() => setDiscard(false)}>{text("keep")}</Button></div>}
    {busy && <p role="status">{p("loading")}</p>}
    {notice && <p role="status">{text(notice)}</p>}
    {error && <div className="error-card" role="alert"><strong>{error.message}</strong><span>{error.next_action}</span><code>{error.code}</code></div>}
  </section>;
}
