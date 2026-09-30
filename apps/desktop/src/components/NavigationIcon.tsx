import type { Navigation } from "./Sidebar";
import { Activity, Blocks, Cable, FolderKanban, House, Monitor, Settings2 } from "lucide-react";

const icons = {
  home: House,
  projects: FolderKanban,
  connection: Cable,
  extensions: Blocks,
  activity: Activity,
  settings: Settings2,
  console: Monitor,
} satisfies Record<Navigation, typeof House>;

export function NavigationIcon({ name }: { name: Navigation }) {
  const Icon = icons[name];
  return <Icon className="navigation-icon" size={18} strokeWidth={1.75} aria-hidden="true" />;
}
