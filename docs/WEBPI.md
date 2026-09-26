# WebPi

**Current upgrade contract:** [WebPi identity and isolated configuration](WEBPI_IDENTITY.md). New native binaries are `webpi`, `webpi-server`, and `webpi-runner`; runtime configuration uses `WEBPI_*`. Existing WebPi-owned legacy env files require explicit `webpi.cmd migrate-config` after stopping the old instance. Historical deployment notes do not prove the new build is online.

WebPi is an independent web coding harness that reuses selected implementation patterns from WebCodex while owning its own Server, Runner, state, deployment, and Pi capability/extension ecosystem.

## Goal

Keep the **web ChatGPT model** as the only reasoning/coding agent. Do not start a second Pi model loop and do not require ChatGPT Work to perform repository coding.

Architecture:

```text
ChatGPT web / Custom GPT
        |
        | GPT Actions / OpenAPI
        v
     WebPi Server
        |
        v
     WebPi Runner
        |
        +-- inherited hardened runtime primitives
        |     project/session/edit/process/jobs/git
        |
        +-- WebPi capability gateway
              |
              v
          pi-bridge
              |
              +-- Pi read/grep/find/ls
              +-- native Pi ResourceLoader / PackageManager
              +-- trusted + hash-approved Pi extensions
              +-- skills / prompts / context / commands
              +-- extension lifecycle + persistent state
```

WebPi keeps the upstream-derived `/openapi.json` + canonical ToolDefinition projection because it is a proven web-agent transport pattern. The WebPi Server serves its own schema and routes only to its own WebPi Runner; it does not depend on a running WebCodex Server or Runner.

## Independence contract

WebPi and WebCodex are separate products and deployments:

- separate Git repository/worktree;
- separate Server process and listen port;
- separate database/data directory;
- separate connection/profile directory;
- separate Runner client identity and `runner.toml`;
- separate project registry;
- separate Plugin/Pi extension lifecycle;
- no WebPi provider is installed into a WebCodex Runner;
- no WebCodex process is required for WebPi to remain online.

WebCodex is treated as an upstream/reference implementation. Proven fixes may be ported or cherry-picked deliberately, but WebPi-specific Pi capabilities evolve in WebPi itself.

## Standalone Windows deployment

The validated independent Windows root is `E:\\WebPi\\webpi-core`. WebPi keeps local deployment state under `.webpi-state/` and portable runtime assets under `.webpi-runtime/`. The deployment uses its own Server on `127.0.0.1:56542`, Runner client id `webpi-local`, and project identity `agent:webpi-local:webpi-core`.

Normal local use does not require a running WebCodex instance:

```text
.\webpi.cmd status
.\webpi.cmd run
```

`webpi.cmd run` owns the WebPi Server and Runner as one foreground lifecycle; Ctrl+C stops both. Runner configuration, project registry, user credentials, and the WebPi Action credential all remain under `.webpi-state/`.

For a fresh checkout, build the inherited hardened Rust runtime and the Pi bridge first, then initialize/enroll with `scripts/webpi/standalone.py`. Enrollment keeps the one-time pairing code off argv, installs `pi-bridge` only into WebPi's Runner config, and provisions the ordinary coding scopes plus `plugin:inspect + plugin:invoke` without granting `plugin:manage`.

Before enabling the public OpenAI Tunnel, bootstrap the pinned WebPi-local tunnel client:

```powershell
C:\Python314\python.exe scripts\webpi\bootstrap_tunnel_client.py
```

ChatGPT developer-mode App/MCP mode uses `.\webpi.cmd tunnel-config` followed by
`.\webpi.cmd run-web`. WebPi does not read WebCodex/global
`CONTROL_PLANE_*` variables as configuration input and does not depend on the
WebCodex managed tunnel-client cache. The Secure MCP Tunnel carries MCP traffic;
Custom GPT Actions remain a separate ordinary-HTTPS exposure of WebPi's
`/openapi.json` and `/api/actions/*` routes. See
[WebPi authentication and credential boundaries](WEBPI_AUTH_BOUNDARIES.md).

### Existing Cloudflare Tunnel / Custom GPT Actions

When Cloudflare Tunnel is already managed outside WebPi, do not start a second tunnel process. Point the Cloudflare **published application** hostname at `http://127.0.0.1:56542`, then register only the public HTTPS origin in WebPi:

```powershell
.\webpi.cmd cloudflare-config https://webpi.piforme.vip
.\webpi.cmd status
```

`cloudflare-config` never stores a Cloudflare tunnel token. It writes `WEBPI_PUBLIC_URL` and enforces the standalone WebPi auth profile: arbitrary shared-key fallback, anonymous mode, the shared-key OAuth bridge, and MCP query-token auth are disabled. Custom GPT Actions use the WebPi Action PAT as Bearer authentication. `/openapi.json` and static console assets may be publicly fetchable through the HTTPS hostname, but `/mcp` and `/api/*` execution surfaces remain bearer-authenticated.

A running Server reads this policy at process start. If `status` reports the new values while an older Server is still running, restart the WebPi Server/Runner lifecycle once before exposing the hostname publicly.

## Why not raw ChatGPT MCP on Pro?

Current ChatGPT plan/workspace policy can restrict custom MCP write/modify actions independently of what the server supports. WebPi therefore keeps WebCodex's GPT Actions compatibility path available for a Custom GPT/web workflow. Transport choice does not change Runner authority.

### Compatibility fallback for older upstream-derived Action schemas

The current WebPi build exposes `plugin_tool` as a direct GPT Action and should use that direct operation. Older upstream-derived schemas may expose only the generic `callRuntimeTool` operation at `/api/tools/call`; those schemas can still reach `plugin_tool`.

Because both the generic wrapper and `plugin_tool describe` use a field named `tool`, call the Pi bridge through the canonical `params` envelope on this version:

```json
{
  "tool": "plugin_tool",
  "params": {
    "action": "describe",
    "runner": "<exact-runner-id>",
    "plugin": "pi-bridge",
    "tool": "pi_read"
  }
}
```

Then use the returned binding:

```json
{
  "tool": "plugin_tool",
  "params": {
    "action": "call",
    "binding": "wc_pbind_...",
    "arguments": {
      "path": "README.md",
      "limit": 120
    }
  }
}
```

This fallback was validated against an older upstream-compatible runtime gateway. When the running WebPi OpenAPI document contains a direct `plugin_tool` operation, prefer it instead of wrapping it.

## Pi bridge

`plugins/pi-bridge` is a first-party Native Tool Plugin. It does not implement MCP and does not run another model.

The bridge now hosts the pinned Pi coding-agent ecosystem directly: native resource loading, package resolution, trusted/approved extensions, extension tools and commands, lifecycle hooks, dynamic resources, skills, prompt templates, AGENTS/context, hot reload, mixed text/image results, and persistent extension state. `pi_capability_report` exposes the exact parity classification to the web model.

WebPi deliberately does **not** duplicate Pi's bash/edit/write authority. Safety-critical mutations, processes, durable Jobs, Git review, stale-write fences and recovery stay on WebPi's hardened canonical runtime. This is an equivalent harness capability, not a missing Pi feature.

Executable Pi extension import requires both Pi project trust and WebPi approval of the exact resolved candidate/content fingerprint. WebPi now also exposes Pi's native `DefaultPackageManager` lifecycle to the web agent through `pi_package_install`, `pi_package_update`, and `pi_package_remove`, so ChatGPT web can install and manage Pi packages instead of relying on a second Pi agent. Native npm/git package installation may execute package lifecycle scripts; each web mutation therefore requires an explicit `confirmLifecycleScripts=true`, project trust, and remains auditable as a consequential plugin call. Newly resolved extensions still stay unloaded until the web agent obtains their candidate id + SHA-256 with `pi_extension_candidate_status`, approves that exact fingerprint with `pi_extension_approve`, and calls `pi_resource_reload`. `pi_extension_revoke` removes approval and reloads immediately.

See [WebPi ↔ Pi coding-agent parity](WEBPI_PI_PARITY.md).

For exact WebCodex reuse boundaries, see [the asset map](WEBPI_ASSET_MAP.md). For observed authentication failures, remediation evidence and the blocked production handoff, see [the 2026-09-19 security review](WEBPI_SECURITY_REVIEW_2026-09-19.md). Source changes and isolated tests must not be reported as a successful production deployment.

## Build the bridge on Windows

```powershell
C:\Python314\python.exe scripts\webpi\bootstrap_pi_bridge.py
```

This downloads project-local Node 24.21.0 from nodejs.org, verifies the official SHA256 manifest, installs the bridge dependencies with lifecycle scripts disabled, builds, and runs protocol tests.

The script prints the absolute native Node executable and compiled plugin paths. The standalone enrollment flow installs them into WebPi's own Runner configuration. For manual deployments the provider block is:

```toml
[[plugins.providers]]
id = "pi-bridge"
name = "WebPi Pi Bridge"
command = "C:\\...\\webpi-core\\.webpi-runtime\\node\\node.exe"
args = ["C:\\...\\webpi-core\\plugins\\pi-bridge\\dist\\plugin.js"]
cwd = "C:\\absolute\\path\\to\\the\\registered\\project"
timeout_secs = 30
```

Then run `plugin_tool check` before `plugin_tool reload`. A successful reload atomically replaces the provider set; failed candidates leave the previous committed providers intact.

## Web GPT instructions

Use the WebPi runtime workflow:

1. `work_on_project` establishes the exact Project and Workflow Session.
2. Prefer WebPi's canonical `read_files/search_project_texts` for general work.
3. When Pi resources matter, use `plugin_tool` to call `pi_resource_inventory`; read only relevant skills/prompts/context and inspect `pi_capability_report` when compatibility matters.
4. For Pi ecosystem expansion, use `pi_package_list/install/update/remove`. Treat npm/git package mutation as code execution because native package lifecycle scripts may run.
5. After installation or update, call `pi_extension_candidate_status`. Approve only the returned exact `candidateId + sha256` with `pi_extension_approve`, then call `pi_resource_reload` before invoking the new extension.
6. Invoke trusted + approved extension capabilities through `pi_extension_tool_list` → `pi_extension_tool_describe` → `pi_extension_tool_call`, or command list/call. `pi_extension_revoke` blocks future calls; it does not roll back prior side effects or arbitrary extension-owned processes.
7. Keep ordinary repository writes on `apply_text_edits`/other guarded canonical edit tools and commands/tests on structured process/Job tools.
8. Review with `show_changes`; close with `finish_coding_task`.

## Pi local administration

WebPi reuses Pi's native trust/package implementation behind a local management boundary:

```text
.\webpi.cmd pi trust-status
.\webpi.cmd pi trust-set true
.\webpi.cmd pi package-list
.\webpi.cmd pi package-install <source>
.\webpi.cmd pi package-update [source]
.\webpi.cmd pi package-remove <source>
.\webpi.cmd pi extension-candidates
.\webpi.cmd pi extension-approve <candidate>
.\webpi.cmd pi extension-approvals
.\webpi.cmd pi extension-revoke <candidate>
```

Approval is content-fingerprint-bound and is checked before extension import. A changed extension therefore returns to pending approval rather than silently inheriting old execution authority.

The model may design, inspect and invoke capabilities; package installation, project trust and executable-code approval remain separately reviewable host authority.
