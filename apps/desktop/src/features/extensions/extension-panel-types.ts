import type { ComponentType } from "react";
import type { LucideIcon } from "lucide-react";
import type { ProductKey, useProduct } from "../../i18n/product";
import type { useConnectionsTools } from "../../i18n/connections-tools";
import type { useRunnerCapabilitiesText } from "../../i18n/runner-capabilities";
import type { DesktopState, PluginRegistration, RunnerSettings } from "../../models/topology";
import type { ExtensionsSnapshot, InstructionSummary } from "../../models/workspace";

export type ExtensionPathKind = "instructions" | "skills";

// Bundled components receive observed state and existing, exact-target actions.
// Registration is presentation only, not a new native API or permission surface.
export interface ExtensionPanelContext {
  state: DesktopState;
  onState: (state: DesktopState) => void;
  settings: RunnerSettings | null;
  settingsFailed: boolean;
  disabled: boolean;
  catalog: ExtensionsSnapshot | null;
  project: string;
  projectLabel: string;
  pendingRestart: boolean;
  onRefresh: () => void;
  onRestarted: () => void;
  onRestart: () => Promise<void>;
  onAddPlugin: (provider: PluginRegistration) => Promise<boolean>;
  onReloadPlugin: (id: string) => Promise<void>;
  onOpenDocument: (file: InstructionSummary) => void;
}

export interface ExtensionPanelText {
  product: ReturnType<typeof useProduct>;
  connections: ReturnType<typeof useConnectionsTools>;
  capabilities: ReturnType<typeof useRunnerCapabilitiesText>;
}

export interface ExtensionPanelDefinition {
  readonly id: string;
  readonly Icon: LucideIcon;
  readonly title: (text: ExtensionPanelText) => string;
  readonly purpose: ProductKey;
  readonly showRefresh: boolean;
  readonly pathKind?: ExtensionPathKind;
  readonly projectPreview?: { readonly title: ProductKey; readonly emptyKind: "document" | "skill" };
  readonly Content: ComponentType<ExtensionPanelContext>;
  readonly Configuration?: ComponentType<ExtensionPanelContext>;
  // Keep only draft-owning configuration mounted across navigation. These
  // components must defer observation until active; other panels mount on demand.
  readonly PersistentConfiguration?: ComponentType<ExtensionPanelContext & { active: boolean }>;
}
