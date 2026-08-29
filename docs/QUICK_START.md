# Quick Trial

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

This page does one thing: use `webcodex share` to temporarily expose one local repository so you can decide within a few minutes whether WebCodex fits your workflow. This is a **temporary, single-project, restricted experience** that ends when the command exits.

For everyday WebCodex use and the full regular Server + Runner coding experience, go directly to the [Full Setup guide](PERSONAL_SETUP.md) instead of treating `share` as the default long-lived deployment.

## Prerequisites

- Node.js 18 or newer.
- Git and a repository you are comfortable letting an AI inspect.
- Linux, macOS, or Windows x64 for the fully managed default Cloudflare `share` flow.

Windows supports explicit local `webcodex share`. On Windows ARM64, the pinned Cloudflare release has no official ARM64 binary, so `--tunnel cloudflare` requires a trusted `WEBCODEX_CLOUDFLARED_BIN`/`PATH` binary; managed OpenAI `tunnel-client` and `--tunnel none` remain available.

## 1. Run WebCodex

From the repository you want the AI to use:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

A temporary trial does not require `setup`, `doctor`, or `run` first. The one-command flow creates a temporary public HTTPS MCP endpoint protected by that run's temporary credential; both the endpoint and credential stop working when the command exits.

## 2. Wait for `WebCodex ready`

Keep that terminal open. WebCodex prints the values needed by the MCP client. On Linux and macOS it normally copies the MCP URL to your clipboard and an interactive terminal can use **Enter** to open ChatGPT App settings. On Windows, copy the printed MCP URL manually and open **Settings -> Apps -> Create**.

## 3. Add WebCodex to ChatGPT

1. If needed, enable **Developer Mode** and choose **Create** in ChatGPT App settings.
2. Paste the copied **MCP URL** (or use the URL printed by WebCodex).
3. For the default share, choose **Access token / API key** or the equivalent Bearer-token option.
4. Paste the printed temporary **Credential**.
5. Run **Scan Tools**.

ChatGPT labels can vary by workspace and rollout. Developer Mode, custom MCP Apps, and write/modify actions also depend on your ChatGPT plan, workspace, and administrator policy; WebCodex cannot enable capabilities the client does not grant. The values printed by WebCodex are the source of truth.

### If there is no Bearer/access-token option

If ChatGPT tries OAuth automatically and reports **does not implement OAuth**, or your client has no Bearer-token field, stop the current share and run:

```bash
npx --yes @yyjeqhc/webcodex share --auth query-token
```

Paste the complete `/mcp?token=...` URL, choose **No authentication**, and run **Scan Tools** again. This fallback requires WebCodex 0.3.9 or later. The complete URL contains a temporary secret; do not publish or log it. If WebCodex is installed globally, `webcodex share --auth query-token` is equivalent.

## 4. Try a read-only request

```text
Inspect this repository and summarize its structure. Do not make changes.
```

A successful answer confirms that the client can reach WebCodex and the intended repository.

## 5. Make a small change

Once the read-only request works, try a small, reviewable task such as:

```text
Fix one small issue in this repository and run the relevant tests. Show me what changed.
```

Use Git or the WebCodex review surfaces to inspect the result before accepting it.

## Done

The temporary trial is working. Keep the WebCodex terminal open while using this share; Ctrl-C ends it.

If you want to keep using WebCodex for daily development, move next to the [Full Setup guide](PERSONAL_SETUP.md) for the regular Server + Runner coding experience. Other references:

- [ChatGPT, Claude, and authentication options](MCP.md)
- [Production and advanced deployment](DEPLOYMENT.md)
- [CLI reference](CLI.md)
- [Troubleshooting](TROUBLESHOOTING.md)
- [Security](../SECURITY.md)
- [Full documentation index](INDEX.md)
