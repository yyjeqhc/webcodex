import { useRef, useState, type FormEvent } from "react";
import { Alert, Button, Checkbox, PasswordInput, TextInput } from "@mantine/core";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState } from "../../models/topology";
import type { TunnelConnection } from "../../models/connections-tools";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

export function ConnectionEditor({ profile, onState, onClose }: { profile: TunnelConnection | null; onState: (state: DesktopState) => void; onClose: () => void }) {
  const p = useProduct(); const c = useConnectionsTools();
  const [name, setName] = useState(profile?.name || "");
  const [tunnelId, setTunnelId] = useState(profile?.tunnel_id || "");
  const [apiKey, setApiKey] = useState("");
  const [autostart, setAutostart] = useState(profile?.autostart ?? true);
  const [busy, setBusy] = useState(false); const [failed, setFailed] = useState(false);
  const submitted = useRef(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault(); if (submitted.current) return;
    submitted.current = true; setBusy(true); setFailed(false);
    const request = { id: profile?.id ?? null, name: name.trim(), tunnel_id: tunnelId.trim(), api_key: apiKey || null, autostart, expected_revision: profile?.revision ?? null };
    setApiKey("");
    try { onState(await desktopApi.saveTunnelProfile(request)); onClose(); }
    catch { setFailed(true); }
    finally { request.api_key = null; submitted.current = false; setBusy(false); }
  };
  return <WorkspaceDialog title={profile ? `${p("edit")} ${profile.name}` : c("addConnection")} onClose={onClose} busy={busy}>
    <form className="profile-editor" onSubmit={event => void submit(event)} aria-busy={busy}>
      <p className="field-help">{p("tunnelCredentialHelp")}</p>
      <TextInput className="field-group" id="connection-profile-name" label={c("name")} aria-label={c("name")} name="name" value={name} onChange={event => setName(event.currentTarget.value)} required maxLength={160} disabled={busy} autoFocus />
      <TextInput className="field-group" id="connection-profile-tunnel-id" label="Tunnel ID" aria-label="Tunnel ID" name="tunnel_id" value={tunnelId} onChange={event => setTunnelId(event.currentTarget.value)} required maxLength={256} pattern="(?:[A-Za-z0-9_]|-)+" spellCheck={false} autoCapitalize="none" disabled={busy} />
      <PasswordInput className="field-group" id="connection-profile-api-key" label="API Key" aria-label="API Key" name="api_key" value={apiKey} onChange={event => setApiKey(event.currentTarget.value)} required={!profile?.credential_present} maxLength={8192} autoComplete="new-password" spellCheck={false} disabled={busy} description={profile?.credential_present ? p("savedKey") : undefined} />
      <Checkbox className="profile-checkbox" id="connection-profile-autostart" label={c("autostart")} checked={autostart} onChange={event => setAutostart(event.currentTarget.checked)} disabled={busy} />
      {failed && <Alert color="red" role="alert">{c("operationFailed")}</Alert>}
      <div className="connection-actions"><Button type="submit" className="primary-button" disabled={busy || !name.trim() || !/^[A-Za-z0-9_-]+$/.test(tunnelId.trim()) || (!profile?.credential_present && !apiKey.trim())}>{c("saveApply")}</Button><Button type="button" className="secondary-button" variant="default" disabled={busy} onClick={onClose}>{p("cancel")}</Button></div>
    </form>
  </WorkspaceDialog>;
}
