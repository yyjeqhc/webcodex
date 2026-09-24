# 参与 WebCodex 开发

[English](CONTRIBUTING.md)

欢迎通过 bug 报告、文档改进、focused fix，以及符合 WebCodex 当前产品方向的新能力参与贡献。

也欢迎直接使用 WebCodex 本身或其他 coding agent 来完成贡献。无论使用什么工具，提交者仍需要负责检查最终 diff、验证改动，并确认其中不包含 credential 或机器私有数据。

## 开始之前

- 开始较大改动前，先检查最新 `main` 和已有 issues。
- 小型文档修正和 focused fix 可以直接提交；较大的功能或架构改动建议先创建 issue，
  避免重复工作或方向不一致。
- 对于 bug 和客户端兼容性问题，通常建议先创建 issue，确认问题并共享复现信息。
- 安全相关问题请按照 [SECURITY.md](SECURITY.md) 处理，不要创建公开 issue。
- 保持改动 focused，不要在同一个 pull request 中混入无关重构或无关的生成内容。

## 提交 Issue

一个有帮助的 issue 应尽量让维护者无需先追问基础信息，就能理解实际发生了什么。只需要提供与问题相关的信息，不必机械填写无关的 `N/A`。

对于 bug 和兼容性问题，在能够获取时请提供：

- **实际观察与影响：** 哪个操作、工具、界面或工作流出现问题，实际发生了什么，以及它是完全阻塞还是仅影响正常使用。
- **运行环境：** WebCodex version 或 commit；Server 和 Runner 的操作系统；如果已知，说明 Server/Runner build 是否一致；使用的 client 或 Host，例如 ChatGPT、Codex、Claude、Gemini、Grok、CLI、Desktop 或 WebUI；以及认证方式。Browser、proxy、SSH、sandbox 等信息只有在与问题相关时才需要提供。
- **最小复现步骤：** 用最短步骤描述如何触发问题，并说明它是必现、偶发，还是目前只观察到一次。
- **预期行为与实际行为：** 分开说明。
- **原始证据：** 如果能获得，请保留准确的错误文本、`error_kind`、exit code、失败阶段或简短的脱敏日志尾部；不要只留下对错误的二次转述。
- **已经做过的排查：** 例如最新 `main` 是否仍能复现、换 Runner、fresh Session、其他 client 或已知旧版本后结果如何。如果怀疑是 regression，并且已经知道范围，请提供 last known good / first known bad version 或 commit。
- **事实与判断分开：** 明确区分已经直接观察到的事实与推测的 root cause 或解释。

### 由 Coding Agent 创建的 Issue

欢迎 coding agent 直接创建 issue，但在提交前应优先完成自己已经有能力完成的低成本诊断。例如 agent 能查询 runtime status、build alignment、准确的 tool error、相关源码或 focused test evidence 时，通常应该直接把这些信息写进 issue，而不是留给维护者或报告者下一轮再补。

Agent 创建的 issue 还应：

- 明确说明问题是**已经直接复现**、**根据源码或测试证据推断**，还是**由用户报告但 agent 未独立复现**；
- 不要为了让报告显得完整而编造缺失的运行环境、复现结果或 root cause；
- 如果源码检查对报告结论有实质支持，说明检查时对应的 WebCodex commit 和相关文件；
- 只有证据已经建立因果关系时才把 root cause 写成确定结论，否则明确标注为 suspected；
- 做到足够的 focused diagnostics 后及时提交，不要为了穷尽所有可能性而长期拖延一个已经有价值的 issue。

不要提交 token、Authorization header、private key、cookie、password、私有文件内容或其他 secret。安全相关问题请按照 [SECURITY.md](SECURITY.md) 处理，不要创建公开 issue。

## 自己修复问题

维护者响应时间可能有所变化。已经报告的 bug 不需要等待维护者先实现，也欢迎直接准备
focused fix。

推荐的自助修复流程：

1. 更新到最新 `main`，确认问题仍然可以复现。
2. 保留最小、安全的复现步骤和准确错误证据。
3. 创建 focused fix branch。
4. 检查相关实现与测试；欢迎直接使用 WebCodex 本身或其他 coding agent 辅助完成。
5. 做最小完整修复，并在适合时补充 regression test。
6. 运行最小但足够的验证；条件允许时，再对真实受影响工作流进行本地 dogfood。
7. 提交前检查最终 diff，排除无关修改、生成文件、credential 与机器私有数据。
8. 创建 pull request；如果已有 issue，请进行关联。

基础 Rust/source 工具链与 dogfood 构建见 README 的
[从源码构建](README.zh-CN.md#从源码构建)。Desktop 修复请按照
[Desktop 开发与本地打包](docs/DESKTOP_DEVELOPMENT.zh-CN.md) 操作，并在相关时使用
native installer/DMG helper 做实际验证。

## 开发流程

1. 从当前 `main` 创建 focused branch。
2. 遵循 [AGENTS.md](AGENTS.md) 以及其中链接的相关仓库规则。
3. 保持现有架构和命名风格，优先选择能够解决当前问题的最小完整改动。
4. 行为发生变化时补充或更新 focused test；公共行为或运维方式变化时同步更新文档。
5. 对改动文件执行最小但足够的验证。纯文档改动不需要运行 Cargo build。
6. 提交前检查最终 diff 和 worktree 状态。

仓库测试说明见 [docs/TESTING.md](docs/TESTING.md)，coding workflow 与 closeout 约定见
[docs/CODING_WORKFLOW.zh-CN.md](docs/CODING_WORKFLOW.zh-CN.md)。
Desktop 贡献者还应查看 [docs/DESKTOP_DEVELOPMENT.zh-CN.md](docs/DESKTOP_DEVELOPMENT.zh-CN.md)，其中说明源码 runtime、Tauri、原生打包以及 installer/DMG smoke 流程。

开发机上的普通 Cargo `dev` / `test` profile 会有意关闭源码行号 debuginfo，以减小大型测试二进制和链接开销。Linux 开发者可按需使用 `bash scripts/cargo_fast.sh <cargo-args>`；只有在 Linux 且检测到 mold 时才通过 `mold -run` 启动 Cargo。缺少 mold 时会透明回退到普通 Cargo，且不会修改 macOS 或 Windows 的链接器。

## Pull requests

Pull request 应说明改了什么、为什么需要这项改动，以及执行了哪些验证。有相关 issue 时请进行关联。

每个 pull request 应保持可评审，并只解决一个完整而集中的目标。如果改动涉及认证、授权、process lifecycle、持久化、公共协议行为或其他 trust boundary，请提供针对该边界的 focused regression evidence。

提交贡献即表示你同意按照仓库的 [Apache License 2.0](LICENSE) 对该贡献进行许可。
