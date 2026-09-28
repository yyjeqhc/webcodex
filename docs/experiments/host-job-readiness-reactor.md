# Host Job Readiness Reactor 实验与下一步设计

日期：2026-09-28。

本文记录 ChatGPT Host + OE MCP probe 的 readiness / wake-and-continue 实验，并给出基于当前 WebCodex main 的最小正式实现设计。这里的 Host Code Mode 指 Host 自己的 JavaScript / functions.exec 编排，不是 WebCodex nested code_mode_exec。本文不引入新的 scheduler、VM、Runner 协议、持久化 wait ledger 或 Job 生命周期。

记录分支创建时，special 主仓库最新 origin/main 为 6e4b1f30。

## 1. 结论

实验已经证明两个关键事实：

1. 一个 Host JavaScript cell 可以 await 一个真正由服务端事件唤醒的长 MCP request；该 request 返回后，同一 cell 可以继续执行 dependent MCP child call，不需要新的模型轮次。
2. 多个并发 long-lived MCP child calls 不能可靠地用 Promise.race 实现 any-ready。服务端较早完成第一个 request 后，Host JavaScript 仍可能直到同批其他 child call 完成后才重新获得控制。

因此下一步不应让 Host 对每个 Job 各开一个长 wait，也不需要 WebCodex 新造调度器。正确的薄层是一个单一 Server-side Job wait-set call：

~~~
wait_for_job_readiness(exact jobs, any|all, bounded deadline)
~~~

Host 只 await 这一条 MCP request。Server 在任一/全部目标 Job terminal 时返回 compact readiness truth，Host 在同一 cell 中继续当前 DAG。

进一步核对当前 main 后，正式实现还能比 probe 设计更薄：src/tool_runtime/observe_jobs.rs 的 wait_for_observed_jobs 已经使用 canonical Job Notify + revision recheck waiter，明确没有 polling heartbeat，并且已经支持 multi-Job、一个 absolute deadline、terminal/all-terminal。正式实现应复用或抽取这套 waiter，而不是新增第二套 JobReadinessSignal 或 terminal event sink。

## 2. Probe 设置

OE 实验仓库：

~~~
Project: agent:oe:mcp-tool-surface-probe
branch: feat/guidance-limit-probe
probe commit: 2710883 test: add Host readiness reactor probe
~~~

公网 MCP endpoint：

~~~
https://mcp.yyjeqhc.cn/probe-surface-readiness/mcp
~~~

路径：

~~~
ChatGPT Host
  -> HTTPS / SF nginx
  -> sf 127.0.0.1:18812
  -> reverse tunnel
  -> OE mcp-tool-surface-probe
~~~

Probe 仅维护进程内、无用户数据的 transient event state。每个 event key 有单调 generation；re-arm 会 supersede 旧 generation；consume 是 one-shot。Server restart 清空全部 probe state。

## 3. Direct / public controls

OE direct-client smoke 在新增 readiness profile 后 14/14 profiles 通过。

Public HTTPS 路径验证了真正的 long-lived request：

~~~
armed delay:       5000 ms
server waited:     3389 ms
client elapsed:    4984 ms
result:            ready
~~~

这说明 nginx、reverse tunnel 和 MCP transport 可以让 request 保持挂起直到事件到达，不需要短 heartbeat polling。

## 4. Host 单 wait 实验

安装独立 MCP Readiness Probe App 后，一个 functions.exec cell 内顺序执行：

~~~
readiness_arm(delay_ms=5000)
  -> await readiness_wait(wait_ms=12000)
  -> readiness_consume(exact generation)
~~~

Host 返回：

~~~
arm_state:       pending
wait_state:      ready
server_waited:   3619 ms
consume_state:   consumed
outer cell:      same cell
cell wall:       ~11.6 s
~~~

OE authoritative timing：

~~~
00:39:40.272Z  arm generation=1 delay=5000
00:39:41.657Z  wait request starts
00:39:45.276Z  event becomes ready
00:39:45.276Z  wait request finishes ready
00:39:47.270Z  dependent consume reaches Server
~~~

没有短 wait slice，没有重复 observation，也没有新的模型轮次。

## 5. Host control cases

| Case | 结果 |
| --- | --- |
| already ready | waitState=ready，Server waitedMs=0 |
| bounded deadline | wait_ms=1000 后 waitState=deadline，waitedMs=1001 |
| stale generation | re-arm 后旧 generation 立即返回 stale_generation |
| duplicate consume | 已消费 generation 再 consume 返回 already_consumed |

## 6. 并发 Promise.race 的 Host barrier

同一个 cell 同时创建两个 long-lived wait：A 约 6 秒 ready，B 约 10 秒 ready，然后 JavaScript 使用 Promise.race 等第一项。

Server timing：

~~~
00:40:51.393Z  A event ready + A wait response finishes
00:40:57.568Z  B event ready + B wait response finishes
00:40:59.461Z  consume(A) reaches Server
~~~

A 的底层 MCP request 已经比 B 早约 6.2 秒结束，但 dependent consume 并没有在 A 返回后立即到达 Server。这里不推断 Host 内部实现，只保留可复现实验结论：

> 当前 Host JavaScript 不能依赖多个并发 long-lived MCP Promises 的 Promise.race 作为低延迟 any-ready primitive。

因此正式方案必须把 any/all 聚合放在一个 Server-side request 内。

## 7. 当前 main 已有的 readiness 内核

下一轮不需要照搬 probe 的 readinessStates。

当前 src/tool_runtime/observe_jobs.rs 已有内部 wait_for_observed_jobs，它已经具备目标机制：

- 每个 Job 使用 canonical job_log_for_auth(..., wait)；
- Job wait 基于 existing Notify + revision recheck；
- 源码注释明确 no polling heartbeat is needed here；
- multi-Job waiter 使用 bounded unordered fan-in；
- terminal 可以 first-ready 返回；
- all_terminal 等待预定集合全部 terminal；
- nonterminal update 只推进 private wait cursor；
- 所有 re-entry 共用一个 absolute Instant deadline，progress 不延长 deadline。

因此新的 readiness surface 应复用该机制或提取共享 helper，而不是新增 Runner update protocol、全局 Job event registry、第二套 tokio::Notify graph、durable wait ledger 或 timer polling。

Runner #730 的 JobUpdateDeliverySignal 仍然是 Runner 内部 delivery reactor；它不需要为了这一层 Host wait-set 暴露出来。

## 8. 与现有 Job surface 的责任边界

### observe_jobs

用于 logs/tails、observation token、changed state、diagnostics/recovery。它内部可以等待，但 model-facing payload 比 readiness 所需更重。

### job_attention

用于下一次普通 outer coding call 顺带携带 sparse terminal/recovery transition；不主动阻塞当前 Host cell。

### wait_for_job_terminal

用于 durable、单 exact Job、跨 model turn/Host continuation；有 wait row、delivery state、App carrier。

### wait_for_job_readiness（当前实现）

只用于当前仍活着的 Host cell：

- transient；
- exact 1..8 Jobs；
- any/all terminal readiness；
- bounded absolute deadline；
- 无 logs；
- 无 recovery；
- 无 durable row；
- 无 Host App carrier；
- 不自动开始、重试、停止或替换 Job。

## 9. 建议的 model-facing contract

第一版：

~~~json
{
  "job_ids": ["wc_job_A", "wc_job_B"],
  "mode": "any",
  "wait_secs": 12
}
~~~

约束：

- 1..8 个 Job；
- 重复 id 可由 Server 稳定去重；
- mode = any | all；
- wait_secs >= 1；
- hard max 第一版建议 45 秒，给当前 Host Code Mode 55 秒默认 budget + 5 秒 return guard 留出额外 jitter；实现时如已有更合适 SSOT，以当前 contract 为准；
- 所有目标 Job 在 wait 前逐个通过当前 caller visibility/authorization；
- 任一 Job unknown/unauthorized/identity-invalid 时整个 wait-set fail closed，不做 partial authorized subset wait。

当前 model-facing 输出进一步收敛为只保留下一步调度需要的动态事实：

~~~json
{
  "wait_state": "ready",
  "ready": [
    {
      "job_id": "wc_job_A",
      "status": "completed",
      "outcome": "succeeded"
    }
  ],
  "pending_job_ids": ["wc_job_B"]
}
~~~

deadline：

~~~json
{
  "wait_state": "deadline",
  "ready": [],
  "pending_job_ids": ["wc_job_A", "wc_job_B"]
}
~~~

请求的 `mode` 与实际 `waited_ms` 仍由 Server 保存在 ActionAudit / model ergonomics telemetry 中，用于 dogfood 与离线分析；正常模型结果不再重复这些调用方已知或纯测量事实。输出也不包含 stdout/stderr、command text、diagnostics、observation tokens、recovery/retry suggestions、Project path 或 Runner instance details。

## 10. 实现形状

不要新建 Reactor subsystem：

~~~
observe_jobs existing canonical wait core
        |
        +--> observe_jobs
        |      final detailed snapshots
        |
        +--> wait_for_job_readiness
               sparse terminal readiness only
~~~

可以把当前 private wait_for_observed_jobs 中与日志 presentation 无关的部分提取成内部 reusable helper，或者新增 sibling helper。不要通过序列化/再解析 observe_jobs model-facing JSON 来实现。

Authorization 复用现有 RunnerRegistry / job_log_for_auth canonical path，不建立第二套 Project/Job ownership 判断。

## 11. Host budget

Host Code Mode 当前默认：

~~~
host budget:    55s
return guard:    5s
generic continuation slice: 5s
~~~

5 秒 slice 是当前 handoff/continuation 策略，不应被解释为 readiness event 必须每 5 秒醒一次。

Host 只在 run-to-quiescence 后计算剩余预算：

~~~
remaining = host_budget - elapsed - return_guard - jitter
~~~

早期 probe 阶段曾建议先用约 10..15 秒的保守 slice dogfood；当前主线 contract 收敛后不再把固定 slice 当作偏好。Host 应在 run-to-quiescence 后，用 activation 的剩余安全预算减去 return guard/jitter，并受 canonical 45 秒上限约束，选择当前可安全使用的最大 bounded wait。

## 12. Host run-to-quiescence 规则

~~~
run currently ready independent work
        |
follow exact mechanically_followable continuations
        |
semantic boundary? ---- yes ---> return compact evidence to model
        |
ready work remains? --- yes ---> continue
        |
only pending Job dependencies remain
        |
one wait_for_job_readiness(any|all, bounded wait)
        |
ready ---> continue newly unblocked work in same cell
deadline / budget guard ---> yield
~~~

规则：

- readiness wait 只能在没有其它 ready work 时开始；
- blocked set 中任意一个 terminal 就能解锁一条有用 branch 时用 `any`，返回后重新计算 ready/blocked set；
- 只有真正的 aggregate/final join 必须等全部 dependency 时才用 `all`；
- 不使用并发 Promise.race([waitJobA, waitJobB])；
- 不用重复 5 秒 observe_jobs 模拟 readiness；
- deadline 后先重新计算 ready work / blocked set；无新 work、无 set 变化、无 semantic information 时不机械重复同一 wait；
- Job terminal 只表示依赖状态 ready，不等于下一步自动可执行；
- follow_up_kind=mechanically_followable 才能继续机械调用；
- fallback_recovery、semantic ambiguity、权限变化、uncertain effect 仍回模型；
- 需要日志/失败细节时才调用 observe_jobs。

## 13. 与 durable wake 的组合

如果当前 cell 剩余预算不足：

- terminal 不是立即硬依赖：正常 yield，后续通过 passive attention 等继续；
- exact Job terminal 是下一阶段硬依赖且需跨 turn：使用现有 wait_for_job_terminal。

因此：

~~~
transient readiness wait = current executor activation
wait_for_job_terminal    = future activation / durable wake
~~~

二者不合并。

## 14. Telemetry

不新增 telemetry 表。复用 ActionAudit / model ergonomics，给新 operation 增加 compact summary：

~~~
mode
requested_jobs
unique_jobs
waited_ms
wait_state
ready_count
pending_count
already_ready
~~~

若 authoritative terminal event 已有 terminal_observed_at，可在 audit-only 路径计算 terminal_to_wait_return_ms。

重点 dogfood 指标：

- job_terminal_to_host_consume_ms；
- readiness_wait_ms；
- already_ready_rate；
- deadline_rate；
- blocked_with_ready_work_ms（若可可靠推导）；
- Host activation wall time / wait fraction；
- semantic boundary / meaningful outer-call proxy。

工具调用数保持描述性，不作为主要优化目标。

## 15. 测试矩阵

至少覆盖：

1. 单 Job 已 terminal -> immediate ready。
2. 单 Job pending -> terminal event 唤醒，不需要 timer poll。
3. 两 Job any：A terminal 后立即返回，B 仍 pending。
4. 两 Job all：A terminal 不返回，B terminal 后返回两项。
5. nonterminal progress update 不满足 terminal readiness。
6. 多次 nonterminal update 不重置 absolute deadline。
7. deadline 与 terminal race 使用确定性 final-snapshot contract。
8. unknown/unauthorized Job 整体 fail closed，不泄露其它 item。
9. duplicate job ids 机械归一化，不创建重复 waiter。
10. lost/stopped/timeout/protocol-terminalization 都被视为 terminal-ready。
11. Runner instance transfer 不改变 logical Job identity。
12. Server restart 不承诺恢复 transient wait；durable Job truth 不受影响。
13. 新 tool 不进入 nested code_mode_exec allowlist。
14. output schema 禁止 logs/recovery/command/path。
15. live dogfood：同一 cell start two Jobs -> independent work -> wait any -> dependent work -> wait remaining/all。

## 16. 非目标

本轮不做 generic workflow/DAG scheduler、server-owned task graph、新 VM、新 Runner protocol、pidfd/kqueue/IOCP、durable multi-Job wait ledger、新 App continuation card、automatic retry/recovery、Job logs push，也不删除 observe_jobs 或把 nested WebCodex Code Mode 变成 Job executor。

只有 telemetry 证明 Runner terminal detection 的 100..250ms polling 成为主要剩余瓶颈时，才考虑更底层 process reactor。

## 17. 成功标准

第一轮成功不是工具调用数下降，而是：

- Host cell 在 ready DAG 清空后睡在一条 Server-side wait-set request 上；
- exact Job terminal 后不需要下一次 5 秒 observation slice 才被消费；
- any/all 不依赖 Host 并发 child Promise race；
- independent work 不因等待 Job 被提前阻塞；
- logs/recovery 不因 readiness 自动进入模型上下文；
- 不新增 authority、Job identity 或 durable state；
- timeout/cancel 后没有 orphan waiter；
- 长任务仍可在 Host activation 边界使用既有 durable continuation。

架构目标：

~~~
Host ready DAG
    |
run until quiescent
    |
single transient Job wait-set
    |
canonical event wake
    |
same-cell continuation
    |
semantic boundary only -> model
~~~

这是从当前“已有事件，但 Host 仍按 observation slice 消费”的阶段，进入 readiness-driven Host orchestration 的最小一步。
