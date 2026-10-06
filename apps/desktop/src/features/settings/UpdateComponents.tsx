import type { LocalUpdateStatus } from "../../models/runtime-shell";
import { useLocale } from "../../i18n/locale";
import { useUpdateText } from "../../i18n/update-text";

const components = ["webcodex-desktop", "webcodex", "webcodex-server", "webcodex-runner"];
export function UpdateComponents({ local }: { local: LocalUpdateStatus | null }) {
  const u = useUpdateText(); const { locale } = useLocale();
  const build = (info: { version?: string | null; git_commit?: string | null; git_dirty?: boolean | null } | null | undefined, state?: string) => info?.version
    ? <><span>{info.version}</span><code>{info.git_commit ?? u("Unknown")}</code>{info.git_dirty && <span>{u("Dirty build")}</span>}</>
    : <span>{u(state === "not_local" ? "Not local" : state === "not_applicable" ? "Not applicable" : "Unknown")}</span>;
  return <div className="update-components"><table><caption>{u("Local update components")}</caption><thead><tr><th scope="col">{u("Component")}</th><th scope="col">{u("Installed files")}</th><th scope="col">{u("Running process")}</th><th scope="col">{u("Verified candidate")}</th><th scope="col">{u("Local scope")}</th></tr></thead><tbody>{components.map(binary => {
    const running = local?.running.find(row => row.binary === binary);
    const component = binary === "webcodex" ? "cli" : binary.replace("webcodex-", "");
    const filesIncluded = Boolean(local?.view.candidate_components.find(row => row.binary === binary)?.build || local?.view.upgrade?.files.includes(component));
    const service = running?.service_state;
    const serviceLabel = service === "running" ? "Service running" : service === "stopped" ? "Service stopped" : service === "absent" ? "Service absent" : service === "not_local" || service === "not_applicable" ? "Service excluded" : "Service state unknown";
    return <tr key={binary}><th scope="row">{binary.replace("webcodex-", "").replace(/^webcodex$/, "CLI")}</th><td>{build(local?.view.installed.find(row => row.binary === binary)?.build)}</td><td>{build(running, running?.state)}</td><td>{build(local?.view.candidate_components.find(row => row.binary === binary)?.build)}</td><td><span>{u(filesIncluded ? "Files included" : "File scope unknown")}</span><span>{u(serviceLabel)}</span></td></tr>;
  })}</tbody></table><p className="field-help">{u("Installed file identities come from the last explicit inspection.")}{local?.installed_checked_at_ms != null && <time dateTime={new Date(local.installed_checked_at_ms).toISOString()}> · {new Date(local.installed_checked_at_ms).toLocaleString(locale)}</time>}</p><p className="field-help">{u("File versions and running processes are separate observations. Remote services are outside this local update.")}</p></div>;
}
