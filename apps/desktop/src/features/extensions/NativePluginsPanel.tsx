import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { PluginRegistrationForm } from "./PluginRegistrationForm";
import type { ExtensionPanelContext } from "./extension-panel-types";

export function NativePluginConfiguration({ settings, settingsFailed, disabled, pendingRestart, onRestart, onAddPlugin }: ExtensionPanelContext) {
  const { t } = useLocale(); const p = useProduct();
  return <>
    {pendingRestart && <div className="extension-apply-bar" role="status"><span>{p("needsRestart")}</span>{settings?.can_restart && <button className="secondary-button" onClick={() => void onRestart()} disabled={disabled}>{p("restartRunner")}</button>}</div>}
    <div className="configured-plugins">
      {settings && <PluginRegistrationForm disabled={disabled || settingsFailed} onAdd={onAddPlugin} />}
      <h3>{p("configuredPlugins")}</h3><p className="field-help">{p("configuredPluginsHelp")}</p>
      {settings?.plugin_ids.map(id => <article className="extension-row" key={id}><strong>{id}</strong><span>{p("registered")}</span></article>)}
      {settings && !settings.plugin_ids.length && <p className="workspace-empty">{t("extensions.noPlugins")}</p>}
    </div>
  </>;
}

export function NativePluginsPanel({ catalog, disabled, onReloadPlugin }: ExtensionPanelContext) {
  const p = useProduct();
  const plugins = catalog?.plugins.catalog?.plugins || [];
  return <>
    {plugins.map(plugin => <article className="extension-row native-plugin-row" key={plugin.plugin} data-plugin-id={plugin.plugin}><div><strong>{plugin.name || plugin.plugin}</strong><span>{p(plugin.status === "ready" ? "available" : plugin.status === "failed" ? "pluginLoadFailed" : "unavailable")}</span>
      {plugin.status === "failed" && <p>{p("pluginFailureHelp")}</p>}
      {plugin.errorCode && <details><summary>{p("details")}</summary><code>{plugin.errorCode}</code></details>}
    </div>
      {catalog?.can_reload_plugins && <button className="secondary-button" disabled={disabled} onClick={() => void onReloadPlugin(plugin.plugin)}>{p("reload")}</button>}
    </article>)}
    {catalog?.plugins.available && !plugins.length && <p className="workspace-empty">{p("noRunnerPlugins")}</p>}
    {catalog && !catalog.plugins.available && <p className="workspace-notice">{p("unavailable")}</p>}
    {catalog?.plugins.catalog?.truncated && <p>{p("partial")}</p>}
  </>;
}
