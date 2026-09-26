# WebPi ↔ Pi coding-agent parity contract

WebPi pins and tests against `@earendil-works/pi-coding-agent 0.85.x`. The goal is coding-harness and extension-ecosystem parity while keeping ChatGPT web as the only LLM reasoning loop.

This is not a claim that a browser harness should emulate Pi's terminal UI or take ownership of the model provider. Capabilities are classified as:

- **native** — WebPi runs Pi's implementation directly.
- **equivalent** — WebPi provides the same coding outcome through its hardened web-runtime primitive.
- **limited** — the common useful subset is implemented, but Pi owns a broader lifecycle than WebPi can honestly expose.
- **not applicable** — the feature assumes Pi owns the LLM loop or terminal UI, which WebPi intentionally does not.

## Current parity matrix

| Pi capability | WebPi status | WebPi implementation |
| --- | --- | --- |
| Extension discovery/loading | native | `DefaultResourceLoader` + `ExtensionRunner`, gated by Pi project trust and WebPi hash approval before import |
| Extension tools | native | Exact input-schema description via `pi_extension_tool_describe`, TypeBox validation, `tool_call`, execute, `tool_result`, balanced `tool_execution_start/end`, mixed text/image results |
| Extension commands | native | Pi registered commands execute with Pi `ExtensionCommandContext`; `reload()` is supported |
| Extension lifecycle | native | startup/reload/quit `session_start/session_shutdown`, plus `resources_discover` and dynamic skills/prompts/themes |
| Extension state | equivalent | Native Pi `SessionManager` API backed by a WebPi-owned bounded atomic FileEntry snapshot so `appendEntry` survives plugin restart without a fake Pi assistant message |
| Skills | native | Pi resource loader + native skill metadata/content |
| Prompt templates | native | Pi prompt-template parser/substitution behavior |
| AGENTS/context | native | Pi project context discovery; bounded snapshots are exposed to the web agent |
| Pi packages | native + stricter | `pi_package_inspect` performs read-only npm registry metadata preflight without downloading tarballs or running lifecycle scripts; install/list/update/remove then call Pi `DefaultPackageManager`; web mutations require explicit lifecycle-script confirmation, while resolved extension imports still require exact WebPi fingerprint approval |
| Project trust | native + stricter | Pi `ProjectTrustStore`; executable extension code additionally requires WebPi hash approval |
| Hot reload | native + guarded | Pi cache reset and lifecycle events; failed reload leaves execution unavailable instead of retaining a stale callable runner. This is not automatic side-effect rollback. |
| Text/image read | native + guarded | Pi read uses a bounded, physically checked file snapshot; image content is preserved within plugin wire limits. |
| Grep/find/ls | equivalent | Bounded ripgrep backend and Pi find/ls formatting, mandatory sensitive exclusions, no link following, guarded rereads and absolute executable resolution. |
| File mutation | equivalent | WebPi canonical guarded edit tools, read-revision fences and protected paths |
| Process execution | equivalent | WebPi structured process + durable Jobs/observation |
| Git/review | equivalent | WebPi canonical Git/diff/review primitives |
| Coding session continuity | equivalent | WebPi Workflow Session is authoritative for the web agent; Pi SessionManager remains extension-local state |
| Pi tool-event coverage | limited | Native events surround Pi extension-tool calls; WebPi canonical edit/process/Git calls do not masquerade as Pi tool calls |
| Pi new/fork/tree/switch session control | limited | These cannot replace the authoritative WebPi Workflow Session; unsupported controls fail explicitly rather than report fake success |
| Provider/model hooks | not applicable | ChatGPT web owns the model request, so Pi provider request/model/thinking hooks cannot govern it |
| Agent/turn/message/input loop events | not applicable | They require Pi to own the LLM loop; WebPi deliberately has no second Pi agent loop |
| TUI widgets/renderers/keybindings/editor UI | not applicable | WebPi is a browser harness, not a terminal renderer |

The same matrix is available to the model through `pi_capability_report`.

## Security model

Discovery is not execution. A project extension must pass both:

1. Pi project trust; and
2. WebPi approval of the exact resolved extension candidate and SHA-256 content fingerprint.

Local administration retains the native Pi package/trust/approval commands:

```text
webpi.cmd pi trust-status
webpi.cmd pi trust-set true
webpi.cmd pi package-list
webpi.cmd pi package-install <source>
webpi.cmd pi package-update [source]
webpi.cmd pi package-remove <source>
webpi.cmd pi extension-candidates
webpi.cmd pi extension-approve <candidate>
webpi.cmd pi extension-approvals
webpi.cmd pi extension-revoke <candidate>
```

The normal GPT Action token has `plugin:inspect + plugin:invoke`, not Runner provider `plugin:manage`. Inside the first-party `pi-bridge`, the web model can use Pi-native package install/update/remove after the operator has trusted the project. This is intentionally consequential: native npm/git installation may execute lifecycle scripts, so the bridge requires explicit lifecycle-script confirmation. Extension loading is a separate gate: downloaded/resolved extension code is not imported until `pi_extension_candidate_status` returns its exact fingerprint, `pi_extension_approve` accepts that still-current candidate id + SHA-256, and `pi_resource_reload` creates the next runtime generation. Fingerprint changes automatically return the extension to pending approval.

## Web-agent usage

After `work_on_project`, use `plugin_tool` to reach `pi-bridge`.

For a project that uses Pi resources, a practical bootstrap is:

1. call `pi_capability_report` when capability compatibility matters;
2. call `pi_resource_inventory` to see extensions, skills, prompts, context, themes, and packages;
3. use `pi_package_inspect` first for npm metadata/lifecycle-script preflight, then `pi_package_list/install/update/remove` for native Pi package lifecycle when expansion is required;
4. call `pi_extension_candidate_status`, inspect the exact SHA-256, and use `pi_extension_approve` only for the intended candidate;
5. call `pi_resource_reload`, then `pi_extension_tool_list` → `pi_extension_tool_describe` → `pi_extension_tool_call`; parse the returned `parametersJson` before constructing arguments;
6. read relevant skills with `pi_skill_read` and expand prompts with `pi_prompt_expand`;
7. use `pi_extension_revoke` to block future candidate calls before rollback/removal; separately stop extension-owned processes and reverse side effects when necessary.

Safety-critical writes, commands, Jobs, Git and recovery remain on WebPi's canonical runtime.

## Security and compatibility limits

The bridge rejects linked approval targets/ancestors/descendants, special files, oversized or excessively deep candidate trees, malformed approval stores, and concurrent approval writers. Loaded candidate approval is checked again before extension commands/tools execute. A changed or revoked candidate invalidates further calls until review/reload.

These checks do not sandbox native code, cover arbitrary dynamic import dependencies, or undo already running code. An explicitly trusted project permits consequential web package administration; `confirmLifecycleScripts` is an acknowledgement, not a separate human credential. An owner-level canonical shell remains owner-level execution. See [asset and authority map](WEBPI_ASSET_MAP.md).

## Upgrade rule

When the pinned Pi version changes, parity is not assumed. Re-run the bridge/admin suites, review Pi's extension/resource/package/session API changes, update `pi_capability_report`, then run the standalone WebPi E2E in the canonical independent root.
