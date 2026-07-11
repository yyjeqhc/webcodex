# 优化评审（2026-07-11）

一次针对最近提交与项目整体的评审记录。范围：`main` 分支 `fbe2e3e` 及之前
约 13 次 LSP 相关提交，加上仓库整体结构、构建、发布与代码健康度。
这是一份时点快照，不是持续维护的产品文档；条目落地后可删改。

## 最近提交评审结论

最近 13 次提交（`a399222`…`fbe2e3e`）完成了 LSP Phase 1 与 Phase 2 只读
MVP。总体质量很高，没有发现真正的正确性 bug：

- 边界设计严谨：请求是封闭枚举（未知操作在反序列化即失败）、路径全部
  项目相对化、错误信息脱敏（240 字符截断 + 路径替换）、结果去重排序。
- 并发正确：`DiagnosticsCache` 的 Condvar 等待处理了虚假唤醒、deadline
  与关闭标志；supervisor 慢操作（shutdown）不持全局 map 锁；agent 侧
  `dispatch_request` 走 `spawn_blocking`，慢 LSP 请求不会阻塞文件/shell
  请求（`src/bin/webcodex_agent/transport.rs`）。
- 资源有界：服务器数量、消息大小、文档大小、stderr、各类文本截断都有
  上限，且有测试固定安全配置（buildScripts/procMacro 关闭）。
- `fbe2e3e` 的 rustup shim 检测（纯文件系统判断，不 spawn 进程）在
  歧义时回退到旧语义，取舍得当。

## 本次已直接修复

1. **`docs/LSP_NAVIGATION.md` 死链与内容过期**（已修复，待提交）。
   `docs/INDEX.md` / `INDEX.zh-CN.md`（提交 `aa93927`）链接了该文件，但
   文件本身一直未纳入 git——克隆仓库的人会遇到死链。且本地这份还停留在
   Phase 1：写着"四个工具"“无 diagnostics/workspace symbols"“无文档同
   步"，与已提交实现（7 个工具、`full_text_sync_only`、磁盘全文同步）
   矛盾。已按代码实际行为重写过期章节（工具表、资源限制表、已知限制、
   排障表），limit 数值与 `src/lsp_bridge.rs` 常量一致。
2. **`Cargo.toml` 增加 `[profile.release]`**（strip + thin LTO +
   codegen-units=1）。三个二进制通过 npm 分发，此前未 strip：
   webcodex 27.4 MB、agent 14.6 MB、cli 11.4 MB。实测结果见下方
   "构建产物体积"。未启用 `panic = "abort"`（会改变 unwinding 行为，
   不值得为体积冒险）。

### 构建产物体积（实测，Linux x64）

| 二进制 | 之前 | 之后 | 变化 |
|---|---|---|---|
| webcodex | 27.4 MB | 19.1 MB | −30% |
| webcodex-agent | 14.6 MB | 11.1 MB | −24% |
| webcodex-cli | 11.4 MB | 8.2 MB | −28% |
| 合计 | 53.3 MB | 38.4 MB | −28% |

全量 release 构建耗时 4 分 04 秒（thin LTO + codegen-units=1 会比默认
慢一些，只影响 release 构建，不影响日常 `cargo check`/`test`）。三个
二进制 `--version` 冒烟验证通过（strip 不影响 build_info 注入）。

## 建议（按优先级）

### P0：补上 CI

仓库没有任何 CI（无 `.github/workflows/`）。项目已有约 1,856 个测试、
rustfmt 干净、脚本化冒烟测试齐全，就差一个入口。建议最小工作流：

```yaml
# .github/workflows/ci.yml
name: CI
on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --bins --tests -- -D warnings   # 先清理存量告警，或暂去掉 -D
      - run: cargo test --bins
      - run: cargo build --release --bins                  # 保证发布配置可构建
```

说明：当前 clippy 有约 47 条存量告警（见 P1），`-D warnings` 之前需要
先清零，或先以非阻断方式跑。npm 包侧可以把
`scripts/npm_package_smoke.sh` 挂为第二个 job。

### P1：清理 clippy 存量告警

`cargo clippy --bins` 当前约 47 条，分布：

- 8×"too many arguments (8/7)"、6×"9/7"、2×"11/7" —— 集中在 dispatch/
  enqueue 类函数，可以把参数收拢为上下文结构体（顺带改善可读性）；
- 7× needless borrow、4× needless `as_bytes`、4× 可用 `sort_by_key`、
  2× 可折叠 `if`、2× 可派生 `impl`、2× 多余 `Ok(...?)` 等机械项；
- 2×"`if` has identical blocks"（`src/db/audit.rs:152`、
  `src/tool_runtime/jobs.rs:537`）——确认过不是 bug，两个分支确实同值，
  但值得合并以消除"看起来像 bug"的噪声；
- 3×"large size difference between variants"——`Box` 大变体即可。

清零后在 `Cargo.toml` 加 `[lints.clippy]` 或在 CI 用 `-D warnings`
防止回潮。

### P1：用 workspace 拆分替代 `#[path]` 模块共享

三个二进制通过 `#[path = "../x.rs"] #[allow(dead_code)] mod x;` 共享
`lsp_bridge`、`shell_protocol`、`admin_cli`、`agent_init` 等模块，全仓
共约 90 处 `allow(dead_code)`，多数是这个模式的症状。建议拆成
workspace：`webcodex-core`（共享协议/类型）+ `webcodex-server` +
`webcodex-agent` + `webcodex-cli`。收益：

- dead_code 抑制大幅减少，真正的死代码重新可见；
- agent/cli 不再重复编译 server 依赖面（salvo 等），增量构建和
  `cargo check` 更快；
- 每个二进制的依赖边界显式化（agent 理论上不需要 salvo/rusqlite）。

这是结构性工程，建议单独立项、分阶段迁移（先抽 `shell_protocol` +
`lsp_bridge` 这两个纯类型模块）。

### P2：生产路径的 `unwrap()` 收敛

非测试目录下约 1,652 处 `unwrap()`（含 inline `#[cfg(test)]`，实际生产
路径更少但仍可观），集中在 `src/bin/webcodex-agent.rs`（403）、
`src/db.rs`（218）、`src/runtime_http.rs`（120）、
`src/shell_client/mod.rs`（119）、`src/agent_ws.rs`（112）。服务器/
agent 是长驻进程，panic 即掉线。建议：

- 不搞一刀切替换；按文件做审计，锁/时间等"不可能失败"的场景换
  `expect("原因")`，I/O、解析、协议路径换错误传播；
- LSP 模块已有 `lock_unpoison` 模式，可推广到其他模块。

### P2：`semantic_navigation.rs` 错误分类去字符串化

`src/tool_runtime/semantic_navigation.rs:306` 依赖
`error.contains("unknown shell client")` / `contains("does not support")`
对 `enqueue_lsp` 的 `String` 错误分类。上游改一个字，startup 概要
的 `reason_code` 就会静默退化为 `probe_failed`。建议 `enqueue_lsp`
返回带枚举的错误类型（或在错误上带稳定 code），字符串匹配只留作兜底。

### P2：多平台 npm 分发

`npm/webcodex` 目前只覆盖 Linux x64（`install.js` + manifest）。macOS
（arm64 尤其）是本地开发主力平台，`README` 的 npm 安装路径对这批用户
直接失败。建议在 CI 里加 matrix 构建（linux-x64 / darwin-arm64 /
darwin-x64，win 视需求），release 时上传各平台产物并扩展 manifest。

### P3：文档一致性检查脚本

这次的死链（INDEX → 未提交的 LSP_NAVIGATION.md）属于一类可机械检查的
问题。建议加 `scripts/docs_check.sh`：校验 `docs/*.md` 相互链接的目标
文件存在，并挂进 CI 或 `release_check.sh`。中英对照目前是"面向用户的
文档成对、内部/运维文档只有英文"（共 9 个无中文版，含 ARCHITECTURE、
OAUTH2_*、TESTING 等）；如果这是有意约定，可在脚本里维护一个豁免清单，
让"漏翻译"也变成可检查项。

### P3：其他小项

- `edition = "2021"` 可升级到 2024（低风险，`cargo fix --edition`）；
  顺带补 `rust-version`（MSRV）字段，npm 包用户从源码构建时报错更友好。
- `Cargo.toml` 缺 `description` / `repository` 字段（`cargo publish`
  会拒绝；即使不发 crates.io，补上对工具链也有益）。
- 启动探针（`probe_semantic_navigation_for_startup`）每次
  `start_coding_task` 都要一个 RTT（上限 2 s）。若实际使用中连续开任务
  常见，可按 (client_id, project) 做 10–30 s 的结果缓存；若不常见则
  维持现状（缓存会引入状态陈旧问题，不必提前做）。

## 明确不建议做的

- `panic = "abort"`：体积收益有限，会改变 unwinding/`catch_unwind`
  行为，长驻服务不值得。
- 为拆而拆大文件：`files.rs`（4.1k 行）等虽大但内聚、测试充分，拆分
  应跟随 workspace 重构自然发生，而不是单独的行数指标运动。
- 追求 `opt-level = "z"`：这是网络 I/O 密集型服务，不该为体积牺牲
  运行性能；strip + LTO 已拿走大头。
