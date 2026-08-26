# Quick Start

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

This page is only the shortest path from a local repository to a first successful AI-assisted development request. You do not need to understand WebCodex internals before starting.

## Prerequisites

- Node.js 18 or newer.
- Git and a repository you are comfortable letting an AI inspect.
- Linux or macOS for the one-command local `share` flow.

Windows uses a remote Linux WebCodex Server instead of local `share`; start with [MCP setup](MCP.md) or [Deployment](DEPLOYMENT.md).

## 1. Run WebCodex

From the repository you want the AI to use:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

You do not need to run `setup`, `doctor`, or `run` first. WebCodex prepares the temporary connection it needs automatically.

## 2. Wait for `WebCodex ready`

Keep that terminal open. WebCodex prints the values needed by the MCP client and normally copies the MCP URL to your clipboard.

## 3. Add WebCodex to ChatGPT

1. Enable **Developer Mode** in ChatGPT.
2. Open **Settings -> Apps -> Create**.
3. Paste the printed **MCP URL**.
4. For the default share, choose **Access token / API key** or the equivalent Bearer-token option.
5. Paste the printed temporary **Credential**.
6. Run **Scan Tools**.

ChatGPT labels can vary by workspace and rollout. The values printed by WebCodex are the source of truth.

### If there is no Bearer/access-token option

If ChatGPT tries OAuth automatically and reports **does not implement OAuth**, or your client has no Bearer-token field, stop the current share and run:

```bash
npx --yes @yyjeqhc/webcodex share --auth query-token
```

Paste the complete `/mcp?token=...` URL, choose **No authentication**, and run **Scan Tools** again. The complete URL contains a temporary secret; do not publish or log it. If WebCodex is installed globally, `webcodex share --auth query-token` is equivalent.

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

You now have a working first connection. Keep the WebCodex terminal open while using this temporary share; Ctrl-C ends it.

Next steps:

- [ChatGPT, Claude, and authentication options](MCP.md)
- [Windows and permanent/self-hosted deployment](DEPLOYMENT.md)
- [CLI reference](CLI.md)
- [Troubleshooting](TROUBLESHOOTING.md)
- [Security](../SECURITY.md)
- [Full documentation index](INDEX.md)
