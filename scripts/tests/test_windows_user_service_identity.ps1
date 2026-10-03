$ErrorActionPreference = 'Stop'
$source = Join-Path $PSScriptRoot '../../crates/webcodex-environment/src/service/windows_user.ps1'
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'service script contains parse errors' }
foreach ($name in @('Account-Sid', 'Owned-Task')) {
    $function = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true)
    if ($null -ne $function) { . ([ScriptBlock]::Create($function.Extent.Text)) }
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$inputData = [pscustomobject]@{ sid=$identity.User.Value; description='identity regression fixture'; program='C:\fixture\runner.exe'; arguments='"--config" "C:\fixture\runner.toml"'; directory='C:\fixture' }
function Collection($item) {
    $collection = [pscustomobject]@{ Count=1; Value=$item }
    $collection | Add-Member -MemberType ScriptMethod -Name Item -Value { param($index) if ($index -ne 1) { throw 'unexpected index' }; $this.Value }
    return $collection
}
$principal = [pscustomobject]@{ UserId=$identity.Name; LogonType=3; RunLevel=0 }
$action = [pscustomobject]@{ Type=0; Path=$inputData.program; Arguments=$inputData.arguments; WorkingDirectory=$inputData.directory }
$trigger = [pscustomobject]@{ Type=9; UserId=$identity.Name; Enabled=$true }
$settings = [pscustomobject]@{ MultipleInstances=2; ExecutionTimeLimit='PT0S'; AllowHardTerminate=$true; RestartCount=3; RestartInterval='PT1M'; StartWhenAvailable=$true; DisallowStartIfOnBatteries=$false; StopIfGoingOnBatteries=$false }
$task = [pscustomobject]@{ Definition=[pscustomobject]@{ RegistrationInfo=[pscustomobject]@{ Description=$inputData.description }; Principal=$principal; Actions=(Collection $action); Triggers=(Collection $trigger); Settings=$settings } }
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
