import { useEffect, useState } from "react";
import type { DesktopState } from "../../models/topology";
import type { MachineBuildInfo, RuntimeSettings } from "../../models/runtime-shell";
import type { RuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import { useShellText } from "../../i18n/runtime-shell";
import { desktopApi } from "../../lib/desktop-api";
import { UpdateWorkflow } from "./UpdateWorkflow";

export function AboutPanel({ state, updates }: { state: DesktopState; updates?: RuntimeUpdates }) {
  const s = useShellText(); const [desktop, setDesktop] = useState<MachineBuildInfo | null>(null); const [runtime, setRuntime] = useState<RuntimeSettings | null>(null);
  const [linkError, setLinkError] = useState(false);
  useEffect(() => { let alive = true;
    void Promise.resolve().then(() => desktopApi.desktopBuildInfo()).then(value => { if (alive) setDesktop(value); }).catch(() => undefined);
    void Promise.resolve().then(() => desktopApi.runtimeSettings()).then(value => { if (alive) setRuntime(value); }).catch(() => undefined);
    return () => { alive = false; };
  }, [state.binaries?.directory, state.binaries?.git_commit]);
  const link = (kind: "documentation" | "github" | "report_issue" | "contributing" | "desktop_development") => { setLinkError(false); void desktopApi.openDiagnosticResource(kind).catch(() => setLinkError(true)); };
  return <section className="settings-section" aria-labelledby="about-webcodex-title"><h2 id="about-webcodex-title">{s("About WebCodex")}</h2>
    <dl className="runtime-facts"><div><dt>{s("Desktop version")}</dt><dd>{desktop?.version ?? s("Unknown")}</dd></div><div><dt>{s("Desktop revision")}</dt><dd><code>{desktop?.git_commit ?? s("Unknown")}</code>{desktop?.git_dirty ? ` · ${s("Dirty build")}` : ""}</dd></div>
      <div><dt>{s("Runtime versions")}</dt><dd>{runtime?.selected?.binaries.map(binary => `${binary.name} ${binary.metadata?.version ?? "—"}`).join(" · ") || state.binaries?.version || s("Unknown")}</dd></div>
      <div><dt>{s("Compatibility")}</dt><dd>{s(runtime?.selected?.compatibility === "compatible" ? "Compatible" : runtime?.selected?.compatibility === "incompatible" ? "Incompatible" : "Unknown")}</dd></div></dl>
    <p className="field-help">{s("Stable updates can download automatically. Installation and closing Desktop always require your confirmation.")}</p>
    {updates && <><p role="status">{s(updates.checking ? "Checking for updates…" : updates.manualError ? "Update check unavailable; Runtime is unaffected." : updates.status?.update_available ? "A new stable WebCodex release is available." : updates.status?.state === "up_to_date" ? "Up to date" : "Update status unknown")}{updates.status?.latest ? ` · ${updates.status.latest.version}` : ""}</p>
      <button type="button" className="secondary-button" disabled={updates.checking || updates.actionBusy} onClick={() => void updates.check()}>{s("Check for updates")}</button>
      <label className="update-preference"><input type="checkbox" checked={updates.status?.automatic_download ?? true} disabled={!updates.status || updates.actionBusy} onChange={event => void updates.setAutomaticDownload(event.currentTarget.checked)} />{s("Automatically download stable updates")}</label>
      {(updates.status?.update_available || updates.status?.download?.pending_install) && <UpdateWorkflow updates={updates} />}</>}
    <div className="shell-actions"><button type="button" className="text-button" onClick={() => link("documentation")}>{s("Documentation")}</button><button type="button" className="text-button" onClick={() => link("report_issue")}>{s("Report issue")}</button><button type="button" className="text-button" onClick={() => link("desktop_development")}>{s("Build from source")}</button><button type="button" className="text-button" onClick={() => link("contributing")}>{s("Contribute")}</button><button type="button" className="text-button" onClick={() => link("github")}>{s("GitHub")}</button></div>
    {linkError && <p role="status">{s("Unable to open this location.")}</p>}
  </section>;
}
export function UpdateBanner({ updates }: { updates: RuntimeUpdates }) {
  const s = useShellText(); const notice = updates.status?.latest;
  const pending = updates.status?.download?.pending_install;
  if ((!notice || !updates.status?.show_banner) && !pending) return null;
  const version = pending ? updates.status?.download.version : notice?.version;
  return <aside className="runtime-update-banner" aria-label={`WebCodex ${version}`}>
    <div><strong>WebCodex {version}</strong>{notice && <p>{s(notice.compatibility === "runtime_compatible" ? "Your current Desktop can use this Runtime update." : notice.compatibility === "desktop_required" ? "This release requires a Desktop update." : "A new stable WebCodex release is available.")}</p>}</div>
    <UpdateWorkflow updates={updates} banner />
  </aside>;
}
