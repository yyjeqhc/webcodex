# Native process-handle binding plus deterministic Task Scheduler lifecycle races.
# No scheduler connection, registration, or service operation is performed here.
$ErrorActionPreference = 'Stop'
$tokens = $null
$parseErrors = $null
$syntax = [Management.Automation.Language.Parser]::ParseInput($env:WEBCODEX_PARSE_SCRIPT, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw 'production task script does not parse' }
$names = @('Initialize-TaskProcessWait','Open-TaskProcessWait','Test-TaskProcessExited',
    'Close-TaskProcessWait','Capture-TaskStop','Wait-Stopped','Wait-Started',
    'Start-OwnedTask','Stop-OwnedTask','Restart-OwnedTask')
$functions = @($syntax.FindAll({
    param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in $names
}, $true))
if ($functions.Count -ne $names.Count) { throw 'lifecycle functions missing' }
foreach ($definition in $functions) { . ([ScriptBlock]::Create($definition.Extent.Text)) }

# Exercise the real minimum-rights P/Invoke against this test process only.
$currentHandle = Open-TaskProcessWait $PID
try {
    if (Test-TaskProcessExited $currentHandle) { throw 'current test process is not exited' }
} finally { Close-TaskProcessWait @($currentHandle) }
if (!$currentHandle.IsClosed) { throw 'native observation handle leaked' }

function Reset-Fixture {
    $script:phase='initial'
    $script:stopCalls=0
    $script:runCalls=0
    $script:stopPolls=0
    $script:startPolls=0
    $script:actionReads=0
    $script:openedHandles=@()
    $script:identityFault=$null
    $script:replaceDuringStop=$false
    $script:delayInstanceRetirement=$false
    $script:failDuringStart=$false
    $script:replaceDuringStart=$false
    $script:reuseInstance=$false
    $script:task=[pscustomobject]@{Owned=$true;Enabled=$true;State=4}
    $script:oldInstance=[pscustomobject]@{InstanceGuid='{723A491B-825B-4C30-BB2B-F86AA98F94AA}'}
    $script:newInstance=[pscustomobject]@{InstanceGuid='{D55260D3-00A1-43C6-894E-36A119A4430D}'}
    $script:currentInstance=$script:oldInstance
    $script:instances=[pscustomobject]@{Count=1}
    $script:instances | Add-Member ScriptMethod Item { param($index) return $script:currentInstance }
    $script:task | Add-Member ScriptMethod GetInstances { param($flags) return $script:instances }
    $script:oldInstance | Add-Member ScriptMethod Stop {
        $script:stopCalls+=1
        $script:phase='stopped'
        $script:task.State=3
        $script:instances.Count=0
        if ($script:delayInstanceRetirement) { $script:instances.Count=1 }
        # Deliberately report Ready before either owned process has exited.
    }
    $script:task | Add-Member ScriptMethod Run {
        param($argument)
        $script:runCalls+=1
        if ($script:instances.Count -ne 0) { throw 'replacement overlapped a retired action still reported by Scheduler' }
        foreach ($handle in $script:openedHandles) {
            if (!$handle.Exited) { throw 'replacement overlapped an old owned process' }
        }
        $script:phase='starting'
        $script:task.State=2
        $script:instances.Count=1
        $script:currentInstance=$script:newInstance
        if ($script:reuseInstance) { return $script:oldInstance }
        if ($script:replaceDuringStart) {
            $script:currentInstance=[pscustomobject]@{InstanceGuid='{144088F0-3333-4444-8888-242424242424}'}
        }
        return $script:newInstance
    }
}
function Owned-Task($value) { return $null -ne $value -and $value.Owned }
function Read-Task { return $script:task }
function Use-ConsoleFreeAction($value) { return $value }
function Read-ActionProcess($value) {
    if (!(Owned-Task $value) -or $value.State -ne 4) { return $null }
    $script:actionReads+=1
    $created=[DateTime]::Parse('2026-11-22T00:00:00Z').ToUniversalTime()
    $observation=[pscustomobject]@{
        process_id=102;action_process_id=101
        process_created=$created;action_created=$created
        instance_guid=$script:currentInstance.InstanceGuid;instance=$script:currentInstance
    }
    if ($script:phase -eq 'starting') {
        $observation.process_id=202
        $observation.action_process_id=201
    }
    if ($script:actionReads -eq 2) {
        switch ($script:identityFault) {
            'worker_pid' { $observation.process_id=103 }
            'host_pid' { $observation.action_process_id=104 }
            'worker_birth' { $observation.process_created=$created.AddSeconds(1) }
            'host_birth' { $observation.action_created=$created.AddSeconds(1) }
            'instance' { $observation.instance_guid=$script:newInstance.InstanceGuid }
            'unverified' { return $null }
        }
    }
    return $observation
}
function Open-TaskProcessWait($processId) {
    $handle=[pscustomobject]@{ProcessId=$processId;Exited=$false;Disposed=$false}
    $handle | Add-Member ScriptMethod Dispose { $this.Disposed=$true }
    $script:openedHandles+=@($handle)
    return $handle
}
function Test-TaskProcessExited($handle) { return $handle.Exited }
function Start-Sleep {
    param($Milliseconds)
    if ($script:phase -eq 'stopped') {
        $script:stopPolls+=1
        # Host exits first, worker only on the third observation tick.
        foreach ($handle in $script:openedHandles) {
            if ($handle.ProcessId -eq 101 -or $script:stopPolls -ge 3) { $handle.Exited=$true }
        }
        if ($script:delayInstanceRetirement -and $script:stopPolls -ge 5) { $script:instances.Count=0 }
        if ($script:replaceDuringStop) { $script:task.Owned=$false }
    } elseif ($script:phase -eq 'starting') {
        $script:startPolls+=1
        if ($script:failDuringStart) {
            $script:task.State=3
            $script:instances.Count=0
        } else { $script:task.State=4 }
    }
}
function Assert-HandlesClosed {
    foreach ($handle in $script:openedHandles) {
        if (!$handle.Disposed) { throw 'observation handle was not closed' }
    }
}
function Expect-Rejected($action) {
    $rejected=$false
    try { & $action | Out-Null } catch { $rejected=$true }
    if (!$rejected) { throw 'unconfirmed lifecycle was accepted' }
}
$cases=0
Reset-Fixture
$result=Restart-OwnedTask $script:task ([DateTime]::UtcNow.AddSeconds(10))
if ($result.State -ne 4 -or $script:stopCalls -ne 1 -or $script:runCalls -ne 1 -or $script:stopPolls -ne 3 -or $script:startPolls -ne 1) {
    throw 'restart did not wait for both exact old processes and observe the new worker'
}
Assert-HandlesClosed
$cases+=1

Reset-Fixture
$script:delayInstanceRetirement=$true
$result=Restart-OwnedTask $script:task ([DateTime]::UtcNow.AddSeconds(10))
if ($result.State -ne 4 -or $script:stopCalls -ne 1 -or $script:runCalls -ne 1 -or $script:stopPolls -ne 5) {
    throw 'restart did not wait for the previous Scheduler instance to retire'
}
Assert-HandlesClosed
$cases+=1

foreach ($fault in @('worker_pid','host_pid','worker_birth','host_birth','instance','unverified')) {
    Reset-Fixture
    $script:identityFault=$fault
    Expect-Rejected { Restart-OwnedTask $script:task ([DateTime]::UtcNow.AddSeconds(10)) }
    if ($script:stopCalls -ne 0 -or $script:runCalls -ne 0) { throw 'changed identity was mutated' }
    Assert-HandlesClosed
    $cases+=1
}

Reset-Fixture
$script:replaceDuringStop=$true
Expect-Rejected { Restart-OwnedTask $script:task ([DateTime]::UtcNow.AddSeconds(10)) }
if ($script:stopCalls -ne 1 -or $script:runCalls -ne 0) { throw 'replacement task was started' }
Assert-HandlesClosed
$cases+=1

foreach ($mode in @('failDuringStart','replaceDuringStart','reuseInstance')) {
    Reset-Fixture
    Set-Variable -Name $mode -Scope Script -Value $true
    Expect-Rejected { Restart-OwnedTask $script:task ([DateTime]::UtcNow.AddSeconds(10)) }
    if ($script:stopCalls -ne 1 -or $script:runCalls -ne 1) { throw 'uncertain Run was replayed' }
    Assert-HandlesClosed
    $cases+=1
}

Reset-Fixture
$context=Capture-TaskStop $script:task ([DateTime]::UtcNow.AddSeconds(10))
$null=$context.instance.Stop()
try {
    Expect-Rejected { Wait-Stopped $context.handles ([DateTime]::UtcNow.AddSeconds(-1)) }
    if ($script:runCalls -ne 0) { throw 'stop timeout started a replacement' }
} finally { Close-TaskProcessWait $context.handles }
Assert-HandlesClosed
$cases+=1

Reset-Fixture
$script:task.State=2
Expect-Rejected { Restart-OwnedTask $script:task ([DateTime]::UtcNow.AddSeconds(10)) }
if ($script:stopCalls -ne 0 -or $script:runCalls -ne 0) { throw 'unobserved queued action was mutated' }
$cases+=1

Reset-Fixture
$result=Stop-OwnedTask $script:task ([DateTime]::UtcNow.AddSeconds(10))
if ($result.State -ne 3 -or $script:stopCalls -ne 1 -or $script:runCalls -ne 0 -or $script:stopPolls -ne 3) {
    throw 'stop returned before owned workers exited'
}
Assert-HandlesClosed
$cases+=1
Write-Output ("Windows user-task lifecycle cases passed: " + $cases)
exit 0
