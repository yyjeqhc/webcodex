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
    function Resolve-AccountSid($identity) {
        if ([string]::IsNullOrWhiteSpace($identity)) { return $null }
        try {
            if ($identity -match '^S-1-') {
                return [Security.Principal.SecurityIdentifier]::new($identity)
            }
            return ([Security.Principal.NTAccount]::new($identity)).Translate([Security.Principal.SecurityIdentifier])
        } catch { return $null }
    }
    # Task Scheduler may return an account name even when registration used a SID.
    function Account-Sid($identity) {
        $resolved = Resolve-AccountSid $identity
        if ($null -ne $resolved -and $resolved.Value -ceq $inputData.sid) { return $resolved.Value }
        $computer = [Environment]::MachineName
        # Only the bare local computer/user collision permits another lookup.
        # Qualified names, UPNs and SID literals never change authority here.
        if ([string]::IsNullOrWhiteSpace($identity) -or $identity -match '[\\/@]' -or $identity -match '^S-1-' -or
            ![string]::Equals($identity, $computer, [StringComparison]::OrdinalIgnoreCase)) {
            if ($null -ne $resolved) { return $resolved.Value }
            return $null
        }
        $local = Resolve-AccountSid ($computer + '\' + $identity)
        if ($null -eq $local -or $local.Value -cne $inputData.sid) { return $null }
        # The bare name can identify the local SAM domain instead of its user
        # (NTAccount.Translate may reject that domain SID). Never override a
        # successfully resolved different user, group or foreign domain.
        if ($null -ne $resolved -and
            ($null -eq $local.AccountDomainSid -or $resolved.Value -cne $local.AccountDomainSid.Value)) { return $null }
        return $local.Value
    }
    function Owned-Task($task) {
        if ($null -eq $task) { return $false }
        $d = $task.Definition
        return $d.RegistrationInfo.Description -ceq $inputData.description -and
            (Account-Sid $d.Principal.UserId) -eq $inputData.sid -and $d.Principal.LogonType -eq 3 -and $d.Principal.RunLevel -eq 0 -and
            $d.Actions.Count -eq 1 -and $d.Actions.Item(1).Type -eq 0 -and
            (($d.Actions.Item(1).Path -ceq $inputData.program -and $d.Actions.Item(1).Arguments -ceq $inputData.arguments) -or
             ($inputData.host -and $d.Actions.Item(1).Path -ceq $inputData.host -and $d.Actions.Item(1).Arguments -ceq $inputData.host_arguments)) -and $d.Actions.Item(1).WorkingDirectory -ceq $inputData.directory -and
            $d.Triggers.Count -eq 1 -and $d.Triggers.Item(1).Type -eq 9 -and (Account-Sid $d.Triggers.Item(1).UserId) -eq $inputData.sid -and
            $d.Triggers.Item(1).Enabled -eq $true -and
            $d.Settings.MultipleInstances -eq 2 -and $d.Settings.ExecutionTimeLimit -eq 'PT0S' -and
            $d.Settings.AllowHardTerminate -eq $true -and $d.Settings.RestartCount -eq 3 -and $d.Settings.RestartInterval -eq 'PT1M' -and
            $d.Settings.StartWhenAvailable -eq $true -and
            $d.Settings.DisallowStartIfOnBatteries -eq $false -and $d.Settings.StopIfGoingOnBatteries -eq $false
    }
    function Initialize-ActionArgv {
        if ('WebCodexTaskArgv' -as [type]) { return }
        # Parse with the OS argv rules; never evaluate observed command text.
        # https://learn.microsoft.com/windows/win32/api/shellapi/nf-shellapi-commandlinetoargvw
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class WebCodexTaskArgv {
    [DllImport("shell32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern IntPtr CommandLineToArgvW(string command, out int count);
    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr value);
    public static string[] Parse(string command) {
        if (String.IsNullOrEmpty(command) || command.Length > 32767)
            throw new InvalidOperationException("invalid command size");
        int count;
        IntPtr values = CommandLineToArgvW(command, out count);
        if (values == IntPtr.Zero) throw new InvalidOperationException("argv unavailable");
        try {
            if (count < 1 || count > 4096) throw new InvalidOperationException("invalid argv count");
            string[] args = new string[count];
            for (int i = 0; i < count; i++)
                args[i] = Marshal.PtrToStringUni(Marshal.ReadIntPtr(values, i * IntPtr.Size));
            return args;
        } finally { LocalFree(values); }
    }
}
'@ | Out-Null
    }
    function Executable-PathMatches([string]$observed, [string]$expected) {
        if ([string]::IsNullOrEmpty($observed) -or [string]::IsNullOrEmpty($expected)) { return $false }
        if ([string]::Equals($observed, $expected, [StringComparison]::OrdinalIgnoreCase)) { return $true }
        # CIM can omit the extended DOS prefix retained in the child's argv[0].
        # Only bridge an otherwise identical, unambiguous absolute DOS path.
        # Never normalize relative/device/UNC paths or reinterpret path segments.
        # https://learn.microsoft.com/windows/win32/fileio/naming-a-file
        $prefix = '\\?\'
        $observedExtended = $observed.StartsWith($prefix, [StringComparison]::Ordinal)
        $expectedExtended = $expected.StartsWith($prefix, [StringComparison]::Ordinal)
        if ($observedExtended -eq $expectedExtended) { return $false }
        if ($observedExtended) { $normal = $expected; $extended = $observed.Substring($prefix.Length) }
        else { $normal = $observed; $extended = $expected.Substring($prefix.Length) }
        if (![string]::Equals($normal, $extended, [StringComparison]::OrdinalIgnoreCase) -or
            $normal -cnotmatch '^[A-Za-z]:\\') { return $false }
        foreach ($component in $normal.Substring(3).Split([char]'\')) {
            if ([string]::IsNullOrEmpty($component) -or
                $component -cmatch '[\x00-\x1f<>:"/|?*]' -or
                $component.EndsWith('.') -or $component.EndsWith(' ') -or
                $component.ToUpperInvariant() -cmatch '^(CON|PRN|AUX|NUL|CONIN\$|CONOUT\$|COM[1-9\u00b9\u00b2\u00b3]|LPT[1-9\u00b9\u00b2\u00b3])[ ]*(\.|$)') {
                return $false
            }
        }
        return $true
    }
    function Action-Matches($process, $program, $arguments) {
        if ($null -eq $process -or [string]::IsNullOrEmpty($process.ExecutablePath) -or
            [string]::IsNullOrEmpty($process.CommandLine) -or
            !(Executable-PathMatches $process.ExecutablePath $program)) { return $false }
        Initialize-ActionArgv
        $actual = [WebCodexTaskArgv]::Parse([string]$process.CommandLine)
        $expected = [WebCodexTaskArgv]::Parse('webcodex ' + $arguments)
        if ($actual.Count -ne $expected.Count -or
            !(Executable-PathMatches $actual[0] $program)) { return $false }
        for ($index=1; $index -lt $actual.Count; $index++) {
            if ($actual[$index] -cne $expected[$index]) { return $false }
        }
        return $true
    }
    function Read-ActionProcess($task) {
        if (!(Owned-Task $task) -or $task.State -ne 4) { return $null }
        $instances = $task.GetInstances(0)
        if ($instances.Count -ne 1) { return $null }
        $instance = $instances.Item(1)
        $instanceGuid = [string]$instance.InstanceGuid
        $enginePid = [uint32]$instance.EnginePID
        if ($instance.State -ne 4 -or [string]::IsNullOrEmpty($instanceGuid) -or $enginePid -eq 0) { return $null }
        # EnginePID identifies the Scheduler engine, which need not be the action.
        # Accept it only when the OS independently confirms this exact action and
        # owner. A separate taskeng/svchost engine is deliberately unconfirmed:
        # neither a matching unrelated process nor a saved heartbeat proves the
        # action belongs to this instance.
        # https://learn.microsoft.com/windows/win32/taskschd/runningtask-enginepid
        $action = $task.Definition.Actions.Item(1)
        $hosted = $inputData.host -and $action.Path -ceq $inputData.host
        $worker = $null
        for ($pass = 0; $pass -lt 2; $pass++) {
            $candidates = @(Get-CimInstance -ClassName Win32_Process -Filter ("ProcessId = " + $enginePid) -Property ProcessId,ExecutablePath,CommandLine,CreationDate -OperationTimeoutSec 5)
            if ($candidates.Count -ne 1) { return $null }
            $matched = $candidates[0]
            if ($matched.ProcessId -ne $enginePid -or !(Action-Matches $matched $action.Path $action.Arguments) -or $null -eq $matched.CreationDate) { return $null }
            $owner = Invoke-CimMethod -InputObject $matched -MethodName GetOwnerSid -OperationTimeoutSec 5
            if ($owner.ReturnValue -ne 0 -or $owner.Sid -cne $inputData.sid) { return $null }
            if ($pass -eq 1 -and $matched.CreationDate -ne $hostBirth) { return $null }
            $hostBirth = $matched.CreationDate
            if ($hosted) {
                # The GUI action owns exactly one managed direct worker. Never
                # select by executable name, heartbeat, or an unrelated engine.
                $children = @(Get-CimInstance -ClassName Win32_Process -Filter ("ParentProcessId = " + $enginePid) -Property ProcessId,ParentProcessId,ExecutablePath,CommandLine,CreationDate -OperationTimeoutSec 5)
                $matchesWorker = @($children | Where-Object { $_.ParentProcessId -eq $enginePid -and (Action-Matches $_ $inputData.program $inputData.arguments) })
                if ($matchesWorker.Count -ne 1) { return $null }
                $matched = $matchesWorker[0]
                if (!$matched.ProcessId -or $null -eq $matched.CreationDate -or $matched.CreationDate -lt $hostBirth) { return $null }
                $owner = Invoke-CimMethod -InputObject $matched -MethodName GetOwnerSid -OperationTimeoutSec 5
                if ($owner.ReturnValue -ne 0 -or $owner.Sid -cne $inputData.sid) { return $null }
            }
            if ($pass -eq 1 -and ($matched.ProcessId -ne $worker.ProcessId -or $matched.CreationDate -ne $worker.CreationDate)) { return $null }
            $worker = $matched
        }
        $current = Read-Task
        if (!(Owned-Task $current) -or $current.State -ne 4 -or $current.Definition.Actions.Item(1).Path -cne $action.Path) { return $null }
        $currentInstances = $current.GetInstances(0)
        if ($currentInstances.Count -ne 1) { return $null }
        $currentInstance = $currentInstances.Item(1)
        if ($currentInstance.State -ne 4 -or $currentInstance.InstanceGuid -cne $instanceGuid -or
            [uint32]$currentInstance.EnginePID -ne $enginePid) { return $null }
        return @{ process_id=$worker.ProcessId; instance_guid=$instanceGuid; action_process_id=$enginePid;
            process_created=$worker.CreationDate; action_created=$hostBirth; instance=$currentInstance }
    }
    function Use-ConsoleFreeAction($observed) {
        if (!$inputData.host -or $observed.Definition.Actions.Item(1).Path -ceq $inputData.host) { return $observed }
        # Migration is limited to the exact stopped legacy task. UPDATE cannot
        # create a missing task. Leave the principal, triggers, flags and ACL intact.
        $current = Read-Task
        if (!(Owned-Task $current) -or $current.State -notin @(1,3) -or $current.Xml -cne $observed.Xml) { throw 'task changed during migration' }
        $definition = $current.Definition
        $definition.Actions.Item(1).Path = $inputData.host
        $definition.Actions.Item(1).Arguments = $inputData.host_arguments
        $null = $folder.RegisterTaskDefinition($inputData.name, $definition, 4, $inputData.sid, $null, 3, $null)
        $current = Read-Task
        if (!(Owned-Task $current) -or $current.Definition.Actions.Item(1).Path -cne $inputData.host) { throw 'task migration unconfirmed' }
        return $current
    }
    function Initialize-TaskProcessWait {
        if ('WebCodexTaskProcessWait' -as [type]) { return }
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class WebCodexTaskProcessWait {
    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern SafeWaitHandle OpenProcess(uint access, bool inherit, uint processId);
    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern uint WaitForSingleObject(SafeWaitHandle process, uint milliseconds);
    public static SafeWaitHandle Open(uint processId) {
        // Observation/synchronization only: never acquire termination rights.
        SafeWaitHandle handle = OpenProcess(0x00101000, false, processId);
        if (handle.IsInvalid) {
            int error = Marshal.GetLastWin32Error();
            handle.Dispose();
            throw new Win32Exception(error);
        }
        return handle;
    }
    public static bool Exited(SafeWaitHandle process) {
        uint result = WaitForSingleObject(process, 0);
        if (result == 0) return true;
        if (result == 258) return false;
        throw new Win32Exception(Marshal.GetLastWin32Error());
    }
}
'@ | Out-Null
    }
    function Open-TaskProcessWait($processId) {
        Initialize-TaskProcessWait
        return [WebCodexTaskProcessWait]::Open([uint32]$processId)
    }
    function Test-TaskProcessExited($handle) {
        return [WebCodexTaskProcessWait]::Exited($handle)
    }
    function Close-TaskProcessWait($handles) {
        foreach ($handle in $handles) { $handle.Dispose() }
    }
    function Capture-TaskStop($task, $deadline) {
        if ([DateTime]::UtcNow -ge $deadline) { throw 'stop observation deadline expired' }
        $before = Read-ActionProcess $task
        # A queued or unobserved action supplies no process identity. Do not
        # replace it with an executable-name scan or start a competing instance.
        if ($null -eq $before) { throw 'task process unconfirmed before stop' }
        $handles = @()
        try {
            foreach ($processId in (@($before.action_process_id, $before.process_id) | Select-Object -Unique)) {
                $handles += Open-TaskProcessWait $processId
            }
            # The open handles pin the objects/PIDs. Revalidate the complete
            # task, argv, SID and creation-time fences after opening them.
            $after = Read-ActionProcess (Read-Task)
            if ($null -eq $after -or $after.instance_guid -cne $before.instance_guid -or
                $after.action_process_id -ne $before.action_process_id -or $after.process_id -ne $before.process_id -or
                $after.action_created -ne $before.action_created -or $after.process_created -ne $before.process_created) {
                throw 'task process changed before stop'
            }
            if ([DateTime]::UtcNow -ge $deadline) { throw 'stop observation deadline expired' }
            return @{ handles=$handles; instance=$after.instance; instance_guid=$after.instance_guid }
        } catch {
            Close-TaskProcessWait $handles
            throw
        }
    }
    function Wait-Stopped($handles, $deadline) {
        $phaseDeadline = [DateTime]::UtcNow.AddSeconds(20)
        if ($phaseDeadline -lt $deadline) { $deadline = $phaseDeadline }
        while ($true) {
            $observed = Read-Task
            if (!(Owned-Task $observed)) { throw 'task replaced during stop' }
            $exited = $true
            foreach ($handle in $handles) {
                if (!(Test-TaskProcessExited $handle)) { $exited = $false }
            }
            # Scheduler Ready can precede termination of the GUI action and
            # its managed worker. Wait for their exact kernel objects and the
            # scheduler instance to retire before admitting a replacement.
            if ($observed.State -in @(1,3) -and $exited -and $observed.GetInstances(0).Count -eq 0) { return $observed }
            if ($observed.State -notin @(1,2,3,4) -or [DateTime]::UtcNow -ge $deadline) { throw 'stop outcome unknown' }
            Start-Sleep -Milliseconds 100
        }
    }
    function Wait-Started($instanceGuid, $deadline) {
        $phaseDeadline = [DateTime]::UtcNow.AddSeconds(20)
        if ($phaseDeadline -lt $deadline) { $deadline = $phaseDeadline }
        $seenInstance = $false
        while ($true) {
            $observed = Read-Task
            if (!(Owned-Task $observed) -or !$observed.Enabled) { throw 'task replaced during start' }
            $instances = $observed.GetInstances(0)
            if ($instances.Count -gt 1) { throw 'task instances changed during start' }
            if ($instances.Count -eq 1) {
                if ($instances.Item(1).InstanceGuid -cne $instanceGuid) { throw 'task instance changed during start' }
                $seenInstance = $true
            } elseif ($seenInstance -and $observed.State -in @(1,3)) {
                throw 'task exited during start'
            }
            if ($observed.State -eq 4) {
                $process = Read-ActionProcess $observed
                if ($null -ne $process) {
                    if ($process.instance_guid -cne $instanceGuid) { throw 'task process changed during start' }
                    return $observed
                }
            }
            if ($observed.State -notin @(2,3,4) -or [DateTime]::UtcNow -ge $deadline) { throw 'start outcome unknown' }
            Start-Sleep -Milliseconds 100
        }
    }
    function Start-OwnedTask($task, $deadline, $previousInstanceGuid) {
        if (!(Owned-Task $task) -or !$task.Enabled -or $task.State -ne 3 -or [DateTime]::UtcNow -ge $deadline) {
            throw 'task changed before start'
        }
        # Run is one mutation. Its return is an accepted instance, not proof
        # that the action/worker survived startup; subsequent work only observes.
        $started = $task.Run($null)
        $instanceGuid = [string]$started.InstanceGuid
        if ([string]::IsNullOrEmpty($instanceGuid) -or
            ($previousInstanceGuid -and $instanceGuid -ceq $previousInstanceGuid)) { throw 'new task instance unconfirmed' }
        $observed = Wait-Started $instanceGuid $deadline
        return $observed
    }
    function Stop-OwnedTask($task, $deadline) {
        $stopping = $null
        try {
            if ($task.State -in @(2,4)) {
                $stopping = Capture-TaskStop $task $deadline
                $null = $stopping.instance.Stop()
            }
            $observed = Wait-Stopped $stopping.handles $deadline
            return $observed
        } finally { if ($null -ne $stopping) { Close-TaskProcessWait $stopping.handles } }
    }
    function Restart-OwnedTask($task, $deadline) {
        $stopping = $null
        try {
            if ($task.State -in @(2,4)) {
                $stopping = Capture-TaskStop $task $deadline
                $null = $stopping.instance.Stop()
            }
            $observed = Wait-Stopped $stopping.handles $deadline
            $observed = Use-ConsoleFreeAction $observed
            $observed = Start-OwnedTask $observed $deadline $stopping.instance_guid
            return $observed
        } finally { if ($null -ne $stopping) { Close-TaskProcessWait $stopping.handles } }
    }
    # Leave headroom inside the Rust adapter's 55-second command deadline.
    $operationDeadline = [DateTime]::UtcNow.AddSeconds(45)
    $task = Read-Task
    if ($inputData.operation -notin @('inspect', 'running_pid')) {
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
                    if ($inputData.host) {
                        $action.Path = $inputData.host
                        $action.Arguments = $inputData.host_arguments
                    } else {
                        $action.Path = $inputData.program
                        $action.Arguments = $inputData.arguments
                    }
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
                } else {
                    if ($task.State -in @(1,3)) { $task = Use-ConsoleFreeAction $task }
                    if (!$task.Enabled) { $task.Enabled = $true }
                }
            }
            'start' {
                if ($null -eq $task) { throw 'missing task' }
                if (!$task.Enabled) { throw 'disabled task' }
                if ($task.State -eq 3) {
                    $task = Use-ConsoleFreeAction $task
                    $task = Start-OwnedTask $task $operationDeadline $null
                } elseif ($task.State -notin @(2,4)) { throw 'start state unknown' }
            }
            'stop' {
                if ($null -eq $task) { throw 'missing task' }
                $task = Stop-OwnedTask $task $operationDeadline
            }
            'restart' {
                if ($null -eq $task -or !$task.Enabled) { throw 'missing or disabled task' }
                if ($task.State -notin @(2,3,4)) { throw 'restart state unknown' }
                $task = Restart-OwnedTask $task $operationDeadline
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
    else {
        $observation = @{ exists=$true; owned=(Owned-Task $task); enabled=[bool]$task.Enabled; state=[int]$task.State }
        if ($observation.owned -and $task.State -in @(1,3)) {
            $observation.last_task_result = [int64]$task.LastTaskResult
        }
        if ($inputData.operation -eq 'running_pid') {
            $process = Read-ActionProcess $task
            if ($null -ne $process) {
                $observation.process_id = $process.process_id
                $observation.instance_guid = $process.instance_guid
            }
        }
        $observation | ConvertTo-Json -Compress
    }
} catch {
    # Do not return PowerShell's exception text, task data or local paths.
    @{ error='native_task_operation_unconfirmed' } | ConvertTo-Json -Compress
    exit 1
}
