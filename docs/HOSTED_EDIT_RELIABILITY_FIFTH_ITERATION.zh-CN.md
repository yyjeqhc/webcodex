# Hosted Edit Reliability：第五轮实现与后续计划

> 状态：第五轮核心纵向切片，2026-07-15。本文记录已经进入代码的编辑、读取、搜索和启动语义，以及下一轮围绕 validation 的开发顺序。

## 1. 本轮的目标

前四轮已经把 hosted chat 接入、Task/Run/Result、隔离 workspace 和可复用构建资源串成一条主路径。第五轮不增加第九个模型能力，也不再增加状态聚合；它直接减少一次普通改动中的重复读取、schema 重试和半成功写入。

模型仍只看到原来的 8 项 project-bound capability。变化发生在现有能力内部：

| 能力 | 第五轮行为 |
|---|---|
| `task_start` | 返回紧凑 Project Brief |
| `files_read` | 每个结果返回完整文件 SHA-256 |
| `files_search` | 返回 query-bound cursor/page |
| `edits_apply` | 一次事务式处理多个 edit/create/delete/rename |

底层 runtime 的同名 `apply_text_edits` 也完成硬切换，不保留旧的单文件双形状。MCP、runtime registry 和 GPT Action `callRuntimeTool` 使用同一份 `changes` 语义。

## 2. 事务式多文件编辑

`edits_apply` 现在接受 1 到 16 个 file change。现有文件的 `edit`、`delete`、`rename` 必须携带 `files_read` 返回的 `expected_sha256`；`create` 的前置条件是目标不存在。

```json
{
  "task_id": "wc_task_...",
  "operation_id": "edit-parser-01",
  "changes": [
    {
      "kind": "edit",
      "path": "src/parser.rs",
      "expected_sha256": "<64 lowercase hex>",
      "edits": [
        {
          "kind": "replace_exact",
          "old_text": "old block",
          "new_text": "new block"
        }
      ]
    },
    {
      "kind": "create",
      "path": "src/parser_tests.rs",
      "content": "..."
    },
    {
      "kind": "rename",
      "path": "src/legacy.rs",
      "to_path": "src/compat.rs",
      "expected_sha256": "<64 lowercase hex>"
    }
  ]
}
```

Server 在 enqueue 前验证 schema、敏感路径、字段组合、大小和所有 source/destination path 是否互相重叠。Owning agent 收到一次结构化请求后再次验证路径，并完成整批 preflight：

1. 所有现有路径都必须是普通、非 symlink、UTF-8 文件；
2. 所有 SHA-256 必须匹配 live worktree；
3. 所有 exact text/anchor 必须唯一，单文件 edits 不能重叠；
4. create/rename 目标必须不存在；
5. 任一 preflight 冲突立即返回精确的 `change_index`、`path` 和错误类型，零文件写入。

Preflight 全部通过后才开始 mutation。Agent 会在每个 source mutation 前再次核对 live hash；文件写入使用同目录临时文件，rename 使用 no-clobber 语义。若后续文件操作失败，agent 按逆序恢复已编辑/删除/重命名的文件并删除已创建文件。返回值包含逐文件 `old_sha256`、`new_sha256`、`changed`、`would_change` 和准确的 `changed_paths`。

这里的“事务式”保证覆盖正常请求生命周期内的全批预检与失败回滚，不虚假宣称多个 POSIX 文件 rename 具有数据库级 crash atomicity。若进程恰好在 filesystem mutation 与 SQLite result 落库之间退出，恢复策略优先阻止盲目重复写，见下一节。

## 3. Operation id：网络重试不能变成重复编辑

每次 connector `edits_apply` 都要求 caller 生成 `operation_id`。SQLite 以 `(task_id, operation_id)` 为唯一键，并绑定 task/run、完整 changes、dry-run flag 和全部 precondition 的 request hash：

- 第一次请求先写入 `pending`，成功后持久化结构化结果；
- 相同 id、相同 request 重试时直接返回已持久化结果，设置 `idempotent_replay=true`，不再 dispatch agent；
- 相同 id 携带不同 change/hash 时返回 `operation_id_conflict`；
- 明确的 preflight/tool rejection 会把 operation 标成 failed；相同 request 可安全重试，修正后的不同 request 必须使用新 id；
- transport timeout/disconnect 或 agent 明确报告 rollback incomplete 后保留 pending 并返回 `edit_operation_uncertain`，跨设备也不能自动重放。用户必须先 review/read，再用新 id 和 fresh hash 表达新的写意图。

这不是把所有请求做成“至少一次执行”，而是在不确定边界选择 fail closed。

## 4. 读取 hash 与搜索分页

Agent 的 range read 在扫描完整文件以确定 `total_lines` 时同步计算 SHA-256。`files_read` 因此即使只返回 200 行，也会返回完整文件的 `sha256`；模型不需要为了取得 optimistic-concurrency guard 再读一遍整文件。Hash 基于原始 bytes，不受 line-number projection 影响。

`files_search` 新增 opaque cursor：

- cursor 绑定 task、pattern、path、globs、context、result mode 和 page size；
- 换 query 复用 cursor 会被拒绝；
- executor 仍按 path 稳定排序，connector 只返回当前 page；
- 单个 query 的 live window 上限为 200 records，达到上限会返回 `window_exhausted`，不会为了“翻完所有结果”放大模型上下文；
- cursor 不是 workspace snapshot。发生编辑后必须重新搜索，避免 offset 在变化结果集上漂移。

## 5. 小型 Project Brief

`task_start` 在 workspace prepare 阶段做一次有界、只读、无内容扫描，并返回：

- baseline commit/tree、dirty 和 conflict count；
- isolated/reusable slot 或 target checkout 策略；
- 最多 8 个 language markers、12 个 manifests；
- 最多 5 个 instruction paths，只返回路径，不重复注入文档正文；
- 最多 5 个按 marker 推导的 recommended checks；
- overview/git evidence 不可用时的短 warning code。

Brief 不包含 runtime status、agent id、绝对路径、历史 events、stdout 或旧 session 聚合。它的作用只是让 hosted model 在陌生项目里选中第一次 targeted read，而不是代替后续按需读取。

## 6. 验收证据

本轮新增和更新的自动化证据包括：

- 多文件 edit/create/delete/rename 在一次 agent request 中成功；
- 第二个文件 hash 冲突时，第一个文件也保持原样；
- read-only session/task 在 enqueue 前拒绝新 batch shape；
- SQLite operation pending/completed/failed/replay/conflict/retry 状态；
- connector durable replay 不触发 executor；
- cursor 只允许原 query 翻页；
- MCP metadata、runtime schema 和 OpenAPI flattened `changes` 保持一致；
- ranged read 返回完整文件 SHA-256。

后续 golden tasks 继续记录首次编辑成功率、schema retry、平均工具调用数、重复读取和 changed-path 准确率。当前实现先建立可测语义，不在没有 baseline 数据时宣称百分比改善。

## 7. 第六轮：project-aware validation

下一轮最有价值的改动是把 `checks_run` 从固定 Rust 三件套变成 project-aware validation，而不是继续扩充 edit schema：

1. 将 Project Brief 的 language/manifests 变成明确的 validation profile evidence；
2. 支持 Rust、Node、Python、Go 的 bounded format/check/test recipe，并允许 Project 声明固定 recipe；
3. 模型只选择 validation intent，不拼接 shell command；
4. 输出统一为 path、line、severity、code、message 和 bounded evidence；
5. 根据 changed paths 选择最小相关检查，同时保留显式 full suite；
6. 记录 checks 所针对的 workspace revision，`task_finish` 区分 passed、failed、not_run、stale；
7. 长检查复用现有 Task event/result 观察，不增加 job/status 模型工具。

第六轮的主要指标是 warm validation 时间、诊断可操作率、修复后的重跑次数和 stale rate。

## 8. 当前边界

- 单批最多 16 个 file changes、每个 edit 文件最多 20 个 exact edits，序列化 payload 最多 1 MiB；
- edit/delete/rename 当前只处理普通 UTF-8 文件，不操作目录、symlink 或二进制文件；
- rename 使用 no-clobber hard link + unlink，源与目标必须位于同一 filesystem；
- transaction rollback 无法保证机器掉电时的跨文件 crash atomicity；uncertain operation 会冻结而不是自动重放；
- search cursor 是 200 records 内的 live sorted window，不是持久 snapshot；
- Project Brief 只做深度 2、最多 200 entries 的 metadata scan，深层规则仍需按需查找；
- 真实 OpenAI/Cloudflare provider 账号验收仍是并行 acceptance lane，本轮未读取任何 provider credential。
