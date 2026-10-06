import { useEffect, useRef, useState } from "react";
import type { DesktopState } from "../../models/topology";
import type { MachineBuildInfo, RuntimeSettings } from "../../models/runtime-shell";
import type { RuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";
import { desktopApi } from "../../lib/desktop-api";
import { useUpdateText } from "../../i18n/update-text";
import { UpdateComponents } from "./UpdateComponents";
import { UpdateWorkflow } from "./UpdateWorkflow";

export function AboutPanel({ state, updates }: { state: DesktopState; updates?: RuntimeUpdates }) {
  const s = useShellText(); const p = useProduct(); const u = useUpdateText();
  const [desktop, setDesktop] = useState<MachineBuildInfo | null>(null);
  const [runtime, setRuntime] = useState<RuntimeSettings | null>(null);
  const [runtimeBusy, setRuntimeBusy] = useState(false);
  const [runtimeError, setRuntimeError] = useState(false);
  const runtimeRequest = useRef(0);
  const showWorkflow = Boolean(updates?.local?.view.upgrade || updates?.local?.view.restart_required || updates?.status?.update_available || updates?.status?.download?.pending_install || updates?.status?.download?.error_kind === "recovery_required");
  const [linkError, setLinkError] = useState(false);
  useEffect(() => { let alive = true;
    const request = ++runtimeRequest.current;
    setRuntime(null); setRuntimeError(false); setRuntimeBusy(true);
    void Promise.resolve().then(() => desktopApi.desktopBuildInfo()).then(value => { if (alive) setDesktop(value); }).catch(() => undefined);
    void Promise.resolve().then(() => desktopApi.runtimeSettings()).then(value => {
      if (alive && request === runtimeRequest.current) setRuntime(value);
    }).catch(() => {
      if (alive && request === runtimeRequest.current) setRuntimeError(true);
    }).finally(() => {
      if (alive && request === runtimeRequest.current) setRuntimeBusy(false);
    });
    return () => { alive = false; ++runtimeRequest.current; };
  }, [state.binaries?.directory, state.binaries?.git_commit, state.binaries?.version]);
  const recheck = async () => {
    if (runtimeBusy || state.current_operation) return;
    const request = ++runtimeRequest.current;
    setRuntimeBusy(true); setRuntimeError(false); setRuntime(null);
    try {
      const next = await desktopApi.recheckRuntime();
      if (request === runtimeRequest.current) setRuntime(next);
    } catch {
      if (request === runtimeRequest.current) setRuntimeError(true);
    } finally {
      if (request === runtimeRequest.current) setRuntimeBusy(false);
    }
  };
  const selected = runtime?.selected;
  const runtimeUnconfirmed = runtimeError || Boolean(runtime?.unavailable_code || selected?.error_code || (selected?.compatibility === "unknown" && selected.checked_at_ms > 0));
  const runtimeVersions = selected?.binaries.map(binary => `${binary.name} ${binary.metadata?.version ?? p(binary.error_code ? "runtimeVersionUnavailable" : "runtimeStartupNotChecked")}`).join(" · ");
  const compatibility = selected?.compatibility === "incompatible" ? p("runtimeIncompatible") : runtimeUnconfirmed ? p("runtimeVersionUnavailable") : selected?.compatibility === "compatible" ? p("runtimeCompatible") : p("runtimeStartupNotChecked");
  const updateMessage = updates?.checking ? u("Checking for updates…") : updates?.manualError ? u("Update check unavailable; Runtime is unaffected.") : updates?.status?.update_available ? u("A new stable WebCodex release is available.") : updates?.status?.state === "up_to_date" ? p("noNewerStableRelease") : u("Update status unknown");
  const link = (kind: "documentation" | "github" | "report_issue" | "contributing" | "desktop_development") => { setLinkError(false); void desktopApi.openDiagnosticResource(kind).catch(() => setLinkError(true)); };
  return <section className="settings-section" aria-labelledby="about-webcodex-title"><h2 id="about-webcodex-title">{s("About WebCodex")}</h2>
    <dl className="runtime-facts"><div><dt>{p("desktopBuildVersion")}</dt><dd>{desktop?.version ?? s("Unknown")}</dd></div><div><dt>{p("desktopBuildRevision")}</dt><dd><code>{desktop?.git_commit ?? s("Unknown")}</code>{desktop?.git_dirty ? ` · ${s("Dirty build")}` : ""}</dd></div>
      <div><dt>{p(updates?.manualError || updates?.status?.cached ? "latestKnownStableRelease" : "latestStableRelease")}</dt><dd>{updates?.status?.latest?.version ?? p(updates?.manualError ? "runtimeVersionUnavailable" : "runtimeStartupNotChecked")}</dd></div>
      <div><dt>{p("localRuntimeVersions")}</dt><dd>{runtimeVersions || p(runtimeUnconfirmed ? "runtimeVersionUnavailable" : "runtimeStartupNotChecked")}</dd></div>
      <div><dt>{p("localRuntimeCompatibility")}</dt><dd>{compatibility}</dd></div></dl>
    {updates && <><UpdateComponents local={updates.local} /><button type="button" className="secondary-button" disabled={updates.actionBusy || Boolean(state.current_operation)} onClick={() => void updates.refreshLocal(true)}>{u("Inspect installed files")}</button>{updates.localError && <p role="status">{u("Local update status could not be confirmed.")}</p>}</>}
    <p className="field-help">{p("localRuntimeVersionsHelp")}</p>
    <button type="button" className="secondary-button" disabled={runtimeBusy || Boolean(state.current_operation)} onClick={() => void recheck()}>{p("recheckLocalRuntime")}</button>
    {runtimeBusy && <p role="status">{p("checkingLocalRuntime")}</p>}
    {runtimeUnconfirmed && <p role="alert">{p(selected?.compatibility === "incompatible" ? "runtimeMismatchHelp" : "localRuntimeCheckFailed")}</p>}
    <p className="field-help">{u("Stable updates can download automatically. Installation and closing Desktop always require your confirmation.")}</p>
    {updates && <><p role="status">{updateMessage}</p>
      <button type="button" className="secondary-button" disabled={updates.checking || updates.actionBusy} onClick={() => void updates.check()}>{u("Check for updates")}</button>
      <label className="update-preference"><input type="checkbox" checked={updates.status?.automatic_download ?? true} disabled={!updates.status || updates.actionBusy} onChange={event => void updates.setAutomaticDownload(event.currentTarget.checked)} />{u("Automatically download stable updates")}</label>
      {updates.actionError && !showWorkflow && <p role="status">{u("The update action could not be completed. Review the update status.")}</p>}
      {showWorkflow && <UpdateWorkflow updates={updates} />}</>}
    <div className="shell-actions"><button type="button" className="text-button" onClick={() => link("documentation")}>{s("Documentation")}</button><button type="button" className="text-button" onClick={() => link("report_issue")}>{s("Report issue")}</button><button type="button" className="text-button" onClick={() => link("desktop_development")}>{s("Build from source")}</button><button type="button" className="text-button" onClick={() => link("contributing")}>{s("Contribute")}</button><button type="button" className="text-button" onClick={() => link("github")}>{s("GitHub")}</button></div>
    {linkError && <p role="status">{s("Unable to open this location.")}</p>}
  </section>;
}
export function UpdateBanner({ updates }: { updates: RuntimeUpdates }) {
  const s = useUpdateText(); const notice = updates.status?.latest;
  const pending = updates.status?.download?.pending_install;
  const recovery = updates.status?.download?.error_kind === "recovery_required";
  const durable = updates.local?.view.upgrade || updates.local?.view.restart_required;
  if ((!notice || !updates.status?.show_banner) && !pending && !recovery && !durable) return null;
  const version = pending ? updates.status?.download.version : notice?.version ?? updates.local?.view.upgrade?.version;
  return <aside className="runtime-update-banner" aria-label={version ? `WebCodex ${version}` : s("Stable update")}>
    <div><strong>{version ? `WebCodex ${version}` : s("Stable update")}</strong>{notice && !recovery && <p>{s(notice.compatibility === "runtime_compatible" ? "Your current Desktop can use this Runtime update." : notice.compatibility === "desktop_required" ? "This release requires a Desktop update." : "A new stable WebCodex release is available.")}</p>}</div>
    <UpdateWorkflow updates={updates} banner />
  </aside>;
}
