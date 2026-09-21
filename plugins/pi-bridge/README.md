# WebPi Pi Bridge

A WebPi Native Tool Plugin that hosts Pi's coding-agent resource/extension ecosystem without starting a second LLM agent loop.

## Why this exists

WebPi keeps the web ChatGPT model as the only reasoning/coding agent. WebPi's own Server/Runner owns authentication, Project authority, Workflow Sessions, guarded writes, Jobs, Git, and recovery. This provider adds the pinned Pi coding-agent runtime as a Runner-local capability substrate.

The bridge now supports native Pi extension loading/tools/commands/lifecycle, skills, prompt templates, context files, packages, hot reload, project trust, explicit extension hash approval, mixed text/image results, and extension state persistence. Use `pi_capability_report` for the machine-readable parity contract.

Safety-critical mutation/process/Git behavior remains on WebPi canonical `apply_text_edits`, `run_process`, Jobs and Git tools rather than duplicating Pi bash/edit/write authority.

## Build

Node.js 22.19+ is required.

```text
npm install --ignore-scripts
npm test
```

## Runner provider

Configure the provider with `cwd` equal to the exact WebPi Project root:

```toml
[[plugins.providers]]
id = "pi-bridge"
name = "WebPi Pi Bridge"
command = "C:\\absolute\\path\\to\\node.exe"
args = ["C:\\absolute\\path\\to\\webpi-core\\plugins\\pi-bridge\\dist\\plugin.js"]
cwd = "C:\\absolute\\path\\to\\project"
timeout_secs = 30
```

The provider is then reached through the canonical `plugin_tool describe -> call` path. On GPT Actions, `plugin_tool` is a direct operation.

## Extension and package security

`pi_extension_inventory` is filesystem-only discovery and never imports extension code. Native extension execution is a separate path requiring Pi project trust plus WebPi approval of the resolved candidate/content fingerprint.

Local administration is exposed as `webpi.cmd pi ...`. After the operator explicitly trusts the project, the first-party bridge also supports consequential web package install/update/remove and exact candidate/hash approval. Package mutations require lifecycle-script acknowledgement; Runner provider `plugin:manage` remains a separate scope. Do not treat `plugin:invoke` as read-only or claim native package scripts are sandboxed.

Use `pi_extension_tool_describe` to obtain exact native input schemas before calls. Changed or revoked loaded approvals invalidate subsequent tool/command execution. Failed reload and shutdown leave no stale callable runner. Revocation does not roll back files, network effects or arbitrary extension-owned processes.

Pi read preserves bounded images. Read/search/discovery use physical-path, sensitive-file, link and size guards; ripgrep is resolved to an absolute existing executable without cwd probing or request-time downloads. These guards are defense in depth, not isolation from a malicious process with the same OS-user privileges.

See `docs/WEBPI_PI_PARITY.md` for the full native/equivalent/limited/not-applicable capability matrix.
