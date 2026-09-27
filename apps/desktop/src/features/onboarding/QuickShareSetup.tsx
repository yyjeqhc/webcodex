import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { displayProjectPath } from "../../../../../frontend/src/ui/projectPresentation";
import { desktopApi, type QuickShareProvider } from "../../lib/desktop-api";
import { useLocale } from "../../i18n/locale";
import { normalizeDesktopError, desktopErrorPresentation } from "../../i18n/presentation";
import type { DesktopError, DesktopState, ProjectSelection } from "../../models/topology";

// Temporary single-project sharing remains separate from persistent setup.
export function QuickShareSetup({ state, onState, onBack, onComplete }: {
  state: DesktopState; onState: (state: DesktopState) => void; onBack: () => void; onComplete?: () => void;
}) {
  const { t } = useLocale();
  const [project, setProject] = useState<ProjectSelection | null>(null);
  const [provider, setProvider] = useState<QuickShareProvider>("cloudflare");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const blocked = busy || Boolean(state.current_operation);
  const chooseProject = async () => {
    if (blocked) return;
    setBusy(true); setError(null);
    try {
      const path = await open({ directory: true, multiple: false, title: t("setup.chooseProject") });
      if (typeof path === "string") setProject(await desktopApi.inspectProject(path));
    } catch (value) { setError(normalizeDesktopError(value)); }
    finally { setBusy(false); }
  };
  const run = async () => {
    if (blocked || !project) return;
    setBusy(true); setError(null);
    try { onState(await desktopApi.startQuickShare(project.path, provider)); onComplete?.(); }
    catch (value) { setError(normalizeDesktopError(value)); }
    finally { setBusy(false); }
  };
  const presentation = error ? desktopErrorPresentation(error, t) : null;
  return <form className="setup-shell" data-webcodex-page="setup" aria-labelledby="share-title"
    aria-busy={blocked} onSubmit={event => { event.preventDefault(); void run(); }}>
    <button type="button" className="back-button" disabled={blocked} onClick={onBack}>{t("setup.back")}</button>
    <div className="eyebrow">{t("setup.shareLabel")}</div>
    <h1 id="share-title">{t("setup.shareTitle")}</h1>
    <p className="lede">{t("setup.shareDescription")}</p>
    <div className="project-picker-card">
      <strong>{project ? displayProjectPath(project.path) : t("setup.projectRequired")}</strong>
      <button type="button" className="secondary-button" disabled={blocked} onClick={() => void chooseProject()}
        data-webcodex-action="choose-project">{t(project ? "setup.changeFolder" : "setup.chooseFolder")}</button>
    </div>
    <fieldset disabled={blocked}><legend>{t("setup.providerLegend")}</legend>
      {(["cloudflare", "openai", "none"] as const).map(value => <label className="checkbox-row" key={value}>
        <input type="radio" name="share-provider" value={value} checked={provider === value} onChange={() => setProvider(value)} />
        {t(value === "cloudflare" ? "provider.cloudflareDescription" : value === "openai" ? "provider.openaiDescription" : "provider.localDescription")}
      </label>)}
    </fieldset>
    {error && <div className="error-card" role="alert"><strong>{presentation?.title}</strong><span>{presentation?.action}</span>
      <details><summary>{t("common.details")}</summary><code>{error.code}</code></details></div>}
    <div className="setup-actions"><button type="submit" className="primary-button" disabled={blocked || !project}
      data-webcodex-action="start-quick-share">{t(blocked ? "common.checking" : "setup.startShare")}</button></div>
  </form>;
}
