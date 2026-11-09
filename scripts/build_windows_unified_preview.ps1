# Offline Windows x64 Unified Preview: no release claims, no automatic install.
# Run from a clean committed WebCodex checkout. Never ship as an official release.
[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$OldLocation = Get-Location
$OriginalCargoTarget = $env:CARGO_TARGET_DIR
$OriginalSha = $env:WEBCODEX_GIT_COMMIT
$OriginalDirty = $env:WEBCODEX_GIT_DIRTY
$OriginalBuiltAt = $env:WEBCODEX_BUILT_AT

function Assert-Success([string]$Step) {
    if ($LASTEXITCODE -ne 0) { throw "$Step failed (exit $LASTEXITCODE)" }
}

try {
    Set-Location $Root
    $Dirty = @(& git.exe status --porcelain --untracked-files=all)
    Assert-Success "git status"
    if ($Dirty.Count -gt 0) { throw "Commit/stash changes before building. The worktree must be clean." }
    $Sha = (& git.exe rev-parse HEAD).Trim().ToLowerInvariant()
    Assert-Success "git rev-parse"
    if ($Sha -notmatch '^[0-9a-f]{40}$') { throw "Invalid source SHA" }
    $Short = $Sha.Substring(0, 12)
    $Version = (& node.exe -p "require('./npm/webcodex/package.json').version").Trim()
    Assert-Success "package version"
    $BuiltAt = (& git.exe show -s --format=%ct HEAD).Trim()
    Assert-Success "source commit time"
    if ($BuiltAt -notmatch '^[0-9]+$') { throw "Invalid source timestamp" }
    $Python = (& py.exe -3.12 -c "import sys; print(sys.executable)").Trim()
    Assert-Success "Python 3.12"
    if (!(Test-Path -LiteralPath $Python -PathType Leaf)) { throw "Python 3.12 missing" }
    $Makensis = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'NSIS\makensis.exe'
    if (!(Test-Path -LiteralPath $Makensis -PathType Leaf)) { throw "NSIS 3.13+ not installed" }
    # Tauri's NSIS bundle patches Desktop PE resources, changing its bytes
    # after the initial source collection. Inspect the actual inner package.
    $SevenZip = $null
    $SevenZipCommand = Get-Command '7z.exe' -ErrorAction SilentlyContinue
    if ($SevenZipCommand) { $SevenZip = $SevenZipCommand.Source }
    if (!$SevenZip) {
        foreach ($Path in @(
            (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) '7-Zip\7z.exe'),
            (Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) '7-Zip\7z.exe'),
            'C:\ProgramData\chocolatey\tools\7z.exe'
        )) {
            if (Test-Path -LiteralPath $Path -PathType Leaf) { $SevenZip = $Path; break }
        }
    }
    if (!$SevenZip) { throw "7-Zip is required to verify actual inner NSIS package bytes. Install 7-Zip before building." }
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw "Only native Windows x64 is supported" }
    $Platform = "win32-x64"

    $OutRoot = Join-Path $Root ("target\unified-preview\" + $Short)
    if (Test-Path -LiteralPath $OutRoot) {
        throw "Preview output already exists: $OutRoot. Inspect and remove it explicitly before rebuilding."
    }
    New-Item -Path $OutRoot -ItemType Directory -Force | Out-Null
    $env:WEBCODEX_GIT_COMMIT = $Sha
    $env:WEBCODEX_GIT_DIRTY = "false"
    $env:WEBCODEX_BUILT_AT = $BuiltAt

    Write-Host "Building LOCAL Windows Unified Preview for $Sha (NOT OFFICIAL)"
    & "$PSScriptRoot\build_desktop_windows_local.ps1"
    Assert-Success "standard Desktop build"

    # Only the CLI gets the non-default, local-test-only Cargo feature.
    & cargo.exe build --locked --profile dogfood -p webcodex-cli --features local-preview-installer
    Assert-Success "local-preview CLI build"

    $BinaryDir = Join-Path $OutRoot 'native\artifacts\bin'
    New-Item -Path $BinaryDir -ItemType Directory -Force | Out-Null
    foreach ($Name in @('webcodex', 'webcodex-server', 'webcodex-runner')) {
        Copy-Item -LiteralPath (Join-Path $Root "target\dogfood\$Name.exe") -Destination (Join-Path $BinaryDir "$Name.exe")
    }
    Copy-Item -LiteralPath (Join-Path $Root 'target\desktop-local-tauri\release\WebCodex.exe') -Destination (Join-Path $BinaryDir 'webcodex-desktop.exe')
    foreach ($Name in @('webcodex', 'webcodex-server', 'webcodex-runner', 'webcodex-desktop')) {
        $Info = & (Join-Path $BinaryDir "$Name.exe") --build-info-json | ConvertFrom-Json
        Assert-Success "$Name build-info"
        if ($Info.git_commit -ne $Sha -or $Info.git_dirty -ne $false -or $Info.version -ne $Version) {
            throw "$Name is not a clean same-commit build"
        }
        if ($Info.target -ne 'x86_64-pc-windows-msvc') { throw "$Name native target mismatch" }
    }

    $InputRoot = Join-Path $OutRoot 'native'
    $Candidate = Join-Path $OutRoot 'candidate'
    $CollectArgs = @("$PSScriptRoot\collect_unified_installer_inputs.py", '--local-preview', '--platform', $Platform, '--input-root', $InputRoot, '--webcodex', 'artifacts/bin/webcodex.exe', '--webcodex-server', 'artifacts/bin/webcodex-server.exe', '--webcodex-runner', 'artifacts/bin/webcodex-runner.exe', '--webcodex-desktop', 'artifacts/bin/webcodex-desktop.exe', '--desktop-payload', 'artifacts/bin/webcodex-desktop.exe', '--desktop-executable', 'webcodex-desktop.exe', '--version', $Version, '--source-sha', $Sha, '--output-dir', $Candidate)
    & $Python @CollectArgs
    Assert-Success "local-preview candidate collection"

    $Staged = Join-Path $OutRoot 'staged'
    & $Python "$PSScriptRoot\prepare_windows_unified_installer.py" --local-preview --candidate-dir $Candidate --platform $Platform --output-dir $Staged
    Assert-Success "local-preview NSIS staging"

    $env:CARGO_TARGET_DIR = Join-Path $OutRoot 'tauri-build'
    $Overlay = Join-Path $Staged 'tauri.unified-installer.conf.json'
    Push-Location (Join-Path $Root 'apps\desktop')
    try {
        & node.exe node_modules/@tauri-apps/cli/tauri.js build --bundles nsis --config $Overlay --ci --no-sign -- --locked
        Assert-Success "inner Tauri NSIS build"
    } finally {
        Pop-Location
    }
    $Bundles = @(Get-ChildItem -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'release\bundle\nsis') -Filter '*-setup.exe' -File)
    if ($Bundles.Count -ne 1) { throw "Expected one inner installer; found $($Bundles.Count)" }
    $Inner = Join-Path $OutRoot 'tauri-inner.exe'
    Copy-Item -LiteralPath $Bundles[0].FullName -Destination $Inner

    # Rebind the manifest to the EXACT files inside the packaged installer.
    # Tauri's NSIS bundling modifies WebCodex.exe after the first Build Info
    # collection; a manifest based on the earlier EXE makes finish() fail,
    # causing an unnecessary rollback despite a valid native installation.
    $InnerExtracted = Join-Path $OutRoot 'inner-extracted'
    New-Item -Path $InnerExtracted -ItemType Directory | Out-Null
    & $SevenZip x -y "-o$InnerExtracted" $Inner | Out-Null
    Assert-Success "inspect actual inner NSIS package"
    $FinalRoot = Join-Path $OutRoot 'inner-native'
    $FinalBinaryDir = Join-Path $FinalRoot 'artifacts\bin'
    New-Item -Path $FinalBinaryDir -ItemType Directory -Force | Out-Null
    foreach ($Name in @('webcodex', 'webcodex-server', 'webcodex-runner')) {
        $Actual = Join-Path $InnerExtracted "webcodex-runtime\$Name.exe"
        $Initial = Join-Path $BinaryDir "$Name.exe"
        if (!(Test-Path -LiteralPath $Actual -PathType Leaf)) { throw "Inner installer omits $Name.exe" }
        if ((Get-FileHash -LiteralPath $Actual -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $Initial -Algorithm SHA256).Hash) {
            throw "Inner installer altered the $Name runtime executable"
        }
        Copy-Item -LiteralPath $Actual -Destination (Join-Path $FinalBinaryDir "$Name.exe")
    }
    $InnerDesktop = Join-Path $InnerExtracted 'WebCodex.exe'
    if (!(Test-Path -LiteralPath $InnerDesktop -PathType Leaf)) { throw "Inner installer omits WebCodex.exe" }
    Copy-Item -LiteralPath $InnerDesktop -Destination (Join-Path $FinalBinaryDir 'webcodex-desktop.exe')
    foreach ($Name in @('webcodex', 'webcodex-server', 'webcodex-runner', 'webcodex-desktop')) {
        $Info = & (Join-Path $FinalBinaryDir "$Name.exe") --build-info-json | ConvertFrom-Json
        Assert-Success "bundled $Name Build Info"
        if ($Info.git_commit -ne $Sha -or $Info.git_dirty -ne $false -or $Info.version -ne $Version) {
            throw "Bundled $Name is not a clean same-commit executable"
        }
        if ($Info.target -ne 'x86_64-pc-windows-msvc') { throw "Bundled $Name target mismatch" }
    }
    $FinalCandidate = Join-Path $OutRoot 'candidate-final'
    $FinalCollectArgs = @("$PSScriptRoot\collect_unified_installer_inputs.py", '--local-preview', '--platform', $Platform, '--input-root', $FinalRoot, '--webcodex', 'artifacts/bin/webcodex.exe', '--webcodex-server', 'artifacts/bin/webcodex-server.exe', '--webcodex-runner', 'artifacts/bin/webcodex-runner.exe', '--webcodex-desktop', 'artifacts/bin/webcodex-desktop.exe', '--desktop-payload', 'artifacts/bin/webcodex-desktop.exe', '--desktop-executable', 'webcodex-desktop.exe', '--version', $Version, '--source-sha', $Sha, '--output-dir', $FinalCandidate)
    & $Python @FinalCollectArgs
    Assert-Success "collect exact packaged executable manifests"
    $Manifest = Get-Content -LiteralPath (Join-Path $FinalCandidate 'source-manifest.json') -Raw | ConvertFrom-Json
    foreach ($Name in @('webcodex', 'webcodex-server', 'webcodex-runner', 'webcodex-desktop')) {
        $Expected = $Manifest.artifacts.$Name.sha256
        $Actual = (Get-FileHash -LiteralPath (Join-Path $FinalBinaryDir "$Name.exe") -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($Expected -ne $Actual) { throw "Final manifest does not match packaged $Name" }
    }
    Write-Host "Inner NSIS payload: all four component SHA-256 digests agree with final manifest"

    $Output = Join-Path $OutRoot ("webcodex-unified-LOCAL-PREVIEW-$Short-v$Version-win32-x64.exe")
    & $Python "$PSScriptRoot\build_windows_unified_bootstrap.py" --local-preview --candidate-dir $FinalCandidate --inner-installer $Inner --platform $Platform --makensis $Makensis --output $Output
    Assert-Success "outer guarded Preview bootstrap"
    $Hash = (Get-FileHash -LiteralPath $Output -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText("$Output.sha256", "$Hash  $(Split-Path -Leaf $Output)" + [Environment]::NewLine, [System.Text.Encoding]::ASCII)
    Write-Host "PREVIEW_BUILD_OK: $Output"
    Write-Host "SHA256: $Hash"
    Write-Host "LOCAL CANDIDATE: $FinalCandidate"
    Write-Host "NOT INSTALLED: complete an offline backup and stop legacy Desktop processes before any real upgrade."
} finally {
    $env:CARGO_TARGET_DIR = $OriginalCargoTarget
    $env:WEBCODEX_GIT_COMMIT = $OriginalSha
    $env:WEBCODEX_GIT_DIRTY = $OriginalDirty
    $env:WEBCODEX_BUILT_AT = $OriginalBuiltAt
    Set-Location $OldLocation
}
