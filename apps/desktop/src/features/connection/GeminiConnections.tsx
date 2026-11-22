import { useState, type ReactNode } from "react";
import { useConnectionsTools } from "../../i18n/connections-tools";
import type { TunnelConnection } from "../../models/connections-tools";
import { CloudflareConnectionCard } from "./CloudflareConnectionCard";

interface Props {
  profiles: TunnelConnection[];
  preparation?: ReactNode;
  busy: boolean;
  preparing: boolean;
  canStart: boolean;
  onPrepare: (profile: TunnelConnection | null) => void;
  onNamed: () => void;
  onRepair: () => void;
  onRestartServer?: () => void;
  onEdit: (profile: TunnelConnection) => void;
  onDelete: (profile: TunnelConnection) => void;
}

export function GeminiConnections({ profiles, preparation, busy, preparing, canStart, onPrepare, onNamed, onRepair, onRestartServer, onEdit, onDelete }: Props) {
  const c = useConnectionsTools();
  const [selection, setSelection] = useState<{ id: string } | { kind: "cloudflare_named" | "cloudflare_quick" } | null>(null);
  const selected = profiles.find(profile => selection && ("id" in selection ? profile.id === selection.id : profile.provider?.kind === selection.kind))
    ?? profiles.find(profile => profile.autostart)
    ?? profiles.find(profile => profile.oauth_dynamic_registration_enabled)
    ?? profiles[0];
  const named = selected?.provider?.kind === "cloudflare_named";
  const chooseAlternative = () => {
    const alternative = profiles.find(profile => profile.provider?.kind === (named ? "cloudflare_quick" : "cloudflare_named"));
    if (alternative) setSelection({id: alternative.id});
    else {
      setSelection({kind: named ? "cloudflare_quick" : "cloudflare_named"});
      if (named) onPrepare(null);
      else onNamed();
    }
  };
  return <section className="gemini-connection" aria-labelledby="gemini-heading">
    <header className="workspace-section-heading">
      <h2 id="gemini-heading" tabIndex={-1}>{c("connectGemini")}</h2>
    </header>
    <p className="gemini-entry-help">{c("geminiEntryHelp")}</p>
    {preparation ?? <>
      {preparing && <p role="status">{c("preparingConnection")}</p>}
      {!selected && <>
        <p>{c("quickReauthorize")}</p>
        <button type="button" className="primary-button" disabled={busy} onClick={() => onPrepare(null)}>{c("connectGemini")}</button>
      </>}
      {selected && <CloudflareConnectionCard
        profile={selected} canStart={canStart} busy={busy} onEdit={() => onEdit(selected)} onDelete={() => onDelete(selected)}
        onRepair={onRepair} onRestartServer={onRestartServer} onEnableRegistration={() => onPrepare(selected)} />}
      <details className="connection-details"><summary>{c("profileOptions")}</summary>
        {profiles.length > 1 && <label>{c("chooseProfile")} <select disabled={busy} value={selected?.id} onChange={event => setSelection({id: event.currentTarget.value})}>
          {profiles.map(profile => <option key={profile.id} value={profile.id}>{profile.name}</option>)}
        </select></label>}
        <button type="button" className="secondary-button" disabled={busy} onClick={chooseAlternative}>{c(named ? "quickTrial" : "fixedAddress")}</button>
      </details>
    </>}
    <details className="connection-details"><summary>{c("geminiSteps")}</summary>
      <p>{c("geminiInstructions")}</p><p>{c("geminiEligibility")}</p>
    </details>
  </section>;
}
