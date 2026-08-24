$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows_runner_lifecycle.ps1')

function Assert-True($Value, [string]$Message) {
    if (-not $Value) { throw $Message }
}
function Assert-False($Value, [string]$Message) {
    if ($Value) { throw $Message }
}
function Assert-Equal($Expected, $Actual, [string]$Message) {
    if ($Expected -ne $Actual) { throw "$Message (expected=$Expected actual=$Actual)" }
}
function Assert-Throws([scriptblock]$Action, [string]$Pattern) {
    try { & $Action; throw "Expected failure matching '$Pattern'" }
    catch {
        if ($_.Exception.Message -notmatch $Pattern) { throw }
        return $_.Exception.Message
    }
}
function Assert-HasMismatch($Items, [string]$Field, [string]$Message) {
    if (@($Items | Where-Object { $_.field -eq $Field }).Count -ne 1) { throw $Message }
}
function New-TestTaskObservation {
    param(
        [Parameter(Mandatory = $true)]$Expected,
        [bool]$Exists = $true,
        [bool]$Enabled = $true,
        [string]$State = 'Running',
        [int]$ActionCount = 1,
        [string]$ActionExecutable,
        [string]$ActionArguments,
        [string]$WorkingDirectory,
        [bool]$IsLifecycleLike = $true
    )
    if (-not $Exists) {
        return [pscustomobject]@{
            Exists=$false; Enabled=$false; State='Missing'; ActionCount=0; ActionExecutable=$null; ActionArguments=$null
            WorkingDirectory=$null; SupervisorPath=$null; IsLifecycleLike=$false
            PrincipalSid=$null; PrincipalLogonType=$null; PrincipalRunLevel=$null
            TriggerCount=0; TriggerType=$null; TriggerSid=$null; TriggerEnabled=$false
            MultipleInstances=$null; RestartCount=$null; RestartInterval=$null; ExecutionTimeLimit=$null; StartWhenAvailable=$false
        }
    }
    if (-not $PSBoundParameters.ContainsKey('ActionExecutable')) { $ActionExecutable = $Expected.ActionExecutable }
    if (-not $PSBoundParameters.ContainsKey('ActionArguments')) { $ActionArguments = $Expected.ActionArguments }
    if (-not $PSBoundParameters.ContainsKey('WorkingDirectory')) { $WorkingDirectory = $Expected.WorkingDirectory }
    return [pscustomobject]@{
        Exists=$true; Enabled=$Enabled; State=$State; ActionCount=$ActionCount
        ActionExecutable=$ActionExecutable; ActionArguments=$ActionArguments
        WorkingDirectory=$WorkingDirectory; SupervisorPath=$Expected.SupervisorPath
        IsLifecycleLike=$IsLifecycleLike
        PrincipalSid=$Expected.PrincipalSid; PrincipalLogonType=$Expected.PrincipalLogonType; PrincipalRunLevel=$Expected.PrincipalRunLevel
        TriggerCount=$Expected.TriggerCount; TriggerType=$Expected.TriggerType; TriggerSid=$Expected.TriggerSid; TriggerEnabled=$Expected.TriggerEnabled
        MultipleInstances=$Expected.MultipleInstances; RestartCount=$Expected.RestartCount; RestartInterval=$Expected.RestartInterval
        ExecutionTimeLimit=$Expected.ExecutionTimeLimit; StartWhenAvailable=$Expected.StartWhenAvailable
    }
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('webcodex-lifecycle-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tempRoot | Out-Null
try {
    $runnerPath = Join-Path $tempRoot 'webcodex-runner.exe'
    $configPath = Join-Path $tempRoot 'agent.toml'
    $supervisorPath = Join-Path $tempRoot 'runner supervisor.ps1'
    $workingDirectory = Join-Path $tempRoot 'work'
    $secret = 'wc_pat_lifecycle_must_not_leak_0123456789'
    New-Item -ItemType File -Path $runnerPath | Out-Null
    Set-Content -LiteralPath $configPath -Encoding UTF8 -Value @('client_id = "msi-test"', ('token = "{0}"' -f $secret))
    Set-Content -LiteralPath $supervisorPath -Encoding UTF8 -Value '# fixture supervisor'
    New-Item -ItemType Directory -Path $workingDirectory | Out-Null

    # A. Deterministic install plan and idempotence.
    $expected = New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath $configPath -SupervisorPath $supervisorPath -TaskName 'WebCodex Test Runner' -WorkingDirectory $workingDirectory
    $expectedAgain = New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath $configPath -SupervisorPath $supervisorPath -TaskName 'WebCodex Test Runner' -WorkingDirectory $workingDirectory
    Assert-Equal ($expected | ConvertTo-Json -Compress) ($expectedAgain | ConvertTo-Json -Compress) 'fresh lifecycle spec was not deterministic'
    Assert-Equal $expected.SupervisorPath (Get-PowerShellFileArgument -Arguments $expected.ActionArguments) 'supported supervisor argv was not recognized'
    Assert-Equal $null (Get-PowerShellFileArgument -Arguments ($expected.ActionArguments + ' --token ' + $secret)) 'extra potentially-secret supervisor argv was treated as safe'

    $definition = New-WindowsRunnerScheduledTaskDefinition -ExpectedSpec $expected
    Assert-Equal 1 @($definition.Actions).Count 'task definition action count changed'
    Assert-Equal $expected.ActionExecutable ([string]$definition.Actions[0].Execute) 'task definition executable changed'
    Assert-Equal $expected.ActionArguments ([string]$definition.Actions[0].Arguments) 'task definition arguments changed'
    Assert-Equal $expected.WorkingDirectory ([string]$definition.Actions[0].WorkingDirectory) 'task definition working directory changed'
    Assert-Equal 1 @($definition.Triggers).Count 'task definition trigger count changed'
    Assert-Equal 'MSFT_TaskLogonTrigger' ([string]$definition.Triggers[0].CimClass.CimClassName) 'task definition trigger type changed'
    Assert-Equal 'Interactive' ([string]$definition.Principal.LogonType) 'task definition logon type changed'
    Assert-Equal 'Highest' ([string]$definition.Principal.RunLevel) 'task definition run level changed'
    Assert-Equal 20 ([int]$definition.Settings.RestartCount) 'task definition restart count changed'
    Assert-Equal 'PT1M' ([string]$definition.Settings.RestartInterval) 'task definition restart interval changed'
    Assert-Equal 'PT0S' ([string]$definition.Settings.ExecutionTimeLimit) 'task definition execution limit changed'
    Assert-Equal 'IgnoreNew' ([string]$definition.Settings.MultipleInstances) 'task definition multiple-instance policy changed'
    Assert-True ([bool]$definition.Settings.StartWhenAvailable) 'task definition StartWhenAvailable changed'

    $missingTask = New-TestTaskObservation -Expected $expected -Exists $false
    $freshPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $missingTask -PrimaryInventory @()
    Assert-Equal 'create' $freshPlan.task_operation 'missing task did not produce create plan'
    Assert-True $freshPlan.can_apply 'fresh create plan was unexpectedly blocked'

    $unsupervisedPrimary = [pscustomobject]@{ pid=77; process_creation_filetime=[uint64]88; normalized_executable_path=$expected.RunnerPath; runner_config_path=$expected.RunnerConfigPath; role='primary' }
    $unsafeFreshPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $missingTask -PrimaryInventory @($unsupervisedPrimary)
    Assert-HasMismatch $unsafeFreshPlan.runtime_mismatches 'unsupervised_primary_runner_count' 'fresh task plan ignored an already-running unsupervised primary'
    Assert-False $unsafeFreshPlan.can_apply 'fresh task plan could create beside an unsupervised primary'

    $exactTask = New-TestTaskObservation -Expected $expected
    $exactPrimary = [pscustomobject]@{ pid=101; process_creation_filetime=[uint64]202; normalized_executable_path=$expected.RunnerPath; runner_config_path=$expected.RunnerConfigPath; role='primary' }
    $exactPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $exactTask -PrimaryInventory @($exactPrimary)
    Assert-Equal 'noop' $exactPlan.task_operation 'repeated same lifecycle config was not idempotent'
    Assert-True $exactPlan.idempotent_noop 'exact lifecycle did not report idempotent noop'

    $actionMismatch = New-TestTaskObservation -Expected $expected -ActionArguments '-NoProfile -File "C:\wrong\supervisor.ps1"'
    $actionPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $actionMismatch -PrimaryInventory @($exactPrimary)
    Assert-Equal 'update' $actionPlan.task_operation 'action mismatch did not require update'
    Assert-HasMismatch $actionPlan.task_mismatches 'task_action_arguments' 'action mismatch was not reported'

    $executableMismatch = New-TestTaskObservation -Expected $expected -ActionExecutable 'C:\\unexpected\\powershell.exe'
    $executablePlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $executableMismatch -PrimaryInventory @($exactPrimary)
    Assert-HasMismatch $executablePlan.task_mismatches 'task_action_executable' 'action executable mismatch was not reported'

    $unrelatedTask = New-TestTaskObservation -Expected $expected -IsLifecycleLike $false
    $unrelatedPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $unrelatedTask -PrimaryInventory @($exactPrimary)
    Assert-False $unrelatedPlan.can_apply 'unrecognized existing task was allowed to update'
    Assert-HasMismatch $unrelatedPlan.task_mismatches 'task_identity' 'unrecognized task mismatch was not reported'

    $otherWorking = Join-Path $tempRoot 'other-work'
    New-Item -ItemType Directory -Path $otherWorking | Out-Null
    $workingMismatch = New-TestTaskObservation -Expected $expected -WorkingDirectory $otherWorking
    $workingPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $workingMismatch -PrimaryInventory @($exactPrimary)
    Assert-HasMismatch $workingPlan.task_mismatches 'task_working_directory' 'working-directory mismatch was not reported'

    $settingsMismatch = New-TestTaskObservation -Expected $expected
    $settingsMismatch.RestartCount = 1
    $settingsPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $settingsMismatch -PrimaryInventory @($exactPrimary)
    Assert-HasMismatch $settingsPlan.task_mismatches 'task_restart_count' 'owned restart setting mismatch was not reported'

    $otherRunner = Join-Path $tempRoot 'other\webcodex-runner.exe'
    New-Item -ItemType Directory -Path (Split-Path -Parent $otherRunner) | Out-Null
    New-Item -ItemType File -Path $otherRunner | Out-Null
    $wrongPrimary = [pscustomobject]@{ pid=303; process_creation_filetime=[uint64]404; normalized_executable_path=[System.IO.Path]::GetFullPath($otherRunner); runner_config_path=$expected.RunnerConfigPath; role='primary' }
    $runnerPathPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $exactTask -PrimaryInventory @($wrongPrimary)
    Assert-HasMismatch $runnerPathPlan.runtime_mismatches 'runner_path' 'Runner path mismatch was not reported'
    Assert-False $runnerPathPlan.can_apply 'running wrong-path lifecycle was allowed to apply silently'

    $planText = $exactPlan | ConvertTo-Json -Depth 8 -Compress
    Assert-False $planText.Contains($secret) 'lifecycle plan leaked config secret contents'

    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath (Join-Path $tempRoot 'missing.exe') -RunnerConfigPath $configPath -SupervisorPath $supervisorPath -TaskName 'WebCodex Test Runner' -WorkingDirectory $workingDirectory } 'does not exist|Cannot find path'
    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath (Join-Path $tempRoot 'missing.toml') -SupervisorPath $supervisorPath -TaskName 'WebCodex Test Runner' -WorkingDirectory $workingDirectory } 'does not exist|Cannot find path'
    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath $configPath -SupervisorPath (Join-Path $tempRoot 'missing.ps1') -TaskName 'WebCodex Test Runner' -WorkingDirectory $workingDirectory } 'does not exist|Cannot find path'
    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath $configPath -SupervisorPath $supervisorPath -TaskName 'WebCodex Test Runner' -WorkingDirectory (Join-Path $tempRoot 'missing-dir') } 'does not exist|Cannot find path'
    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath $configPath -SupervisorPath $supervisorPath -TaskName 'Unrelated Task' -WorkingDirectory $workingDirectory } 'TaskName must identify a WebCodex task'

    # B. Status projection preserves exact PID+creation identity and handles cardinality/task state.
    $statusOne = New-WindowsRunnerLifecycleStatusProjection -ExpectedRunnerPath $expected.RunnerPath -TaskObservation $exactTask -PrimaryInventory @($exactPrimary)
    Assert-Equal 1 $statusOne.primary_runner_count 'exactly-one primary status count changed'
    Assert-Equal 101 $statusOne.primary_runners[0].pid 'status lost primary PID'
    Assert-Equal ([uint64]202) $statusOne.primary_runners[0].process_creation_filetime 'status lost creation FILETIME'
    Assert-Equal 'primary' $statusOne.primary_runners[0].role 'status role changed'

    $statusZero = New-WindowsRunnerLifecycleStatusProjection -ExpectedRunnerPath $expected.RunnerPath -TaskObservation $exactTask -PrimaryInventory @()
    Assert-Equal 0 $statusZero.primary_runner_count 'zero-primary status changed'
    $secondPrimary = [pscustomobject]@{ pid=505; process_creation_filetime=[uint64]606; normalized_executable_path=$expected.RunnerPath; runner_config_path=$expected.RunnerConfigPath; role='primary' }
    $statusMany = New-WindowsRunnerLifecycleStatusProjection -ExpectedRunnerPath $expected.RunnerPath -TaskObservation $exactTask -PrimaryInventory @($exactPrimary,$secondPrimary)
    Assert-Equal 2 $statusMany.primary_runner_count 'multiple-primary status changed'

    Assert-False (Test-PrimaryRunnerArguments -Arguments @('webcodex-runner.exe','--webcodex-internal-detached-supervisor','x')) 'canonical internal role classification regressed'
    $disabledTask = New-TestTaskObservation -Expected $expected -Enabled $false -State 'Disabled'
    $statusDisabled = New-WindowsRunnerLifecycleStatusProjection -ExpectedRunnerPath $expected.RunnerPath -TaskObservation $disabledTask -PrimaryInventory @()
    Assert-False $statusDisabled.task_enabled 'disabled task was reported enabled'
    $statusMissing = New-WindowsRunnerLifecycleStatusProjection -ExpectedRunnerPath $expected.RunnerPath -TaskObservation $missingTask -PrimaryInventory @()
    Assert-False $statusMissing.task_exists 'missing task was reported present'
    Assert-Equal 'Missing' $statusMissing.task_state 'missing task state changed'

    # C. Dogfood-first path resolution.
    $repoRoot = Join-Path $tempRoot 'repo'
    $dogfoodDir = Join-Path $repoRoot 'target\dogfood'
    New-Item -ItemType Directory -Path $dogfoodDir -Force | Out-Null
    $dogfoodCli = Join-Path $dogfoodDir 'webcodex.exe'
    $dogfoodRunner = Join-Path $dogfoodDir 'webcodex-runner.exe'
    $explicitCli = Join-Path $tempRoot 'explicit-webcodex.exe'
    $explicitRunner = Join-Path $tempRoot 'explicit-runner.exe'
    $installedCli = Join-Path $tempRoot 'installed-webcodex.exe'
    foreach ($path in @($dogfoodCli,$dogfoodRunner,$explicitCli,$explicitRunner,$installedCli)) { New-Item -ItemType File -Path $path | Out-Null }
    $supportAll = { param($Path) return $true }

    $cliSelection = Resolve-WebCodexOperatorCliPath -ExplicitPath $explicitCli -RepoRoot $repoRoot -InstalledPath $installedCli -SupportsOpsRunner $supportAll
    Assert-Equal 'explicit' $cliSelection.Source 'explicit CLI path did not win'
    Assert-Equal ([System.IO.Path]::GetFullPath($explicitCli)) $cliSelection.Path 'explicit CLI path changed'

    $cliSelection = Resolve-WebCodexOperatorCliPath -RepoRoot $repoRoot -InstalledPath $installedCli -SupportsOpsRunner $supportAll
    Assert-Equal 'repo_dogfood' $cliSelection.Source 'repo dogfood CLI was not preferred'
    Assert-Equal ([System.IO.Path]::GetFullPath($dogfoodCli)) $cliSelection.Path 'repo dogfood CLI path changed'

    Remove-Item -LiteralPath $dogfoodCli -Force
    $supportNone = { param($Path) return $false }
    $null = Assert-Throws { Resolve-WebCodexOperatorCliPath -RepoRoot $repoRoot -InstalledPath $installedCli -SupportsOpsRunner $supportNone } 'stale or unsupported.*ops runner'

    $candidateSelection = Resolve-WebCodexRunnerCandidatePath -ExplicitPath $explicitRunner -RepoRoot $repoRoot
    Assert-Equal 'explicit' $candidateSelection.Source 'explicit Runner candidate did not win'
    $candidateSelection = Resolve-WebCodexRunnerCandidatePath -RepoRoot $repoRoot
    Assert-Equal 'repo_dogfood' $candidateSelection.Source 'repo dogfood Runner candidate was not preferred'

    Write-Output 'Windows Runner lifecycle focused tests passed.'
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
