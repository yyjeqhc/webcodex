# Build an installable Windows WebCodex Desktop NSIS package for local dogfood.
#
# This mirrors the native CI flow without creating a release: install JS
# dependencies, build the exact dogfood runtime, stage it into Tauri resources,
# build an unsigned current-user NSIS installer, and optionally run the native
# install/uninstall smoke.
[CmdletBinding()]
param(
    [switch]$Smoke,
    [switch]$AllowDirty
)

$ErrorActionPreference = "Stop"

function Fail([string]$Message) {
    throw "Windows Desktop local build failed: $Message"
}

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Fail "required command not found: $Name"
    }
}

$Root = Split-Path -Parent $PSScriptRoot
$TauriTarget = Join-Path $Root "target\desktop-local-tauri"
$OutputDir = Join-Path $Root "target\desktop-local-dist"
$StageDir = Join-Path $Root ("target\desktop-local-stage." + [guid]::NewGuid().ToString("N"))
$PreviousLocation = Get-Location
$PreviousBuiltAt = $env:WEBCODEX_BUILT_AT
$PreviousCargoTarget = $env:CARGO_TARGET_DIR

try {
    foreach ($command in @("git", "node", "npm", "cargo")) {
        Require-Command $command
    }

    Set-Location $Root

    $nodeVersion = [string](& node --version)
    if ($LASTEXITCODE -ne 0 -or $nodeVersion -notmatch '^v22(?:\.|$)') {
        Fail "Node.js 22 is required; got '$($nodeVersion.Trim())'"
    }

    $status = @(& git status --porcelain --untracked-files=all)
    if ($LASTEXITCODE -ne 0) {
        Fail "git status failed"
    }
    $GitDirty = $status.Count -ne 0
    if ($GitDirty -and -not $AllowDirty) {
        Fail "worktree has uncommitted changes; commit/stash them or rerun with -AllowDirty for local dogfood"
    }

    $SourceSha = ([string](& git rev-parse HEAD)).Trim()
    if ($LASTEXITCODE -ne 0 -or $SourceSha -notmatch '^[0-9A-Fa-f]{40}$') {
        Fail "could not resolve the exact Git HEAD"
    }
    $ShortSource = ([string](& git rev-parse --short=12 HEAD)).Trim().ToLowerInvariant()
    if ($LASTEXITCODE -ne 0 -or -not $ShortSource) {
        Fail "could not resolve the short Git HEAD"
    }
    $Version = ([string](& node -p "require('./npm/webcodex/package.json').version")).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $Version) {
        Fail "could not resolve the package version"
    }
    $BuiltAtText = ([string](& git show -s --format=%ct HEAD)).Trim()
    if ($LASTEXITCODE -ne 0 -or $BuiltAtText -notmatch '^[0-9]+$') {
        Fail "could not resolve the source commit timestamp"
    }
    $BuiltAt = [Int64]$BuiltAtText
    $env:WEBCODEX_BUILT_AT = "$BuiltAt"

    $nativeArch = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }
    $Platform = switch ($nativeArch.ToUpperInvariant()) {
        "AMD64" { "win32-x64" }
        "ARM64" { "win32-arm64" }
        default { Fail "unsupported Windows architecture: $nativeArch" }
    }

    Write-Host "Building WebCodex Desktop local installer"
    Write-Host "  source:   $SourceSha"
    Write-Host "  version:  $Version"
    Write-Host "  platform: $Platform"
    Write-Host "  dirty:    $GitDirty"
    Write-Host "  smoke:    $Smoke"

    & npm ci --prefix frontend
    if ($LASTEXITCODE -ne 0) { Fail "frontend npm ci failed" }
    & npm ci --prefix apps/desktop
    if ($LASTEXITCODE -ne 0) { Fail "Desktop npm ci failed" }

    & cargo build --locked --profile dogfood -p webcodex -p webcodex-cli -p webcodex-runner
    if ($LASTEXITCODE -ne 0) { Fail "WebCodex dogfood runtime build failed" }

    & .\scripts\prepare_desktop_bundle.ps1 `
        -BinDir "target\dogfood" `
        -Version $Version `
        -SourceSha $SourceSha `
        -BuiltAt $BuiltAt `
        -OutputDir $StageDir `
        -GitDirty $GitDirty

    $Config = Join-Path $StageDir "tauri.bundle.conf.json"
    if (-not (Test-Path -LiteralPath $Config -PathType Leaf)) {
        Fail "Desktop staging did not produce the Tauri config overlay"
    }

    New-Item -ItemType Directory -Force -Path $TauriTarget, $OutputDir | Out-Null
    $BundleDir = Join-Path $TauriTarget "release\bundle\nsis"
    if (Test-Path -LiteralPath $BundleDir) {
        Remove-Item -LiteralPath $BundleDir -Recurse -Force
    }

    $env:CARGO_TARGET_DIR = $TauriTarget
    Push-Location (Join-Path $Root "apps\desktop")
    try {
        & node node_modules/@tauri-apps/cli/tauri.js build --bundles nsis --config $Config --ci --no-sign -- --locked
        if ($LASTEXITCODE -ne 0) { Fail "Tauri NSIS build failed" }
    } finally {
        Pop-Location
    }

    $Installers = @(Get-ChildItem -LiteralPath $BundleDir -Filter "*-setup.exe" -File)
    if ($Installers.Count -ne 1) {
        Fail "expected exactly one NSIS installer, found $($Installers.Count) in $BundleDir"
    }

    $SourceLabel = if ($GitDirty) { "dirty-$ShortSource" } else { $ShortSource }
    $OutputInstaller = Join-Path $OutputDir "webcodex-desktop-local-$SourceLabel-v$Version-$Platform-setup.exe"
    Copy-Item -LiteralPath $Installers[0].FullName -Destination $OutputInstaller -Force

    $Hash = (Get-FileHash -LiteralPath $OutputInstaller -Algorithm SHA256).Hash.ToLowerInvariant()
    $ChecksumPath = "$OutputInstaller.sha256"
    [System.IO.File]::WriteAllText(
        $ChecksumPath,
        "$Hash  $(Split-Path -Leaf $OutputInstaller)`n",
        [System.Text.Encoding]::ASCII
    )

    if ($Smoke) {
        & .\scripts\desktop_install_windows_smoke.ps1 `
            -Installer $OutputInstaller `
            -Version $Version `
            -SourceSha $SourceSha `
            -BuiltAt $BuiltAt `
            -Platform $Platform `
            -GitDirty $GitDirty
    }

    Write-Host ""
    Write-Host "Local Desktop installer ready:"
    Write-Host "  $OutputInstaller"
    Write-Host "SHA-256: $Hash"
    if ($GitDirty) {
        Write-Warning "This installer was built from an uncommitted worktree and is intentionally marked dirty; do not publish it as a release artifact."
    }
    if (-not $Smoke) {
        Write-Host "Native install/uninstall smoke was skipped. Re-run with -Smoke on a test user/host with no existing WebCodex Desktop installation."
    }
} finally {
    if (Test-Path -LiteralPath $StageDir) {
        Remove-Item -LiteralPath $StageDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    $env:WEBCODEX_BUILT_AT = $PreviousBuiltAt
    $env:CARGO_TARGET_DIR = $PreviousCargoTarget
    Set-Location $PreviousLocation
}
