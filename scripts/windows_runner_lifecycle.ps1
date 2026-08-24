# Focused Windows Runner lifecycle operator helpers.
#
# This module intentionally owns only the supported Scheduled Task topology:
#   Scheduled Task -> PowerShell supervisor -> WMI Win32_Process.Create -> Runner.
# The host-specific supervisor remains an explicit input and is not rewritten here.

if (-not (Get-Command Get-PrimaryRunnerProcesses -ErrorAction SilentlyContinue)) {
    . (Join-Path $PSScriptRoot "windows_runner_process_identity.ps1")
}

function Resolve-WindowsRunnerLifecycleExistingPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][ValidateSet('File','Directory')][string]$Kind,
        [Parameter(Mandatory = $true)][string]$Description,
        [switch]$RequireExe
    )

    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw "$Description path is required"
    }
    $item = Get-Item -LiteralPath $Path -ErrorAction Stop
    if ($Kind -eq 'File' -and $item.PSIsContainer) {
        throw "$Description must be a regular file: $Path"
    }
    if ($Kind -eq 'Directory' -and -not $item.PSIsContainer) {
        throw "$Description must be a directory: $Path"
    }
    if ($RequireExe -and $item.Extension -ine '.exe') {
        throw "$Description must be an .exe file: $Path"
    }
    return [System.IO.Path]::GetFullPath([string]$item.FullName)
}

function ConvertTo-WindowsRunnerLifecycleFullPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) { return $null }
    return [System.IO.Path]::GetFullPath($Path)
}

function Get-WindowsRunnerSupervisorArguments {
    param([Parameter(Mandatory = $true)][string]$SupervisorPath)
    if ($SupervisorPath.Contains('"')) {
        throw "Supervisor path contains an unsupported quote character"
    }
    return '-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "{0}"' -f $SupervisorPath
}

function Get-PowerShellFileArgument {
    param([Parameter(Mandatory = $true)][string]$Arguments)

    try {
        $argv = @(ConvertFrom-WindowsCommandLine -CommandLine ('powershell.exe ' + $Arguments))
    } catch {
        return $null
    }
    if ($argv.Count -ne 9) { return $null }
    if ($argv[1] -ine '-NoProfile') { return $null }
    if ($argv[2] -ine '-NonInteractive') { return $null }
    if ($argv[3] -ine '-WindowStyle' -or $argv[4] -ine 'Hidden') { return $null }
    if ($argv[5] -ine '-ExecutionPolicy' -or $argv[6] -ine 'Bypass') { return $null }
    if ($argv[7] -ine '-File' -or [string]::IsNullOrWhiteSpace([string]$argv[8])) { return $null }
    return [string]$argv[8]
}

function Get-PowerShellLifecycleSupervisorArgument {
    param([Parameter(Mandatory = $true)][string]$Arguments)

    try {
        $argv = @(ConvertFrom-WindowsCommandLine -CommandLine ('powershell.exe ' + $Arguments))
    } catch {
        return $null
    }
    if ($argv.Count -lt 3) { return $null }

    $seen = @{}
    for ($i = 1; $i -lt $argv.Count; $i++) {
        $argument = [string]$argv[$i]
        if ($argument -ieq '-File') {
            if ($seen.ContainsKey('File') -or $i + 1 -ne $argv.Count - 1) { return $null }
            $supervisor = [string]$argv[$i + 1]
            if ([string]::IsNullOrWhiteSpace($supervisor)) { return $null }
            return $supervisor
        }
        if ($argument -ieq '-NoProfile' -or $argument -ieq '-NonInteractive') {
            $key = $argument.ToLowerInvariant()
            if ($seen.ContainsKey($key)) { return $null }
            $seen[$key] = $true
            continue
        }
        if ($argument -ieq '-WindowStyle' -or $argument -ieq '-ExecutionPolicy') {
            $key = $argument.ToLowerInvariant()
            if ($seen.ContainsKey($key) -or $i + 1 -ge $argv.Count) { return $null }
            $value = [string]$argv[$i + 1]
            if ([string]::IsNullOrWhiteSpace($value) -or $value.StartsWith('-')) { return $null }
            $seen[$key] = $true
            $i++
            continue
        }
        return $null
    }
    return $null
}

function Test-WindowsRunnerLifecycleSupervisorOwnership {
    param(
        [string]$ObservedSupervisorPath,
        [string]$ExpectedSupervisorPath
    )

    if ([string]::IsNullOrWhiteSpace($ObservedSupervisorPath) -or [string]::IsNullOrWhiteSpace($ExpectedSupervisorPath)) {
        return $false
    }
    try {
        if (-not [System.IO.Path]::IsPathRooted($ObservedSupervisorPath) -or -not [System.IO.Path]::IsPathRooted($ExpectedSupervisorPath)) {
            return $false
        }
        $observed = [System.IO.Path]::GetFullPath($ObservedSupervisorPath)
        $expected = [System.IO.Path]::GetFullPath($ExpectedSupervisorPath)
        return $observed -ieq $expected
    } catch {
        return $false
    }
}

function Get-WindowsRunnerLifecycleTaskEnabled {
    param([Parameter(Mandatory = $true)]$Task)
    return [bool]$Task.Settings.Enabled
}

function ConvertTo-WindowsAccountSid {
    param([Parameter(Mandatory = $true)][string]$UserId)
    try {
        $account = New-Object System.Security.Principal.NTAccount($UserId)
        return $account.Translate([System.Security.Principal.SecurityIdentifier]).Value
    } catch {
        return $null
    }
}

function New-WindowsRunnerLifecycleExpectedSpec {
    param(
        [Parameter(Mandatory = $true)][string]$RunnerPath,
        [Parameter(Mandatory = $true)][string]$RunnerConfigPath,
        [Parameter(Mandatory = $true)][string]$SupervisorPath,
        [Parameter(Mandatory = $true)][string]$TaskName,
        [string]$TaskPath = '\',
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    $runner = Resolve-WindowsRunnerLifecycleExistingPath -Path $RunnerPath -Kind File -Description 'Runner' -RequireExe
    $config = Resolve-WindowsRunnerLifecycleExistingPath -Path $RunnerConfigPath -Kind File -Description 'Runner config'
    $supervisor = Resolve-WindowsRunnerLifecycleExistingPath -Path $SupervisorPath -Kind File -Description 'Supervisor'
    $working = Resolve-WindowsRunnerLifecycleExistingPath -Path $WorkingDirectory -Kind Directory -Description 'Working directory'
    if ([string]::IsNullOrWhiteSpace($TaskName)) { throw 'TaskName is required' }
    if (-not $TaskName.StartsWith('WebCodex ', [System.StringComparison]::OrdinalIgnoreCase)) { throw 'TaskName must identify a WebCodex task (prefix: WebCodex )' }
    if ($TaskPath -ne '\') { throw 'Only the root Scheduled Task path is supported for the WebCodex Windows Runner lifecycle' }

    $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
    $userSid = [string]$identity.User.Value

    return [pscustomobject][ordered]@{
        TaskName = $TaskName
        TaskPath = '\'
        RunnerPath = $runner
        RunnerConfigPath = $config
        SupervisorPath = $supervisor
        WorkingDirectory = $working
        ActionExecutable = 'powershell.exe'
        ActionArguments = Get-WindowsRunnerSupervisorArguments -SupervisorPath $supervisor
        PrincipalSid = $userSid
        PrincipalLogonType = 'Interactive'
        PrincipalRunLevel = 'Highest'
        TriggerCount = 1
        TriggerType = 'MSFT_TaskLogonTrigger'
        TriggerSid = $userSid
        TriggerEnabled = $true
        MultipleInstances = 'IgnoreNew'
        RestartCount = 20
        RestartInterval = 'PT1M'
        ExecutionTimeLimit = 'PT0S'
        StartWhenAvailable = $true
    }
}

function New-WindowsRunnerScheduledTaskDefinition {
    param([Parameter(Mandatory = $true)]$ExpectedSpec)

    $user = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
    $action = New-ScheduledTaskAction `
        -Execute $ExpectedSpec.ActionExecutable `
        -Argument $ExpectedSpec.ActionArguments `
        -WorkingDirectory $ExpectedSpec.WorkingDirectory
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType $ExpectedSpec.PrincipalLogonType -RunLevel $ExpectedSpec.PrincipalRunLevel
    $settings = New-ScheduledTaskSettingsSet `
        -MultipleInstances $ExpectedSpec.MultipleInstances `
        -RestartCount $ExpectedSpec.RestartCount `
        -RestartInterval ([System.Xml.XmlConvert]::ToTimeSpan([string]$ExpectedSpec.RestartInterval)) `
        -ExecutionTimeLimit ([System.Xml.XmlConvert]::ToTimeSpan([string]$ExpectedSpec.ExecutionTimeLimit)) `
        -StartWhenAvailable:$ExpectedSpec.StartWhenAvailable
    return New-ScheduledTask `
        -Action $action `
        -Trigger $trigger `
        -Principal $principal `
        -Settings $settings `
        -Description 'WebCodex Windows Runner lifecycle: Scheduled Task -> PowerShell supervisor -> WMI Win32_Process.Create -> primary Runner.'
}

function Get-WindowsRunnerLifecycleTaskObservation {
    param(
        [Parameter(Mandatory = $true)][string]$TaskName,
        [string]$TaskPath = '\',
        [string]$ExpectedSupervisorPath
    )

    if (-not $TaskName.StartsWith('WebCodex ', [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'TaskName must identify a WebCodex task (prefix: WebCodex )'
    }
    if ($TaskPath -ne '\') {
        throw 'Only the root Scheduled Task path is supported for the WebCodex Windows Runner lifecycle'
    }

    $tasks = @(Get-ScheduledTask -TaskPath $TaskPath -ErrorAction Stop | Where-Object { [string]$_.TaskName -ieq $TaskName })
    if ($tasks.Count -gt 1) {
        throw "Multiple Scheduled Tasks matched exact identity: $TaskPath$TaskName"
    }
    $task = if ($tasks.Count -eq 1) { $tasks[0] } else { $null }
    if (-not $task) {
        return [pscustomobject][ordered]@{
            Exists = $false
            Enabled = $false
            State = 'Missing'
            ActionCount = 0
            ActionExecutable = $null
            ActionArguments = $null
            WorkingDirectory = $null
            SupervisorPath = $null
            IsLifecycleLike = $false
            PrincipalSid = $null
            PrincipalLogonType = $null
            PrincipalRunLevel = $null
            TriggerCount = 0
            TriggerType = $null
            TriggerSid = $null
            TriggerEnabled = $false
            MultipleInstances = $null
            RestartCount = $null
            RestartInterval = $null
            ExecutionTimeLimit = $null
            StartWhenAvailable = $false
        }
    }

    $actions = @($task.Actions)
    $action = if ($actions.Count -eq 1) { $actions[0] } else { $null }
    $execute = if ($action) { [string]$action.Execute } else { $null }
    $rawArguments = if ($action) { [string]$action.Arguments } else { $null }
    $working = if ($action) { [string]$action.WorkingDirectory } else { $null }
    $safeSupervisor = if ($rawArguments) { Get-PowerShellFileArgument -Arguments $rawArguments } else { $null }
    $supervisor = if ($rawArguments) { Get-PowerShellLifecycleSupervisorArgument -Arguments $rawArguments } else { $null }
    $arguments = if ($safeSupervisor) { $rawArguments } elseif ($rawArguments) { '<unrecognized>' } else { $null }
    $isPowerShell = $false
    if ($execute) {
        $isPowerShell = ([System.IO.Path]::GetFileName($execute) -ieq 'powershell.exe')
    }
    $ownsExpectedSupervisor = Test-WindowsRunnerLifecycleSupervisorOwnership `
        -ObservedSupervisorPath $supervisor `
        -ExpectedSupervisorPath $ExpectedSupervisorPath

    $principalSid = ConvertTo-WindowsAccountSid -UserId ([string]$task.Principal.UserId)
    $triggers = @($task.Triggers)
    $trigger = if ($triggers.Count -eq 1) { $triggers[0] } else { $null }
    $triggerType = if ($trigger) { [string]$trigger.CimClass.CimClassName } else { $null }
    $triggerSid = $null
    $triggerEnabled = $false
    if ($trigger) {
        $triggerEnabled = [bool]$trigger.Enabled
        if ($trigger.UserId) { $triggerSid = ConvertTo-WindowsAccountSid -UserId ([string]$trigger.UserId) }
    }

    return [pscustomobject][ordered]@{
        Exists = $true
        Enabled = Get-WindowsRunnerLifecycleTaskEnabled -Task $task
        State = [string]$task.State
        ActionCount = $actions.Count
        ActionExecutable = $execute
        ActionArguments = $arguments
        WorkingDirectory = $working
        SupervisorPath = $supervisor
        IsLifecycleLike = ($actions.Count -eq 1 -and $isPowerShell -and $supervisor -and $TaskName.StartsWith('WebCodex ', [System.StringComparison]::OrdinalIgnoreCase) -and $ownsExpectedSupervisor)
        PrincipalSid = $principalSid
        PrincipalLogonType = [string]$task.Principal.LogonType
        PrincipalRunLevel = [string]$task.Principal.RunLevel
        TriggerCount = $triggers.Count
        TriggerType = $triggerType
        TriggerSid = $triggerSid
        TriggerEnabled = $triggerEnabled
        MultipleInstances = [string]$task.Settings.MultipleInstances
        RestartCount = [int]$task.Settings.RestartCount
        RestartInterval = [string]$task.Settings.RestartInterval
        ExecutionTimeLimit = [string]$task.Settings.ExecutionTimeLimit
        StartWhenAvailable = [bool]$task.Settings.StartWhenAvailable
    }
}

function Get-RunnerConfigPathFromPrimaryIdentity {
    param([Parameter(Mandatory = $true)]$Identity)

    $argv = @(ConvertFrom-WindowsCommandLine -CommandLine ([string]$Identity.CommandLine))
    for ($i = 0; $i -lt $argv.Count; $i++) {
        if ($argv[$i] -ieq '--config' -and $i + 1 -lt $argv.Count) {
            $value = [string]$argv[$i + 1]
            if ([System.IO.Path]::IsPathRooted($value)) { return [System.IO.Path]::GetFullPath($value) }
            return $value
        }
        if ($argv[$i].StartsWith('--config=', [System.StringComparison]::OrdinalIgnoreCase)) {
            $value = $argv[$i].Substring('--config='.Length)
            if ([System.IO.Path]::IsPathRooted($value)) { return [System.IO.Path]::GetFullPath($value) }
            return $value
        }
    }
    return $null
}

function Get-WindowsRunnerPrimaryInventory {
    $records = @(Get-CimInstance Win32_Process -Filter "Name = 'webcodex-runner.exe'" -ErrorAction Stop)
    $paths = @($records | Where-Object { $_.ExecutablePath } | ForEach-Object {
        try { [System.IO.Path]::GetFullPath([string]$_.ExecutablePath) } catch { $null }
    } | Where-Object { $_ } | Sort-Object -Unique)
    if ($paths.Count -gt 32) {
        throw "Refusing unbounded Windows Runner process inventory: found $($paths.Count) executable paths"
    }

    $seen = @{}
    $result = @()
    foreach ($path in $paths) {
        foreach ($identity in @(Get-PrimaryRunnerProcesses -ExactPath $path)) {
            $key = '{0}:{1}' -f $identity.Id,$identity.CreationTime
            if ($seen.ContainsKey($key)) { continue }
            $seen[$key] = $true
            $result += [pscustomobject][ordered]@{
                pid = [uint32]$identity.Id
                process_creation_filetime = [uint64]$identity.CreationTime
                normalized_executable_path = [System.IO.Path]::GetFullPath([string]$identity.Path)
                runner_config_path = Get-RunnerConfigPathFromPrimaryIdentity -Identity $identity
                role = 'primary'
            }
        }
    }
    return @($result | Sort-Object pid)
}

function New-WindowsRunnerLifecycleStatusProjection {
    param(
        [Parameter(Mandatory = $true)][string]$ExpectedRunnerPath,
        [Parameter(Mandatory = $true)]$TaskObservation,
        [object[]]$PrimaryInventory = @()
    )

    $expected = ConvertTo-WindowsRunnerLifecycleFullPath -Path $ExpectedRunnerPath
    $matching = @($PrimaryInventory | Where-Object { $_.normalized_executable_path -ieq $expected })
    $unexpected = @($PrimaryInventory | Where-Object { $_.normalized_executable_path -ine $expected })

    return [pscustomobject][ordered]@{
        task_exists = [bool]$TaskObservation.Exists
        task_enabled = [bool]$TaskObservation.Enabled
        task_state = [string]$TaskObservation.State
        task_action_executable = $TaskObservation.ActionExecutable
        task_action_arguments = $TaskObservation.ActionArguments
        task_working_directory = $TaskObservation.WorkingDirectory
        expected_runner_path = $expected
        primary_runner_count = $matching.Count
        primary_runners = @($matching)
        unexpected_primary_runner_count = $unexpected.Count
        unexpected_primary_runners = @($unexpected | Select-Object -First 8)
    }
}

function Get-WindowsRunnerLifecyclePlan {
    param(
        [Parameter(Mandatory = $true)]$ExpectedSpec,
        [Parameter(Mandatory = $true)]$CurrentTask,
        [object[]]$PrimaryInventory = @()
    )

    $taskMismatches = @()
    $runtimeMismatches = @()
    $definitionMismatch = $false
    $blocked = $false

    if (-not $CurrentTask.Exists) {
        $taskOperation = 'create'
    } else {
        $ownsExpectedSupervisor = Test-WindowsRunnerLifecycleSupervisorOwnership `
            -ObservedSupervisorPath ([string]$CurrentTask.SupervisorPath) `
            -ExpectedSupervisorPath ([string]$ExpectedSpec.SupervisorPath)
        if (-not $CurrentTask.IsLifecycleLike -or -not $ownsExpectedSupervisor) {
            $blocked = $true
            $taskMismatches += [pscustomobject]@{ field = 'task_identity'; expected = 'WebCodex lifecycle task owned by the exact expected supervisor path'; observed = 'unrecognized or differently-owned existing task' }
        }
        if (-not $ownsExpectedSupervisor) {
            $taskMismatches += [pscustomobject]@{ field = 'task_supervisor_path'; expected = $ExpectedSpec.SupervisorPath; observed = $CurrentTask.SupervisorPath }
        }
        if ($CurrentTask.ActionCount -ne 1) {
            $definitionMismatch = $true
            $taskMismatches += [pscustomobject]@{ field = 'task_action_count'; expected = 1; observed = $CurrentTask.ActionCount }
        }
        if ([string]$CurrentTask.ActionExecutable -ine [string]$ExpectedSpec.ActionExecutable) {
            $definitionMismatch = $true
            $taskMismatches += [pscustomobject]@{ field = 'task_action_executable'; expected = $ExpectedSpec.ActionExecutable; observed = $CurrentTask.ActionExecutable }
        }
        if ([string]$CurrentTask.ActionArguments -cne [string]$ExpectedSpec.ActionArguments) {
            $definitionMismatch = $true
            $taskMismatches += [pscustomobject]@{ field = 'task_action_arguments'; expected = $ExpectedSpec.ActionArguments; observed = $CurrentTask.ActionArguments }
        }
        $observedWorking = $null
        if (-not [string]::IsNullOrWhiteSpace([string]$CurrentTask.WorkingDirectory)) {
            try { $observedWorking = [System.IO.Path]::GetFullPath([string]$CurrentTask.WorkingDirectory) } catch { $observedWorking = [string]$CurrentTask.WorkingDirectory }
        }
        if ($observedWorking -ine [string]$ExpectedSpec.WorkingDirectory) {
            $definitionMismatch = $true
            $taskMismatches += [pscustomobject]@{ field = 'task_working_directory'; expected = $ExpectedSpec.WorkingDirectory; observed = $CurrentTask.WorkingDirectory }
        }
        $ownedFields = @(
            @('task_principal_sid', $ExpectedSpec.PrincipalSid, $CurrentTask.PrincipalSid, $true),
            @('task_principal_logon_type', $ExpectedSpec.PrincipalLogonType, $CurrentTask.PrincipalLogonType, $false),
            @('task_principal_run_level', $ExpectedSpec.PrincipalRunLevel, $CurrentTask.PrincipalRunLevel, $false),
            @('task_trigger_count', $ExpectedSpec.TriggerCount, $CurrentTask.TriggerCount, $false),
            @('task_trigger_type', $ExpectedSpec.TriggerType, $CurrentTask.TriggerType, $false),
            @('task_trigger_sid', $ExpectedSpec.TriggerSid, $CurrentTask.TriggerSid, $true),
            @('task_trigger_enabled', $ExpectedSpec.TriggerEnabled, $CurrentTask.TriggerEnabled, $false),
            @('task_multiple_instances', $ExpectedSpec.MultipleInstances, $CurrentTask.MultipleInstances, $false),
            @('task_restart_count', $ExpectedSpec.RestartCount, $CurrentTask.RestartCount, $false),
            @('task_restart_interval', $ExpectedSpec.RestartInterval, $CurrentTask.RestartInterval, $false),
            @('task_execution_time_limit', $ExpectedSpec.ExecutionTimeLimit, $CurrentTask.ExecutionTimeLimit, $false),
            @('task_start_when_available', $ExpectedSpec.StartWhenAvailable, $CurrentTask.StartWhenAvailable, $false)
        )
        foreach ($owned in $ownedFields) {
            $field = [string]$owned[0]
            $expectedValue = $owned[1]
            $observedValue = $owned[2]
            $caseInsensitive = [bool]$owned[3]
            $equal = if ($caseInsensitive) { [string]$expectedValue -ieq [string]$observedValue } else { $expectedValue -eq $observedValue }
            if (-not $equal) {
                $definitionMismatch = $true
                $taskMismatches += [pscustomobject]@{ field = $field; expected = $expectedValue; observed = $observedValue }
            }
        }

        if (-not $CurrentTask.Enabled) {
            $taskMismatches += [pscustomobject]@{ field = 'task_enabled'; expected = $true; observed = $false }
        }

        if ($definitionMismatch) { $taskOperation = 'update' }
        elseif (-not $CurrentTask.Enabled) { $taskOperation = 'enable' }
        else { $taskOperation = 'noop' }
    }

    $primaryCount = @($PrimaryInventory).Count
    $taskRunning = ($CurrentTask.Exists -and [string]$CurrentTask.State -eq 'Running')
    if ($taskRunning) {
        if ($primaryCount -ne 1) {
            $runtimeMismatches += [pscustomobject]@{ field = 'primary_runner_count'; expected = 1; observed = $primaryCount }
        }
    } elseif ($primaryCount -ne 0) {
        $runtimeMismatches += [pscustomobject]@{ field = 'unsupervised_primary_runner_count'; expected = 0; observed = $primaryCount }
    }

    if ($primaryCount -eq 1) {
        $primary = @($PrimaryInventory)[0]
        if ([string]$primary.normalized_executable_path -ine [string]$ExpectedSpec.RunnerPath) {
            $runtimeMismatches += [pscustomobject]@{ field = 'runner_path'; expected = $ExpectedSpec.RunnerPath; observed = $primary.normalized_executable_path }
        }
        if ([string]$primary.runner_config_path -ine [string]$ExpectedSpec.RunnerConfigPath) {
            $runtimeMismatches += [pscustomobject]@{ field = 'runner_config_path'; expected = $ExpectedSpec.RunnerConfigPath; observed = $primary.runner_config_path }
        }
    }

    $canApply = (-not $blocked -and $runtimeMismatches.Count -eq 0)
    return [pscustomobject][ordered]@{
        task_name = $ExpectedSpec.TaskName
        task_path = $ExpectedSpec.TaskPath
        task_operation = $taskOperation
        can_apply = $canApply
        idempotent_noop = ($taskOperation -eq 'noop' -and $canApply -and $runtimeMismatches.Count -eq 0)
        expected_runner_path = $ExpectedSpec.RunnerPath
        expected_runner_config_path = $ExpectedSpec.RunnerConfigPath
        expected_supervisor_path = $ExpectedSpec.SupervisorPath
        expected_working_directory = $ExpectedSpec.WorkingDirectory
        task_mismatches = @($taskMismatches)
        runtime_mismatches = @($runtimeMismatches)
    }
}

function Assert-WindowsRunnerLifecycleSafeConvergence {
    param([Parameter(Mandatory = $true)]$Plan)
    if (-not [bool]$Plan.idempotent_noop) {
        throw 'Scheduled Task lifecycle apply did not converge to a safe idempotent lifecycle state'
    }
}

function Assert-WindowsRunnerLifecycleEffectStillSafe {
    param(
        [Parameter(Mandatory = $true)]$FreshPlan,
        [Parameter(Mandatory = $true)][ValidateSet('create','update','enable')][string]$ExpectedOperation
    )
    if (-not [bool]$FreshPlan.can_apply -or [string]$FreshPlan.task_operation -ne $ExpectedOperation) {
        throw "Scheduled Task lifecycle changed before $ExpectedOperation effect; refusing stale mutation"
    }
    return $FreshPlan
}

function Test-WebCodexOpsRunnerSupport {
    param([Parameter(Mandatory = $true)][string]$Path)
    try {
        $output = @(& $Path ops runner --help 2>&1)
        $code = $LASTEXITCODE
        if ($code -ne 0) { return $false }
        return (($output -join "`n").IndexOf('Usage: webcodex ops runner', [System.StringComparison]::OrdinalIgnoreCase) -ge 0)
    } catch {
        return $false
    }
}

function Resolve-WebCodexOperatorCliPath {
    param(
        [string]$ExplicitPath,
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$InstalledPath = "$env:USERPROFILE\.local\bin\webcodex.exe",
        [scriptblock]$SupportsOpsRunner = { param($Path) Test-WebCodexOpsRunnerSupport -Path $Path }
    )

    if (-not [string]::IsNullOrWhiteSpace($ExplicitPath)) {
        $path = Resolve-WindowsRunnerLifecycleExistingPath -Path $ExplicitPath -Kind File -Description 'Explicit WebCodex CLI'
        if (-not (& $SupportsOpsRunner $path)) {
            throw "Explicit WebCodex CLI does not support 'webcodex ops runner': $path"
        }
        return [pscustomobject]@{ Path = $path; Source = 'explicit' }
    }

    $repo = [System.IO.Path]::GetFullPath((Join-Path $RepoRoot 'target\dogfood\webcodex.exe'))
    if (Test-Path -LiteralPath $repo -PathType Leaf) {
        if (-not (& $SupportsOpsRunner $repo)) {
            throw "Repo dogfood WebCodex CLI exists but does not support 'webcodex ops runner': $repo"
        }
        return [pscustomobject]@{ Path = $repo; Source = 'repo_dogfood' }
    }

    if (-not (Test-Path -LiteralPath $InstalledPath -PathType Leaf)) {
        throw "No supported WebCodex operator CLI found; build target\dogfood\webcodex.exe or pass -WebCodexCliPath explicitly"
    }
    $installed = Resolve-WindowsRunnerLifecycleExistingPath -Path $InstalledPath -Kind File -Description 'Installed WebCodex CLI'
    if (-not (& $SupportsOpsRunner $installed)) {
        throw "Installed WebCodex CLI is stale or unsupported and does not support 'webcodex ops runner': $installed"
    }
    return [pscustomobject]@{ Path = $installed; Source = 'installed' }
}

function Resolve-WebCodexRunnerCandidatePath {
    param(
        [string]$ExplicitPath,
        [Parameter(Mandatory = $true)][string]$RepoRoot
    )

    if (-not [string]::IsNullOrWhiteSpace($ExplicitPath)) {
        $path = Resolve-WindowsRunnerLifecycleExistingPath -Path $ExplicitPath -Kind File -Description 'Explicit Runner candidate' -RequireExe
        return [pscustomobject]@{ Path = $path; Source = 'explicit' }
    }
    $repo = [System.IO.Path]::GetFullPath((Join-Path $RepoRoot 'target\dogfood\webcodex-runner.exe'))
    if (-not (Test-Path -LiteralPath $repo -PathType Leaf)) {
        throw "Runner candidate is unavailable; build target\dogfood\webcodex-runner.exe or pass -CandidatePath explicitly"
    }
    return [pscustomobject]@{ Path = $repo; Source = 'repo_dogfood' }
}
