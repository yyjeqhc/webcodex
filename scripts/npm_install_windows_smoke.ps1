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
#   - packaged `server init` + foreground `server run --env-file` reaches HTTP readiness
#     with an isolated local config/data root and leaves no Server process behind
#   - packaged explicit `share --tunnel none` reaches local MCP readiness with
#     isolated project state and exact tracked Server/Runner children
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

function Get-BoundedShareLogTail {
    param(
        [string]$Path,
        [int]$Lines = 80
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return "<missing>"
    }
    try {
        $text = (@(Get-Content -LiteralPath $Path -Tail $Lines -ErrorAction Stop) -join [Environment]::NewLine)
        if ([string]::IsNullOrWhiteSpace($text)) {
            return "<empty>"
        }
        # These logs should already avoid credentials, but keep failure diagnostics
        # safe even if a future log message accidentally includes an auth header or
        # one of WebCodex's opaque credential forms.
        $text = $text -replace '(?i)(authorization\s*:\s*bearer\s+)[^\s"]+', '$1<redacted>'
        $text = $text -replace '\b(?:wc_pat|wc_agent|wc_csec|webcodex)_[A-Za-z0-9_-]{8,}\b', '<redacted>'
        return $text
    } catch {
        return "<unreadable>"
    }
}

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
    # 6. Windows foreground Server runtime. Exercise the installed CLI so this
    #    proves sibling webcodex-server.exe discovery and WEBCODEX_ENV_FILE
    #    propagation, not merely that the Server binary itself starts.
    # -----------------------------------------------------------------------
    $ServerSmokeRoot = Join-Path $TempRoot "foreground-server"
    $ServerData = Join-Path $ServerSmokeRoot "data"
    $ServerEnv = Join-Path $ServerSmokeRoot "webcodex.env"
    $ServerStdout = Join-Path $ServerSmokeRoot "server.stdout.log"
    $ServerStderr = Join-Path $ServerSmokeRoot "server.stderr.log"
    New-Item -ItemType Directory -Force -Path $ServerSmokeRoot | Out-Null

    $PortProbe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $PortProbe.Start()
    try {
        $ServerPort = ([System.Net.IPEndPoint]$PortProbe.LocalEndpoint).Port
    } finally {
        $PortProbe.Stop()
    }
    $Listen = "127.0.0.1:$ServerPort"

    # Process environment wins over env-file values, so clear the canonical
    # Server keys that could override this smoke's isolated listener/data/auth
    # state. Also keep operator control-plane/tunnel credentials out of scope.
    $SensitiveEnvNames = @(
        "CONTROL_PLANE_API_KEY",
        "CONTROL_PLANE_TUNNEL_ID",
        "OPENAI_TUNNEL_TOKEN",
        "WEBCODEX_ENV_FILE",
        "WEBCODEX_ADDR",
        "WEBCODEX_DATA",
        "WEBCODEX_TOKEN",
        "WEBCODEX_SHARED_KEY_ENABLED",
        "WEBCODEX_ALLOW_ANONYMOUS",
        "WEBCODEX_PUBLIC_URL",
        "WEBCODEX_OAUTH2_ENABLED",
        "WEBCODEX_OAUTH2_ISSUER",
        "WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE"
    )
    $SavedSensitiveEnv = @{}
    foreach ($name in $SensitiveEnvNames) {
        $item = Get-Item "Env:$name" -ErrorAction SilentlyContinue
        if ($null -ne $item) { $SavedSensitiveEnv[$name] = $item.Value }
        Remove-Item "Env:$name" -ErrorAction SilentlyContinue
    }

    $ServerCliProcess = $null
    $ServerChildPid = $null
    $ServerExe = Join-Path $VendorBin "webcodex-server.exe"
    $BaselineServerPids = @(
        Get-CimInstance Win32_Process -Filter "Name='webcodex-server.exe'" -ErrorAction SilentlyContinue |
            Where-Object { $_.ExecutablePath -eq $ServerExe } |
            Select-Object -ExpandProperty ProcessId
    )
    try {
        $initOutput = @(& $cli server init --listen $Listen --data-dir $ServerData --env-file $ServerEnv --json)
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $ServerEnv -PathType Leaf)) {
            throw "Windows foreground server init failed with exit code $LASTEXITCODE"
        }
        $envText = [System.IO.File]::ReadAllText($ServerEnv)
        $tokenMatch = [regex]::Match($envText, '(?m)^WEBCODEX_TOKEN=(.+)$')
        if (-not $tokenMatch.Success -or [string]::IsNullOrWhiteSpace($tokenMatch.Groups[1].Value)) {
            throw "server init did not write WEBCODEX_TOKEN to the isolated env file"
        }
        $secretToken = $tokenMatch.Groups[1].Value.Trim()
        if (($initOutput -join "`n").Contains($secretToken)) {
            throw "server init leaked WEBCODEX_TOKEN to stdout"
        }

        $quotedEnvFile = '"' + $ServerEnv.Replace('"', '\"') + '"'
        $ServerCliProcess = Start-Process -FilePath $cli `
            -ArgumentList @("server", "run", "--env-file", $quotedEnvFile) `
            -RedirectStandardOutput $ServerStdout -RedirectStandardError $ServerStderr `
            -PassThru -WindowStyle Hidden

        $ready = $false
        $deadline = [System.Diagnostics.Stopwatch]::StartNew()
        while ($deadline.Elapsed -lt [TimeSpan]::FromSeconds(20)) {
            if ($ServerCliProcess.HasExited) {
                throw "foreground webcodex CLI exited before Server readiness (exit $($ServerCliProcess.ExitCode))"
            }
            if ($null -eq $ServerChildPid) {
                $child = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($ServerCliProcess.Id)" -ErrorAction SilentlyContinue |
                    Where-Object { $_.Name -eq "webcodex-server.exe" -and $_.ExecutablePath -eq $ServerExe } |
                    Select-Object -First 1
                if ($null -ne $child) { $ServerChildPid = [int]$child.ProcessId }
            }
            try {
                $response = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$ServerPort/openapi.json" -TimeoutSec 1
                if ($response.StatusCode -eq 200) {
                    $ready = $true
                    break
                }
            } catch {
                # Startup connection failures are expected until the one absolute deadline.
            }
            Start-Sleep -Milliseconds 100
        }
        if (-not $ready) {
            throw "foreground Windows Server did not reach /openapi.json readiness before the absolute deadline"
        }
        while ($null -eq $ServerChildPid -and $deadline.Elapsed -lt [TimeSpan]::FromSeconds(20)) {
            $child = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($ServerCliProcess.Id)" -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -eq "webcodex-server.exe" -and $_.ExecutablePath -eq $ServerExe } |
                Select-Object -First 1
            if ($null -ne $child) {
                $ServerChildPid = [int]$child.ProcessId
                break
            }
            Start-Sleep -Milliseconds 50
        }
        if ($null -eq $ServerChildPid) {
            throw "foreground CLI readiness succeeded without observing its real webcodex-server.exe child before the same absolute deadline"
        }
    } finally {
        if ($null -ne $ServerChildPid) {
            Stop-Process -Id $ServerChildPid -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $ServerCliProcess -and -not $ServerCliProcess.HasExited) {
            Stop-Process -Id $ServerCliProcess.Id -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $ServerCliProcess) {
            try { $ServerCliProcess.WaitForExit(5000) | Out-Null } catch {}
        }
        foreach ($name in $SensitiveEnvNames) {
            Remove-Item "Env:$name" -ErrorAction SilentlyContinue
            if ($SavedSensitiveEnv.ContainsKey($name)) {
                Set-Item "Env:$name" $SavedSensitiveEnv[$name]
            }
        }
        $newServerProcesses = @(
            Get-CimInstance Win32_Process -Filter "Name='webcodex-server.exe'" -ErrorAction SilentlyContinue |
                Where-Object { $_.ExecutablePath -eq $ServerExe -and $_.ProcessId -notin $BaselineServerPids }
        )
        foreach ($process in $newServerProcesses) {
            Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
        }
        if ($newServerProcesses.Count -gt 0) {
            Start-Sleep -Milliseconds 200
            $leftRunning = @($newServerProcesses | Where-Object { Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue })
            if ($leftRunning.Count -gt 0) {
                throw "foreground Windows Server smoke left webcodex-server.exe running"
            }
        }
    }

    # -----------------------------------------------------------------------
    # 7. Explicit Windows share with no tunnel. This is the deterministic CI
    #    share lane: installed CLI -> private state -> Server -> Runner -> MCP.
    #    Public Cloudflare/OpenAI E2E remains native dogfood evidence.
    # -----------------------------------------------------------------------
    $ShareSmokeRoot = Join-Path $TempRoot "share-none"
    $ShareRepo = Join-Path $ShareSmokeRoot "repo"
    $ShareState = Join-Path $ShareSmokeRoot "state"
    New-Item -ItemType Directory -Force -Path $ShareRepo | Out-Null
    & git -C $ShareRepo init --quiet
    if ($LASTEXITCODE -ne 0) {
        throw "could not initialize isolated Git repo for Windows share smoke"
    }
    # ProjectConnector normal-mode readiness requires a real Git baseline so the
    # first writable task can create its managed isolated worktree. An empty
    # `git init` repository is intentionally not writable-ready.
    $ShareReadme = Join-Path $ShareRepo "README.md"
    [System.IO.File]::WriteAllText(
        $ShareReadme,
        "Windows share smoke`n",
        [System.Text.UTF8Encoding]::new($false)
    )
    & git -C $ShareRepo add README.md
    if ($LASTEXITCODE -ne 0) {
        throw "could not stage initial Windows share smoke fixture"
    }
    & git -C $ShareRepo -c user.name="WebCodex CI" -c user.email="ci@webcodex.invalid" commit --quiet -m "Initialize Windows share smoke"
    if ($LASTEXITCODE -ne 0) {
        throw "could not create initial Windows share smoke commit"
    }
    & git -C $ShareRepo rev-parse --verify HEAD | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Windows share smoke fixture does not have a readable Git HEAD"
    }

    $ShareEnvNames = @(
        "CONTROL_PLANE_API_KEY",
        "CONTROL_PLANE_TUNNEL_ID",
        "OPENAI_ADMIN_KEY",
        "OPENAI_API_KEY",
        "OPENAI_TUNNEL_TOKEN",
        "WEBCODEX_AGENT_BIN",
        "WEBCODEX_AGENT_TOKEN",
        "WEBCODEX_CLOUDFLARED_BIN",
        "WEBCODEX_TUNNEL_CLIENT_BIN",
        "WEBCODEX_ENV_FILE",
        "WEBCODEX_MCP_MODEL_SURFACE",
        "WEBCODEX_ADDR",
        "WEBCODEX_DATA",
        "WEBCODEX_TOKEN",
        "WEBCODEX_SHARED_KEY_ENABLED",
        "WEBCODEX_ALLOW_ANONYMOUS",
        "WEBCODEX_PUBLIC_URL",
        "WEBCODEX_OAUTH2_ENABLED",
        "WEBCODEX_OAUTH2_ISSUER",
        "WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE"
    )
    $SavedShareEnv = @{}
    foreach ($name in $ShareEnvNames) {
        $item = Get-Item "Env:$name" -ErrorAction SilentlyContinue
        if ($null -ne $item) { $SavedShareEnv[$name] = $item.Value }
        Remove-Item "Env:$name" -ErrorAction SilentlyContinue
    }

    $ShareCliProcess = $null
    $ShareServerPid = $null
    $ShareRunnerPid = $null
    $ShareStdoutBuffer = [System.Text.StringBuilder]::new()
    $ShareStderrBuffer = [System.Text.StringBuilder]::new()
    $ShareStdoutTask = $null
    $ShareStderrTask = $null
    $ShareServerExe = Join-Path $VendorBin "webcodex-server.exe"
    $ShareRunnerExe = Join-Path $VendorBin "webcodex-runner.exe"
    try {
        $quotedShareRepo = '"' + $ShareRepo.Replace('"', '\"') + '"'
        $quotedShareState = '"' + $ShareState.Replace('"', '\"') + '"'
        # Start-Process keeps -RedirectStandardOutput files exclusively open on
        # Windows PowerShell while the child is alive. Observe the long-lived
        # share process through redirected pipes instead so readiness can be read
        # without racing the redirection file handle, and drain stderr in parallel
        # so neither pipe can block the child.
        $shareStartInfo = [System.Diagnostics.ProcessStartInfo]::new()
        $shareStartInfo.FileName = $cli
        $shareStartInfo.Arguments = "share --root $quotedShareRepo --state-dir $quotedShareState --tunnel none --no-copy-url"
        $shareStartInfo.UseShellExecute = $false
        $shareStartInfo.RedirectStandardOutput = $true
        $shareStartInfo.RedirectStandardError = $true
        $shareStartInfo.CreateNoWindow = $true
        $ShareCliProcess = [System.Diagnostics.Process]::new()
        $ShareCliProcess.StartInfo = $shareStartInfo
        if (-not $ShareCliProcess.Start()) {
            throw "could not start webcodex share --tunnel none"
        }
        $ShareStdoutTask = $ShareCliProcess.StandardOutput.ReadLineAsync()
        $ShareStderrTask = $ShareCliProcess.StandardError.ReadLineAsync()

        $shareDeadline = [System.Diagnostics.Stopwatch]::StartNew()
        $shareReady = $false
        $shareOutput = ""
        while ($shareDeadline.Elapsed -lt [TimeSpan]::FromSeconds(65)) {
            while ($null -ne $ShareStdoutTask -and $ShareStdoutTask.IsCompleted) {
                $line = $ShareStdoutTask.Result
                if ($null -eq $line) {
                    $ShareStdoutTask = $null
                    break
                }
                $null = $ShareStdoutBuffer.AppendLine($line)
                $ShareStdoutTask = $ShareCliProcess.StandardOutput.ReadLineAsync()
            }
            while ($null -ne $ShareStderrTask -and $ShareStderrTask.IsCompleted) {
                $line = $ShareStderrTask.Result
                if ($null -eq $line) {
                    $ShareStderrTask = $null
                    break
                }
                $null = $ShareStderrBuffer.AppendLine($line)
                $ShareStderrTask = $ShareCliProcess.StandardError.ReadLineAsync()
            }
            $shareOutput = $ShareStdoutBuffer.ToString()
            if ($ShareCliProcess.HasExited) {
                $shareError = $ShareStderrBuffer.ToString().Trim()
                $agentTail = Get-BoundedShareLogTail -Path (Join-Path $ShareState "logs\agent.log")
                $serverTail = Get-BoundedShareLogTail -Path (Join-Path $ShareState "logs\server.log")
                throw "webcodex share --tunnel none exited before readiness (exit $($ShareCliProcess.ExitCode)): $shareError`n--- agent.log tail ---`n$agentTail`n--- server.log tail ---`n$serverTail"
            }
            $children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($ShareCliProcess.Id)" -ErrorAction SilentlyContinue)
            if ($null -eq $ShareServerPid) {
                $serverChild = $children | Where-Object { $_.Name -eq "webcodex-server.exe" -and $_.ExecutablePath -eq $ShareServerExe } | Select-Object -First 1
                if ($null -ne $serverChild) { $ShareServerPid = [int]$serverChild.ProcessId }
            }
            if ($null -eq $ShareRunnerPid) {
                $runnerChild = $children | Where-Object { $_.Name -eq "webcodex-runner.exe" -and $_.ExecutablePath -eq $ShareRunnerExe } | Select-Object -First 1
                if ($null -ne $runnerChild) { $ShareRunnerPid = [int]$runnerChild.ProcessId }
            }
            if ($shareOutput.Contains("WebCodex ready")) {
                $shareReady = $true
                break
            }
            Start-Sleep -Milliseconds 100
        }
        if (-not $shareReady) {
            $agentTail = Get-BoundedShareLogTail -Path (Join-Path $ShareState "logs\agent.log")
            $serverTail = Get-BoundedShareLogTail -Path (Join-Path $ShareState "logs\server.log")
            throw "Windows share --tunnel none did not become ready before the absolute smoke deadline`n--- agent.log tail ---`n$agentTail`n--- server.log tail ---`n$serverTail"
        }
        if ($null -eq $ShareServerPid -or $null -eq $ShareRunnerPid) {
            throw "Windows share readiness did not expose the expected exact Server + Runner child processes"
        }

        $mcpMatch = [regex]::Match($shareOutput, 'http://127\.0\.0\.1:\d+/mcp')
        $credentialMatch = [regex]::Match($shareOutput, '(?m)^\d+\. Credential \(this share only\): (.+)$')
        if (-not $mcpMatch.Success -or -not $credentialMatch.Success) {
            throw "Windows share output did not contain the local MCP endpoint and temporary Bearer contract"
        }
        $shareCredential = $credentialMatch.Groups[1].Value.Trim()
        if ([string]::IsNullOrWhiteSpace($shareCredential)) {
            throw "Windows share printed an empty temporary Bearer credential"
        }
        $initialize = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"windows-package-smoke","version":"1"}}}'
        $headers = @{
            Authorization = "Bearer $shareCredential"
            Accept = "application/json, text/event-stream"
            "MCP-Protocol-Version" = "2025-06-18"
        }
        $response = Invoke-WebRequest -UseBasicParsing -Method Post -Uri $mcpMatch.Value `
            -Headers $headers -ContentType "application/json" -Body $initialize -TimeoutSec 3
        if ($response.StatusCode -ne 200) {
            throw "Windows share local MCP initialize returned HTTP $($response.StatusCode)"
        }
        if (-not (Test-Path -LiteralPath $ShareState -PathType Container)) {
            throw "Windows share did not create isolated project state"
        }
    } finally {
        # CI termination is containment only; native MSI dogfood separately proves
        # Ctrl-C/foreground cleanup. Never kill by process name.
        if ($null -ne $ShareServerPid) {
            Stop-Process -Id $ShareServerPid -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $ShareRunnerPid) {
            Stop-Process -Id $ShareRunnerPid -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $ShareCliProcess -and -not $ShareCliProcess.HasExited) {
            Stop-Process -Id $ShareCliProcess.Id -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $ShareCliProcess) {
            try { $ShareCliProcess.WaitForExit(5000) | Out-Null } catch {}
        }
        foreach ($trackedPid in @($ShareServerPid, $ShareRunnerPid)) {
            if ($null -ne $trackedPid -and $null -ne (Get-Process -Id $trackedPid -ErrorAction SilentlyContinue)) {
                throw "Windows share smoke left a tracked child process running"
            }
        }
        foreach ($name in $ShareEnvNames) {
            Remove-Item "Env:$name" -ErrorAction SilentlyContinue
            if ($SavedShareEnv.ContainsKey($name)) {
                Set-Item "Env:$name" $SavedShareEnv[$name]
            }
        }
    }

    # -----------------------------------------------------------------------
    # 8. An install failure must not break the previous binary set.
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
    # 9. Staging/temporary cleanup.
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
