# Documentation images

[English](README.md) | [简体中文](README.zh-CN.md)

This directory stores screenshots and diagrams used by the user-facing docs.

## GPT Action screenshots

The `gpt-action-*.png` files document the ChatGPT GPT builder flow for creating a WebCodex Action. The GPT Actions docs use them as UI landmarks and pair each screenshot with a configuration checklist, because ChatGPT may rename or move controls over time.

- `gpt-action-1.png` — open or create a GPT in ChatGPT.
- `gpt-action-2.png` — configure the GPT.
- `gpt-action-3.png` — open the Actions section and add an Action.
- `gpt-action-4.png` — configure Action authentication with a Bearer `wc_pat_xxx` token.
- `gpt-action-5.png` — import the WebCodex OpenAPI schema from `/openapi.json` and set required metadata such as privacy policy URL.

Replace these screenshots only when the documented UI landmarks are no longer recognizable or a critical deployment step is missing.

These images are referenced from [../GPT_ACTIONS.md](../GPT_ACTIONS.md) and [../GPT_ACTIONS.zh-CN.md](../GPT_ACTIONS.zh-CN.md).

## MCP app / connector screenshots

The `mcp-*.png` files document the ChatGPT apps/connectors flow for connecting WebCodex over MCP:

- `mcp-1.png` — open ChatGPT apps/connectors.
- `mcp-2.png` — choose or create the `webcodex` app.
- `mcp-3.png` — configure the MCP server URL and authentication.
- `mcp-4.png` — connect and authorize the `webcodex` app in ChatGPT.

These images are referenced from [../MCP.md](../MCP.md) and [../MCP.zh-CN.md](../MCP.zh-CN.md).

Do not commit screenshots that contain real tokens, private repository names, private domains that should not be public, or account identifiers that should not be shared.
