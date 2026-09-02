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
        $message = [string]$_.Exception.Message
        $errorId = [string]$_.FullyQualifiedErrorId
        if ($message -notmatch $Pattern -and $errorId -notmatch $Pattern) { throw }
        return $message
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
    $configPath = Join-Path $tempRoot 'runner.toml'
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
    Assert-Equal 'Limited' $expected.PrincipalRunLevel 'first-class lifecycle must use the least-privileged Scheduled Task run level'
    Assert-Equal $expected.SupervisorPath (Get-PowerShellFileArgument -Arguments $expected.ActionArguments) 'supported supervisor argv was not recognized'
    Assert-Equal $expected.SupervisorPath (Get-PowerShellLifecycleSupervisorArgument -Arguments $expected.ActionArguments) 'canonical supervisor ownership argv was not recognized'
    Assert-Equal $null (Get-PowerShellFileArgument -Arguments ($expected.ActionArguments + ' --token ' + $secret)) 'extra potentially-secret supervisor argv was treated as safe'
    Assert-Equal $null (Get-PowerShellLifecycleSupervisorArgument -Arguments ($expected.ActionArguments + ' --token ' + $secret)) 'extra supervisor argv established lifecycle ownership'
    Assert-True (Test-WindowsRunnerLifecycleSupervisorOwnership -ObservedSupervisorPath $expected.SupervisorPath -ExpectedSupervisorPath $expected.SupervisorPath) 'exact expected supervisor path did not establish lifecycle ownership'

    $differentSupervisorPath = Join-Path $tempRoot 'different supervisor.ps1'
    $webCodexBackupSupervisorPath = Join-Path $tempRoot 'webcodex-backup.ps1'
    Set-Content -LiteralPath $differentSupervisorPath -Encoding UTF8 -Value '# different fixture supervisor'
    Set-Content -LiteralPath $webCodexBackupSupervisorPath -Encoding UTF8 -Value '# unrelated webcodex-named fixture supervisor'
    Assert-False (Test-WindowsRunnerLifecycleSupervisorOwnership -ObservedSupervisorPath $differentSupervisorPath -ExpectedSupervisorPath $expected.SupervisorPath) 'different supervisor path established lifecycle ownership'
    Assert-False (Test-WindowsRunnerLifecycleSupervisorOwnership -ObservedSupervisorPath $webCodexBackupSupervisorPath -ExpectedSupervisorPath $expected.SupervisorPath) 'path merely containing webcodex established lifecycle ownership'

    $definition = New-WindowsRunnerScheduledTaskDefinition -ExpectedSpec $expected
    Assert-Equal 1 @($definition.Actions).Count 'task definition action count changed'
    Assert-Equal $expected.ActionExecutable ([string]$definition.Actions[0].Execute) 'task definition executable changed'
    Assert-Equal $expected.ActionArguments ([string]$definition.Actions[0].Arguments) 'task definition arguments changed'
    Assert-Equal $expected.WorkingDirectory ([string]$definition.Actions[0].WorkingDirectory) 'task definition working directory changed'
    Assert-Equal 1 @($definition.Triggers).Count 'task definition trigger count changed'
    Assert-Equal 'MSFT_TaskLogonTrigger' ([string]$definition.Triggers[0].CimClass.CimClassName) 'task definition trigger type changed'
    Assert-Equal 'Interactive' ([string]$definition.Principal.LogonType) 'task definition logon type changed'
    Assert-Equal 'Limited' ([string]$definition.Principal.RunLevel) 'task definition run level changed'
    Assert-Equal 20 ([int]$definition.Settings.RestartCount) 'task definition restart count changed'
    Assert-Equal 'PT1M' ([string]$definition.Settings.RestartInterval) 'task definition restart interval changed'
    Assert-Equal 'PT0S' ([string]$definition.Settings.ExecutionTimeLimit) 'task definition execution limit changed'
    Assert-Equal 'IgnoreNew' ([string]$definition.Settings.MultipleInstances) 'task definition multiple-instance policy changed'
    Assert-True ([bool]$definition.Settings.StartWhenAvailable) 'task definition StartWhenAvailable changed'

    $missingTask = New-TestTaskObservation -Expected $expected -Exists $false
    $freshPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $missingTask -PrimaryInventory @()
    Assert-Equal 'create' $freshPlan.task_operation 'missing task did not produce create plan'
    Assert-True $freshPlan.can_apply 'fresh create plan was unexpectedly blocked'
    $null = Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $freshPlan -ExpectedOperation 'create'

    $unsupervisedPrimary = [pscustomobject]@{ pid=77; process_creation_filetime=[uint64]88; normalized_executable_path=$expected.RunnerPath; runner_config_path=$expected.RunnerConfigPath; role='primary' }
    $unsafeFreshPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $missingTask -PrimaryInventory @($unsupervisedPrimary)
    Assert-HasMismatch $unsafeFreshPlan.runtime_mismatches 'unsupervised_primary_runner_count' 'fresh task plan ignored an already-running unsupervised primary'
    Assert-False $unsafeFreshPlan.can_apply 'fresh task plan could create beside an unsupervised primary'
    $null = Assert-Throws { Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $unsafeFreshPlan -ExpectedOperation 'create' } 'changed before create effect'

    $exactTask = New-TestTaskObservation -Expected $expected
    $exactPrimary = [pscustomobject]@{ pid=101; process_creation_filetime=[uint64]202; normalized_executable_path=$expected.RunnerPath; runner_config_path=$expected.RunnerConfigPath; role='primary' }
    $exactPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $exactTask -PrimaryInventory @($exactPrimary)
    Assert-Equal 'noop' $exactPlan.task_operation 'repeated same lifecycle config was not idempotent'
    Assert-True $exactPlan.idempotent_noop 'exact lifecycle did not report idempotent noop'
    Assert-True $exactPlan.can_apply 'exact expected-supervisor lifecycle was not updateable'

    $highestTask = New-TestTaskObservation -Expected $expected
    $highestTask.PrincipalRunLevel = 'Highest'
    $highestPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $highestTask -PrimaryInventory @($exactPrimary)
    Assert-Equal 'update' $highestPlan.task_operation 'legacy Highest lifecycle did not require a least-privilege update'
    Assert-True $highestPlan.can_apply 'owned legacy Highest lifecycle could not be migrated to Limited'
    Assert-HasMismatch $highestPlan.task_mismatches 'task_principal_run_level' 'Highest-to-Limited privilege drift was not reported'
    Assert-Equal 'Limited' $highestPlan.expected_principal_run_level 'plan did not expose the least-privileged expected run level'

    $differentSupervisorTask = New-TestTaskObservation -Expected $expected
    $differentSupervisorTask.SupervisorPath = [System.IO.Path]::GetFullPath($differentSupervisorPath)
    $differentSupervisorTask.ActionArguments = Get-WindowsRunnerSupervisorArguments -SupervisorPath $differentSupervisorTask.SupervisorPath
    $differentSupervisorTask.IsLifecycleLike = $false
    $differentSupervisorPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $differentSupervisorTask -PrimaryInventory @($exactPrimary)
    Assert-False $differentSupervisorPlan.can_apply 'different valid supervisor path was allowed to overwrite an existing task'
    Assert-HasMismatch $differentSupervisorPlan.task_mismatches 'task_identity' 'different supervisor ownership did not report task identity mismatch'
    Assert-HasMismatch $differentSupervisorPlan.task_mismatches 'task_supervisor_path' 'different supervisor ownership did not report supervisor mismatch'

    $webCodexBackupTask = New-TestTaskObservation -Expected $expected
    $webCodexBackupTask.SupervisorPath = [System.IO.Path]::GetFullPath($webCodexBackupSupervisorPath)
    $webCodexBackupTask.ActionArguments = Get-WindowsRunnerSupervisorArguments -SupervisorPath $webCodexBackupTask.SupervisorPath
    $webCodexBackupTask.IsLifecycleLike = $false
    $webCodexBackupPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $webCodexBackupTask -PrimaryInventory @($exactPrimary)
    Assert-False $webCodexBackupPlan.can_apply 'webcodex-named unrelated supervisor path established ownership'

    $driftArguments = $expected.ActionArguments.Replace('-WindowStyle Hidden', '-WindowStyle Normal')
    Assert-Equal $null (Get-PowerShellFileArgument -Arguments $driftArguments) 'noncanonical action drift was treated as safe for raw projection'
    $driftSupervisor = Get-PowerShellLifecycleSupervisorArgument -Arguments $driftArguments
    Assert-Equal $expected.SupervisorPath $driftSupervisor 'reconcilable action drift lost exact supervisor ownership'
    $actionMismatch = New-TestTaskObservation -Expected $expected -ActionArguments '<unrecognized>'
    $actionMismatch.SupervisorPath = $driftSupervisor
    $actionMismatch.IsLifecycleLike = Test-WindowsRunnerLifecycleSupervisorOwnership -ObservedSupervisorPath $driftSupervisor -ExpectedSupervisorPath $expected.SupervisorPath
    $actionPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $actionMismatch -PrimaryInventory @($exactPrimary)
    Assert-Equal 'update' $actionPlan.task_operation 'action mismatch did not require update'
    Assert-True $actionPlan.can_apply 'same-supervisor action drift was not safely reconcilable'
    Assert-HasMismatch $actionPlan.task_mismatches 'task_action_arguments' 'action mismatch was not reported'
    $null = Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $actionPlan -ExpectedOperation 'update'
    $null = Assert-Throws { Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $exactPlan -ExpectedOperation 'update' } 'changed before update effect'
    $null = Assert-Throws { Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $differentSupervisorPlan -ExpectedOperation 'update' } 'changed before update effect'

    $executableMismatch = New-TestTaskObservation -Expected $expected -ActionExecutable 'C:\\unexpected\\powershell.exe'
    $executablePlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $executableMismatch -PrimaryInventory @($exactPrimary)
    Assert-HasMismatch $executablePlan.task_mismatches 'task_action_executable' 'action executable mismatch was not reported'

    $unrelatedTask = New-TestTaskObservation -Expected $expected -IsLifecycleLike $false
    $unrelatedPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $unrelatedTask -PrimaryInventory @($exactPrimary)
    Assert-False $unrelatedPlan.can_apply 'unrecognized existing task was allowed to update'
    Assert-HasMismatch $unrelatedPlan.task_mismatches 'task_identity' 'unrecognized task mismatch was not reported'
    Assert-Equal 'noop' $unrelatedPlan.task_operation 'unrecognized exact-definition task unexpectedly changed task operation'
    Assert-False $unrelatedPlan.idempotent_noop 'task_operation=noop masked failed ownership in idempotent convergence'
    $null = Assert-Throws { Assert-WindowsRunnerLifecycleSafeConvergence -Plan $unrelatedPlan } 'did not converge to a safe idempotent lifecycle state'

    $otherWorking = Join-Path $tempRoot 'other-work'
    New-Item -ItemType Directory -Path $otherWorking | Out-Null
    $workingMismatch = New-TestTaskObservation -Expected $expected -WorkingDirectory $otherWorking
    $workingPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $workingMismatch -PrimaryInventory @($exactPrimary)
    Assert-HasMismatch $workingPlan.task_mismatches 'task_working_directory' 'working-directory mismatch was not reported'

    $settingsMismatch = New-TestTaskObservation -Expected $expected
    $settingsMismatch.RestartCount = 1
    $settingsPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $settingsMismatch -PrimaryInventory @($exactPrimary)
    Assert-HasMismatch $settingsPlan.task_mismatches 'task_restart_count' 'owned restart setting mismatch was not reported'

    $runningDisabledRawTask = [pscustomobject]@{ State='Running'; Settings=[pscustomobject]@{ Enabled=$false } }
    $readyEnabledRawTask = [pscustomobject]@{ State='Ready'; Settings=[pscustomobject]@{ Enabled=$true } }
    Assert-False (Get-WindowsRunnerLifecycleTaskEnabled -Task $runningDisabledRawTask) 'Settings.Enabled=false was ignored while task state was Running'
    Assert-True (Get-WindowsRunnerLifecycleTaskEnabled -Task $readyEnabledRawTask) 'Settings.Enabled=true was ignored while task state was Ready'

    $runningDisabledTask = New-TestTaskObservation -Expected $expected -Enabled $false -State 'Running'
    $runningDisabledPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $runningDisabledTask -PrimaryInventory @($exactPrimary)
    Assert-False $runningDisabledTask.Enabled 'running disabled fixture lost Settings.Enabled authority'
    Assert-Equal 'enable' $runningDisabledPlan.task_operation 'running task with Settings.Enabled=false did not require enable'
    $null = Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $runningDisabledPlan -ExpectedOperation 'enable'
    $null = Assert-Throws { Assert-WindowsRunnerLifecycleEffectStillSafe -FreshPlan $exactPlan -ExpectedOperation 'enable' } 'changed before enable effect'

    $readyEnabledTask = New-TestTaskObservation -Expected $expected -Enabled $true -State 'Ready'
    $readyEnabledStatus = New-WindowsRunnerLifecycleStatusProjection -ExpectedRunnerPath $expected.RunnerPath -TaskObservation $readyEnabledTask -PrimaryInventory @()
    Assert-True $readyEnabledStatus.task_enabled 'ready task with Settings.Enabled=true was reported disabled'

    $otherRunner = Join-Path $tempRoot 'other\webcodex-runner.exe'
    New-Item -ItemType Directory -Path (Split-Path -Parent $otherRunner) | Out-Null
    New-Item -ItemType File -Path $otherRunner | Out-Null
    $wrongPrimary = [pscustomobject]@{ pid=303; process_creation_filetime=[uint64]404; normalized_executable_path=[System.IO.Path]::GetFullPath($otherRunner); runner_config_path=$expected.RunnerConfigPath; role='primary' }
    $runnerPathPlan = Get-WindowsRunnerLifecyclePlan -ExpectedSpec $expected -CurrentTask $exactTask -PrimaryInventory @($wrongPrimary)
    Assert-HasMismatch $runnerPathPlan.runtime_mismatches 'runner_path' 'Runner path mismatch was not reported'
    Assert-False $runnerPathPlan.can_apply 'running wrong-path lifecycle was allowed to apply silently'
    Assert-Equal 'noop' $runnerPathPlan.task_operation 'runtime mismatch unexpectedly changed task definition operation'
    Assert-False $runnerPathPlan.idempotent_noop 'task_operation=noop masked runtime mismatch in idempotent convergence'
    $null = Assert-Throws { Assert-WindowsRunnerLifecycleSafeConvergence -Plan $runnerPathPlan } 'did not converge to a safe idempotent lifecycle state'
    $null = Assert-WindowsRunnerLifecycleSafeConvergence -Plan $exactPlan

    $planText = $exactPlan | ConvertTo-Json -Depth 8 -Compress
    Assert-False $planText.Contains($secret) 'lifecycle plan leaked config secret contents'

    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath (Join-Path $tempRoot 'missing.exe') -RunnerConfigPath $configPath -SupervisorPath $supervisorPath -TaskName 'WebCodex Test Runner' -WorkingDirectory $workingDirectory } 'does not exist|Cannot find path|PathNotFound'
    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath (Join-Path $tempRoot 'missing.toml') -SupervisorPath $supervisorPath -TaskName 'WebCodex Test Runner' -WorkingDirectory $workingDirectory } 'does not exist|Cannot find path|PathNotFound'
    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath $configPath -SupervisorPath (Join-Path $tempRoot 'missing.ps1') -TaskName 'WebCodex Test Runner' -WorkingDirectory $workingDirectory } 'does not exist|Cannot find path|PathNotFound'
    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath $configPath -SupervisorPath $supervisorPath -TaskName 'WebCodex Test Runner' -WorkingDirectory (Join-Path $tempRoot 'missing-dir') } 'does not exist|Cannot find path|PathNotFound'
    $null = Assert-Throws { New-WindowsRunnerLifecycleExpectedSpec -RunnerPath $runnerPath -RunnerConfigPath $configPath -SupervisorPath $supervisorPath -TaskName 'Unrelated Task' -WorkingDirectory $workingDirectory } 'TaskName must identify a WebCodex task'

    # B. Status projection preserves exact PID+creation identity and handles cardinality/task state.
    $statusOne = New-WindowsRunnerLifecycleStatusProjection -ExpectedRunnerPath $expected.RunnerPath -TaskObservation $exactTask -PrimaryInventory @($exactPrimary)
    Assert-Equal 1 $statusOne.primary_runner_count 'exactly-one primary status count changed'
    Assert-Equal 101 $statusOne.primary_runners[0].pid 'status lost primary PID'
    Assert-Equal ([uint64]202) $statusOne.primary_runners[0].process_creation_filetime 'status lost creation FILETIME'
    Assert-Equal 'primary' $statusOne.primary_runners[0].role 'status role changed'
    Assert-Equal 'Limited' $statusOne.task_principal_run_level 'status did not expose the Scheduled Task run level'

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
