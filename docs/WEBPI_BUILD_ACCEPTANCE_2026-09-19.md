# WebPi Windows 构建接续验收 — 2026-09-19

## 范围与状态

工作树：`E:\WebPi\webpi-core`，分支 `webpi`，基准 HEAD `3cb489dce8557a94a3e56c51ed71c634e3169939`，包含已有未提交的产品身份与配置隔离改动。本轮接续格式化、组件构建和 Windows CLI 回归，不是发布或生产部署，也不是对所有平台、全部历史能力和未知漏洞的穷尽证明。

本轮实际复查时，原生三组件已有一次成功构建；首次 `cargo fmt --all -- --check` 也通过。CLI 完整库测试为 346 通过、1 失败，而不是截图历史状态中的 5 失败。唯一复现失败为 `tests::usage::webcodex_cli_runner_help_mentions_lifecycle_subcommands`，仍期待旧帮助文本 `webcodex run`。

## 本轮修复

- `crates/webcodex-cli/src/webcodex_cli/tests/usage.rs`：按现有 WebPi 身份契约将该断言更新为 `webpi run`，同时明确禁止旧执行提示；增加 `webpi_help_uses_product_binaries_and_config_paths` 回归，分别检查 Server、Plugin、Runner 安装和 Runner 状态帮助。
- `crates/webcodex-cli/src/webcodex_cli/usage.rs`：修正残留的 `webcodex plugin`、`webcodex-server`、`webcodex-runner` 和 `$XDG_CONFIG_HOME/webcodex` 操作提示。保留本地兼容 SDK 的真实包名，不改协议和凭据格式。
- 运行一次最终 `cargo fmt --all`，再运行检查模式。没有关闭鉴权、放松路径限制、跳过失败测试或删除历史失败记录。

## 本轮验证结果

以下均为本机实际执行结果，不代表远程 CI 已运行：

| 检查 | 结果 |
| --- | --- |
| 最终 `cargo fmt --all -- --check` | 通过 |
| `cargo test --profile dogfood --locked -p webcodex-cli --lib` | 348 通过，0 失败，0 忽略 |
| `cargo test --profile dogfood --locked -p webcodex-core -p webcodex-runner-config -p webcodex-validation --lib` | 三包合计 381 项通过 |
| `python -m unittest discover -s scripts/webpi/tests -v` | 46 项通过 |
| 前端 TypeScript `--noEmit` | 通过 |
| 前端 `scripts/build.mjs --out-dir dist --check` | 31 个产物与当前源码一致 |
| 前端 `node --test` | 178 通过，0 失败、0 跳过 |
| Plugin SDK 类型检查、构建 | 通过 |
| Plugin SDK `node --test test/*.test.mjs` | 17 通过，0 失败、0 跳过 |
| Pi bridge TypeScript 构建 | 通过 |
| Pi bridge `node --test *.test.mjs` | 32 通过，0 失败、0 跳过 |
| 新 WebPi 原生三组件 dogfood 构建 | 通过，退出码 0 |
| `python scripts/webpi/http_auth_fixture.py` | 29 项检查通过，临时监听器关闭，未修改生产服务 |

CLI 最早失败保留在本轮验证历史中，修复后的同名验证已通过；不把成功重跑当作抹去失败历史。

## 真实 HTTP 验证所证明的内容

fixture 使用全新临时数据、随机 loopback 端口和本次构建的程序，不占用生产端口 56542。它验证三个受保护路径对缺失 Bearer、未知普通 Bearer、未知托管 Bearer及错误认证方案共 12 个案例均返回 401；验证真实托管 PAT 可以读取启用认证的 WebPi 状态，OpenAPI 公网 origin 正确；验证三程序版本身份、OpenAPI/MCP/runtime 产品身份、拒绝旧配置和宽松模式、缺少同安装目录程序不回退 PATH，以及继承的 WebCodex 数据目录未被使用。

fixture 还注册自己的 Runner，并通过临时 bootstrap 管理凭据描述和调用 `pi_read`，实际返回 `engine=pi` 及测试文件内容。此检查证明插件协议和调用链可用；它不等于生产 Action PAT 的权限或外网访问已经验收，也不等于网页 GPT 已完成实际调用。

fixture 完成后报告 `fixture_listener_closed=true`、`production_service_modified=false`；随后检查 `.webpi-runtime/auth-fixture-*` 无剩余目录。

## 构建产物与身份

构建命令：

```powershell
cargo build --profile dogfood --locked -p webcodex -p webcodex-cli -p webcodex-runner --bin webpi --bin webpi-server --bin webpi-runner
```

产物均位于 `E:\WebPi\webpi-core\target\dogfood`。三个程序均自报版本 `0.4.1`、commit `3cb489dce855`、`dirty=true`、`built_at=1789790089`。这里忠实记录程序输出；`built_at` 不作为本轮实际执行时间的证明，SHA-256 用于辨认本轮验收的二进制字节。

| 文件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `webpi.exe` | 13050368 | `70ae4dcf910d321fc6ca5e89a3f88e44dd62d34e22c51a3b58e312a87f4832ee` |
| `webpi-server.exe` | 68356096 | `32285dcc0a397efce995080a539f19ee9f434834b2734bb52506ab9dc1cadb8c` |
| `webpi-runner.exe` | 19931136 | `bbeb18be6d0789ba8de316a3f318c15de25d52766e1d16014a67be307aae82e2` |

保留了 Windows 下的 unused/dead-code 和 linker-message 警告，没有将警告伪称为编译错误，也没有为了清理警告改动无关运行逻辑。此构建不是无警告的正式发布。

## 部署边界与下一步

本轮未执行实际安装的配置迁移，未启动/停止/重载生产服务，未改动 Cloudflare Tunnel，未复制/轮换生产凭据，未提交、推送、打标签或发布。已有未提交改动保留；内部 Cargo 名称、`webcodex-plugin-v1`、`wc_*` 凭据/会话标识保留兼容，不是又运行了另一套 WebCodex。

操作员部署时遵循 [WebPi 身份与配置迁移契约](WEBPI_IDENTITY.md)：核对旧 WebPi 的路径并停止旧实例、保持公网转发关闭，先 `webpi.cmd migrate-config --check`，再执行明确迁移和配置，然后启动新版并运行本机正负向认证验证。只有本机通过后，恢复自己的 cloudflared 并进行公网与真实 Action PAT 验收。不要按同名进程批量结束 WebCodex Desktop，也不要复制其状态。

Desktop 打包被明确禁用；本轮没有把解除这一限制当成“修复构建”。Linux/Docker/macOS、完整 workspace 全量测试、生产 PAT 与公网端到端仍不在本轮通过结论中。
