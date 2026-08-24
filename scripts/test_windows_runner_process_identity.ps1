$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "windows_runner_process_identity.ps1")

function Assert-True($Value, [string]$Message) {
    if (-not $Value) { throw $Message }
}
function Assert-False($Value, [string]$Message) {
    if ($Value) { throw $Message }
}
function Assert-Throws([scriptblock]$Action, [string]$Pattern) {
    try { & $Action; throw "Expected failure matching '$Pattern'" }
    catch {
        if ($_.Exception.Message -notmatch $Pattern) { throw }
    }
}

# Primary classification: ordinary argv is accepted.
Assert-True (Test-PrimaryRunnerArguments -Arguments @("webcodex-runner.exe", "--config", "runner.toml")) "normal Runner argv was not classified primary"

# Internal mode exclusion: both current modes and any future internal prefix fail closed.
Assert-False (Test-PrimaryRunnerArguments -Arguments @("webcodex-runner.exe", "--webcodex-internal-detached-supervisor", "x")) "detached supervisor was classified primary"
Assert-False (Test-PrimaryRunnerArguments -Arguments @("webcodex-runner.exe", "--webcodex-internal-detached-watchdog", "x")) "detached watchdog was classified primary"
Assert-False (Test-PrimaryRunnerArguments -Arguments @("webcodex-runner.exe", "--webcodex-internal-future-mode")) "future internal mode was classified primary"

# Same executable path is intentionally not sufficient: role comes from exact argv.
$normal = Test-PrimaryRunnerArguments -Arguments @("C:\same\webcodex-runner.exe")
$internal = Test-PrimaryRunnerArguments -Arguments @("C:\same\webcodex-runner.exe", "--webcodex-internal-detached-watchdog")
Assert-True ($normal -and -not $internal) "same executable path did not preserve role distinction"

# Windows command-line parsing is exact argv parsing rather than substring matching.
$parsed = @(ConvertFrom-WindowsCommandLine -CommandLine '"C:\Program Files\webcodex-runner.exe" --config "C:\Runner Config\runner.toml"')
Assert-True ($parsed.Count -eq 3 -and $parsed[1] -eq "--config" -and $parsed[2] -eq "C:\Runner Config\runner.toml") "Windows argv parsing contract failed"

# Deterministic creation-identity mismatch seam: live check must reject the current
# PID when the captured creation FILETIME is not the current creation FILETIME.
$currentPid = [uint32]$PID
$currentCreation = [WebCodex.WindowsProcessIdentity]::GetCreationTime($currentPid)
Assert-True ([WebCodex.WindowsProcessIdentity]::IsLive($currentPid, $currentCreation)) "current process exact identity should be live"
$differentCreation = if ($currentCreation -eq [uint64]::MaxValue) { [uint64]0 } else { $currentCreation + [uint64]1 }
Assert-False ([WebCodex.WindowsProcessIdentity]::IsLive($currentPid, $differentCreation)) "creation identity mismatch did not fail closed"
Assert-False ([WebCodex.WindowsProcessIdentity]::CreationIdentityMatches($currentCreation, $differentCreation)) "creation mismatch seam would allow an effect"
Assert-True ([WebCodex.WindowsProcessIdentity]::CreationIdentityMatches($currentCreation, $currentCreation)) "matching creation identity seam rejected exact identity"
Assert-Throws { [WebCodex.WindowsProcessIdentity]::TerminateExact($currentPid, $differentCreation) } "creation identity mismatch"
Assert-True ([WebCodex.WindowsProcessIdentity]::IsLive($currentPid, $currentCreation)) "mismatched termination attempt affected the current process"

Write-Output "Windows Runner process identity focused tests passed."
