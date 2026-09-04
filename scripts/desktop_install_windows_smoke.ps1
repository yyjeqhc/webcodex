# Install, inspect, and uninstall one newly-built WebCodex Desktop NSIS package.
# The helper never kills by executable name and never removes WebCodex user state.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$SourceSha,
    [Parameter(Mandatory = $true)][Int64]$BuiltAt
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

function Get-WebCodexUninstallEntry {
    $root = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall"
    if (-not (Test-Path -LiteralPath $root)) { return $null }
    $entries = @(
        Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue |
            ForEach-Object { Get-ItemProperty -LiteralPath $_.PSPath -ErrorAction SilentlyContinue } |
            Where-Object { $_.DisplayName -eq "WebCodex" }
    )
    if ($entries.Count -gt 1) {
        throw "multiple current-user WebCodex uninstall entries found"
    }
    return @($entries | Select-Object -First 1)[0]
}

function Resolve-UninstallExecutable([string]$command) {
    if (-not $command) { throw "WebCodex uninstall command is missing" }
    $match = [regex]::Match($command, '^\s*"([^"]+)"')
    if ($match.Success) { return $match.Groups[1].Value }
    return ($command -split '\s+', 2)[0]
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
    throw "refusing Desktop installer smoke because WebCodex is already installed for this user"
}

$installedDir = $null
$uninstaller = $null
$installed = $false
try {
    $installProcess = Start-Process -FilePath $Installer -ArgumentList "/S" -Wait -PassThru
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
        [System.IO.Path]::GetFullPath([string]$entry.InstallLocation)
    } else {
        Split-Path -Parent ([System.IO.Path]::GetFullPath($uninstaller))
    }

    $desktopExe = Join-Path $installedDir "WebCodex.exe"
    if (-not (Test-Path -LiteralPath $desktopExe -PathType Leaf)) {
        throw "installed WebCodex Desktop executable is missing: $desktopExe"
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
        $line = @(& $binary --version | Select-Object -First 1)[0]
        if ($LASTEXITCODE -ne 0 -or -not $line) {
            throw "$name.exe --version failed after Desktop install"
        }
        $expected = "$name $Version (commit $shortSource, dirty=false, built_at=$BuiltAt)"
        if ($line.TrimEnd() -ne $expected) {
            throw "unexpected installed $name.exe identity: '$($line.TrimEnd())' (expected '$expected')"
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
            $uninstallProcess = Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait -PassThru
            if ($uninstallProcess.ExitCode -ne 0) {
                throw "Desktop silent uninstall failed with exit code $($uninstallProcess.ExitCode)"
            }
            Wait-Until { $null -eq (Get-WebCodexUninstallEntry) } 30 "Desktop uninstall entry remained after silent uninstall"
            if ($installedDir) {
                Wait-Until { -not (Test-Path -LiteralPath $installedDir) } 30 "Desktop install directory remained after silent uninstall: $installedDir"
            }
        }
    }
}

Write-Output "Desktop uninstall smoke passed"
