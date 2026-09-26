# WebPi 全维度体检、修复、发布与网页插件迁移 Goal

## Scope

目标对象固定为用户 PC 上的 `E:\WebPi\webpi-core`。同一台 PC 的 Ubuntu 26.04 WSL2 只作为 Pi 参考实现与兼容验证环境。

最终闭环：建立真实基线 -> 深度 review -> 修复/优化 -> 严格 TDD/完整烟测 -> diff/PR -> 可回滚部署 -> 将已部署 WebPi 迁移为网页 GPT 插件 -> 再次 review/修复/优化/TDD/烟测/PR/部署。

## Quality gates

- 不覆盖或清理既有脏工作区；所有本次修改必须可单独识别、审查。
- 权限、事务、状态机、并发、恢复、缓存、跨 Shell/跨平台行为修改优先采用可复现 Red -> Green TDD。
- 不把 HTTP 200、进程启动、编译成功、零测试或未认证响应当作部署成功。
- Release 需要 clean revision、精确构建身份、rollback 点、部署后 loopback/public/auth/tool smoke。
- WebPi 对外命名必须统一；`webcodex-*` crate、稳定 wire/resource/db/credential 标识仅在明确兼容边界内保留。
- Pi 扩展采用发现 -> 审查 -> 精确候选/fingerprint -> 用户授权 -> reload -> describe -> 调用 -> 验收；不自动批准未知扩展。
- 网页插件层不得复制第二套执行权限/任务真相；优先复用 WebPi Goal/AgentTask/WorkflowSession/Job 与 MCP/Plugin gateway。

## Current baseline

- Runtime/Runner: WebPi 0.4.1，`webpi-local` online/readiness ready。
- Git HEAD: `63640bb8e9b11f6996b13d5a99812ba301eddbcd` (`webpi` branch)。
- 当前工作区在本 goal 开始前已高度 dirty；Server/Runner build 也为 dirty，`source_alignment=different`。
- 当前 credential 可读 runtime、写 project、运行 job；缺少 `communication:read/manage`、`service:restart/deploy`、`plugin:mutate` 等权限。原生 `create_goal` 因 `communication:read` 缺失而 fail-closed，因此此文件作为当前可追踪 Goal 载体，待 scope 合法补齐后再镜像到原生 Goal。
- Baseline checks: `cargo check --all-targets` PASS；WebPi Python tests 104/104 PASS；Pi bridge Node tests 21/21 PASS。
- 已发现并 TDD 修复首个真实缺陷：Windows `git_log` 把 POSIX 脚本错误送给 PowerShell；新增回归测试先 RED（`run_shell` vs `run_internal_posix_script`）后 GREEN。

## Reference plugins

WSL2 已确认：`pi-goal-x` 0.31.6、local `pi-goal-x-cachefix`、`@juicesharp/rpiv-todo`。参考其 prompt-cache、ledger/scheduler/auditor、per-session todo、依赖图/循环拒绝、单 in-progress、失败不完成等语义；不复制其存储模型。
