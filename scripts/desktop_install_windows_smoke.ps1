# Install, inspect, and uninstall one newly-built WebCodex Desktop NSIS package.
# The helper never kills by executable name and never removes WebCodex user state.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$SourceSha,
    [Parameter(Mandatory = $true)][Int64]$BuiltAt,
    [Parameter(Mandatory = $true)][ValidateSet("win32-x64", "win32-arm64")][string]$Platform,
    [string]$InstallDir
)

$ErrorActionPreference = "Stop"
$Installer = [System.IO.Path]::GetFullPath($Installer)
if (-not (Test-Path -LiteralPath $Installer -PathType Leaf)) {
    throw "Desktop installer does not exist: $Installer"
}
if ($SourceSha -notmatch '^[0-9A-Fa-f]{40}$') {
    throw "SourceSha must be one exact 40-hex Git commit"
}
if ($BuiltAt -le 0) {
    throw "BuiltAt must be a positive Unix timestamp"
}
$requestedInstallDir = $null
if ($InstallDir) {
    $requestedInstallDir = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
    if (Test-Path -LiteralPath $requestedInstallDir) {
        throw "refusing custom-directory smoke because the requested install directory already exists: $requestedInstallDir"
    }
}

function Get-WebCodexUninstallEntry {
    $root = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall"
    if (-not (Test-Path -LiteralPath $root)) { return $null }
    $entries = @(
        Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue |
            ForEach-Object { Get-ItemProperty -LiteralPath $_.PSPath -ErrorAction SilentlyContinue } |
            Where-Object { $_.DisplayName -eq "WebCodex Desktop" }
    )
    if ($entries.Count -gt 1) {
        throw "multiple current-user WebCodex Desktop uninstall entries found"
    }
    return @($entries | Select-Object -First 1)[0]
}

function Resolve-UninstallExecutable([string]$command) {
    if (-not $command) { throw "WebCodex uninstall command is missing" }
    $match = [regex]::Match($command, '^\s*"([^"]+)"')
    if ($match.Success) { return $match.Groups[1].Value }
    return ($command -split '\s+', 2)[0]
}

function Resolve-RegistryPath([string]$value, [string]$field) {
    if (-not $value) { throw "WebCodex $field is missing" }
    $trimmed = $value.Trim()
    if ($trimmed.StartsWith('"') -or $trimmed.EndsWith('"')) {
        if (-not ($trimmed.StartsWith('"') -and $trimmed.EndsWith('"') -and $trimmed.Length -ge 2)) {
            throw "WebCodex $field has malformed quoting"
        }
        $trimmed = $trimmed.Substring(1, $trimmed.Length - 2)
    }
    if (-not $trimmed -or $trimmed.Contains('"')) {
        throw "WebCodex $field is not one executable-system path"
    }
    return [System.IO.Path]::GetFullPath($trimmed)
}

function Get-VersionLine([string]$binary, [string]$name) {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $binary
    $start.Arguments = "--version"
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) {
            throw "$name.exe --version could not start after Desktop install"
        }
        $stdout = $process.StandardOutput.ReadToEnd()
        $null = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) {
            throw "$name.exe --version failed after Desktop install with exit code $($process.ExitCode)"
        }
        $lines = @($stdout -split "`r?`n" | Where-Object { $_ -ne "" })
        if ($lines.Count -ne 1) {
            throw "$name.exe --version returned an unexpected line count after Desktop install"
        }
        return $lines[0].TrimEnd()
    } finally {
        $process.Dispose()
    }
}

function Get-PeMachine([string]$binary) {
    $bytes = [System.IO.File]::ReadAllBytes($binary)
    if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
        throw "installed Desktop executable is not a PE image: $binary"
    }
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    if ($peOffset -lt 0 -or $peOffset -gt $bytes.Length - 6) {
        throw "installed Desktop executable has an invalid PE header offset: $binary"
    }
    if (
        $bytes[$peOffset] -ne 0x50 -or
        $bytes[$peOffset + 1] -ne 0x45 -or
        $bytes[$peOffset + 2] -ne 0x00 -or
        $bytes[$peOffset + 3] -ne 0x00
    ) {
        throw "installed Desktop executable has an invalid PE signature: $binary"
    }
    return [BitConverter]::ToUInt16($bytes, $peOffset + 4)
}

function Wait-Until([scriptblock]$Condition, [int]$Seconds, [string]$Failure) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw $Failure
}

if (Get-WebCodexUninstallEntry) {
    throw "refusing Desktop installer smoke because WebCodex Desktop is already installed for this user"
}

$installedDir = $null
$uninstaller = $null
$installed = $false
try {
    # NSIS requires /D= to be the final argument. Its value intentionally consumes
    # the remainder of the command line, so a path containing spaces is passed
    # literally without shell-style quoting or escaping.
    $installArguments = @("/S")
    if ($requestedInstallDir) {
        $installArguments += "/D=$requestedInstallDir"
    }
    $installProcess = Start-Process -FilePath $Installer -ArgumentList $installArguments -Wait -PassThru
    if ($installProcess.ExitCode -ne 0) {
        throw "Desktop silent install failed with exit code $($installProcess.ExitCode)"
    }
    Wait-Until { $null -ne (Get-WebCodexUninstallEntry) } 30 "Desktop installer did not register a current-user uninstall entry"
    $installed = $true

    $entry = Get-WebCodexUninstallEntry
    $uninstaller = Resolve-UninstallExecutable ([string]$entry.UninstallString)
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
        throw "registered WebCodex uninstaller does not exist: $uninstaller"
    }
    $installedDir = if ($entry.InstallLocation) {
        Resolve-RegistryPath ([string]$entry.InstallLocation) "InstallLocation"
    } else {
        Split-Path -Parent ([System.IO.Path]::GetFullPath($uninstaller))
    }
    if ($requestedInstallDir) {
        $actualInstallDir = $installedDir.TrimEnd('\')
        if (-not [string]::Equals($actualInstallDir, $requestedInstallDir, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "custom Desktop InstallLocation mismatch: expected '$requestedInstallDir', got '$installedDir'"
        }
    }

    $desktopExe = Join-Path $installedDir "WebCodex.exe"
    if (-not (Test-Path -LiteralPath $desktopExe -PathType Leaf)) {
        throw "installed WebCodex Desktop executable is missing: $desktopExe"
    }
    $expectedDesktopMachine = if ($Platform -eq "win32-x64") { 0x8664 } else { 0xAA64 }
    $actualDesktopMachine = Get-PeMachine $desktopExe
    if ($actualDesktopMachine -ne $expectedDesktopMachine) {
        throw ("installed WebCodex Desktop architecture mismatch: expected 0x{0:x4}, got 0x{1:x4}" -f $expectedDesktopMachine, $actualDesktopMachine)
    }
    $runtimeDir = Join-Path $installedDir "webcodex-runtime"
    if (-not (Test-Path -LiteralPath $runtimeDir -PathType Container)) {
        throw "installed bundled runtime directory is missing: $runtimeDir"
    }

    $shortSource = $SourceSha.Substring(0, 12).ToLowerInvariant()
    foreach ($name in @("webcodex", "webcodex-server", "webcodex-runner")) {
        $binary = Join-Path $runtimeDir "$name.exe"
        if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
            throw "installed bundled binary is missing: $binary"
        }
        $line = Get-VersionLine $binary $name
        $expected = "$name $Version (commit $shortSource, dirty=false, built_at=$BuiltAt)"
        if ($line -ne $expected) {
            throw "unexpected installed $name.exe identity: '$line' (expected '$expected')"
        }
    }

    Write-Output "Desktop install smoke passed: $installedDir"
    Write-Output "Bundled runtime: $runtimeDir"
} finally {
    if ($installed) {
        if (-not $uninstaller) {
            $entry = Get-WebCodexUninstallEntry
            if ($entry) { $uninstaller = Resolve-UninstallExecutable ([string]$entry.UninstallString) }
        }
        if ($uninstaller -and (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
            if (-not $installedDir) {
                throw "WebCodex install directory is unknown before silent uninstall"
            }
            # NSIS normally copies the uninstaller to a temporary directory and exits the
            # original process. `_?=$INSTDIR` keeps the real uninstall in this process so
            # `-Wait` is authoritative; the harness then removes only the now-unlocked
            # uninstaller that this NSIS wait mode intentionally cannot self-delete.
            $uninstallProcess = Start-Process -FilePath $uninstaller -ArgumentList "/S _?=$installedDir" -Wait -PassThru
            if ($uninstallProcess.ExitCode -ne 0) {
                throw "Desktop silent uninstall failed with exit code $($uninstallProcess.ExitCode)"
            }
            Wait-Until { $null -eq (Get-WebCodexUninstallEntry) } 30 "Desktop uninstall entry remained after silent uninstall"

            $desktopExe = Join-Path $installedDir "WebCodex.exe"
            $runtimeDir = Join-Path $installedDir "webcodex-runtime"
            Wait-Until {
                -not (Test-Path -LiteralPath $desktopExe) -and
                -not (Test-Path -LiteralPath $runtimeDir)
            } 30 "Desktop installer-owned payload remained after silent uninstall: $installedDir"

            if (Test-Path -LiteralPath $installedDir -PathType Container) {
                $remaining = @(
                    Get-ChildItem -LiteralPath $installedDir -Force -ErrorAction SilentlyContinue |
                        Where-Object { $_.FullName -ne $uninstaller }
                )
                if ($remaining.Count -ne 0) {
                    throw "Desktop installer-owned files remained after silent uninstall: $($remaining.Name -join ', ')"
                }
                if (Test-Path -LiteralPath $uninstaller -PathType Leaf) {
                    Remove-Item -LiteralPath $uninstaller -Force
                }
                Remove-Item -LiteralPath $installedDir -Force
            }
            if (Test-Path -LiteralPath $installedDir) {
                throw "Desktop install directory remained after deterministic uninstall cleanup: $installedDir"
            }
        }
    }
}

Write-Output "Desktop uninstall smoke passed"
