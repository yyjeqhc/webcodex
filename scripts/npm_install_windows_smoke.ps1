# Windows-native packaging + npm-install smoke for WebCodex.
#
# Exercises the full Windows distribution chain on one Windows machine,
# without Git Bash, WSL, or the npm registry:
#
#   cargo build -> Windows artifact -> local manifest + real SHA-256
#     -> npm installer (postinstall) -> temporary npm prefix
#     -> vendor/bin/*.exe -> npm wrapper
#
# Verifies:
#   - the artifact is created and contains exactly the three .exe binaries
#   - the installer verifies the checksum and unpacks the tar.gz
#   - version and build identity are consistent
#   - the npm wrapper finds webcodex.exe
#   - webcodex.exe --version / --help and webcodex-runner.exe --version work
#   - an install failure does not break the previous binary set
#   - staging/temporary files are cleaned up
#
# Never touches the real npm registry (the registry URL is pinned to a
# non-routable address so any accidental network access fails loudly) and
# never publishes anything.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\npm_install_windows_smoke.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\npm_install_windows_smoke.ps1 -BinDir E:\webcodex\target\release -Platform win32-x64
[CmdletBinding()]
param(
    # Directory containing webcodex.exe / webcodex-server.exe / webcodex-runner.exe.
    # Defaults to the debug build, which this script builds first.
    [string]$BinDir,
    # Native Windows release platform exercised by this host.
    [string]$Platform
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$Version = (Get-Content -LiteralPath (Join-Path $Root "npm\webcodex\package.json") -Raw | ConvertFrom-Json).version
if (-not $Platform) {
    $Platform = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "win32-arm64" } else { "win32-x64" }
}
if ($Platform -notin @("win32-x64", "win32-arm64")) {
    throw "invalid Windows smoke platform '$Platform'"
}
$ExpectedNodeArch = if ($Platform -eq "win32-arm64") { "arm64" } else { "x64" }
$NodeArch = (& node -p "process.arch").Trim()
if ($LASTEXITCODE -ne 0 -or $NodeArch -ne $ExpectedNodeArch) {
    throw "native npm smoke requires Node $ExpectedNodeArch for $Platform; got '$NodeArch'"
}

# ---------------------------------------------------------------------------
# 0. Build the three Windows binaries unless an explicit BIN_DIR was given.
# ---------------------------------------------------------------------------
if (-not $BinDir) {
    $BinDir = Join-Path $Root "target\debug"
    Write-Host "building Windows binaries (debug)..."
    # Pin the build timestamp so every binary built in this invocation reports
    # the identical `built_at` in its revision identity (webcodex-core's
    # build.rs reads WEBCODEX_BUILT_AT when set). This mirrors what a release
    # build on the release host must do.
    #
    # The three binaries live in three packages: webcodex.exe is the
    # webcodex-cli package's bin, webcodex-server.exe is the root webcodex
    # package's bin, and webcodex-runner.exe is its own package.
    $env:WEBCODEX_BUILT_AT = [string][long](Get-Date -UFormat %s)
    try {
        & cargo build --locked -p webcodex -p webcodex-cli -p webcodex-runner
    } finally {
        Remove-Item Env:WEBCODEX_BUILT_AT -ErrorAction SilentlyContinue
    }
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
}
$BinDir = [System.IO.Path]::GetFullPath($BinDir)

# ---------------------------------------------------------------------------
# Isolate everything under one smoke root; the registry is pinned to a
# non-routable address so the smoke provably never contacts npmjs.org.
# ---------------------------------------------------------------------------
$TempRoot = Join-Path $env:TEMP ("webcodex-smoke-" + [guid]::NewGuid().ToString("N"))
$ArtifactOut = Join-Path $TempRoot "artifact-out"
$Prefix = Join-Path $TempRoot "install-prefix"
$ManifestPath = Join-Path $TempRoot "local-manifest.json"
New-Item -ItemType Directory -Force -Path $ArtifactOut | Out-Null
$env:npm_config_registry = "http://127.0.0.1:9"
try {
    # -----------------------------------------------------------------------
    # 1. Windows release artifact.
    # -----------------------------------------------------------------------
    Write-Host "creating Windows release artifact..."
    $artifactOutput = & (Join-Path $PSScriptRoot "package_release_artifact.ps1") `
        -BinDir $BinDir -OutDir $ArtifactOut -Platform $Platform -AllowDevelopmentBuild
    if ($LASTEXITCODE -ne 0) {
        throw "package_release_artifact.ps1 failed with exit code $LASTEXITCODE"
    }
    $shaLine = @($artifactOutput)[0]
    $Archive = @($artifactOutput)[1]
    if (-not $Archive -or -not (Test-Path -LiteralPath $Archive)) {
        throw "packaging did not report a produced archive: $artifactOutput"
    }
    $sha256 = ($shaLine -split "\s+", 2)[0]
    if ($sha256 -notmatch '^[a-f0-9]{64}$') {
        throw "packaging did not report a SHA-256: $shaLine"
    }
    $ExpectedArchiveName = "webcodex-v$Version-$Platform.tar.gz"
    if ((Split-Path -Leaf $Archive) -ne $ExpectedArchiveName) {
        throw "unexpected artifact name: $(Split-Path -Leaf $Archive) (expected $ExpectedArchiveName)"
    }

    # -----------------------------------------------------------------------
    # 2. The archive must contain exactly the three .exe binaries. The
    #    Windows System32 tar is used explicitly (Git Bash tar is not
    #    supported and mangles Windows paths).
    # -----------------------------------------------------------------------
    $systemTar = Join-Path $env:SystemRoot "System32\tar.exe"
    if (-not (Test-Path -LiteralPath $systemTar -PathType Leaf)) {
        throw "tar.exe was not found at $systemTar; required to inspect the artifact"
    }
    $entries = @(& $systemTar -tf $Archive)
    $expectedEntries = @("webcodex.exe", "webcodex-server.exe", "webcodex-runner.exe")
    $diff = @($entries | Where-Object { $_ -notin $expectedEntries }) + `
        @($expectedEntries | Where-Object { $_ -notin $entries })
    if ($diff.Count -ne 0) {
        throw "artifact does not contain exactly the three binaries; entries: $($entries -join ', ')"
    }

    # -----------------------------------------------------------------------
    # 3. Local manifest with the actual SHA-256.
    # -----------------------------------------------------------------------
    $artifacts = [ordered]@{}
    $artifacts[$Platform] = [ordered]@{
        url = [System.Uri]::new($Archive).AbsoluteUri
        sha256 = $sha256
    }
    $manifest = [ordered]@{
        version = $Version
        binaries = @("webcodex", "webcodex-server", "webcodex-runner")
        artifacts = $artifacts
    }
    # BOM-less UTF-8: install.js parses the manifest with JSON.parse, which
    # rejects a leading byte-order mark.
    [System.IO.File]::WriteAllText(
        $ManifestPath,
        ($manifest | ConvertTo-Json -Depth 6),
        [System.Text.UTF8Encoding]::new($false)
    )

    # -----------------------------------------------------------------------
    # 4. Pack the npm package locally, then install it into a temp prefix.
    # -----------------------------------------------------------------------
    $PackageDir = Join-Path $Root "npm\webcodex"
    Push-Location $PackageDir
    try {
        # This smoke intentionally installs from the local development manifest
        # above, so it must not impersonate a publish-ready package. Keep the
        # package's prepack release guard intact and bypass lifecycle scripts only
        # for this local tgz creation; npm install below still runs postinstall.
        & npm pack --ignore-scripts --pack-destination $TempRoot --silent
        if ($LASTEXITCODE -ne 0) {
            throw "npm pack failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
    $Tarball = Get-ChildItem -LiteralPath $TempRoot -Filter "yyjeqhc-webcodex-*.tgz" |
        Select-Object -First 1 -ExpandProperty FullName
    if (-not $Tarball) {
        throw "npm pack produced no tarball"
    }

    Write-Host "installing from local artifact via npm installer..."
    $env:WEBCODEX_MANIFEST = $ManifestPath
    try {
        & npm install --prefix $Prefix --no-audit --no-fund $Tarball
        if ($LASTEXITCODE -ne 0) {
            throw "npm install failed with exit code $LASTEXITCODE"
        }
    } finally {
        Remove-Item Env:WEBCODEX_MANIFEST -ErrorAction SilentlyContinue
    }

    # -----------------------------------------------------------------------
    # 5. vendor/bin/*.exe, version/identity, wrapper, --help.
    # -----------------------------------------------------------------------
    $Installed = Join-Path $Prefix "node_modules\@yyjeqhc\webcodex"
    $VendorBin = Join-Path $Installed "vendor\bin"
    foreach ($name in @("webcodex", "webcodex-server", "webcodex-runner")) {
        if (-not (Test-Path -LiteralPath (Join-Path $VendorBin "$name.exe") -PathType Leaf)) {
            throw "installed package is missing $name.exe in vendor\bin"
        }
    }

    $cli = Join-Path $VendorBin "webcodex.exe"
    $versionOut = & $cli --version
    if ($LASTEXITCODE -ne 0 -or -not $versionOut -or -not $versionOut.StartsWith("webcodex $Version ")) {
        throw "webcodex.exe --version produced unexpected output: $versionOut"
    }
    $helpOut = & $cli --help
    if ($LASTEXITCODE -ne 0 -or -not $helpOut) {
        throw "webcodex.exe --help failed"
    }
    $runnerOut = & (Join-Path $VendorBin "webcodex-runner.exe") --version
    if ($LASTEXITCODE -ne 0 -or -not $runnerOut -or -not $runnerOut.StartsWith("webcodex-runner $Version ")) {
        throw "webcodex-runner.exe --version produced unexpected output: $runnerOut"
    }

    # The npm wrapper (vendor/bin resolution + spawn) through the .bin shim.
    $wrapperShim = Join-Path $Prefix "node_modules\.bin\webcodex.cmd"
    if (-not (Test-Path -LiteralPath $wrapperShim)) {
        throw "npm did not create the .bin\webcodex wrapper shim"
    }
    $wrapperOut = & $wrapperShim --version
    if ($LASTEXITCODE -ne 0 -or -not $wrapperOut -or -not $wrapperOut.StartsWith("webcodex $Version ")) {
        throw "npm wrapper --version produced unexpected output: $wrapperOut"
    }

    # -----------------------------------------------------------------------
    # 6. An install failure must not break the previous binary set.
    # -----------------------------------------------------------------------
    $badArtifacts = [ordered]@{}
    $badArtifacts[$Platform] = [ordered]@{
        url = [System.Uri]::new($Archive).AbsoluteUri
        sha256 = "0" * 64
    }
    $badManifest = [ordered]@{
        version = $Version
        binaries = @("webcodex", "webcodex-server", "webcodex-runner")
        artifacts = $badArtifacts
    }
    $BadManifestPath = Join-Path $TempRoot "bad-manifest.json"
    [System.IO.File]::WriteAllText(
        $BadManifestPath,
        ($badManifest | ConvertTo-Json -Depth 6),
        [System.Text.UTF8Encoding]::new($false)
    )
    $env:WEBCODEX_MANIFEST = $BadManifestPath
    try {
        & node (Join-Path $Installed "install.js")
        if ($LASTEXITCODE -eq 0) {
            throw "install.js must fail against a manifest with a wrong checksum"
        }
    } finally {
        Remove-Item Env:WEBCODEX_MANIFEST -ErrorAction SilentlyContinue
    }
    $afterFailure = & $cli --version
    if ($LASTEXITCODE -ne 0 -or $afterFailure -ne $versionOut) {
        throw "the previous binary set was damaged by the failed install"
    }

    # -----------------------------------------------------------------------
    # 7. Staging/temporary cleanup.
    # -----------------------------------------------------------------------
    $leftovers = Get-ChildItem -LiteralPath $env:TEMP -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like "webcodex-artifact-*" -or $_.Name -like "webcodex-manifest-*" -or $_.Name -like "webcodex-npm-test-*" } |
        Select-Object -ExpandProperty FullName
    $stagingLeftovers = Get-ChildItem -LiteralPath (Split-Path $Installed) -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like ".bin-staging-*" -or $_.Name -like ".bin-backup-*" } |
        Select-Object -ExpandProperty FullName
    if ($leftovers -or $stagingLeftovers) {
        throw "installer left staging/temp files behind: $($leftovers + $stagingLeftovers -join '; ')"
    }

    Write-Host "Windows artifact -> npm install smoke passed for $Platform"
    Write-Host "  artifact:  $Archive"
    Write-Host "  sha256:    $sha256"
    Write-Host "  installed: $VendorBin"
} finally {
    Remove-Item Env:npm_config_registry -ErrorAction SilentlyContinue
    Remove-Item Env:WEBCODEX_MANIFEST -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $TempRoot) {
        Remove-Item -LiteralPath $TempRoot -Recurse -Force
    }
}
