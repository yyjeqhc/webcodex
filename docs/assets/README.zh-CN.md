# Documentation images

[English](README.md) | [简体中文](README.zh-CN.md)

该目录保存用户文档中使用的截图和图表。

## GPT Action screenshots

`gpt-action-*.png` 文件记录了在 ChatGPT GPT builder 中创建 WebCodex Action 的流程。GPT Actions 文档把它们作为 UI 路标，并为每张截图配套配置核对清单，因为 ChatGPT 可能随时间调整控件名称或位置。

- `gpt-action-1.png` — 在 ChatGPT 中打开或创建 GPT。
- `gpt-action-2.png` — 配置 GPT。
- `gpt-action-3.png` — 打开 Actions 区域并添加 Action。
- `gpt-action-4.png` — 使用 Bearer `wc_pat_xxx` token 配置 Action authentication。
- `gpt-action-5.png` — 从 `/openapi.json` 导入 WebCodex OpenAPI schema，并填写 privacy policy URL 等必要 metadata。

只有在文档中的 UI 路标已无法识别，或缺少关键部署步骤时，才应替换这些截图。

这些图片被 [../GPT_ACTIONS.md](../GPT_ACTIONS.md) 和 [../GPT_ACTIONS.zh-CN.md](../GPT_ACTIONS.zh-CN.md) 引用。

## MCP app / connector screenshots

`mcp-*.png` 文件记录了在 ChatGPT apps/connectors 中通过 MCP 连接 WebCodex 的流程：

- `mcp-1.png` — 打开 ChatGPT apps/connectors。
- `mcp-2.png` — 选择或创建 `webcodex` app。
- `mcp-3.png` — 配置 MCP server URL 和 authentication。
- `mcp-4.png` — 在 ChatGPT 中连接并授权 `webcodex` app。

这些图片被 [../MCP.md](../MCP.md) 和 [../MCP.zh-CN.md](../MCP.zh-CN.md) 引用。

不要提交包含真实 tokens、私有仓库名、不应公开的私有域名或账号标识的截图。
