import { useEffect, useRef, useState } from "react";
import { useLocale } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { desktopApi } from "../../lib/desktop-api";
import { desktopIdentity } from "../../lib/desktop-observation";
import type { DesktopError, DesktopState } from "../../models/topology";

const WINGET_INSTALL_COMMAND = "winget install --id Microsoft.PowerShell --source winget";

type Props = { state: DesktopState; onState: (state: DesktopState) => void };

export function PowerShellInstallGuidance(props: Props) {
  return <PowerShellGuidanceControls key={desktopIdentity(props.state)} {...props} />;
}

function PowerShellGuidanceControls({
  state,
  onState,
}: Props) {
  const { t } = useLocale();
  const [busy, setBusy] = useState<"guide" | "recheck" | null>(null);
  const [error, setError] = useState<DesktopError | null>(null);
  const alive = useRef(true), actionInFlight = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const runtime = state.powershell_runtime;

  const openGuide = async () => {
    if (!alive.current || actionInFlight.current) return;
    actionInFlight.current = true;
    setBusy("guide");
    setError(null);
    try {
      await desktopApi.openPowerShellInstallGuide();
    } catch (value) {
      if (alive.current) setError(normalizeDesktopError(value));
    } finally {
      actionInFlight.current = false;
      if (alive.current) setBusy(null);
    }
  };

  const recheck = async () => {
    if (!alive.current || actionInFlight.current) return;
    actionInFlight.current = true;
    setBusy("recheck");
    setError(null);
    try {
      const next = await desktopApi.getState();
      if (alive.current) onState(next);
    } catch (value) {
      if (alive.current) setError(normalizeDesktopError(value));
    } finally {
      actionInFlight.current = false;
      if (alive.current) setBusy(null);
    }
  };

  if (!runtime || runtime.pwsh_available) return null;

  const presentation = error ? desktopErrorPresentation(error, t) : null;

  return (
    <article className="detail-card tunnel-config-diagnostics" role="status" data-webcodex-diagnostic="powershell-7">
      <span className="section-kicker">{t("powershell7.kicker")}</span>
      <strong>{t("powershell7.title")}</strong>
      <p>
        {runtime.windows_powershell_available
          ? t("powershell7.fallbackAvailable")
          : t("powershell7.fallbackUnavailable")}
      </p>
      <p className="tunnel-config-guidance">
        {t("powershell7.installCommand")} <code>{WINGET_INSTALL_COMMAND}</code>
      </p>
      <p className="tunnel-config-guidance">{t("powershell7.restartHint")}</p>
      <div className="tunnel-config-actions">
        <button
          type="button"
          className="secondary-button"
          onClick={() => void openGuide()}
          disabled={busy !== null}
          data-webcodex-action="open-powershell-install-guide"
        >
          {t("powershell7.openGuide")}
        </button>
        <button
          type="button"
          className="secondary-button"
          onClick={() => void recheck()}
          disabled={busy !== null}
          data-webcodex-action="recheck-powershell-7"
        >
          {busy === "recheck" ? t("powershell7.rechecking") : t("powershell7.recheck")}
        </button>
      </div>
      {error && presentation && (
        <div className="error-card" role="alert">
          <strong>{presentation.title}</strong>
          <span>{presentation.action}</span>
          <details>
            <summary>{t("common.details")}</summary>
            <code>{error.code}</code>
            <p>{error.message}</p>
          </details>
        </div>
      )}
    </article>
  );
}
