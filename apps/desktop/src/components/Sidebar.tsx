import { NavigationIcon } from "./NavigationIcon";
import type { DesktopState } from "../models/topology";
import { LANGUAGES, useLocale } from "../i18n/locale";
import { useProduct } from "../i18n/product";
import { useConnectionsTools } from "../i18n/connections-tools";
import { statusKey } from "../features/workspace/WorkspaceStatus";
import { useAppearance } from "../hooks/useAppearance";
import { AccentPicker } from "./AccentPicker";
import { BrandMark } from "../../../../frontend/src/ui/BrandMark";
import { ActionIcon, Menu, Tooltip } from "@mantine/core";
import { Check, Languages, Monitor, Moon, Sun } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
export type Navigation = "home" | "projects" | "connection" | "extensions" | "activity" | "settings";
export const NAVIGATION: Navigation[] = ["home", "projects", "activity", "connection", "extensions", "settings"];
const NAVIGATION_GROUPS: Array<{ label: "nav.work" | "nav.configure"; items: Navigation[] }> = [
  { label: "nav.work", items: ["home", "projects", "activity"] },
  { label: "nav.configure", items: ["connection", "extensions", "settings"] },
];
export function Sidebar({ state, navigation, setNavigation }: { state: DesktopState; navigation: Navigation; setNavigation: (page: Navigation) => void }) {
  const { locale, setLocale, t } = useLocale();
  const { appearance, setAppearance, accent, setAccent } = useAppearance();
  const reduceMotion = useReducedMotion();
  const p = useProduct(); const c = useConnectionsTools();
  const nextAppearance = appearance === "system" ? "light" : appearance === "light" ? "dark" : "system";
  const AppearanceIcon = appearance === "dark" ? Moon : appearance === "light" ? Sun : Monitor;
  const hasLocalRunner = state.topology?.runner?.kind !== "none";
  return (
      <aside className="sidebar">
        <div className="brand"><BrandMark /><div><strong>WebCodex</strong><span>Desktop</span></div></div>
        <nav aria-label={t("nav.main")}>
          {NAVIGATION_GROUPS.map((group) => (
            <section className="nav-group" key={group.label} aria-label={t(group.label)}>
              <span className="nav-group-label">{t(group.label)}</span>
              {group.items.map((item) => {
                const index = NAVIGATION.indexOf(item);
                return <button
                  key={item}
                  className={navigation === item ? "active" : ""}
                  onClick={() => setNavigation(item)}
                  aria-current={navigation === item ? "page" : undefined}
                  aria-keyshortcuts={`Control+${index + 1} Meta+${index + 1}`}
                  title={`${item === "connection" ? c("connections") : t(`nav.${item}`)} (⌘ / Ctrl + ${index + 1})`}
                  data-webcodex-action={`navigate-${item}`}
                >
                  {navigation === item && <motion.span
                    className="sidebar-selection"
                    layoutId="desktop-sidebar-selection"
                    initial={false}
                    transition={reduceMotion ? { duration: 0 } : { type: "spring", stiffness: 420, damping: 38 }}
                    aria-hidden="true"
                  />}
                  <NavigationIcon name={item} />
                  <span className="nav-label">{item === "connection" ? c("connections") : t(`nav.${item}`)}</span>
                  <kbd aria-hidden="true">{index + 1}</kbd>
                </button>;
              })}
            </section>
          ))}
        </nav>
        <div className="sidebar-preferences" aria-label={t("settings.interface")}>
          <Menu shadow="md" width={180} position="top-start" withArrow>
            <Menu.Target>
              <Tooltip label={t("locale.label")} withArrow>
                <ActionIcon variant="subtle" size="lg" aria-label={t("locale.label")} data-webcodex-control="locale">
                  <Languages size={17} />
                </ActionIcon>
              </Tooltip>
            </Menu.Target>
            <Menu.Dropdown>
              <Menu.Label>{t("locale.label")}</Menu.Label>
              {LANGUAGES.map((language) => <Menu.Item key={language.value} onClick={() => setLocale(language.value)} rightSection={locale === language.value ? <Check size={14} strokeWidth={2} aria-hidden="true" /> : undefined}>{language.label}</Menu.Item>)}
            </Menu.Dropdown>
          </Menu>
          <Tooltip label={`${t("appearance.label")} · ${t(`appearance.${appearance}`)}`} withArrow>
            <ActionIcon variant="subtle" size="lg" aria-label={`${t("appearance.label")} · ${t(`appearance.${appearance}`)}`} onClick={() => setAppearance(nextAppearance)} data-webcodex-control="appearance">
              <AppearanceIcon size={17} />
            </ActionIcon>
          </Tooltip>
          <AccentPicker color={accent} onChange={setAccent} compact />
        </div>
        <div className="sidebar-status">
          <i className={`status-dot ${(hasLocalRunner ? state.readiness.runtime_ready : state.readiness.server === "ready") ? "ready" : "unknown"}`} aria-hidden="true" />
          <div><strong>{hasLocalRunner ? `Runner · ${p(statusKey(state.readiness.runner))}` : `${p("serverConnection")} · ${p(statusKey(state.readiness.server))}`}</strong><span>{c("connections")} · {state.connections?.running ?? 0} / {state.connections?.profiles.length ?? 0}</span></div>
        </div>
      </aside>
  );
}
