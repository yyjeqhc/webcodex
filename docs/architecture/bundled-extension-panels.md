# Bundled Desktop extension panels

Desktop's Extensions page composes its six built-in categories from
`apps/desktop/src/features/extensions/extension-panel-registry.ts`. This is an
internal source-level registry, not a third-party UI plugin API.

## Ownership

Each definition owns its stable navigation ID, localized title, purpose, icon,
refresh visibility, optional project-preview metadata, and React components.
`ExtensionTab` derives from this inventory; the same order drives buttons and
keyboard navigation. Existing App deep links keep their IDs. Unknown initial
selection falls back to Coding Agents without registering or executing anything.

`ExtensionsPanel` owns the layout and accessible tab relationship, not a second
category switch. `useExtensionsState` owns the existing settings/catalog
observations and exact-target native actions. The new Native Plugin and project
preview components own their domain-specific display. Existing Coding Agent,
SSH and MCP panels are composed directly, without changing their APIs or work.

Registration does not confer authority. Save, reload, restart, managed-file
revision checks, permission confirmation, and Runner identity fences remain in
the existing native/domain paths. Native Plugin executable arguments are still
write-only and are cleared by the existing registration form.

## Mounting and drafts

Ordinary category components mount only when selected. Merely enumerating panel
definitions does not query SSH resources, authorize capabilities, register a
Plugin, restart a Runner, or open an editor.

Two existing draft lifetimes intentionally differ from ordinary panels:

- The single `ExtensionPathsEditor` remains mounted while switching categories.
  It owns separate instruction and Skill path drafts and resets them only when
  the corresponding observed settings/target changes. A project-preview switch
  or failed settings refresh must not reset those drafts.
- Managed instructions are a persistent configuration contribution. Its own
  `active` input defers the initial file read until selected and preserves the
  draft on later navigation. Its observer/mutation sequencing stays in
  `ManagedInstructionsPanel`; enabling instructions does not clear an unrelated
  pending Plugin restart.

Do not key the shared path editor by tab or put retained configuration behind an
active-only conditional. Do not mount every network-backed panel invisibly to
achieve draft retention. The page controller still cancels superseded catalog
observations and ignores results after unmount.

## Incremental rollout

This change can be adopted independently of the MCP App resource/template layer.
It requires no Server/Runner protocol, tool schema, database, settings-file, or
Native Tool Plugin migration. The existing panels remain normal React components,
so additional categories can move into definitions individually without a
partially initialized runtime registry or dual dispatch path.

Definitions are immutable bundled code. There is no reload/unload registry,
subscription, lock, dynamic import, evaluation of provider JavaScript, or exposed
Tauri bridge added here. External UI code would need a separate isolated-host
security design; trusted native provider status is not UI execution permission.

## Validation

`extension-panel-registry.test.ts` covers inventory, localization and navigation.
`WorkspaceSettings.test.tsx` covers retained path/managed drafts, initial deep
links, ARIA/focus, inactive observations, and the existing exact-target mutations.
`workspace/Workspace.test.tsx` retains real preview, instruction-read and explicit
Plugin-reload coverage. The existing path/managed-editor tests continue to own
stale snapshots, failed refreshes, write fences and asynchronous editor lifetimes.
