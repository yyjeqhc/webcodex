# agent-browser Native Tool Plugin

`agent-browser` is a first-party WebCodex Native Tool Plugin that uses an already-installed local [Agent Browser](https://agent-browser.dev/) executable.

It covers one case that WebCodex Browser intentionally does not own: using the operator's existing local Agent Browser configuration/profile, or attaching to a running Chrome instance that the operator explicitly enabled for remote debugging.

The Plugin does **not** bundle Agent Browser, copy credentials itself, bypass Chrome consent, or add another MCP server. WebCodex still exposes the normal `plugin_tool describe -> call` path; the Runner starts this local Plugin and the Plugin invokes the configured local `agent-browser` binary with structured argv and isolated session/namespace state.

## Build and test

Requirements:

- Node.js 18 or newer;
- local `agent-browser` **0.38.1** (the Plugin fails closed on unvalidated backend versions);
- a local Chrome-family browser for live smoke tests.

Repository CI uses the checkout's current Plugin SDK:

```bash
npm ci
npm run typecheck
npm test
```

Optional real-browser smoke tests:

```bash
npm run test:live
npm run test:native
```

Both smoke suites use synthetic/local test state. They do not use a person's daily browser profile.

## Two recommended providers

Keep native/profile use and current-Chrome attachment as two separate Runner providers. This makes the authority boundary visible and avoids editing one config every time the caller changes modes.

Copy the example configs to private Runner-local files and adjust the executable path when needed.

### Native profile/config

`config.example.json`:

```json
{
  "executable": "agent-browser",
  "connection": "native"
}
```

Native mode lets Agent Browser read its normal user/project configuration and `AGENT_BROWSER_*` environment. A named Chrome profile or persistent profile directory is interpreted by Agent Browser itself; this Plugin does not copy cookies or browser credentials.

If interactive-shell exports such as `AGENT_BROWSER_PROFILE` are needed by a Desktop-launched Runner, prepare them with a provider-specific Runner shell profile. Do not change the global default shell profile for unrelated projects.

### Current Chrome

`config.current-chrome.example.json`:

```json
{
  "executable": "agent-browser",
  "connection": "auto"
}
```

This mode only attaches to a Chrome instance that the operator has enabled and approved for remote debugging. Failure to find an authorized Chrome is an error; the Plugin does not silently launch a replacement browser.

Example Runner entries:

```toml
[[plugins.providers]]
id = "agent-browser"
name = "Agent Browser"
command = "node"
args = ["/absolute/path/to/webcodex/plugins/agent-browser/dist/plugin.js", "--config", "/private/path/agent-browser-native.json"]
cwd = "/absolute/path/to/webcodex/plugins/agent-browser"
timeout_secs = 90

[[plugins.providers]]
id = "agent-browser-current-chrome"
name = "Agent Browser — Current Chrome"
command = "node"
args = ["/absolute/path/to/webcodex/plugins/agent-browser/dist/plugin.js", "--config", "/private/path/agent-browser-current-chrome.json"]
cwd = "/absolute/path/to/webcodex/plugins/agent-browser"
timeout_secs = 90
```

Then use the normal author/operator loop:

```text
plugin_tool(action="check", runner=<exact runner>, plugin="agent-browser")
plugin_tool(action="check", runner=<exact runner>, plugin="agent-browser-current-chrome")
plugin_tool(action="reload", runner=<exact runner>)
plugin_tool(action="describe", runner=<exact runner>, plugin="agent-browser", tool="browser_connect")
```

## Tools

The Plugin exposes 16 `browser_*` tools:

- status, connect, tabs, open, navigate, reload;
- snapshot, click, fill, select_option, set_checked, press, scroll;
- screenshot, close_page, disconnect.

Element actions require a fresh semantic snapshot. Snapshot and element identities are opaque and page-bound. Every action consumes the current snapshot; reload/navigation/provider reload invalidate prior identities.

Snapshot observation is adaptive by default: ordinary pages return full semantic text, while large pages automatically compact to interactive controls. Callers can force `interactive_only=true` for action selection or `interactive_only=false` when static text is required for page understanding or outcome verification.

Screenshots are private provider-local files under this Plugin checkout. Native Plugin v1 is text/structured-content only and has no Project-artifact/image handoff, so the returned provider-relative path is not automatically addressable through WebCodex Project artifact tools.

## Safety boundary

- Existing user tabs are never closed by `browser_close_page`; only tabs created by this Plugin instance can be closed.
- Disconnect detaches from external Chrome without closing it. Native mode closes only the browser owned by this Plugin session.
- The Plugin uses an isolated short Agent Browser session/namespace and never `close --all`.
- It exposes no arbitrary shell, JavaScript/eval, CDP method, cookies/storage API, credential extraction, profile path, remote host, or caller-selected screenshot path.
- HTTP(S) navigation is allowed; local files, browser-internal pages, extensions, data URLs, and credential-bearing URLs are rejected.
- Known preflight/application failures are explicit. Ambiguous effect failures are not converted into retry-safe errors; the provider terminates so WebCodex can preserve `OutcomeUnknown`.
- File upload is intentionally not exposed. A future upload tool needs an explicit WebCodex Project read-authority/path-fencing design instead of accepting arbitrary local paths.

This Plugin is optional dogfood for local Browser control. WebCodex's built-in Browser remains the safer default when no real local profile/current-browser integration is needed.
