# Read-only status for the supported Windows Runner Scheduled Task lifecycle.
[CmdletBinding()]
param(
    [string]$RunnerPath = "$env:USERPROFILE\.local\bin\webcodex-runner.exe",
    [string]$TaskName = 'WebCodex MSI Dogfood Runner',
    [string]$TaskPath = '\',
    [switch]$Json
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows_runner_lifecycle.ps1')

$expectedRunnerPath = ConvertTo-WindowsRunnerLifecycleFullPath -Path $RunnerPath
$task = Get-WindowsRunnerLifecycleTaskObservation -TaskName $TaskName -TaskPath $TaskPath
$inventory = @(Get-WindowsRunnerPrimaryInventory)
$status = New-WindowsRunnerLifecycleStatusProjection `
    -ExpectedRunnerPath $expectedRunnerPath `
    -TaskObservation $task `
    -PrimaryInventory $inventory

if ($Json) {
    $status | ConvertTo-Json -Depth 8
} else {
    $status
}
