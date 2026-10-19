import { NavigationIcon } from "./NavigationIcon";
import type { DesktopState } from "../models/topology";
import { LANGUAGES, useLocale, type MessageKey } from "../i18n/locale";
import { useProduct } from "../i18n/product";
import { useConnectionsTools } from "../i18n/connections-tools";
import { desktopStatusPresentation, statusKey } from "../features/workspace/WorkspaceStatus";
import { useAppearance } from "../hooks/useAppearance";
import { useShortcutHints } from "../hooks/useShortcutHints";
import { AppearanceControls } from "../../../../frontend/src/ui/AppearanceControls";
import { BrandMark } from "../../../../frontend/src/ui/BrandMark";
import { ActionIcon, Menu, Tooltip } from "@mantine/core";
import { Languages } from "lucide-react";
export type Navigation = "home" | "projects" | "connection" | "extensions" | "activity" | "settings" | "console";
export const NAVIGATION: Navigation[] = ["home", "projects", "activity", "connection", "extensions", "settings", "console"];
const NAVIGATION_GROUPS: Array<{ label: "nav.work" | "nav.configure"; items: Navigation[] }> = [
  { label: "nav.work", items: ["home", "projects", "activity", "console"] },
  { label: "nav.configure", items: ["connection", "extensions", "settings"] },
];
export function Sidebar({ state, navigation, setNavigation }: { state: DesktopState; navigation: Navigation; setNavigation: (page: Navigation) => void }) {
  const { locale, setLocale, t } = useLocale();
  const { appearance, setAppearance, accent, setAccent } = useAppearance();
  const p = useProduct(); const c = useConnectionsTools();
  const nextAppearance = appearance === "system" ? "light" : appearance === "light" ? "dark" : "system";
  const appearanceLabels: Record<string, MessageKey> = {
    "Accent color": "accent.label", "Custom color": "accent.custom",
    Appearance: "appearance.label", System: "appearance.system", Light: "appearance.light", Dark: "appearance.dark",
  };
  const presentation = desktopStatusPresentation(state, p);
  useShortcutHints();
  return (
      <aside className="sidebar">
        <div className="brand"><BrandMark /><div><strong>WebCodex</strong><span>Desktop</span></div></div>
        <nav aria-label={t("nav.main")}>
          {NAVIGATION_GROUPS.map((group) => (
            <section className="nav-group" key={group.label} aria-label={t(group.label)}>
              <span className="nav-group-label">{t(group.label)}</span>
              {group.items.map((item) => {
                const index = NAVIGATION.indexOf(item);
                const label = item === "connection" ? c("connections") : item === "console" ? p("runtimeConsole") : t(`nav.${item}`);
                return <button
                  key={item}
                  className={navigation === item ? "active" : ""}
                  onClick={() => setNavigation(item)}
                  aria-current={navigation === item ? "page" : undefined}
                  aria-keyshortcuts={`Control+${index + 1} Meta+${index + 1}`}
                  title={`${label} (⌘ / Ctrl + ${index + 1})`}
                  data-webcodex-action={`navigate-${item}`}
                >
                  <NavigationIcon name={item} />
                  <span className="nav-label">{label}</span>
                  <kbd aria-hidden="true">{index + 1}</kbd>
                </button>;
              })}
            </section>
          ))}
        </nav>
        <div className="sidebar-preferences" role="group" aria-label={t("settings.interface")}>
          <AppearanceControls accent={accent} appearance={appearance} onAccentChange={setAccent}
            onAppearanceCycle={() => setAppearance(nextAppearance)}
            translate={value => appearanceLabels[value] ? t(appearanceLabels[value]) : value} />
          <Menu shadow="md" width={180} position="top-end" withinPortal returnFocus closeOnEscape>
            <Menu.Target>
              <Tooltip label={t("locale.label")} openDelay={400}>
                <ActionIcon className="desktop-preference-button" variant="subtle" color="gray" size={36} radius="md"
                  aria-label={t("locale.label")} data-webcodex-control="locale">
                  <Languages size={18} />
                </ActionIcon>
              </Tooltip>
            </Menu.Target>
            <Menu.Dropdown className="appearance-preference-popover">
              <Menu.Label>{t("locale.label")}</Menu.Label>
              <Menu.RadioGroup value={locale} onChange={value => {
                const language = LANGUAGES.find(language => language.value === value);
                if (language) setLocale(language.value);
              }}>
                {LANGUAGES.map(language => <Menu.RadioItem key={language.value} value={language.value} closeMenuOnClick>{language.label}</Menu.RadioItem>)}
              </Menu.RadioGroup>
            </Menu.Dropdown>
          </Menu>
        </div>
        <div className="sidebar-status">
          <i className={`status-dot ${presentation.primaryReady ? "ready" : "unknown"}`} aria-hidden="true" />
          <div><strong>{presentation.primaryLabel}<span className="sidebar-service-state"> · {p(statusKey(presentation.primaryStatus))}</span></strong><span>{presentation.connectionText}</span></div>
        </div>
      </aside>
  );
}
