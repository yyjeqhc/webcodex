# Desktop 开发与本地打包

[English](DESKTOP_DEVELOPMENT.md) | [简体中文](DESKTOP_DEVELOPMENT.zh-CN.md)

本文面向希望修改 WebCodex Desktop、从源码运行，或者自己构建 Windows/macOS
可安装包进行测试的贡献者。

普通安装请看 [Desktop 安装与连接](desktop-install.zh-CN.md)，日常使用请看
[Desktop 使用指南](desktop-guide.zh-CN.md)。正式公开发布是另一套 maintainer 流程，
见[发布检查清单](RELEASE_CHECKLIST.md)。

## Desktop 实际由什么组成

WebCodex Desktop 是 Tauri 2 应用，但可分发包不只是 Tauri UI 壳。完整 Desktop 包
还必须包含三个来自**同一个源码基线**的 native WebCodex executable：

```text
webcodex
webcodex-server
webcodex-runner
```

Windows 中它们是 `.exe`；macOS 中是对应架构的 Mach-O executable。

因此 Desktop 开发有两条不同路径：

| 目标 | Runtime 来源 | 打包 |
| --- | --- | --- |
| 从源码运行 Desktop | 默认使用 `target/dogfood`，也可用 `WEBCODEX_DESKTOP_BIN_DIR` | 不需要 installer |
| 构建可安装包 | 先验证并 stage 三个 runtime 到 Tauri resources | Windows NSIS / macOS DMG |

如果跳过 WebCodex runtime staging，直接执行 `tauri build`，可能得到“能编译”的
Tauri 包，但它不是完整可运行的 WebCodex Desktop 分发包。

## 开发环境

所有平台需要：

- Git；
- 仓库当前使用的 stable Rust toolchain；
- Node.js 22 与 npm；
- WebCodex 源码 checkout；
- 足够的磁盘空间保存 root Rust build 与独立的 Desktop Tauri build。

每次新 checkout 或 Desktop dependency 更新后先安装：

```bash
npm ci --prefix apps/desktop
```

Windows 还需要原生 MSVC / Windows SDK 构建环境以及可用的 WebView2 runtime。
Windows x64 安装包在 x64 Windows 上 native 构建；Windows ARM64 安装包在 native
Windows ARM64 主机上构建。

macOS 需要 Xcode Command Line Tools。`darwin-arm64` 在 Apple Silicon 上构建，
`darwin-x64` 在 Intel Mac 上构建。staging helper 会主动拒绝 host/binary architecture
不匹配。

## 打开应用前的快速检查

Frontend：

```bash
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop test
npm --prefix apps/desktop run build
```

Tauri/Rust：

```bash
cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
```

修改 Windows process tree / lifecycle 时，还可能需要运行 [Testing](TESTING.md)
列出的 ignored real-process Desktop suite。

## 构建本地 WebCodex runtime

Desktop debug build 默认会到当前仓库的 `target/dogfood` 找三个 binary。因此要从同一个
checkout 一次性构建 CLI、Server 和 Runner。

PowerShell：

```powershell
$source = (git rev-parse HEAD).Trim()
$short = (git rev-parse --short=12 HEAD).Trim()
$version = (node -e "console.log(require('./npm/webcodex/package.json').version)").Trim()
$builtAt = [Int64]((git show -s --format=%ct HEAD).Trim())

$env:WEBCODEX_BUILT_AT = "$builtAt"
cargo build --locked --profile dogfood -p webcodex -p webcodex-cli -p webcodex-runner
```

macOS/Linux 风格 shell：

```bash
source_sha="$(git rev-parse HEAD)"
short_source="$(git rev-parse --short=12 HEAD)"
version="$(node -p "require('./npm/webcodex/package.json').version")"
built_at="$(git show -s --format=%ct HEAD)"

export WEBCODEX_BUILT_AT="$built_at"
cargo build --locked --profile dogfood -p webcodex -p webcodex-cli -p webcodex-runner
```

Desktop 会验证三个 binary 报告的 version 与 Git commit 一致。

如果本地目录布局不同，可以把 `WEBCODEX_DESKTOP_BIN_DIR` 指向另一个同时包含三个
同源 binary 的目录。这个 override 只属于 debug/development 路径；正式安装的
non-debug Desktop 必须使用包内 runtime resources。

## 从源码运行 Desktop

dogfood runtime 构建好以后：

```bash
npm --prefix apps/desktop run tauri -- dev
```

Tauri 会按配置自动启动 Vite。debug Desktop 的 runtime 解析顺序是：

1. bundle 中已有的 runtime resources；
2. `WEBCODEX_DESKTOP_BIN_DIR`；
3. 当前源码仓库 `target/dogfood`。

只改 UI 时可以执行 `npm --prefix apps/desktop run dev` 启动 Vite，但它并不是完整
Desktop runtime。只要修改涉及 Tauri IPC、native command、process lifecycle、tray/menu、
文件选择器、autostart 或 bundle runtime，就必须用完整 Tauri dev 验证。

## 在 Windows 本地构建 installer

当前 Windows 分发格式是 current-user NSIS installer，项目正式构建目前也是 unsigned。

### 1. 使用干净、已提交的源码

Desktop staging helper 会验证精确 build provenance，并要求三个 embedded runtime
都报告 `dirty=false`。

```powershell
git status --short
git rev-parse HEAD
```

先把真正想打包的代码提交成一个 commit。被 ignore 的 build output 不需要提交。

### 2. 构建精确 dogfood runtime

从仓库根目录执行前面的 PowerShell runtime build，并保留同一个 shell 中的
`$source`、`$version` 与 `$builtAt`。

### 3. Stage runtime resources

必须使用一个不存在的新输出目录；helper 不会覆盖已有目录。

```powershell
$stage = Join-Path $env:TEMP "webcodex-desktop-bundle-$PID-$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())"

& .\scripts\prepare_desktop_bundle.ps1 `
  -BinDir "target\dogfood" `
  -Version $version `
  -SourceSha $source `
  -BuiltAt $builtAt `
  -OutputDir $stage

$config = Join-Path $stage "tauri.bundle.conf.json"
```

helper 会验证三个 executable，把它们 byte-for-byte 复制到生成的
`webcodex-runtime` resource tree，并输出 Tauri config overlay。

### 4. 构建 NSIS

```powershell
$targetDir = Join-Path $PWD "target\desktop-local-tauri-$PID"
$env:CARGO_TARGET_DIR = $targetDir

Push-Location apps\desktop
try {
  npm exec tauri -- build --bundles nsis --config $config --ci --no-sign -- --locked
  if ($LASTEXITCODE -ne 0) { throw "Tauri NSIS build failed" }
} finally {
  Pop-Location
}
```

installer 位于：

```text
target\desktop-local-tauri-<pid>\release\bundle\nsis\
```

native host 与 smoke/release platform 对应关系：

| 主机 | platform |
| --- | --- |
| Windows x64 | `win32-x64` |
| Windows ARM64 | `win32-arm64` |

### 5. Smoke installer

```powershell
$bundleDir = Join-Path $targetDir "release\bundle\nsis"
$installers = @(Get-ChildItem -LiteralPath $bundleDir -Filter "*-setup.exe" -File)
if ($installers.Count -ne 1) {
  throw "Expected exactly one NSIS installer, found $($installers.Count)"
}

$platform = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
  "win32-arm64"
} elseif ($env:PROCESSOR_ARCHITECTURE -eq "AMD64") {
  "win32-x64"
} else {
  throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE"
}

& .\scripts\desktop_install_windows_smoke.ps1 `
  -Installer $installers[0].FullName `
  -Version $version `
  -SourceSha $source `
  -BuiltAt $builtAt `
  -Platform $platform
```

smoke 会真正走 native installer，并验证包内 runtime identity。Windows 脚本如果发现
当前用户已经安装 WebCodex Desktop 会直接拒绝执行，测试完成后也会卸载这次临时安装。
建议在 disposable VM / 测试用户或没有现有 Desktop 安装的主机上运行，不要影响日常使用环境。

## 在 macOS 本地构建 DMG

当前公开 macOS 分发包使用 ad-hoc signing，不做 notarization。

普通本地打包优先使用仓库已经提供的一条完整 helper：

```bash
bash scripts/build_desktop_macos_local.sh
```

它要求 clean worktree，会自动安装 Desktop npm dependency、构建 dogfood runtime、完成 staging、生成 native ad-hoc signed DMG、运行 macOS smoke，并把最终文件放到 `target/desktop-local-dist/`。下面继续保留等价手工流程，方便理解或排查某一个阶段。

### 1. 使用干净、已提交的源码

```bash
git status --short
git rev-parse HEAD
```

先提交希望打包的源码，然后执行前面的 macOS runtime build，并保留
`source_sha`、`version` 与 `built_at`。

### 2. 选择 native platform

```bash
case "$(uname -m)" in
  arm64)  platform="darwin-arm64" ;;
  x86_64) platform="darwin-x64" ;;
  *) echo "Unsupported macOS architecture: $(uname -m)" >&2; exit 1 ;;
esac
```

### 3. Stage runtime resources

```bash
stage="${TMPDIR:-/tmp}/webcodex-desktop-bundle-$$-$(date +%s)"
test ! -e "$stage"

python3 scripts/prepare_desktop_bundle_macos.py \
  --bin-dir target/dogfood \
  --version "$version" \
  --source-sha "$source_sha" \
  --built-at "$built_at" \
  --platform "$platform" \
  --signing-mode adhoc \
  --output-dir "$stage"

config="$stage/tauri.bundle.conf.json"
metadata="$stage/desktop-bundle.json"
```

helper 在生成 Tauri resource overlay 前会检查 source identity、SHA-256 与 Mach-O
architecture。

### 4. 构建 DMG

```bash
export APPLE_SIGNING_IDENTITY="-"
export CARGO_TARGET_DIR="$PWD/target/desktop-local-tauri-$$"

(
  cd apps/desktop
  npm exec tauri -- build \
    --bundles dmg \
    --config "$config" \
    --ci -- --locked
)
```

要求只产生一个 DMG：

```bash
set -- "$CARGO_TARGET_DIR"/release/bundle/dmg/*.dmg
if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
  echo "Expected exactly one DMG" >&2
  exit 1
fi
dmg="$1"
```

### 5. Smoke 签名、架构和 runtime identity

```bash
bash scripts/desktop_install_macos_smoke.sh \
  --dmg "$dmg" \
  --version "$version" \
  --source-sha "$source_sha" \
  --built-at "$built_at" \
  --platform "$platform" \
  --stage-metadata "$metadata" \
  --signing-mode adhoc
```

因为当前是 ad-hoc signed 且没有 notarize，新下载/复制的本地 DMG 第一次启动时可能被
Gatekeeper 阻止。需要时使用 **系统设置 → 隐私与安全性 → 仍要打开（Open Anyway）**；
不要全局关闭 Gatekeeper。

## 本地开发包和正式 Release 的边界

本地 EXE/DMG 构建成功只代表 development evidence，不等于获得正式发布权限。

普通贡献开发**不需要**：

- 创建或 push Git tag；
- 创建 GitHub Release；
- `npm publish`；
- 跑完整六平台 release matrix；
- 执行 release operator 的发布命令。

正式 WebCodex release candidate 会从 immutable tag 通过经过评审的 native workflow
生成，其中 Desktop 包含：

- Windows x64；
- Windows ARM64；
- macOS Intel；
- macOS Apple Silicon；
- 以及六个平台的独立 runtime archive。

正式发布 contract 见
[`.github/workflows/release-build.yml`](../.github/workflows/release-build.yml)
与[发布检查清单](RELEASE_CHECKLIST.md)。不要把本地 dogfood installer 上传成 WebCodex
正式 Release artifact。

## 常见问题

### `bundled_runtime_missing` / `binary_missing`

源码开发时重新构建三个 root binary：

```bash
cargo build --locked --profile dogfood -p webcodex -p webcodex-cli -p webcodex-runner
```

或者让 `WEBCODEX_DESKTOP_BIN_DIR` 指向同时包含三个匹配 binary 的目录。

已经安装的 non-debug Desktop 则应重新安装匹配版本的 Desktop 包，不要手工把来源不明的
binary 塞进应用 resources。

### `binary_version_mismatch`

CLI、Server、Runner 不是同一次源码构建。必须从同一个 checkout 一起重建三个 binary，
不要混用不同 `target` 目录中的文件。

### staging 报 unexpected identity 或 `dirty=true`

打包 helper 刻意只接受可复现的 committed input。先提交源码，把
`WEBCODEX_BUILT_AT` 设置为 `git show -s --format=%ct HEAD`，然后重新构建三个 runtime
再 stage。

### macOS architecture mismatch

`prepare_desktop_bundle_macos.py` 是 native-only。不要在 Apple Silicon 上 stage
`darwin-x64`，也不要在 Intel 上 stage `darwin-arm64`；按项目 CI/release matrix 使用
对应 native Mac。

### Windows installer 能构建，但 Desktop 找不到 runtime

重新使用 `prepare_desktop_bundle.ps1`，并把它生成的
`tauri.bundle.conf.json` 传给 Tauri。跳过 overlay 的 raw NSIS build 不是完整 WebCodex
Desktop package。

### Vite 里 UI 正常，但 Desktop 中失败

使用完整 Tauri dev 重现。浏览器/Vite-only preview 没有 Tauri IPC、native process
lifecycle、tray/menu、autostart 或 bundled runtime resources。

## 权威自动化

如果本文未来与实现不一致，以当前脚本和 native CI 的机械行为为准，并在同一改动中
同步修正文档：

- [ordinary CI](../.github/workflows/ci.yml)；
- [extended native validation](../.github/workflows/extended-native.yml)；
- [release candidate build](../.github/workflows/release-build.yml)；
- `scripts/prepare_desktop_bundle.ps1`；
- `scripts/build_desktop_macos_local.sh`；
- `scripts/prepare_desktop_bundle_macos.py`；
- `scripts/desktop_install_windows_smoke.ps1`；
- `scripts/desktop_install_macos_smoke.sh`。
