# Install or update the supported Windows Runner Scheduled Task lifecycle.
#
# This script never replaces the Runner binary and never reads token contents.
# Without -Apply it is a read-only plan renderer.
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$RunnerPath = "$env:USERPROFILE\.local\bin\webcodex-runner.exe",

    [Parameter(Mandatory = $true)]
    [string]$RunnerConfigPath,

    [Parameter(Mandatory = $true)]
    [string]$SupervisorPath,

    [string]$TaskName = 'WebCodex MSI Dogfood Runner',
    [string]$TaskPath = '\',
    [string]$WorkingDirectory = (Split-Path -Parent $PSScriptRoot),

    [switch]$Apply,
    [switch]$Json
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows_runner_lifecycle.ps1')

$expected = New-WindowsRunnerLifecycleExpectedSpec `
    -RunnerPath $RunnerPath `
    -RunnerConfigPath $RunnerConfigPath `
    -SupervisorPath $SupervisorPath `
    -TaskName $TaskName `
    -TaskPath $TaskPath `
    -WorkingDirectory $WorkingDirectory
$current = Get-WindowsRunnerLifecycleTaskObservation `
    -TaskName $expected.TaskName `
    -TaskPath $expected.TaskPath `
    -ExpectedSupervisorPath $expected.SupervisorPath
$inventory = @(Get-WindowsRunnerPrimaryInventory)
$plan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $current -PrimaryInventory $inventory

if ($Apply) {
    if (-not $plan.can_apply) {
        throw "Windows Runner lifecycle plan is not safe to apply; inspect task_mismatches/runtime_mismatches first"
    }

    $changed = $false
    switch ($plan.task_operation) {
        'create' {
            if ($PSCmdlet.ShouldProcess("$($expected.TaskPath)$($expected.TaskName)", 'Create WebCodex Windows Runner lifecycle Scheduled Task')) {
                $definition = New-WindowsRunnerScheduledTaskDefinition -ExpectedSpec $expected
                $freshCurrent = Get-WindowsRunnerLifecycleTaskObservation `
                    -TaskName $expected.TaskName `
                    -TaskPath $expected.TaskPath `
                    -ExpectedSupervisorPath $expected.SupervisorPath
                $freshInventory = @(Get-WindowsRunnerPrimaryInventory)
                $freshPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $freshCurrent -PrimaryInventory $freshInventory
                $null = Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $freshPlan -ExpectedOperation 'create'
                Register-ScheduledTask -TaskName $expected.TaskName -TaskPath $expected.TaskPath -InputObject $definition -ErrorAction Stop | Out-Null
                $changed = $true
            }
        }
        'update' {
            if (-not $current.IsLifecycleLike) {
                throw "Refusing to update an unrecognized existing Scheduled Task: $($expected.TaskPath)$($expected.TaskName)"
            }
            if ($PSCmdlet.ShouldProcess("$($expected.TaskPath)$($expected.TaskName)", 'Update WebCodex Windows Runner lifecycle Scheduled Task definition')) {
                $definition = New-WindowsRunnerScheduledTaskDefinition -ExpectedSpec $expected
                $freshCurrent = Get-WindowsRunnerLifecycleTaskObservation `
                    -TaskName $expected.TaskName `
                    -TaskPath $expected.TaskPath `
                    -ExpectedSupervisorPath $expected.SupervisorPath
                $freshInventory = @(Get-WindowsRunnerPrimaryInventory)
                $freshPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $freshCurrent -PrimaryInventory $freshInventory
                $null = Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $freshPlan -ExpectedOperation 'update'
                Register-ScheduledTask -TaskName $expected.TaskName -TaskPath $expected.TaskPath -InputObject $definition -Force -ErrorAction Stop | Out-Null
                $changed = $true
            }
        }
        'enable' {
            if ($PSCmdlet.ShouldProcess("$($expected.TaskPath)$($expected.TaskName)", 'Enable WebCodex Windows Runner lifecycle Scheduled Task')) {
                $freshCurrent = Get-WindowsRunnerLifecycleTaskObservation `
                    -TaskName $expected.TaskName `
                    -TaskPath $expected.TaskPath `
                    -ExpectedSupervisorPath $expected.SupervisorPath
                $freshInventory = @(Get-WindowsRunnerPrimaryInventory)
                $freshPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $freshCurrent -PrimaryInventory $freshInventory
                $null = Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $freshPlan -ExpectedOperation 'enable'
                Enable-ScheduledTask -TaskName $expected.TaskName -TaskPath $expected.TaskPath -ErrorAction Stop | Out-Null
                $changed = $true
            }
        }
        'noop' { }
        default { throw "Unknown lifecycle task operation: $($plan.task_operation)" }
    }

    if ($changed) {
        $afterTask = Get-WindowsRunnerLifecycleTaskObservation `
            -TaskName $expected.TaskName `
            -TaskPath $expected.TaskPath `
            -ExpectedSupervisorPath $expected.SupervisorPath
        $afterInventory = @(Get-WindowsRunnerPrimaryInventory)
        $plan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $afterTask -PrimaryInventory $afterInventory
        $null = Assert-WindowsRunnerLifecycleSafeConvergence -Plan $plan
    }

    $plan | Add-Member -NotePropertyName applied -NotePropertyValue $changed -Force
} else {
    $plan | Add-Member -NotePropertyName applied -NotePropertyValue $false -Force
}

if ($Json) {
    $plan | ConvertTo-Json -Depth 8
} else {
    $plan
}
