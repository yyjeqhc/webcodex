import { Blocks, Bot, FileText, Network, Puzzle, Terminal } from "lucide-react";
import { CodingAgentsPanel } from "./CodingAgentsPanel";
import { SshResourcesPanel } from "./SshResourcesPanel";
import { McpProvidersPanel } from "./McpProvidersPanel";
import { NativePluginConfiguration, NativePluginsPanel } from "./NativePluginsPanel";
import { InstructionConfiguration, InstructionsPreview, SkillsPreview } from "./ProjectExtensionPreviews";
import type { ExtensionPanelDefinition } from "./extension-panel-types";

// The only bundled panel inventory: identity, navigation, purpose, scope and
// renderer live together. No dynamic imports, plugin-provided JS or global hooks.
const definitions = [
  { id: "codingAgents", Icon: Bot, title: text => text.capabilities("codingAgents"), purpose: "codingAgentsPurpose", showRefresh: false, Content: CodingAgentsPanel },
  { id: "sshResources", Icon: Terminal, title: text => text.capabilities("sshResources"), purpose: "sshPurpose", showRefresh: false, Content: SshResourcesPanel },
  { id: "mcpProviders", Icon: Network, title: text => text.connections("mcpProviders"), purpose: "mcpPurpose", showRefresh: true, Content: McpProvidersPanel },
  { id: "nativePlugins", Icon: Blocks, title: text => text.product("nativePlugins"), purpose: "pluginsPurpose", showRefresh: true, projectPreview: { title: "projectRunnerPlugins", emptyKind: "skill" }, Configuration: NativePluginConfiguration, Content: NativePluginsPanel },
  { id: "skills", Icon: Puzzle, title: () => "Skills", purpose: "skillsPurpose", showRefresh: true, pathKind: "skills", projectPreview: { title: "projectExtensions", emptyKind: "skill" }, Content: SkillsPreview },
  { id: "instructions", Icon: FileText, title: text => text.product("instructions"), purpose: "instructionsPurpose", showRefresh: true, pathKind: "instructions", projectPreview: { title: "projectExtensions", emptyKind: "document" }, PersistentConfiguration: InstructionConfiguration, Content: InstructionsPreview },
] as const satisfies readonly ExtensionPanelDefinition[];

export type ExtensionTab = typeof definitions[number]["id"];
type BuiltinPanel = ExtensionPanelDefinition & { readonly id: ExtensionTab };
export const EXTENSION_PANELS: readonly BuiltinPanel[] = Object.freeze(definitions.map(panel => Object.freeze(panel)));

export function extensionPanel(id: string | undefined): BuiltinPanel {
  return EXTENSION_PANELS.find(panel => panel.id === id) ?? EXTENSION_PANELS[0];
}

export function extensionTabForKey(current: ExtensionTab, key: string): ExtensionTab | null {
  const index = EXTENSION_PANELS.findIndex(panel => panel.id === current);
  const count = EXTENSION_PANELS.length;
  const next = key === "Home" ? 0 : key === "End" ? count - 1
    : key === "ArrowRight" || key === "ArrowDown" ? (index + 1) % count
    : key === "ArrowLeft" || key === "ArrowUp" ? (index + count - 1) % count : null;
  return next === null ? null : EXTENSION_PANELS[next].id;
}
