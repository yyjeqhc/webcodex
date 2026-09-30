import { useState } from "react";
import { useLocale } from "../../i18n/locale";
import { useConnectionsTools } from "../../i18n/connections-tools";
import type { PluginRegistration } from "../../models/topology";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

export function PluginRegistrationForm({ disabled, onAdd }: { disabled: boolean; onAdd: (provider: PluginRegistration) => Promise<boolean> }) {
  const { t } = useLocale(); const c = useConnectionsTools();
  const [opened, setOpened] = useState(false);
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("[]");
  const [cwd, setCwd] = useState("");
  const [invalid, setInvalid] = useState(false);
  const [failed, setFailed] = useState(false);
  return <div className="plugin-registration">
    <button type="button" className="primary-button" disabled={disabled} onClick={() => setOpened(true)}>{t("plugins.add")}</button>
    {opened && <WorkspaceDialog title={t("plugins.add")} onClose={() => setOpened(false)} busy={disabled}>
    <form onSubmit={async event => {
      event.preventDefault(); if (disabled) return;
      setInvalid(false); setFailed(false);
      let parsed: unknown;
      try { parsed = JSON.parse(args); } catch { setInvalid(true); return; }
      if (!Array.isArray(parsed) || !parsed.every(value => typeof value === "string")) { setInvalid(true); return; }
      // Arguments are write-only and are not retained after submission.
      setArgs("[]");
      if (await onAdd({ id: id.trim(), name: name.trim(), command: command.trim(), args: parsed, cwd: cwd.trim() || null })) {
        setId(""); setName(""); setCommand(""); setCwd(""); setOpened(false);
      } else setFailed(true);
    }}>
      <p className="field-help">{t("plugins.help")}</p>
      <div className="plugin-fields">
        <label htmlFor="plugin-id">{t("plugins.id")}</label>
        <input id="plugin-id" value={id} onChange={e => setId(e.target.value)} maxLength={64} required disabled={disabled} autoComplete="off" spellCheck={false} />
        <label htmlFor="plugin-name">{t("plugins.name")}</label>
        <input id="plugin-name" value={name} onChange={e => setName(e.target.value)} maxLength={128} required disabled={disabled} autoComplete="off" />
        <label htmlFor="plugin-command">{t("plugins.command")}</label>
        <input id="plugin-command" value={command} onChange={e => setCommand(e.target.value)} maxLength={1024} required disabled={disabled} placeholder="node" autoComplete="off" spellCheck={false} />
        <label htmlFor="plugin-args">{t("plugins.args")}</label>
        <textarea id="plugin-args" value={args} onChange={e => setArgs(e.target.value)} maxLength={20000} disabled={disabled} rows={2} spellCheck={false} aria-describedby="plugin-args-help" />
        <span className="field-help" id="plugin-args-help">{t("plugins.argsHelp")}</span>
        <label htmlFor="plugin-cwd">{t("plugins.cwd")}</label>
        <input id="plugin-cwd" value={cwd} onChange={e => setCwd(e.target.value)} maxLength={4096} disabled={disabled} autoComplete="off" spellCheck={false} />
      </div>
      {invalid && <p role="alert">{t("plugins.invalidArgs")}</p>}
      {failed && <p role="alert" className="workspace-notice">{c("operationFailed")}</p>}
      <button type="submit" className="secondary-button" data-webcodex-action="add-plugin-registration" disabled={disabled}>{t("plugins.save")}</button>
    </form>
    </WorkspaceDialog>}
  </div>;
}
