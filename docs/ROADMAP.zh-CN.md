# Roadmap

这份 roadmap 故意保持很短。WebCodex 0.2.x 的重点是让线上 AI 编程更容易试用、更容易 review，而不是承诺完整 IDE 或 autonomous operations platform。

## 0.2.x：产品化 Online Coding Loop

- 更简单的 first-run setup。
- ChatGPT MCP 和 GPT Actions acceptance tasks。
- 更多 fixture dogfood tasks。
- 更清晰的 review 和 rollback workflow。

## LSP Phase 1：已完成

- agent-side supervisor。
- constrained rust-analyzer profile。
- read-only symbol navigation。
- startup capability awareness。

## LSP Phase 2 Read-Only MVP：已完成

- disk-backed full-text document refresh（`didOpen` / `didChange`），不支持 editor-style incremental sync。
- bounded `document_diagnostics`，明确提供 freshness 和 timeout 语义。
- normalized、bounded hover。
- workspace-only symbol search，结果只含 project-relative path。
- ToolRuntime、MCP、generic GPT Action、OAuth、schema 和 startup capability 同步。

## LSP Phase 3 多语言：已完成

- 语言注册表（`lsp/language.rs`）：每个 `LanguageProfile` 描述一个 server 的扩展名/`languageId`、项目 marker、可执行文件解析、`default_args` 以及受约束的只读 `initialize` profile；supervisor 与 navigation handler 不含任何按语言的分支。
- 在 Rust 之外新增 Python（`pyright`）与 TypeScript/JavaScript（`typescript-language-server`），并用真实 pyright 做了端到端验证。

## 后续 LSP 工作

- 让 `workspace_symbols` 在多语言项目里对每个被检测到的语言 fan-out（当前由首个被检测到的语言应答）。
- 让 `start_coding_task.semantic_navigation` 启动摘要支持多语言（当前是 Rust 专用的就绪提示；运行时工具本身已多语言）。
- 受约束只读 profile 之外更完整的 diagnostics fidelity。
- 经过明确设计的 rename、code action 等写能力（不属于 read-only MVP）。
- 更多 languages（每种语言就是一条注册表 profile 加上它的只读安全 profile 与测试）。

## Validation Intelligence MVP：已完成

- 从 bounded safe validation metadata 做确定性的结构化提取。
- bounded cargo-check diagnostics 和 cargo-test failed-test evidence。
- 不做 root-cause inference 的保守 validation failure-kind 分类。
- finish/handoff 共享增强证据，并提供只读 session query `validation_summary`。
- ToolRuntime、MCP、generic GPT Action、OAuth 和 schema 同步（75 个 runtime tools，25 个 GPT Action operations）。

## 后续 Coding Intelligence

- 更丰富的 multi-language validation adapters。
- 可选的 machine-readable Cargo JSON integration。
- review 和 rollback UX。

## Later

- dashboard。
- ops packs。
- browser/computer-use evidence。

## Non-Goals

- 完整 IDE replacement。
- 默认 autonomous DevOps。
- arbitrary computer use 作为核心承诺。
- 保证兼容所有 AI client。
