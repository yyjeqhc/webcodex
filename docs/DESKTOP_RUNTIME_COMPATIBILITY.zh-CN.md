# Desktop 长期运行与 Runtime 兼容

WebCodex 的兼容性由协议与功能能力决定，构建修订用于诊断。正常兼容的 WebCodex 构建可以来自不同源码修订；自行修改代码的语义由操作者负责。完整协议与发布约束见[英文规范](DESKTOP_RUNTIME_COMPATIBILITY.md)。

## Desktop Shell 与 Runtime

Desktop 保留项目选择、凭据引用、连接、配置和进程 ownership。Runtime 是 `webcodex`、`webcodex-server`、`webcodex-runner` 三个可执行文件，Windows 带 `.exe`。正式版默认使用内置 Runtime，也可以在“设置 → Runtime”选择外部文件夹，不需要修改 `.app` 或安装目录。

Desktop、CLI、Server、Runner 的 commit 与版本号不要求一致；dirty 构建允许使用并显示提示。这不等于任意自定义代码都可信或保证兼容。

共享 Desktop management contract 当前为 `[1,1]`，三个 Runtime 文件与 Desktop 必须存在共同支持范围。实际 Server/Runner 功能仍使用现有 Runner wire generation、capability negotiation 和操作级 feature gate。缺少 Project Lifecycle 能力只禁用该操作，不应连带禁用已经支持的 shell/read/Git。新增可选字段/功能不提升 breaking generation；破坏性变更才提升对应 generation。

## 选择与切换

点击“选择 Runtime 文件夹…”后先显示候选检查，不切换当前进程。检查会执行文件的 `--build-info-json` 命令，验证文件、可执行状态、原生架构、结构化元数据与协议范围，并记录文件哈希；它不是代码审计或沙箱，请仅选择你准备运行的官方文件或自己的构建。

不同版本/源码修订/dirty 是提示，不是阻断。缺失文件、错误架构、无法验证的元数据与不兼容协议会阻止激活。点击“使用此 Runtime”后再次检查候选与当前身份；存在任务或任务数未知时，必须确认可能中断工作。

切换只停止 Desktop 管理的相关进程。启动并确认所选 Runtime 就绪后才保存选择；失败只尝试一次恢复之前有效的 Runtime。无法确认停止或恢复时给出明确结果，不会 broad-kill、重新配对、重建项目、生成凭据或偷偷改用内置版本。

退出重开后保留 Custom 选择；文件夹消失时会显示“所选 Runtime 不可用”，请明确选择“使用内置 Runtime”或其他文件夹。升级确认完成前不要删除旧 Runtime 文件。

## 自行构建

从正常 WebCodex workspace 根目录运行：

```sh
cargo build --locked --release -p webcodex-cli -p webcodex -p webcodex-runner --bins
```

选择 `target/release`，其中包含三个 Runtime 文件。显式交叉编译输出在 `target/<target-triple>/release`；请选择当前机器支持的原生架构。开发时可使用仓库已有 profile：

```sh
cargo build --locked --profile dogfood -p webcodex-cli -p webcodex -p webcodex-runner --bins
```

此时选择 `target/dogfood`。三个程序都支持单独运行 `--build-info-json`，在启动服务/读取 Runtime 配置之前输出 schema version 1 的构建信息。不能识别的外部程序不会被默认视为兼容。macOS 签名、Gatekeeper 和 Computer Use 权限是独立的操作系统约束，不能用协议兼容代替它们。

## 故障排查与支持包

“设置 → 故障排查”提供诊断中心。追踪默认 Off；Metadata 记录请求生命周期和关联信息，不记录完整参数/结果；Full 可能含敏感内容，仅建议临时开启，必须明确确认。

保存仅原子修改托管 Server 环境文件的 `WEBCODEX_TOOL_REQUEST_TRACE`，保留其他设置并去除同名重复项，不把环境正文传给 UI。“保存并重启 Runtime”仅重启托管本地 Server，并等待现有 Runner 重连；未确认生效时仍显示需重启/错误原因。

“打开 Runtime Console”使用当前本地 Server 的 `/runtime`，不在 URL 中放凭据。“复制 Runtime Console 凭据”是独立敏感操作，由 native 层读取当前托管用户凭据并复制到剪贴板，不显示、不写入 Activity，不复制 Runner token。剪贴板管理器可能保留数据，请勿分享。

“复制诊断报告”与“导出支持包”只使用固定安全投影，不包含项目代码、凭据、环境或 runner.toml 正文、任意 stdout/stderr、剪贴板或完整请求/结果。ZIP 只有报告 JSON、Markdown 与安全事件 JSON，不能覆盖已有文件。

## 活动与续轮证据

“ChatGPT 调用”展示实际工具、执行与响应状态及关联工作；“工作会话”展示持久任务、进度、Jobs 与验证；“系统事件”展示 Desktop 自身服务状态。没有 response handoff 时间时显示“响应交付未确认”。已返回 HTTP 框架不代表 MCP 客户端已收到，也不代表模型已收到；响应流启动也不等于完整交付。

底层 `next_call_gap_ms` 附着在到达的新请求上，表示它距离前次响应的间隔，不能反过来证明当前事件已有后续调用。只能用后续实际记录确认续轮；无记录的时间可能包含网络、Host 调度、模型推理、用户操作等，不直接归因于“ChatGPT 卡死”。

## 更新与恢复

UI 就绪后进行非阻塞稳定版更新检查，公开 GitHub 无需认证，超时有界，缓存 24 小时。启动检查失败不影响就绪状态；手动检查可以立即重试。只提示，不自动下载/安装。有效 release manifest 与 Desktop range 相交时提示只更换 Runtime 即可；不相交时提示更新 Desktop；缺失/未知 manifest 只给普通发行版链接，不猜兼容性。

现有 Desktop state 原子写入与备份机制继续使用。未来 schema 不支持时保留原文件并进入可诊断状态，不自动以旧备份覆盖；用户确认恢复后仍需明确启动 Runtime。不会通过删除项目、凭据或 Runtime state 来“修复”。
