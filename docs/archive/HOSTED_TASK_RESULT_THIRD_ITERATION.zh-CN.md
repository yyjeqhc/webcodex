# Hosted Task Result：第三轮实现与验收边界

> Archive：历史实现记录，不是当前使用指南。当前入口见 `../QUICK_START.zh-CN.md`。
>
> 状态：第三轮核心纵向切片，2026-07-15。本文记录已经进入代码的个人本地主路径；不把 browser console、跨机器同步或真实 provider 账号验收写成已完成。
>
> 资源策略更新：第四轮已将这里的 per-run worktree 与 accept/reject 后清理，替换为带 Run 租约的固定可复用槽位、`task_finish` 后立即释放和 Project 级 Cargo cache。稳定 Result 与人工决策语义保持不变，详见 [HOSTED_EXECUTION_EFFICIENCY_FOURTH_ITERATION.zh-CN.md](HOSTED_EXECUTION_EFFICIENCY_FOURTH_ITERATION.zh-CN.md)。

## 1. 本轮真正改变的体验

第二轮让线上窗口只看到 8 项 project-bound 能力，但写操作仍共享用户 checkout，`ready_for_review` 也没有一个可由人类决定的稳定结果。第三轮把“允许线上模型工作”和“接受它的代码”拆成两个独立决定：

1. `task_start(mode=normal)` 从目标仓库当前 `HEAD` 创建 detached execution worktree；
2. `files_read/files_search/edits_apply/checks_run/commands_run` 全部路由到该 Run 自己的 executor project；
3. `task_finish` 固化 content-addressed patch、changed paths、validation projection 和 warnings；
4. 线上窗口只能 `task_review`，不能 accept、reject 或批准 raw command；
5. 本机用户通过 `webcodex task ...` 查看和决定结果。

模型可见能力仍然是第二轮的 8 项，没有为了 worktree、Result 或 approval 再增加状态查询工具。Human operation 也没有进入 MCP 或 GPT Actions/OpenAPI。

## 2. 隔离 Run 与 baseline

个人 hosted profile 在私有 state 下新增：

    state/
      runs/wc_run_*/
      results/wc_task_*.patch
      agent/projects.d/wc-run-*.toml

写 Task 启动时记录 target/execution executor ref、target/execution root、baseline commit/tree 和 isolation flag。Git worktree 创建完成后，仍由现有 agent project registration 路径验证 allowed root、owner 和 project policy；注册或 SQLite transaction 失败时会清理未持久化 worktree。

不同 Task 拥有不同 execution root，不再需要用全局 workspace lease 串行所有用户的写调用。同一 Task 的多窗口/多设备请求仍按 `task_id` 串行，避免 `finish` 与尚未结束的 edit/check/command 竞争。

`read_only` Task 不创建 worktree，也不能调用 edit/check/command；它仍可用于低成本检查非 Git 目录。默认写 Task 必须有可读取的 Git `HEAD`，不再在没有 baseline 时猜测 whole-worktree attribution。

## 3. 稳定 Task Result

`task_finish` 不再依赖一次易失的 `show_changes` 响应。它会：

- 在 execution worktree 中建立最终 index snapshot；
- 以 task baseline 生成 `--binary --full-index` patch；
- 保存 patch SHA-256、字节数和 changed paths；
- 从 Task timeline 投影已经运行的 checks；
- 在同一个 SQLite transaction 中写入 Result、结束 Run、迁移 Task 到 `ready_for_review`。

当前保护边界为最多 1,000 个 changed paths、4 MiB patch。Result 若包含 `.env`、credentials、private key 等受保护路径，`task_finish` 会 fail closed，且不会把 Task 假装成已完成。Patch artifact 使用私有目录和 `0600` 原子写入；`task_review`/`task show` 读取前重新校验记录的 size 与 SHA-256，并只返回有界 preview。

Result decision 是 SQLite authority：

    pending -> accepted
            -> rejected

`accepted/rejected` 是 Result decision 的投影，不给原 Task 表增加别名状态，也不向旧 workflow session 双写。

## 4. 本机 accept/reject

Human authority 只存在于本机 CLI：

    webcodex task list
    webcodex task show wc_task_...
    webcodex task accept wc_task_...
    webcodex task reject wc_task_...

命令默认根据当前 Git root 和 `personal` profile 找到与 `webcodex connect` 相同的私有 SQLite；也支持相同的 `--root`、`--profile` 和 `--state-dir`。本机文件权限是这一版的人类 authority，不接受远端 bearer token 代替本机决定。

`accept` 在写目标 checkout 前重新验证：

- target `HEAD` 仍等于 task baseline；
- Result 涉及的目标路径没有本地修改；
- patch artifact size/hash 未变化；
- `git apply --check` 成功。

全部满足后才 `git apply`，随后记录 accepted decision 并清理 execution worktree/project registration。`reject` 不触碰目标 checkout，只记录 rejected decision 并清理 execution workspace。Linux/Unix 下本机 result decision 使用 profile 内的文件锁串行，避免两个本机 accept/reject 进程同时决定。

## 5. raw command 的真实一次性批准

`checks_run` 仍是无需额外人工往返的标准 validation path。`commands_run` 是高级 escape hatch，第一次请求不会 enqueue executor，而是持久化 Approval 并返回：

    approval_required
    approval_id
    action_hash
    expires_at
    webcodex task approve TASK_ID APPROVAL_ID

本机可执行：

    webcodex task show wc_task_...
    webcodex task approve wc_task_... wc_apr_...
    webcodex task deny wc_task_... wc_apr_...

Action hash 绑定 task、run、完整 command、cwd、timeout，以及当前 Git index 与 Git 可见 worktree 内容。Worktree snapshot 通过临时 Git index 计算，不会 stage 用户真实 checkout；ignored 文件和进程环境不属于这个 precondition。批准后只有参数和 workspace precondition 完全相同的重试能原子消费一次；相关参数、index 或文件内容变化会产生新的 Approval，已消费 Approval 不能 replay。TTL 当前为一小时；finish 或 runtime restart 会令未消费 Approval 失效。

Approval event 只保存 id、hash、kind、TTL 和安全摘要，不把 raw command、stdout 或源码写入 Task timeline。本轮实现的是 project-bound connector 的 `commands_run` gate；旧通用 `WEBCODEX_PERMISSION_MODE=require_approval` 仍保持 fail-closed，不冒充已经获得通用审批队列。

## 6. 异常中断与恢复

Connector runtime 启动时会在 transaction 中把同 Project 上遗留的 `running` Run 标记为 `interrupted`，追加 `run_interrupted` event，并使 Task 投影为 `needs_attention`。它不会删除 execution worktree，也不会把未知副作用标成 completed。

本机确认 worktree 和 agent project registration 仍存在后，可以执行：

    webcodex task resume wc_task_...

这会追加 `run_resumed` 并恢复同一个 task/run；线上窗口可继续使用原 `task_id`。应先让本地 connector runtime 保持运行，再从另一个本机终端 resume。第四轮起，若明确不希望继续，也可在本机直接 `webcodex task reject TASK_ID`，放弃未捕获的 interrupted workspace 改动并释放匹配槽位。

## 7. 多用户与多设备边界

线上 Task 的 owner 仍是稳定 subject：同一个 managed user 的不同 PAT/OAuth token 可以跨窗口继续同一 Task，不同 subject 看到统一的 `task_not_found`。隔离 worktree 进一步消除了多个用户 Task 直接写同一 checkout 的主要竞态。

这不等于跨机器同步。个人 profile 的 Task、Result、Approval 和 patch 都在运行 `webcodex connect` 的机器；另一台设备只有连到同一 runtime 且身份映射到同一 subject 时，才能继续线上调用。共享 control plane、Device enrollment、ProjectMembership 和跨 Workspace Result apply 仍属于下一阶段。

## 8. 验收边界

本轮自动化覆盖：

- isolated worktree 中的改动在 accept 前不会进入目标 checkout；
- target changed path 有本地变化时 accept fail closed；
- patch size/hash 在 review/apply 前复验；
- raw command 未批准不 dispatch，批准只消费一次；
- command precondition 随内容变化，且不修改真实 Git index；
- finish 使未消费 Approval 失效；
- runtime restart 将 running Run 标为 interrupted，并可由本机恢复；
- Task/Result/Approval 的 project/subject 边界与单调 event timeline；
- MCP/OpenAPI 仍从同一 registry 投影精确的 8 项能力。

最终命令和通过数量以本次提交报告为准。没有 provider credential 的环境不会读取 credential 文件，也不会把 OpenAI/Cloudflare 账号路径写成已验收。

## 9. 已知限制与下一步

- 目前只有本机 CLI review/approval，没有 browser inbox；
- Approval 本地摘要不保存 raw command 明文，用户需要结合发起请求的线上窗口核对 action hash；
- Result accept 应用到工作区但不自动 commit；
- 个人 topology 没有跨机器 Task/Result 同步；
- 标准 checks 仍以当前 Rust-oriented `format/check/test` 为主；
- accept 的 Git apply 与 SQLite 最终 decision 之间尚不是跨文件系统原子 transaction，极少数进程/磁盘故障需要人工核对目标 checkout 与 Task timeline；
- 真实 ChatGPT Connector、OpenAI Secure MCP Tunnel 和 Cloudflare 账号验收仍需在具备相应凭据的机器上执行。

下一轮最有价值的工作不是继续增加模型工具，而是完善本机 review/approval UX，再进入可选 shared control plane 的 Device/Workspace/ProjectMembership 主路径。
