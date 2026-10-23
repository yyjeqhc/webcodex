# Rust 构建选择与产物观察

[English](rust-build-selection-artifacts.md)

本功能是 [#1273](https://github.com/yyjeqhc/webcodex/issues/1273) 对
[#962](https://github.com/yyjeqhc/webcodex/issues/962) 的独立扩展，沿用
`project_build`。不提供交叉编译、任意 argv、输出路径选择或工具安装。
省略新增字段时保留原有命令和行为。

## 请求与执行

可选 `profile` 为 1–64 字节 Cargo profile 名称，仅含 ASCII 字母、数字、
下划线或连字符，不能以连字符开头。支持 `dev`、`release` 和项目定义的
`dogfood` 等名称；不存在的 profile 由 Cargo 报告实际失败。
可选 `targets` 选择 `library: true` 和/或 `binaries: ["server"]`，最多八个
有界二进制名称，生成规范参数前排序去重。省略时使用 Cargo 默认目标；空选择无效。

`artifact_policy: "observe"` 显式请求所选 Rust 库和二进制输出的有界元数据。
这三个字段仅支持 Rust 或 auto 解析为 Rust，其他后端返回
`build_options_unsupported`。计划派发和 Job 准入均要求新增能力
`project_build_rust_options_v1`。

Runner 生成固定的 `cargo build --profile NAME --lib --bin NAME` 参数，
可组合现有 package、locked、offline 策略。选项进入规范调用摘要和来源记录，
并在排队后及启动前重新验证。新增执行路径禁止 Rustup 自动安装。

## 观察与限制

观察先执行离线 `cargo metadata --no-deps --format-version 1`，再使用
`--message-format=json-render-diagnostics` 运行已规划构建；两个阶段属于原 durable
Job，共享总超时和取消预算，复用准备环境、进程树所有权和管道排空机制。
元数据与构建输出遵守现有流字节上限；截断不能通过观察验收。

元数据确定已注册 Project 中的 package、显式 package 选择或 Cargo 默认
workspace 成员，并应用已准入的 workspace 排除；被排除成员作为依赖构建时也不会变成所选产物。只接收所选 package 身份与库/二进制目标的编译产物，并要求
完整的成功 `build-finished`。读取前后重新核对 Project 所有权和目录身份，
复用产物检查基础机制核验项目相对路径、字节数、SHA-256 和 MIME。
内部精确 Cargo 输出读取器允许生成的 `target/` 文件；通用产物工具及非 Cargo 构建读取器保留批量忽略规则，
凭据路径仍被禁止。
沿用最多 16 文件、总计 256 MiB 的契约及读取器的更严格限制；Project 外的
Cargo target 目录不能通过观察验收。

缓存产物仅证明观察时文件存在，不证明本次创建、正确性或源码新鲜度。
所需目标缺失、文件缺失、输出畸形或不完整、重定向、资源限制和读取失败均不返回
部分成功列表。原生构建退出 0 时，仍可明确报告产物观察失败并保留实际退出码。
Cargo 合法更新锁文件，不会在构建后的产物读取中被误判为启动前计划陈旧。

pending 和恢复保留同一 Job 及规范产物元数据。结果未知不能授权重跑。
观察不改变构建权限或副作用，也不提供文件系统或网络沙箱。

## 验证

Core、Tool Contracts、Runner、Registry、Server 聚焦测试覆盖默认省略、
封闭有界输入、来源记录、旧 Runner 拒绝、输出完整性、Project 所有权、
稀疏结果和原 Job 接续。显式真实进程用例
`runner_real_process_project_build_rust_options_observe_selected_cached_artifacts`
覆盖 dogfood 下的库与指定二进制、故意损坏的未选二进制、独立内容与摘要核验、
缓存重跑、workspace 排除和缺失 profile 失败。同一测试通道的
`runner_real_process_structured_process_preserves_fractional_shared_deadline`
验证不足一秒的进程预算与清理，且不重置共享截止时间。实际平台、构建和部署结果记录在 PR，
本文描述契约，不代表已经完成部署或真实模型验收。
