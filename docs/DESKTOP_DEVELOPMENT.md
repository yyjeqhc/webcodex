# Desktop development and local packaging

[English](DESKTOP_DEVELOPMENT.md) | [简体中文](DESKTOP_DEVELOPMENT.zh-CN.md)

This guide is for contributors who want to modify WebCodex Desktop, run it from
source, or build an installable Windows/macOS package for local testing.

For ordinary installation, use [Desktop installation](desktop-install.md). For
day-to-day product use, see [Using Desktop](desktop-guide.md). Formal public
release publication is a separate maintainer workflow described by the
[release checklist](RELEASE_CHECKLIST.md).

## How Desktop is built

WebCodex Desktop is a Tauri 2 application, but the distributable is not only the
Tauri UI shell. A complete Desktop bundle contains three source-matched native
WebCodex executables:

```text
webcodex
webcodex-server
webcodex-runner
```

On Windows they are `.exe` files. On macOS they are native Mach-O executables.

There are therefore two different development paths:

| Goal | Runtime source | Packaging |
| --- | --- | --- |
| Run Desktop from source | `target/dogfood` by default, or `WEBCODEX_DESKTOP_BIN_DIR` | No installer required |
| Build an installable package | Runtime is first verified and staged into Tauri resources | Windows NSIS or macOS DMG |

A plain `tauri build` that skips the WebCodex runtime staging step can produce a
Tauri bundle that compiles but is not a valid WebCodex Desktop distribution.

## Prerequisites

All platforms need:

- Git;
- the stable Rust toolchain used by the repository;
- Node.js 22 and npm;
- the repository checkout;
- enough disk space for the root Rust build and the separate Desktop Tauri build.

Install Desktop JavaScript dependencies once per checkout/update:

```bash
npm ci --prefix apps/desktop
```

Windows development also needs a native MSVC/Windows SDK build environment and a
working WebView2 runtime. Build the x64 package on an x64 Windows host and the
ARM64 package on a native Windows ARM64 host.

macOS development needs Xcode Command Line Tools. Build `darwin-arm64` on Apple
Silicon and `darwin-x64` on an Intel Mac. The staging helper deliberately rejects
a host/binary architecture mismatch.

## Fast checks before opening the app

Frontend:

```bash
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop test
npm --prefix apps/desktop run build
```

Tauri/Rust:

```bash
cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
```

On Windows, process-tree/lifecycle work may also need the ignored real-process
suite documented in [Testing](TESTING.md).

## Build the local WebCodex runtime

Desktop debug builds look for the three binaries in the repository's
`target/dogfood` directory. Build all three from the same checkout.

PowerShell:

```powershell
$source = (git rev-parse HEAD).Trim()
$short = (git rev-parse --short=12 HEAD).Trim()
$version = (node -e "console.log(require('./npm/webcodex/package.json').version)").Trim()
$builtAt = [Int64]((git show -s --format=%ct HEAD).Trim())

$env:WEBCODEX_BUILT_AT = "$builtAt"
cargo build --locked --profile dogfood -p webcodex -p webcodex-cli -p webcodex-runner
```

macOS/Linux-style shell:

```bash
source_sha="$(git rev-parse HEAD)"
short_source="$(git rev-parse --short=12 HEAD)"
version="$(node -p "require('./npm/webcodex/package.json').version")"
built_at="$(git show -s --format=%ct HEAD)"

export WEBCODEX_BUILT_AT="$built_at"
cargo build --locked --profile dogfood -p webcodex -p webcodex-cli -p webcodex-runner
```

The Desktop runtime resolver requires the CLI, Server, and Runner to report the
same version and Git commit.

For an unusual local layout, set `WEBCODEX_DESKTOP_BIN_DIR` to another directory
containing all three source-matched binaries. This override is a debug/development
path. Installed non-debug Desktop builds require their bundled runtime resources.

## Run Desktop from source

After building the dogfood runtime:

```bash
npm --prefix apps/desktop run tauri -- dev
```

Tauri starts Vite through the configured `beforeDevCommand`. The debug Desktop
then resolves the runtime in this order:

1. bundled runtime resources, when present;
2. `WEBCODEX_DESKTOP_BIN_DIR`;
3. the repository `target/dogfood` directory.

For frontend-only iteration, `npm --prefix apps/desktop run dev` starts Vite, but
that is not a complete Desktop runtime and does not replace a Tauri run when the
change depends on native commands, process lifecycle, tray/menu behavior, file
dialogs, autostart, or bundled runtime behavior.

## Build a Windows installer locally

The current Windows distribution format is a per-user NSIS installer. The
official project build is unsigned.

### 1. Use a clean committed source state

The Desktop staging helper verifies exact build provenance and expects
`dirty=false` in every embedded runtime binary.

```powershell
git status --short
git rev-parse HEAD
```

Commit the source you intend to package before continuing. Generated/ignored
build output does not need to be committed.

### 2. Build the exact dogfood runtime

Run the PowerShell runtime-build block above from the repository root. Keep
`$source`, `$version`, and `$builtAt` in the same shell.

### 3. Stage runtime resources

Use a fresh output path; the staging helper refuses to overwrite an existing
directory.

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

The helper verifies all three executables, copies them byte-for-byte into a
generated `webcodex-runtime` resource tree, and creates the Tauri config overlay.

### 4. Build NSIS

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

The installer is under:

```text
target\desktop-local-tauri-<pid>\release\bundle\nsis\
```

Use the native host that matches the package:

| Host | Package value used by smoke/release evidence |
| --- | --- |
| Windows x64 | `win32-x64` |
| Windows ARM64 | `win32-arm64` |

### 5. Smoke the installer

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

The smoke script exercises the native installer and validates the bundled runtime
identity. It deliberately refuses to run when WebCodex Desktop is already installed
for the current Windows user, and it uninstalls the temporary package after the
check. Use a disposable VM/test user or another host without an existing Desktop
installation rather than disturbing a daily-use installation.

## Build a macOS DMG locally

The current public macOS distribution is ad-hoc signed and not notarized.

For the normal local path, the repository already provides a complete helper:

```bash
bash scripts/build_desktop_macos_local.sh
```

It requires a clean worktree, installs Desktop npm dependencies, builds the dogfood runtime, stages it, creates the native ad-hoc signed DMG, runs the macOS smoke validation, and writes the final file under `target/desktop-local-dist/`. The steps below are the equivalent manual flow for understanding or diagnosing a build.

### 1. Use a clean committed source state

```bash
git status --short
git rev-parse HEAD
```

Commit the source you intend to package. Then run the macOS runtime-build block
above and keep `source_sha`, `version`, and `built_at` in the same shell.

### 2. Select the native platform

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

The helper verifies source identity, SHA-256, and Mach-O architecture before
creating the Tauri resource overlay.

### 4. Build the DMG

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

Find and require the single generated DMG:

```bash
set -- "$CARGO_TARGET_DIR"/release/bundle/dmg/*.dmg
if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
  echo "Expected exactly one DMG" >&2
  exit 1
fi
dmg="$1"
```

### 5. Smoke signing, architecture, and runtime identity

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

Because these builds are ad-hoc signed and not notarized, Gatekeeper may block a
newly downloaded/copied local DMG on first launch. Use **System Settings → Privacy
& Security → Open Anyway** when appropriate. Do not disable Gatekeeper globally.

## Local package versus an official release

A successful local EXE/DMG is development evidence, not publication authority.

For normal contribution work you do **not** need to:

- create or push a Git tag;
- create a GitHub Release;
- publish npm;
- run the full six-platform release matrix;
- use release-operator publication commands.

Official WebCodex release candidates are built from an immutable tagged source by
the reviewed native workflow. That flow packages:

- Windows x64 Desktop;
- Windows ARM64 Desktop;
- macOS Intel Desktop;
- macOS Apple Silicon Desktop;
- the six standalone runtime platforms.

See [.github/workflows/release-build.yml](../.github/workflows/release-build.yml)
and [Release Checklist](RELEASE_CHECKLIST.md) for the formal release contract.
Do not upload a local dogfood installer as an official WebCodex release artifact.

## Common failures

### `bundled_runtime_missing` / `binary_missing`

For source development, rebuild all three root binaries:

```bash
cargo build --locked --profile dogfood -p webcodex -p webcodex-cli -p webcodex-runner
```

Or point `WEBCODEX_DESKTOP_BIN_DIR` at the directory containing the matching
three binaries.

For an installed non-debug Desktop, reinstall a matching packaged Desktop build;
do not manually copy random binaries into application resources.

### `binary_version_mismatch`

The CLI, Server, and Runner came from different builds. Rebuild all three from
one checkout and do not mix files from separate `target` directories.

### Staging reports an unexpected identity or `dirty=true`

The packaging helpers intentionally require reproducible committed input. Commit
the source, set `WEBCODEX_BUILT_AT` from `git show -s --format=%ct HEAD`, and
rebuild all three runtime binaries before staging again.

### macOS architecture mismatch

`prepare_desktop_bundle_macos.py` is native-only. Do not stage `darwin-x64` on an
Apple Silicon host or `darwin-arm64` on Intel. Use the matching native Mac, as the
project's CI/release matrix does.

### Windows installer builds but Desktop cannot find runtime

Rebuild through `prepare_desktop_bundle.ps1` and pass its generated
`tauri.bundle.conf.json` to Tauri. A raw NSIS build without this overlay is not a
complete WebCodex Desktop package.

### UI works in Vite but fails in Desktop

Reproduce with the full Tauri development command. Browser/Vite-only preview does
not provide Tauri IPC, native process lifecycle, tray/menu APIs, autostart, or
bundled runtime resources.

## Authoritative automation

When this guide and implementation ever disagree, treat the current scripts and
native CI as the mechanical source of truth and update the guide in the same
change:

- [ordinary CI](../.github/workflows/ci.yml);
- [extended native validation](../.github/workflows/extended-native.yml);
- [release candidate build](../.github/workflows/release-build.yml);
- `scripts/prepare_desktop_bundle.ps1`;
- `scripts/build_desktop_macos_local.sh`;
- `scripts/prepare_desktop_bundle_macos.py`;
- `scripts/desktop_install_windows_smoke.ps1`;
- `scripts/desktop_install_macos_smoke.sh`.
