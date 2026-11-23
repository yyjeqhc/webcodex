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
        $node.Name -in @('Initialize-ActionArgv', 'Executable-PathMatches', 'Action-Matches', 'Read-ActionProcess', 'Wait-Started')
}, $true))
if ($functions.Count -ne 5) { throw 'action observation functions missing' }
foreach ($definition in $functions) { . ([ScriptBlock]::Create($definition.Extent.Text)) }

function Reset-Fixture {
    $script:inputData = [pscustomobject]@{
        program='C:\Program Files\WebCodex\webcodex-server.exe'
        arguments='"--environment-env" "C:\Users\Test\environment\server\webcodex.env"'
        sid='S-1-5-21-42'
    }
    $actions = New-Object PSObject
    $actions | Add-Member ScriptMethod Item { param($index) return $script:action }
    $script:action = [pscustomobject]@{Path=$script:inputData.program;Arguments=$script:inputData.arguments}
    $script:task = [pscustomobject]@{ Owned=$true; Enabled=$true; State=4; Definition=@{Actions=$actions} }
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
    if ($Filter -cmatch '^ParentProcessId = ([0-9]+)$') { return @($script:children | Where-Object { $_.ParentProcessId -eq [uint32]$Matches[1] }) }
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

# Production comparisons must accept every ordinary/extended image and argv[0]
# combination without changing the remaining argv or process identity checks.
$pathCases = 0
$normalProgram = 'C:\Program Files\WebCodex\测试\webcodex-server.exe'
foreach ($programExtended in @($false, $true)) {
    foreach ($imageExtended in @($false, $true)) {
        foreach ($argvExtended in @($false, $true)) {
            Reset-Fixture
            $script:inputData.program = if ($programExtended) { '\\?\' + $normalProgram } else { $normalProgram }
            $script:action.Path = $script:inputData.program
            $script:processes[0].ExecutablePath = if ($imageExtended) { '\\?\' + $normalProgram } else { $normalProgram }
            $argvProgram = if ($argvExtended) { '\\?\' + $normalProgram } else { $normalProgram }
            $script:processes[0].CommandLine = '"' + $argvProgram + '" ' + $script:inputData.arguments
            if ((Read-ActionProcess $script:task).process_id -ne 4312) { throw 'equivalent DOS path spelling rejected' }
            $pathCases += 1
        }
    }
}
foreach ($normal in @('C:\CON Tools\WebCodex.exe', 'D:\COM10\WebCodex.exe', 'E:\.local\WebCodex.exe')) {
    $extended = '\\?\' + $normal
    if (!(Executable-PathMatches $normal $extended) -or !(Executable-PathMatches $extended $normal) -or
        !(Executable-PathMatches $normal.ToLowerInvariant() $extended.ToUpperInvariant())) {
        throw 'ordinary non-device directory rejected'
    }
    $pathCases += 1
}
# These spellings can change Win32 interpretation when a prefix is removed.
# Test only the new cross-prefix branch; existing exact-equality behavior stays.
$rejected = @(
    'C:Apps\webcodex.exe', '\Apps\webcodex.exe', 'Apps\webcodex.exe',
    'C:/Apps/webcodex.exe', 'C:\Apps\\webcodex.exe',
    'C:\Apps\.\webcodex.exe', 'C:\Apps\..\webcodex.exe',
    'C:\Apps.\webcodex.exe', 'C:\Apps \webcodex.exe',
    'C:\Apps\webcodex.exe.', 'C:\Apps\webcodex.exe ',
    'C:\Apps\webcodex.exe:other', 'C:\Apps\', 'C:\',
    'C:\CON\webcodex.exe', 'C:\nul.txt\webcodex.exe',
    'C:\COM1\webcodex.exe', 'C:\LPT9\webcodex.exe',
    'C:\CONIN$\webcodex.exe', 'C:\CONOUT$.txt\webcodex.exe',
    'C:\AUX .txt\webcodex.exe', 'C:\Apps\PRN.txt', 'C:\Apps\nul.tar.gz',
    ('C:\COM' + [char]0x00b9 + '\webcodex.exe'),
    ('C:\LPT' + [char]0x00b2 + '.txt\webcodex.exe')
)
foreach ($character in @('<', '>', '"', '|', '?', '*', [char]0, [char]1, [char]31)) {
    $rejected += 'C:\App' + $character + 's\webcodex.exe'
}
foreach ($normal in $rejected) {
    if ((Executable-PathMatches $normal ('\\?\' + $normal)) -or
        (Executable-PathMatches ('\\?\' + $normal) $normal)) { throw 'ambiguous DOS path became equivalent' }
    $pathCases += 1
}
foreach ($other in @(
    '\\.\C:\Apps\webcodex.exe', '\??\C:\Apps\webcodex.exe',
    '\\?\GLOBALROOT\Device\HarddiskVolume1\Apps\webcodex.exe',
    '\\?\D:\Apps\webcodex.exe', '\\?\C:\Other\webcodex.exe'
)) {
    if (Executable-PathMatches $other 'C:\Apps\webcodex.exe') { throw 'different namespace or target accepted' }
    $pathCases += 1
}
if ((Executable-PathMatches '\\server\share\webcodex.exe' '\\?\UNC\server\share\webcodex.exe') -or
    (Executable-PathMatches $null $null) -or (Executable-PathMatches '' '')) { throw 'unsupported or empty path accepted' }
$pathCases += 3

Reset-Fixture
$observed=Read-ActionProcess $script:task
if ($observed.process_id -ne 4312 -or $observed.instance_guid -cne $script:instance.InstanceGuid) { throw 'current action was not observed' }
Reset-Fixture
$script:processes[0].CommandLine=$script:processes[0].CommandLine.Replace('webcodex-server.exe','WEBCODEX-SERVER.EXE')
if ((Read-ActionProcess $script:task).process_id -ne 4312) { throw 'executable spelling changed action identity' }
Reset-Fixture
$script:inputData.program='C:\WebCodex\webcodex-server.exe'
$script:action.Path=$script:inputData.program
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
Reset-Fixture
$script:inputData | Add-Member NoteProperty host 'C:\Apps\WebCodex.exe'
$script:action.Path=$script:inputData.host
$script:action.Arguments='--user-task "server" "env" '+$script:inputData.arguments
$script:children=@($script:processes[0].PSObject.Copy())
$script:children[0].ProcessId=5000
$script:children[0] | Add-Member NoteProperty ParentProcessId 4312
$script:processes[0].ExecutablePath=$script:action.Path
$script:processes[0].CommandLine='"'+$script:action.Path+'" '+$script:action.Arguments
if ((Read-ActionProcess $script:task).process_id -ne 5000) { throw 'GUI task must report its worker PID' }
# Match the installed MSI failure: extended expected/argv paths, ordinary CIM
# worker image path, and an extended GUI action. Start observes the same GUID.
$script:inputData.program = '\\?\' + $script:inputData.program
$script:inputData.host = '\\?\' + $script:inputData.host
$script:action.Path = $script:inputData.host
$script:processes[0].ExecutablePath = $script:inputData.host
$script:processes[0].CommandLine = '"' + $script:inputData.host + '" ' + $script:action.Arguments
$script:children[0].CommandLine = '"' + $script:inputData.program + '" ' + $script:inputData.arguments
if ((Read-ActionProcess $script:task).process_id -ne 5000) { throw 'mixed-path GUI worker was not confirmed' }
if ((Wait-Started $script:instance.InstanceGuid ([DateTime]::UtcNow.AddSeconds(5))).State -ne 4) {
    throw 'mixed-path startup did not confirm its owned instance'
}
$pathCases += 1
$command = $script:children[0].CommandLine
$script:children[0].CommandLine = $command.Replace('--environment-env', '--ENVIRONMENT-ENV')
Expect-Unverified 'mixed-path worker with different argv'
$script:children[0].CommandLine = $command
$script:children[0].OwnerSid = 'S-1-5-21-99'
Expect-Unverified 'mixed-path worker with different owner'
$script:children[0].OwnerSid = $script:inputData.sid
$script:children+=@($script:children[0].PSObject.Copy())
Expect-Unverified 'ambiguous GUI worker'
Write-Output ("Windows user-task PID identity fixture passed; DOS path cases: " + $pathCases)
exit 0
