param(
    [switch]$MockOnly,
    [switch]$NativeRoundtrip
)

$ErrorActionPreference = 'Stop'
if ($MockOnly -and $NativeRoundtrip) { throw 'native roundtrip cannot run with MockOnly' }
$source = Join-Path $PSScriptRoot '../../crates/webcodex-environment/src/service/windows_user.ps1'
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'service script contains parse errors' }
foreach ($name in @('Resolve-AccountSid', 'Account-Sid', 'Owned-Task')) {
    $function = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true)
    if ($null -eq $function) { throw "missing production function: $name" }
    . ([ScriptBlock]::Create($function.Extent.Text))
}
function Collection($item) {
    $collection = [pscustomobject]@{ Count=1; Value=$item }
    $collection | Add-Member -MemberType ScriptMethod -Name Item -Value { param($index) if ($index -ne 1) { throw 'unexpected index' }; $this.Value }
    return $collection
}
function New-Fixture($sid) {
    $script:inputData = [pscustomobject]@{ sid=$sid; description='identity regression fixture'; program='C:\fixture\runner.exe'; arguments='"--config" "C:\fixture\runner.toml"'; directory='C:\fixture' }
    $principal = [pscustomobject]@{ UserId=$sid; LogonType=3; RunLevel=0 }
    $action = [pscustomobject]@{ Type=0; Path=$inputData.program; Arguments=$inputData.arguments; WorkingDirectory=$inputData.directory }
    $trigger = [pscustomobject]@{ Type=9; UserId=$sid; Enabled=$true }
    $settings = [pscustomobject]@{ MultipleInstances=2; ExecutionTimeLimit='PT0S'; AllowHardTerminate=$true; RestartCount=3; RestartInterval='PT1M'; StartWhenAvailable=$true; DisallowStartIfOnBatteries=$false; StopIfGoingOnBatteries=$false }
    return [pscustomobject]@{ Definition=[pscustomobject]@{ RegistrationInfo=[pscustomobject]@{ Description=$inputData.description }; Principal=$principal; Actions=(Collection $action); Triggers=(Collection $trigger); Settings=$settings } }
}

# Mock only the OS lookup boundary. Exercise the production collision decision
# and both Owned-Task call sites without renaming a host or creating an account.
# This scope also runs headlessly on PowerShell 7 on non-Windows hosts.
& {
    $computer = [Environment]::MachineName
    $domainSid = 'S-1-5-21-101-202-303'
    $ownerSid = "$domainSid-1001"
    $foreignSid = "$domainSid-1002"
    $task = New-Fixture $ownerSid
    $principal = $task.Definition.Principal
    $trigger = $task.Definition.Triggers.Item(1)
    $lookups = New-Object 'System.Collections.Generic.List[string]'
    $answers = @{}
    function Resolve-AccountSid($identity) {
        $lookups.Add([string]$identity)
        if ($null -eq $identity -or !$answers.ContainsKey([string]$identity)) { return $null }
        $value = $answers[[string]$identity]
        if ($null -eq $value) { return $null }
        return [pscustomobject]@{ Value=$value; AccountDomainSid=[pscustomobject]@{ Value=$domainSid } }
    }
    function Expect-Owner($name, $expected, $label) {
        foreach ($field in @($principal, $trigger)) {
            $principal.UserId=$ownerSid
            $trigger.UserId=$ownerSid
            $field.UserId=$name
            if ((Owned-Task $task) -ne $expected) { throw "incorrect ownership: $label" }
        }
    }
    $answers[$ownerSid]=$ownerSid
    $answers[$foreignSid]=$foreignSid
    $answers["$computer\$computer"]=$ownerSid
    foreach ($direct in @($null, $domainSid, $ownerSid)) {
        $answers[$computer]=$direct
        foreach ($name in @($computer.ToUpperInvariant(), $computer.ToLowerInvariant())) {
            Expect-Owner $name $true 'computer/user collision with matching qualified SID'
        }
    }
    $answers[$computer]=$null
    $principal.UserId=$computer
    $trigger.UserId=$computer
    if (!(Owned-Task $task)) { throw 'collision in both principal and trigger was rejected' }
    foreach ($direct in @($foreignSid, 'S-1-5-21-404-505-606', 'S-1-5-18')) {
        $answers[$computer]=$direct
        Expect-Owner $computer $false 'resolved foreign user, domain or well-known SID'
    }
    $answers[$computer]=$null
    foreach ($qualified in @($null, $foreignSid)) {
        $answers["$computer\$computer"]=$qualified
        Expect-Owner $computer $false 'unresolved or foreign local account'
    }
    $answers["$computer\$computer"]=$ownerSid
    foreach ($name in @('ordinary-user', "$computer\ordinary-user", 'DOMAIN\ordinary-user', 'ordinary-user@example.test', $ownerSid)) {
        $answers[$name]=$ownerSid
        Expect-Owner $name $true 'ordinary resolved identity'
    }
    foreach ($name in @($null, '', ' ', 'missing-user', 'ordinary-user', 'DOMAIN\ordinary-user', 'ordinary-user@example.test', 'S-1-invalid', $foreignSid)) {
        foreach ($direct in @($null, $foreignSid)) {
            if ($null -ne $name) { $answers[$name]=$direct }
            $lookups.Clear()
            Expect-Owner $name $false 'foreign or unresolved identity outside collision'
            if ($lookups.Contains("$computer\$name")) { throw 'non-collision identity was requalified' }
        }
    }
    # Retain every definition fence even when both identities need the fallback.
    $principal.UserId=$computer
    $trigger.UserId=$computer
    $action=$task.Definition.Actions.Item(1)
    foreach ($change in @(
        @($task.Definition.RegistrationInfo, 'Description', 'foreign'),
        @($principal, 'LogonType', 1), @($principal, 'RunLevel', 1),
        @($task.Definition.Actions, 'Count', 2), @($action, 'Type', 5),
        @($action, 'Path', 'foreign'), @($action, 'Arguments', 'foreign'), @($action, 'WorkingDirectory', 'foreign'),
        @($task.Definition.Triggers, 'Count', 2), @($trigger, 'Type', 1), @($trigger, 'Enabled', $false),
        @($task.Definition.Settings, 'MultipleInstances', 0), @($task.Definition.Settings, 'ExecutionTimeLimit', 'PT1H'),
        @($task.Definition.Settings, 'AllowHardTerminate', $false), @($task.Definition.Settings, 'RestartCount', 0),
        @($task.Definition.Settings, 'RestartInterval', 'PT2M'), @($task.Definition.Settings, 'StartWhenAvailable', $false),
        @($task.Definition.Settings, 'DisallowStartIfOnBatteries', $true), @($task.Definition.Settings, 'StopIfGoingOnBatteries', $true)
    )) {
        $object, $property, $value = $change
        $saved=$object.$property
        $object.$property=$value
        if (Owned-Task $task) { throw "changed task definition was accepted: $property" }
        $object.$property=$saved
    }
    if (!(Owned-Task $task)) { throw 'restored collision fixture was rejected' }
    if (Owned-Task $null) { throw 'missing task was accepted' }
}
'PASS: deterministic collision, ambiguity and complete task-definition checks'
if ($MockOnly) { return }

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$task = New-Fixture $identity.User.Value
$principal = $task.Definition.Principal
$trigger = $task.Definition.Triggers.Item(1)
$action = $task.Definition.Actions.Item(1)
foreach ($alias in @($identity.Name, ($identity.Name -split '\\')[-1], $identity.User.Value)) {
    $principal.UserId=$alias
    $trigger.UserId=$alias
    if (!(Owned-Task $task)) { throw 'same-owner alias was rejected' }
}
$principal.UserId=($identity.Name -split '\\')[-1]
$trigger.UserId=$identity.Name
if (!(Owned-Task $task)) { throw 'mixed same-owner aliases were rejected' }
$principal.UserId=$identity.User.Value
$trigger.UserId=$identity.User.Value
$foreignSid = if ($identity.User.Value -eq 'S-1-5-18') { 'S-1-5-19' } else { 'S-1-5-18' }
foreach ($invalid in @('', 'WebCodexMissingAccount_7ce7c11f', 'S-1-invalid', $foreignSid)) {
    $principal.UserId=$invalid
    if (Owned-Task $task) { throw 'foreign or unresolved principal was accepted' }
    $principal.UserId=$identity.User.Value
    $trigger.UserId=$invalid
    if (Owned-Task $task) { throw 'foreign or unresolved trigger was accepted' }
    $trigger.UserId=$identity.User.Value
}
foreach ($property in @('Path', 'Arguments', 'WorkingDirectory')) {
    $saved=$action.$property
    $action.$property='incorrect'
    if (Owned-Task $task) { throw 'changed action definition was accepted' }
    $action.$property=$saved
}
'PASS: owner aliases accepted; foreign and unresolved identities and changed action definitions rejected'

if ($NativeRoundtrip) {
    # Run the full production script in a child-local environment, registering a
    # unique disposable task. Its nonexistent executable is never started.
    $name='WebCodex-user-identity-test-'+[Guid]::NewGuid().ToString('N')
    $request=@{
        operation='install'; name=$name; sid=$identity.User.Value; description=$name
        program=(Join-Path $env:TEMP "$name\missing.exe"); arguments='--identity-fixture'; directory=$env:TEMP
    }
    function Invoke-ServiceScript($operation) {
        $request.operation=$operation
        $start=New-Object Diagnostics.ProcessStartInfo
        $start.FileName=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
        $start.Arguments='-NoLogo -NoProfile -NonInteractive -File "'+$source+'"'
        $start.UseShellExecute=$false
        $start.CreateNoWindow=$true
        $start.RedirectStandardInput=$true
        $start.RedirectStandardOutput=$true
        $start.RedirectStandardError=$true
        $start.EnvironmentVariables['WEBCODEX_SERVICE_REQUEST']=($request | ConvertTo-Json -Compress)
        $process=[Diagnostics.Process]::Start($start)
        try {
            $stdout=$process.StandardOutput.ReadToEndAsync()
            $stderr=$process.StandardError.ReadToEndAsync()
            $process.StandardInput.Close()
            if (!$process.WaitForExit(30000)) {
                $process.Kill()
                $null=$process.WaitForExit(5000)
                throw 'native service fixture timed out'
            }
            if ($process.ExitCode -ne 0) { throw "native service fixture failed: $operation" }
            return ($stdout.GetAwaiter().GetResult() | ConvertFrom-Json)
        } finally { $process.Dispose() }
    }
    $scheduler=New-Object -ComObject 'Schedule.Service'
    $scheduler.Connect()
    $folder=$scheduler.GetFolder('\')
    try {
        foreach ($operation in @('install', 'inspect')) {
            $observed=Invoke-ServiceScript $operation
            if ($observed.exists -ne $true -or $observed.owned -ne $true) { throw "registered task was not owned: $operation" }
        }
        # A changed expected marker must not adopt the registered native task.
        $request.description='foreign'
        $observed=Invoke-ServiceScript 'inspect'
        if ($observed.exists -ne $true -or $observed.owned -ne $false) { throw 'native foreign task was accepted' }
        $request.description=$name
        $observed=Invoke-ServiceScript 'uninstall'
        if ($observed.exists -ne $false) { throw 'native task uninstall was not confirmed' }
    } finally {
        # Cleanup also works when an ownership assertion fails; restrict it to
        # this unique fixture's marker/action and a task that was never started.
        foreach ($candidate in $folder.GetTasks(1)) {
            if ($candidate.Name -cne $name) { continue }
            if ($candidate.Definition.RegistrationInfo.Description -cne $name -or
                $candidate.Definition.Actions.Count -ne 1 -or
                $candidate.Definition.Actions.Item(1).Path -cne $request.program -or
                $candidate.State -notin @(1,3)) { throw 'fixture cleanup ownership unconfirmed' }
            $folder.DeleteTask($name, 0)
        }
    }
    'PASS: native user Task Scheduler install/inspect/foreign/uninstall roundtrip'
}
