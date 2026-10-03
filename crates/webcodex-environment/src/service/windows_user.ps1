$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
try {
    $inputData = $env:WEBCODEX_SERVICE_REQUEST | ConvertFrom-Json
    Remove-Item Env:WEBCODEX_SERVICE_REQUEST
    $actualSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    if ($actualSid -cne $inputData.sid) { throw 'owner mismatch' }
    $scheduler = New-Object -ComObject 'Schedule.Service'
    $scheduler.Connect()
    $folder = $scheduler.GetFolder('\')
    function Read-Task {
        try { return $folder.GetTask($inputData.name) }
        catch {
            # Enumeration must succeed to classify absence. Registration below
            # remains CREATE-only even if a concurrent task appears afterwards.
            foreach ($candidate in $folder.GetTasks(1)) {
                if ($candidate.Name -ceq $inputData.name) { throw 'task unreadable' }
            }
            return $null
        }
    }
    # Task Scheduler may return an account name even when registration used a SID.
    function Account-Sid($identity) {
        if ([string]::IsNullOrWhiteSpace($identity)) { return $null }
        try {
            if ($identity -match '^S-1-') {
                return ([Security.Principal.SecurityIdentifier]::new($identity)).Value
            }
            return ([Security.Principal.NTAccount]::new($identity)).Translate([Security.Principal.SecurityIdentifier]).Value
        } catch { return $null }
    }
    function Owned-Task($task) {
        if ($null -eq $task) { return $false }
        $d = $task.Definition
        return $d.RegistrationInfo.Description -ceq $inputData.description -and
            (Account-Sid $d.Principal.UserId) -eq $inputData.sid -and $d.Principal.LogonType -eq 3 -and $d.Principal.RunLevel -eq 0 -and
            $d.Actions.Count -eq 1 -and $d.Actions.Item(1).Type -eq 0 -and
            $d.Actions.Item(1).Path -ceq $inputData.program -and
            $d.Actions.Item(1).Arguments -ceq $inputData.arguments -and $d.Actions.Item(1).WorkingDirectory -ceq $inputData.directory -and
            $d.Triggers.Count -eq 1 -and $d.Triggers.Item(1).Type -eq 9 -and (Account-Sid $d.Triggers.Item(1).UserId) -eq $inputData.sid -and
            $d.Triggers.Item(1).Enabled -eq $true -and
            $d.Settings.MultipleInstances -eq 2 -and $d.Settings.ExecutionTimeLimit -eq 'PT0S' -and
            $d.Settings.AllowHardTerminate -eq $true -and $d.Settings.RestartCount -eq 3 -and $d.Settings.RestartInterval -eq 'PT1M' -and
            $d.Settings.StartWhenAvailable -eq $true -and
            $d.Settings.DisallowStartIfOnBatteries -eq $false -and $d.Settings.StopIfGoingOnBatteries -eq $false
    }
    function Wait-Stopped {
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        while ($true) {
            $observed = Read-Task
            if (!(Owned-Task $observed)) { throw 'task replaced during stop' }
            if ($observed.State -in @(1,3)) { return $observed }
            if ($observed.State -notin @(2,4) -or [DateTime]::UtcNow -ge $deadline) { throw 'stop outcome unknown' }
            Start-Sleep -Milliseconds 100
        }
    }
    $task = Read-Task
    if ($inputData.operation -ne 'inspect') {
        if ($null -ne $task -and !(Owned-Task $task)) { throw 'foreign task' }
        switch ($inputData.operation) {
            'install' {
                if ($null -eq $task) {
                    $definition = $scheduler.NewTask(0)
                    $definition.RegistrationInfo.Description = $inputData.description
                    $definition.Principal.UserId = $inputData.sid
                    $definition.Principal.LogonType = 3
                    $definition.Principal.RunLevel = 0
                    $trigger = $definition.Triggers.Create(9)
                    $trigger.UserId = $inputData.sid
                    $trigger.Enabled = $true
                    $action = $definition.Actions.Create(0)
                    $action.Path = $inputData.program
                    $action.Arguments = $inputData.arguments
                    $action.WorkingDirectory = $inputData.directory
                    $definition.Settings.MultipleInstances = 2
                    $definition.Settings.ExecutionTimeLimit = 'PT0S'
                    $definition.Settings.AllowHardTerminate = $true
                    $definition.Settings.DisallowStartIfOnBatteries = $false
                    $definition.Settings.StopIfGoingOnBatteries = $false
                    $definition.Settings.StartWhenAvailable = $true
                    $definition.Settings.RestartCount = 3
                    $definition.Settings.RestartInterval = 'PT1M'
                    $definition.Settings.Enabled = $true
                    $null = $folder.RegisterTaskDefinition($inputData.name, $definition, 2, $inputData.sid, $null, 3, $null)
                } elseif (!$task.Enabled) { $task.Enabled = $true }
            }
            'start' {
                if ($null -eq $task) { throw 'missing task' }
                if (!$task.Enabled) { throw 'disabled task' }
                if ($task.State -eq 3) { $null = $task.Run($null) }
                elseif ($task.State -notin @(2,4)) { throw 'start state unknown' }
            }
            'stop' {
                if ($null -eq $task) { throw 'missing task' }
                if ($task.State -in @(2,4)) { $task.Stop(0) }
                $task = Wait-Stopped
            }
            'restart' {
                if ($null -eq $task -or !$task.Enabled) { throw 'missing or disabled task' }
                if ($task.State -notin @(2,3,4)) { throw 'restart state unknown' }
                if ($task.State -in @(2,4)) { $task.Stop(0) }
                $task = Wait-Stopped
                if (!$task.Enabled) { throw 'disabled during restart' }
                $null = $task.Run($null)
            }
            'uninstall' {
                if ($null -ne $task) {
                    if ($task.State -notin @(1,3)) { throw 'stop required' }
                    $folder.DeleteTask($inputData.name, 0)
                }
            }
            default { throw 'unsupported operation' }
        }
        $task = Read-Task
    }
    if ($null -eq $task) { @{ exists=$false } | ConvertTo-Json -Compress }
    else { @{ exists=$true; owned=(Owned-Task $task); enabled=[bool]$task.Enabled; state=[int]$task.State } | ConvertTo-Json -Compress }
} catch {
    # Do not return PowerShell's exception text, task data or local paths.
    @{ error='native_task_operation_unconfirmed' } | ConvertTo-Json -Compress
    exit 1
}
