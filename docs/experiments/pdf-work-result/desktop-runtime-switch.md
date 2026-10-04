# Desktop Runtime 切换诊断与准备状态

记录日期：2026-10-05。PDF 实现和候选 Server 来自干净提交
`45641ab3b87d1c3b519172035009c678b3fca169`。

## 观察到的失败

用户在 Desktop 选择候选 Runtime 后，连续两次得到
`runtime_switch_jobs_confirmation_required`，没有出现可继续的重启确认窗口。
原 `127.0.0.1:18080` Server 仍在运行，版本为 0.4.4，构建身份为
`1546b557ee7c` / `git_dirty=true`。安装的 Desktop 文件版本为 0.4.4。

通过 Desktop 使用的授权 `POST /api/runtime-console/runner` 接口，针对保存的
Runner 身份取得 HTTP 200、`connected=true`、`jobs_running=0`、`jobs_queued=0`。
现有 Workstation 插件的定向 runtime_status 同样报告 Runner 在线、运行和排队
均为 0。检查没有输出凭据、完整配置或项目数据。

## 源码解释

历史提交 `1546b557ee7c` 的 `state/runtime_shell.rs` 存在接口不一致：

- `runtime_settings` 读取 `RunnerDetails`，能显示单个 Runner 的任务数。
- `switch_runtime_transaction` 读取 `Overview`，却按单个 Runner 的字段解析任务
  数。总览无法证明这些字段，解析得到未知值，因此要求 `confirm_interrupt`。
- `start_existing_runtime` 的切换后身份校验也错误读取 `Overview`。

同一版本的 RuntimePanel 在任务数为 0 时直接提交；收到该错误后只显示错误卡，
不会自动打开确认窗口。这解释了重复重试仍失败的路径。由于旧运行程序来自
dirty 构建，这里是依据历史源码和现场行为的诊断，未声称逐字节恢复了旧构建源码。

PDF 分支已包含上游修复 `ecd81438`（#827），三个观察入口统一读取保存身份的
单个 Runner。现有任务确认、候选程序指纹、选择版本和进程所有权检查均保留。

## 已准备的 Desktop 程序

构建源仍为上述干净 `45641ab3`，没有修改 Desktop 源码或发布版本号。
使用本机已有、与锁文件完全一致的 190 个依赖包；构建未下载新包。

```powershell
npm --prefix apps/desktop run build
$env:CARGO_TARGET_DIR='G:\Dev\webcodex-fix\target\pdf-preview-desktop'
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --locked --offline --profile dogfood --features tauri/custom-protocol
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked --offline --profile dogfood --features tauri/custom-protocol --lib state::runtime_shell::tests
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked --offline --profile dogfood --features tauri/custom-protocol --lib runtime_selection::lifecycle::tests
```

前端构建和原生 dogfood 构建通过；Runner 定向观察测试 6 项、切换与回滚事务
测试 6 项全部通过。
原生编译仍有既有 unused/dead_code 警告，没有为构建修改无关代码。
直接 Cargo 构建显式启用 `tauri/custom-protocol`，嵌入生产前端。

候选：`G:\Dev\webcodex-fix\target\pdf-preview-desktop\dogfood\webcodex-desktop.exe`，
文件版本 0.5.0，21,191,168 字节，SHA-256：
`BB36FBF530082019AB660E0100D3F91698655F487D3E25A85748645B10490A97`。

待替换目标：`G:\WebCodex Desktop\WebCodex.exe`。该应用程序的替换与重启仍待
单独授权；此前仅获准更新命名的本地 Server 实例。不会绕过 Desktop 的任务
确认或使用独立进程接管该实例。

## 回滚与后续测试

受限访问的备份目录：
`G:\Dev\webcodex-fix\target\pdf-preview\deployment-backup-20261005-45641ab3`。
原 Desktop、三个 Runtime 程序、Desktop 配置、环境文件和一致性校验通过的数据库
备份已保留。备份中的凭据和数据库不进入 Git。

原 Desktop 23,256,064 字节，SHA-256：
`C9E4D14272ACFC4259D6F6A594006B723E75CC323C4A1AE724371E12716AA62C`。

测试项目已准备在 `target/pdf-preview/live-pdf-project`，包含 text、cjk、scan、
ratio 四个公开合成 PDF。四个文件均已验证暂存 Git blob 与原始文件字节一致，
ratio 的页面尺寸为 630×496。项目尚未注册到现有 Runner。

下一步是正常退出 Desktop、替换已备份的应用程序、启动 Desktop 并通过标准
Runtime 页面切换候选 Server；随后验证原实例构建身份、Runner、Secure Tunnel，
刷新原插件并在真实 ChatGPT 侧栏读取上述合成文件、测试和截图。当前没有推送、
PR、发布、应用程序替换或服务切换。
