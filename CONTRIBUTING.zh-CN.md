# 参与 WebCodex 开发

[English](CONTRIBUTING.md)

欢迎通过 bug 报告、文档改进、focused fix，以及符合 WebCodex 当前产品方向的新能力参与贡献。

也欢迎直接使用 WebCodex 本身或其他 coding agent 来完成贡献。无论使用什么工具，提交者仍需要负责检查最终 diff、验证改动，并确认其中不包含 credential 或机器私有数据。

## 开始之前

- 开始较大改动前，先检查最新 `main` 和已有 issues。
- 对于 bug 和客户端兼容性问题，通常建议先创建 issue，确认问题并共享复现信息。
- 安全相关问题请按照 [SECURITY.md](SECURITY.md) 处理，不要创建公开 issue。
- 保持改动 focused，不要在同一个 pull request 中混入无关重构或无关的生成内容。

一个有帮助的 bug 报告可以根据实际情况包含：

- WebCodex version 或 commit；
- Server 和 Runner 的操作系统；
- 使用的客户端，例如 ChatGPT、Claude、Gemini、Grok 或其他 MCP client；
- Bearer、OAuth 等认证方式；
- 清晰的复现步骤，以及预期行为与实际行为；
- 能帮助定位失败阶段的脱敏日志或错误信息。

不要提交 token、Authorization header、private key、cookie、password 或其他 secret。

## 开发流程

1. 从当前 `main` 创建 focused branch。
2. 遵循 [AGENTS.md](AGENTS.md) 以及其中链接的相关仓库规则。
3. 保持现有架构和命名风格，优先选择能够解决当前问题的最小完整改动。
4. 行为发生变化时补充或更新 focused test；公共行为或运维方式变化时同步更新文档。
5. 对改动文件执行最小但足够的验证。纯文档改动不需要运行 Cargo build。
6. 提交前检查最终 diff 和 worktree 状态。

仓库测试说明见 [docs/TESTING.md](docs/TESTING.md)，coding workflow 与 closeout 约定见
[docs/CODING_WORKFLOW.zh-CN.md](docs/CODING_WORKFLOW.zh-CN.md)。

## Pull requests

Pull request 应说明改了什么、为什么需要这项改动，以及执行了哪些验证。有相关 issue 时请进行关联。

每个 pull request 应保持可评审，并只解决一个完整而集中的目标。如果改动涉及认证、授权、process lifecycle、持久化、公共协议行为或其他 trust boundary，请提供针对该边界的 focused regression evidence。

提交贡献即表示你同意按照仓库的 [Apache License 2.0](LICENSE) 对该贡献进行许可。
