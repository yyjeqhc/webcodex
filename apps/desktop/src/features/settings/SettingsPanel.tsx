import type { DesktopState } from "../../models/topology";
import { useLocale } from "../../i18n/locale";

export function SettingsPanel({ state }: { state: DesktopState }) {
  const { locale, setLocale, t } = useLocale();
  return (
    <section className="page-section" aria-labelledby="settings-title" data-webcodex-page="settings">
      <div className="eyebrow">{t("settings.eyebrow")}</div>
      <h1 id="settings-title">{t("settings.title")}</h1>
      <p className="lede">{t("settings.description")}</p>

      <section className="settings-section" aria-labelledby="settings-interface-title">
        <h2 id="settings-interface-title">{t("settings.interface")}</h2>
        <div className="detail-card setting-row">
          <label htmlFor="desktop-settings-locale">{t("locale.label")}</label>
          <select
            id="desktop-settings-locale"
            value={locale}
            onChange={(event) => setLocale(event.target.value as typeof locale)}
            data-webcodex-control="locale"
          >
            <option value="zh-CN">{t("locale.zh")}</option>
            <option value="en-US">{t("locale.en")}</option>
          </select>
        </div>
      </section>

      <section className="settings-section" aria-labelledby="settings-diagnostics-title">
        <h2 id="settings-diagnostics-title">{t("settings.diagnostics")}</h2>
        <article className="detail-card">
        {state.binaries ? (
          <dl className="detail-list">
            <div><dt>{t("settings.version")}</dt><dd>{state.binaries.version}</dd></div>
            <div><dt>{t("settings.sourceRevision")}</dt><dd>{state.binaries.git_commit}</dd></div>
            <div><dt>{t("settings.binaryDirectory")}</dt><dd>{state.binaries.directory}</dd></div>
            <div><dt>{t("settings.binaryResolution")}</dt><dd>{state.binaries.source}</dd></div>
          </dl>
        ) : (
          <p>{t("settings.binariesPending")}</p>
        )}
        </article>
      </section>

      <section className="settings-section" aria-labelledby="settings-advanced-title">
        <h2 id="settings-advanced-title">{t("settings.advanced")}</h2>
        <article className="detail-card">
          <dl className="detail-list">
            <div><dt>{t("settings.runtimeProjectId")}</dt><dd>{state.project?.runtime_project_id ?? t("settings.notEstablished")}</dd></div>
          </dl>
        </article>
      </section>
    </section>
  );
}

