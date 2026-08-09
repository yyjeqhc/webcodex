# WebCodex 0.3.4

[English](RELEASE_NOTES_v0.3.4.md) | [简体中文](RELEASE_NOTES_v0.3.4.zh-CN.md)

WebCodex 0.3.4 是一次 execution reliability / ergonomics 发布。它让模型侧命令执行的
retry safety 更明确、减少对 shell quoting 的依赖，让长任务能够在不重跑的情况下继续为
durable Job，同时降低批量观察 Job 的对话轮次，让 polling 更顺畅，并让 Windows 输出更
可预测。

## 主要更新

- **可信 execution lifecycle 与 retry safety。** Structured execution state 现在明确区分
  “确定未启动”“可能已经启动但结果未知”“已超时”和“已完成”。模型不再需要从错误文案中
  猜测是否可以安全重试。
- **Structured process 与 script execution。** `run_process` 以 typed data 传递 executable
  与 argv；`run_script` 分离传递有界 script content、script argv 与 stdin。两条路径都不会
  重建 shell text；当 Runner 缺少所需 capability 时会 fail closed，而不是静默回退到 shell。
- **同一次执行的 sync-to-Job handoff。** Structured process/script 超出同步 grace 后，会以
  同一个 durable Job 继续，保持相同 execution identity 与总 timeout budget。handoff 不会
  cancel、restart 或 replay child process。
- **Batch Job observation。** `observe_jobs` 一次可以观察最多 8 个既有 Job，使用一个共享的
  wall-clock wait，并在出现有意义变化后一起返回刷新过的 sibling observations。
- **更顺畅的 polling 与实用 Job concurrency。** 普通长请求执行期间 polling 仍可继续推进，
  且不会 replay execution。Runner Job execution 现在默认并发 4，`max_concurrent_jobs` 的
  有效范围为 1 到 64；原 queued Job 继续按 FIFO promotion，并暴露有界的
  running/queued/limit observability。
- **确定性的 Windows 输出归一化。** Windows 本地 process output 会在 UTF-8、带 BOM 的
  UTF-16 和当前 OEM code page 之间进行确定性解码，并始终以有界合法 UTF-8 提供给模型。
  Streaming 能跨 chunk 保留被拆开的 UTF-8 scalar、UTF-16 unit 和 OEM DBCS character；
  PowerShell 5.1 的 `param(...)` 语义以及 timeout/stop exactly-once 行为保持不变。

## 破坏性变更与兼容性

0.3.4 没有刻意引入 breaking protocol change。新的 execution capability 与 observability
field 都是 additive；旧 Runner 没有声明对应 capability 时会 fail closed。

运维上需要注意一个有意的默认值变化：Runner Job execution concurrency 现在默认是 4。
如果需要严格串行执行，可设置 `max_concurrent_jobs = 1`。该设置仍然需要重启 Runner 后
生效，有效范围会归一化到 1 到 64。Polling dispatch capacity 是独立的固定上限 2，不会从
Job concurrency 推导。

如果 MCP/OpenAPI client 缓存了 tool schema，升级后应刷新缓存，因为 structured execution
和 Job observation surface 已发生变化。

## Structured execution 细节

`run_process` 面向 native executable/argv execution，不会把参数重新拼成 shell command。
Windows 上支持 native `.exe`、`.com` 和 extensionless PE image；`.cmd` / `.bat` 仍属于
shell/script 语义，在 typed path 上会在 spawn 前拒绝。

`run_script` 当前显式支持 `sh`、`bash` 与 `powershell` interpreter，并通过 Runner 自己管理的
temporary script file 执行。本版本没有给 named SSH Session resource 增加 typed
process/script transfer；确实需要 shell 或 remote SSH shell 语义时继续使用 `run_shell`。

如果 capable Runner 无法在有界同步 grace 内结束 structured execution，WebCodex 会把已经
在运行的同一次 execution 暴露为 durable Job。公开的 `timeout_secs` 仍然是一份总预算，
不会在 handoff 时重新计时。

## Job observation 与 concurrency

`observe_jobs` 接受 1 到 8 个既有 Job id，并可带上之前的 observation token。请求中的 wait
是一份 batch deadline，而不是每个 Job 各自拥有一份 wait。它只负责 observation，不会 launch、
retry、stop、schedule 或 subscribe Job。

Runner Job execution 继续使用已有的有界 inventory 与 queue。默认 concurrency 为 4；符合条件
的 queued Job 按 FIFO 使用原本的 `job_id` 与 request exactly once promotion。`list_agents`
和 `runtime_status` 会暴露 effective static limit 以及 authorization-filtered running/queued
count，但不会推导 `available_slots` 或 `saturated`。

## Windows execution 行为

对本地 model-facing process output，Windows 解码顺序固定为：UTF-8 BOM、完整合法 UTF-8、带
BOM 的 UTF-16LE/BE，最后是当前 Windows OEM code page。CRLF 会呈现为 LF，lone CR 保留；
transcoding 后还会再次执行有界 UTF-8 cap，因此 encoding expansion 无法绕过模型侧 byte limit。

0.3.4 acceptance cycle 在真实 Windows/MSVC 上覆盖了 OEM CP936、UTF-8/UTF-16、PowerShell
5.1、split UTF-8/OEM streaming、timeout/stop exactly-once，以及 Runner service-context
polling。

Remote SSH stream 继续使用原有 remote UTF-8/lossy contract，persistent-shell framing 也没有
变化。Windows x64 仍然是连接远端 Linux Server 的受支持 CLI + Runner 平台；长期运行的本地
Windows Server 与产品化 Windows SCM installation 仍不在支持部署模型内。

## 升级说明

1. 从同一个不可变 v0.3.4 revision 一起升级 `webcodex`、`webcodex-server` 和
   `webcodex-runner`。
2. 确认所有 binary 都报告 `0.3.4`、同一个具体 commit，并且 `dirty=false`。
3. 刷新缓存的 MCP/OpenAPI schema，让 client 重新发现 `run_process`、`run_script`、
   `observe_jobs` 以及当前 Job/runtime output field。
4. 如果修改 `max_concurrent_jobs`，请重启 Runner；该设置有意保持为非 hot-reload。
5. Windows 上继续使用 CLI/Runner 连接远端 Linux Server；artifact 中的
   `webcodex-server.exe` 并不意味着长期 Windows Server runtime 已成为受支持部署方式。

## Binary packaging

计划发布的 v0.3.4 artifacts：

- `webcodex-v0.3.4-linux-x64.tar.gz`
- `webcodex-v0.3.4-linux-arm64.tar.gz`
- `webcodex-v0.3.4-darwin-arm64.tar.gz`
- `webcodex-v0.3.4-win32-x64.tar.gz`

每个可发布 artifact 都必须在对应 native host 上从同一个不可变 `v0.3.4` tag 构建，并包含
`webcodex`、`webcodex-server` 和 `webcodex-runner`（Windows 为 `.exe`）。真实 SHA-256 会在
tag 后针对最终 native archive 生成，不会把 placeholder checksum 作为 release metadata 提交。

## 已知限制

- `observe_jobs` 只批量观察已有 Job；本版本没有增加 batch Job launch 或新的 scheduler。
- Named SSH Session resource 的 structured process/script execution 不在本版本范围内；有意的
  SSH shell 工作继续使用 `run_shell`。
- Remote SSH stream decoding 与 persistent-shell framing 不受 Windows 本地输出归一化影响。
- Windows 支持 client/Runner，不支持长期运行的本地 Server、产品化 SCM installation、
  Windows ARM64 或 UNC project root。
- macOS x64 与其他未发布 target 仍不在 release artifact matrix 中。

## 发布验证

Execution A–F cycle 已经完成 Linux 与真实 Windows/MSVC 的跨平台验收，其中包括 MSI Runner
service-context execution。最终 release candidate 仍必须继续执行仓库 release gate：formatting、
workspace compilation/tests、聚焦 runtime/schema coverage、四个 native release host 构建、npm
self-test 与 staged install smoke、provenance/build-identity 检查、Markdown link validation、
checksum verification 和 clean-worktree review。

## 后续

Execution A–F cycle 到这里已经结束。后续 execution 工作只做 maintenance/stabilization，除非
出现新的、具体的 reliability 或产品需求；0.3.4 不会开启新的 execution feature phase。
