# Hosted Execution Efficiency：第四轮实现与后续工具计划

> 状态：第四轮核心纵向切片，2026-07-15。本文记录已经进入代码的执行资源策略，以及接下来围绕工具效果而不是接入层继续开发的顺序。

## 1. 本轮解决的问题

第三轮建立了正确的隔离与 Result 语义，但执行资源策略仍然按 Task 创建目录：Git objects 会共享，Rust 的 `target/` 却会在每个 worktree 下重新生成；并且 worktree 要等 accept/reject 后才释放。对个人用户而言，这会把安全隔离变成明显的磁盘与冷构建成本。

第四轮保留“线上模型不能直接污染目标 checkout”的产品承诺，同时把执行路径改为一个可复用的受管槽位：

    state/
      runs/
        write-slot-01/                 # 固定 detached Git worktree
        .write-slot-01.lease.json      # 当前 Task/Run 的私有租约
      cache/
        cargo-target/                  # 当前 Project 共用的 Cargo 输出
      results/
        wc_task_*.patch                # 不随槽位释放而消失的稳定 Result

默认只有一个 writable slot。它符合当前个人主路径，也让资源占用可预测；read-only Task 不占槽位，仍可并行。未来若真实多用户负载证明需要并行写，再把槽位数量变成 Project policy，而不是现在先复制 N 份 Rust 构建目录。

## 2. 可复用槽位与租约

`task_start(mode=normal)` 现在会原子创建槽位租约。租约绑定准确的 task id、run id 和 baseline commit；槽位被占用时，另一个写 Task 会收到明确的 conflict，而不是再创建一套 worktree。Task 内部的多窗口请求仍由 task lock 串行。

首次使用时，WebCodex 从目标仓库的 `HEAD` 创建 detached worktree。后续 Task 复用同一路径，并在取得租约后：

1. 验证槽位与目标 checkout 指向同一个 Git common directory；
2. 强制回到 detached baseline，避免模型曾切换 branch 后误移动 branch ref；
3. 清除槽位中的 tracked/untracked/ignored 残留；
4. 重新注册固定的内部 executor project。

所有 destructive cleanup 只允许作用于私有 `runs/write-slot-01`，并同时校验受管目录名、直接父目录、Git repository 和 run lease。旧 Result 在 accept/reject 时若发现槽位已经属于新 Run，会保持 no-op，不能清理新任务。

## 3. Result 先持久化，workspace 后释放

写 Task 的完成顺序现在是：

    capture bounded patch
      -> commit Result + completed Run in SQLite
      -> detach/reset/clean slot
      -> remove temporary executor registration
      -> remove lease
      -> record workspace_release event

因此 `task_finish` 成功后，槽位立即可供下一 Task 使用，不再等待用户 accept/reject。`task_review`、`task show` 和 accept 只依赖带 size/SHA-256 校验的 Result artifact，不依赖仍然存在的修改现场。

如果 Result transaction 失败，租约和现场都保留，用户可以修正后重试 finish。如果 Result 已提交但释放失败，Result 仍然有效，cleanup warning 会进入稳定 Result/timeline，槽位保持占用以阻止不安全复用。本机 accept/reject 会在 run lease 仍匹配时再尝试一次安全释放。

## 4. 启动恢复与无主资源回收

Runtime restart 仍先把 running Run 标成 interrupted。之后 workspace recovery 只保留 SQLite 中 interrupted Run 拥有的 execution root 和 executor registration：

- interrupted 槽位缺少 lease 时，按数据库中的 task/run/baseline 恢复 lease，避免另一个 Task 抢占；
- completed Result 与槽位释放之间发生崩溃时，启动恢复会清理并释放无主槽位；
- 第三轮遗留的 `runs/wc_run_*` per-run worktree 只有在没有 interrupted Run 引用时才回收；
- 不认识的目录、非目标 Git repository 和租约不匹配的槽位不会被 destructive cleanup。

这使“保留可恢复现场”和“不要永久泄漏 worktree”成为同一个确定性恢复流程，而不是依赖用户记得 reject。

Interrupted Task 现在有两个本机决定：`webcodex task resume TASK_ID` 保留现场继续；`webcodex task reject TASK_ID` 明确放弃尚未捕获的改动，原子记录一个 rejected/abandoned Result 后释放匹配租约。后者只允许本机 authority 对 interrupted Run 执行，不能由 hosted chat 调用，也不会假装生成过 patch。

## 5. Project 级 Cargo 构建缓存

Hosted executor 进程现在使用私有的绝对 `CARGO_TARGET_DIR=state/cache/cargo-target`，该目录同时加入 executor 的受管 allowed roots。`checks_run` 的 `cargo_check`/`cargo_test` 以及经过本机批准的 Cargo command 都继承这一设置。

收益有两层：

- 构建产物不进入 worktree，因此槽位 cleanup 不会删除 warm artifacts；
- 槽位路径固定，不同 Task 的 source path 也稳定，Cargo 更容易复用 incremental/fingerprint 结果。

Cache 是可重新生成的本机执行数据，不是 Task Result，不会进入 patch、SQLite timeline、MCP 响应或跨设备同步。当前采用每 Project 一份 cache，尚未自动删除超额 cache；在取得真实项目的峰值数据前，不用一个武断阈值反复制造冷构建。

## 6. 本机可见性

`webcodex task list` 在原有 Task 列表上增加一行资源摘要；`--json` 返回同样的结构化信息：

- writable slot 是 uninitialized、idle 还是 occupied；
- occupied 时对应的 task/run；
- reusable checkout 与 shared Cargo cache 的有界磁盘扫描结果；
- 扫描超过 250,000 entries 时明确标记 truncated。

这仍是本机 human surface，不新增模型状态工具，也不把绝对 cache 路径或内部 executor id发送给 hosted chat。

## 7. 第五轮：先提升编辑与读取的成功率

接入层到第四轮已经足够支撑真实工具迭代。下一轮不做 browser console，也不先做 shared control plane；优先减少模型为了完成一次普通改动所需的调用、重试和猜测。

第五轮建议交付一个原子的 multi-file `edits_apply` 语义，而不是增加第九个模型工具：

1. 单次请求支持多个文件的 precise text edits，以及 create/delete/rename；
2. 每个输入文件带 expected SHA-256，整批先 preflight，任一冲突则零写入；
3. 返回每个文件的新 hash、changed/no-op、失败位置和可重试原因；
4. 相同 operation id + 相同 precondition 可安全重放，不把网络重试变成重复编辑；
5. `files_read` 默认返回 content hash，`files_search` 增加稳定 cursor/page，而不是用更大的响应掩盖截断；
6. `task_start` 返回一份小型 Project Brief：Git baseline、dirty/conflict、语言 markers、相关 instructions 和推荐 checks，不返回历史 runtime dump。

验收指标不是字段数量，而是 golden task 的首次编辑成功率、schema retry、平均工具调用数、重复读取次数和 Result changed-path 准确率。

## 8. 第六轮：让 validation 真正匹配项目

第六轮再把 `checks_run` 从 Rust 固定映射提升为 project-aware validation：

- 发现 Rust、Node、Python 等明确 marker，并只选择有证据的 recipe；
- Project 可声明固定 check recipe，模型只选择 format/check/test 意图；
- 诊断统一成 path、line、severity、code、message 和 bounded evidence；
- 根据 changed paths 推荐/执行最小相关检查，同时保留用户要求 full suite 的入口；
- 长检查具有稳定 progress/result，可由现有 `task_review` 查看，不增加 job 状态工具；
- `task_finish` 清楚区分 passed、failed、not_run、stale，不能把“调用过检查”当成“检查通过”。

第六轮的核心指标是 warm validation 时间、诊断可操作率、错误修复后的重跑次数和 validation stale rate。

## 9. 多用户、多设备何时进入

多用户/多设备仍是目标架构的一部分，但应在工具闭环稳定后进入下一主阶段。否则 shared control plane 只会更可靠地同步低成功率的编辑和模糊 validation。

届时优先实现的是 User/Device/ProjectMembership/Workspace routing，以及 Result 在不同 Workspace apply 前的 baseline/precondition 检查；本地个人模式继续使用同一 Task/Run/Result 语义。Writable pool 数量也应由 Workspace 能力与 Project policy 决定，默认个人模式仍为 1。

## 10. 当前边界

- 只有一个 reusable writable slot；interrupted Task 会有意占住它，直到本机 resume/完成或明确 reject 放弃未捕获改动；
- Cargo cache 没有自动 quota/LRU，只提供有界占用可见性；
- cache 是每台设备本地的，不跨设备传输；
- 非 Cargo 生态的共享 cache 与 project-aware recipe 留给第六轮；
- Result apply 与 SQLite accepted decision 仍不是跨文件系统原子 transaction；
- 真实 OpenAI/Cloudflare provider 账号验收不属于本轮，也没有读取 provider credential。
