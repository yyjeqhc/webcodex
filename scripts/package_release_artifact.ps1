# Windows-native release artifact packaging for WebCodex.
#
# Produces `webcodex-v<VERSION>-win32-<ARCH>.tar.gz` from three Windows release
# binaries. This is the Windows release path: it must run on the matching native
# Windows host with nothing but PowerShell and the built-in Windows tooling. It never
# requires Git Bash, WSL, or Unix chmod/install/sha256sum.
#
# The archive keeps the current three-binary npm contract:
#
#   webcodex.exe webcodex-server.exe webcodex-runner.exe
#
# webcodex-server.exe is a supported local foreground runtime on Windows.
# WebCodex-managed Windows Server service lifecycle and `webcodex share` remain
# unsupported; packaging this binary does not claim Windows service/Tunnel parity.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\package_release_artifact.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\package_release_artifact.ps1 `
#       -BinDir E:\webcodex\target\release -OutDir E:\webcodex\dist
#
# Parameter defaults honor the same environment variables as the Unix
# packaging script: WEBCODEX_RELEASE_BIN_DIR and WEBCODEX_RELEASE_OUT_DIR.
#
# Release builds must produce one shared build identity across the three
# binaries. webcodex-core's build.rs honors WEBCODEX_BUILT_AT when set, so a
# release build should pin it once (e.g. `$env:WEBCODEX_BUILT_AT = ...` before
# `cargo build --release`) to keep `built_at` identical across the packages;
# see scripts/npm_install_windows_smoke.ps1 for the pattern.
#
# On success prints the SHA-256 and the archive path. On failure exits
# non-zero and never leaves a partial file under the final archive name.
[CmdletBinding()]
param(
    [string]$BinDir,
    [string]$OutDir,
    [string]$Version,
    [string]$Platform,
    [switch]$AllowDevelopmentBuild
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
if (-not $BinDir) {
    $BinDir = if ($env:WEBCODEX_RELEASE_BIN_DIR) {
        $env:WEBCODEX_RELEASE_BIN_DIR
    } else {
        Join-Path $Root "target\release"
    }
}
if (-not $OutDir) {
    $OutDir = if ($env:WEBCODEX_RELEASE_OUT_DIR) {
        $env:WEBCODEX_RELEASE_OUT_DIR
    } else {
        Join-Path $Root "dist"
    }
}

if (-not $Version) {
    $packageJson = Join-Path $Root "npm\webcodex\package.json"
    $Version = (Get-Content -LiteralPath $packageJson -Raw | ConvertFrom-Json).version
    if (-not $Version) {
        throw "cannot read package version from $packageJson"
    }
}
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
    throw "invalid package version '$Version'"
}

if (-not $Platform) {
    $Platform = if ($env:WEBCODEX_RELEASE_PLATFORM) {
        $env:WEBCODEX_RELEASE_PLATFORM
    } else {
        "win32-x64"
    }
}
if ($Platform -notin @("win32-x64", "win32-arm64")) {
    throw "invalid Windows release platform '$Platform'"
}

$BinDir = [System.IO.Path]::GetFullPath($BinDir)
$OutDir = [System.IO.Path]::GetFullPath($OutDir)

$BinaryNames = @("webcodex", "webcodex-server", "webcodex-runner")
$ArchiveName = "webcodex-v$Version-$Platform.tar.gz"
$Archive = Join-Path $OutDir $ArchiveName
$ArchiveTmp = "$Archive.tmp"
if ((Test-Path -LiteralPath $Archive) -and -not $AllowDevelopmentBuild) {
    throw "refusing to overwrite existing release artifact $Archive; remove it explicitly after verifying its provenance"
}

# Windows 10 1803+ / Windows 11 ship tar.exe (libarchive) at
# %SystemRoot%\System32\tar.exe; it is the supported archiver. It is used
# explicitly because Git Bash/MSYS puts its own tar (which mangles Windows
# paths and is not supported) earlier on PATH.
$Tar = Join-Path $env:SystemRoot "System32\tar.exe"
if (-not (Test-Path -LiteralPath $Tar -PathType Leaf)) {
    throw "tar.exe was not found at $Tar. Windows 11 ships tar.exe in System32; restore it (Windows Features). Git Bash tar is not supported for Windows release artifacts."
}

$ReleaseTagCommit = $null
if (-not $AllowDevelopmentBuild) {
    $git = Get-Command git -ErrorAction SilentlyContinue
    if (-not $git) {
        throw "git is required to verify release artifact provenance against tag v$Version"
    }
    $tagCommitOutput = & $git.Source -C $Root rev-list -n 1 "v$Version" 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $tagCommitOutput) {
        throw "release tag v$Version was not found; create the immutable release tag before packaging, or use -AllowDevelopmentBuild for local smoke only"
    }
    $ReleaseTagCommit = @($tagCommitOutput)[0].Trim()
    $headCommitOutput = & $git.Source -C $Root rev-parse HEAD 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $headCommitOutput) {
        throw "failed to resolve the packaging worktree HEAD while verifying v$Version"
    }
    $headCommit = @($headCommitOutput)[0].Trim()
    if (-not $headCommit.Equals($ReleaseTagCommit, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "packaging worktree HEAD $headCommit is not release tag v$Version ($ReleaseTagCommit)"
    }
    $worktreeStatus = @(& $git.Source -C $Root status --porcelain --untracked-files=all)
    if ($LASTEXITCODE -ne 0) {
        throw "failed to inspect packaging worktree cleanliness for v$Version"
    }
    if ($worktreeStatus.Count -ne 0) {
        throw "packaging worktree is not clean; release artifacts require a clean v$Version checkout"
    }
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$Staging = Join-Path $OutDir (".win32-artifact-staging-" + [guid]::NewGuid().ToString("N"))
try {
    New-Item -ItemType Directory -Force -Path (Join-Path $Staging "package") | Out-Null

    # 1. All three release binaries must exist.
    foreach ($name in $BinaryNames) {
        $source = Join-Path $BinDir "$name.exe"
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "missing release binary: $source"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $Staging "package\$name.exe")
    }

    # 2. Every binary must report the package version.
    # 3. Every binary must report the same build revision identity.
    $identities = @()
    foreach ($name in $BinaryNames) {
        $binary = Join-Path $Staging "package\$name.exe"
        $output = & $binary --version
        if ($LASTEXITCODE -ne 0) {
            throw "$name.exe --version failed with exit code $LASTEXITCODE"
        }
        $line = @($output | Select-Object -First 1)
        if (-not $line -or -not $line[0]) {
            throw "$name.exe --version produced no output"
        }
        $line = $line[0].TrimEnd()
        $expectedPrefix = "$name $Version"
        if ($line -ne $expectedPrefix -and -not $line.StartsWith("$expectedPrefix ")) {
            throw "unexpected $name version output: '$line' (expected '$expectedPrefix ...')"
        }
        $identities += $line.Substring($name.Length).TrimStart()
    }
    if (@($identities | Select-Object -Unique).Count -ne 1) {
        throw "release binaries do not share one build identity: $($identities -join ' | ')"
    }

    # Release packaging is provenance-sensitive by default. A development smoke
    # can opt out explicitly, but a release artifact must identify one clean
    # commit and that commit must be the immutable v<VERSION> tag.
    $identity = @($identities)[0]
    if ($AllowDevelopmentBuild) {
        Write-Warning "development artifact mode: release provenance checks are disabled; do not publish this archive"
    } else {
        $commitMatch = [regex]::Match($identity, '\(commit ([0-9A-Fa-f]{12,40})(?:,|\))')
        if (-not $commitMatch.Success) {
            throw "release build identity must contain a concrete git commit, got '$identity'. Use -AllowDevelopmentBuild only for local smoke artifacts."
        }
        if ($identity -notmatch ', dirty=false(?:,|\))') {
            throw "release build identity must report dirty=false, got '$identity'"
        }
        $binaryCommit = $commitMatch.Groups[1].Value
        if (-not $ReleaseTagCommit.StartsWith($binaryCommit, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "release binary commit $binaryCommit does not match tag v$Version ($ReleaseTagCommit)"
        }
    }

    # 4. Archive only the three binaries, at the archive root.
    Push-Location (Join-Path $Staging "package")
    try {
        & $Tar -czf $ArchiveTmp @($BinaryNames | ForEach-Object { "$_.exe" })
    } finally {
        Pop-Location
    }
    if ($LASTEXITCODE -ne 0) {
        throw "tar.exe failed to create $ArchiveName (exit code $LASTEXITCODE)"
    }

    # 5. Publish the SHA-256, then move the complete archive into place. The
    # final name is only ever created from a fully verified archive.
    $Hash = Get-FileHash -LiteralPath $ArchiveTmp -Algorithm SHA256
    Move-Item -LiteralPath $ArchiveTmp -Destination $Archive
    Write-Output "$($Hash.Hash.ToLower())  $ArchiveName"
    Write-Output $Archive
} finally {
    if (Test-Path -LiteralPath $ArchiveTmp) {
        Remove-Item -LiteralPath $ArchiveTmp -Force
    }
    if (Test-Path -LiteralPath $Staging) {
        Remove-Item -LiteralPath $Staging -Recurse -Force
    }
}
