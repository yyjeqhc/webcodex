# Exercise the actual observation functions with in-memory Task Scheduler/CIM
# objects. This fixture never connects to the scheduler, registers or runs a task.
$ErrorActionPreference = 'Stop'
$tokens = $null
$parseErrors = $null
$syntax = [Management.Automation.Language.Parser]::ParseInput($env:WEBCODEX_PARSE_SCRIPT, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw 'production task script does not parse' }
$functions = @($syntax.FindAll({
    param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -in @('Initialize-ActionArgv', 'Action-Matches', 'Read-ActionProcess')
}, $true))
if ($functions.Count -ne 3) { throw 'action observation functions missing' }
foreach ($definition in $functions) { . ([ScriptBlock]::Create($definition.Extent.Text)) }

function Reset-Fixture {
    $script:inputData = [pscustomobject]@{
        program='C:\Program Files\WebCodex\webcodex-server.exe'
        arguments='"--environment-env" "C:\Users\Test\environment\server\webcodex.env"'
        sid='S-1-5-21-42'
    }
    $script:task = [pscustomobject]@{ Owned=$true; State=4 }
    $script:task | Add-Member ScriptMethod GetInstances { param($flags) return $script:instances }
    $script:instances = [pscustomobject]@{ Count=1 }
    $script:instances | Add-Member ScriptMethod Item { param($index) return $script:instance }
    $script:instance = [pscustomobject]@{ State=4; InstanceGuid='{723A491B-825B-4C30-BB2B-F86AA98F94AA}'; EnginePID=4312 }
    $script:processes = @([pscustomobject]@{
        ProcessId=4312; ExecutablePath=$script:inputData.program
        CommandLine=('"'+$script:inputData.program+'" '+$script:inputData.arguments)
        CreationDate=[DateTime]::Parse('2026-10-12T00:00:00Z').ToUniversalTime()
        OwnerSid=$script:inputData.sid
    })
    $script:replaceProcess=$false
    $script:replaceTask=$false
    $script:replaceEngine=$false
    $script:replaceOwner=$false
    $script:processReads=0
    $script:ownerReads=0
}
function Owned-Task($value) { return $null -ne $value -and $value.Owned }
function Read-Task {
    if ($script:replaceTask) { $script:instance.InstanceGuid='{D55260D3-00A1-43C6-894E-36A119A4430D}' }
    if ($script:replaceEngine) { $script:instance.EnginePID=4313 }
    return $script:task
}
function Get-CimInstance {
    param($ClassName, $Filter, $Property, $OperationTimeoutSec)
    if ($ClassName -cne 'Win32_Process' -or $Filter -cnotmatch '^ProcessId = ([0-9]+)$') { throw 'unexpected CIM query' }
    $queriedPid=[uint32]$Matches[1]
    $script:processReads+=1
    foreach ($process in $script:processes) {
        if ($process.ProcessId -ne $queriedPid) { continue }
        $value=$process.PSObject.Copy()
        if ($script:replaceProcess -and $script:processReads -gt 1) { $value.CreationDate=$value.CreationDate.AddSeconds(1) }
        $value
    }
}
function Invoke-CimMethod {
    param($InputObject, $MethodName, $OperationTimeoutSec)
    if ($MethodName -cne 'GetOwnerSid') { throw 'unexpected CIM method' }
    $script:ownerReads+=1
    $sid=$InputObject.OwnerSid
    if ($script:replaceOwner -and $script:ownerReads -gt 1) { $sid='S-1-5-21-99' }
    return [pscustomobject]@{ ReturnValue=0; Sid=$sid }
}
function Expect-Unverified($label) {
    if ($null -ne (Read-ActionProcess $script:task)) { throw "unexpected process acceptance: $label" }
}

Reset-Fixture
$observed=Read-ActionProcess $script:task
if ($observed.process_id -ne 4312 -or $observed.instance_guid -cne $script:instance.InstanceGuid) { throw 'current action was not observed' }
Reset-Fixture
$script:processes[0].CommandLine=$script:processes[0].CommandLine.Replace('webcodex-server.exe','WEBCODEX-SERVER.EXE')
if ((Read-ActionProcess $script:task).process_id -ne 4312) { throw 'executable spelling changed action identity' }
Reset-Fixture
$script:inputData.program='C:\WebCodex\webcodex-server.exe'
$script:processes[0].ExecutablePath=$script:inputData.program
$script:processes[0].CommandLine=$script:inputData.program+'   --environment-env "C:\Users\Test\environment\server\webcodex.env"'
if ((Read-ActionProcess $script:task).process_id -ne 4312) { throw 'equivalent quoting/spacing changed argv identity' }
Reset-Fixture
$script:task.State=2
Expect-Unverified 'queued task'
Reset-Fixture
$script:task.Owned=$false
Expect-Unverified 'foreign task'
Reset-Fixture
$script:instances.Count=2
Expect-Unverified 'multiple task instances'
Reset-Fixture
$script:instance.EnginePID=0
Expect-Unverified 'unobserved engine PID'
Reset-Fixture
$script:processes[0].CommandLine=$script:processes[0].CommandLine.Replace('--environment-env','--ENVIRONMENT-ENV')
Expect-Unverified 'different argv'
Reset-Fixture
$script:processes[0].ExecutablePath='C:\other\webcodex-server.exe'
Expect-Unverified 'different executable'
Reset-Fixture
$script:processes[0].OwnerSid='S-1-5-21-99'
Expect-Unverified 'different owner'
Reset-Fixture
$script:processes+=@($script:processes[0].PSObject.Copy())
Expect-Unverified 'ambiguous PID observation'
Reset-Fixture
$manual=$script:processes[0].PSObject.Copy()
$manual.ProcessId=4313
$script:processes+=@($manual)
if ((Read-ActionProcess $script:task).process_id -ne 4312) { throw 'unrelated identical command displaced the task action' }
$script:processes[0].ExecutablePath='C:\Windows\System32\taskeng.exe'
$script:processes[0].CommandLine='"C:\Windows\System32\taskeng.exe"'
Expect-Unverified 'manual identical command is not the task engine'
Reset-Fixture
$script:processes=@()
Expect-Unverified 'engine process disappeared'
Reset-Fixture
$script:replaceProcess=$true
Expect-Unverified 'PID reuse'
Reset-Fixture
$script:replaceTask=$true
Expect-Unverified 'new task instance'
Reset-Fixture
$script:replaceEngine=$true
Expect-Unverified 'changed task engine'
Reset-Fixture
$script:replaceOwner=$true
Expect-Unverified 'changed process owner'
Reset-Fixture
$script:processes[0].CommandLine=$null
Expect-Unverified 'unreadable command line'
exit 0
