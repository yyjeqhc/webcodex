$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "windows_runner_process_identity.ps1")
. (Join-Path $PSScriptRoot "windows_runner_readiness.ps1")

function Assert-True($Value, [string]$Message) {
    if (-not $Value) { throw $Message }
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

function New-RunnerObservation {
    param(
        [string]$InstanceId,
        [string]$Commit,
        [bool]$Dirty = $false,
        [bool]$Connected = $true,
        [string]$Compatibility = "compatible",
        [switch]$OmitCompatibility
    )
    $observation = [pscustomobject]@{
        client_id = "msi"
        connected = $Connected
        status = if ($Connected) { "online" } else { "stale" }
        agent_instance_id = $InstanceId
        build = [pscustomobject]@{
            version = "0.3.8"
            git_commit = $Commit
            git_dirty = $Dirty
            built_at = "1787554000"
        }
        compatibility_status = $Compatibility
        source_alignment = [pscustomobject]@{ status = "different" }
    }
    if ($OmitCompatibility) {
        $observation.PSObject.Properties.Remove('compatibility_status')
    }
    return $observation
}

$candidateBuild = [pscustomobject]@{ GitCommit = "candidate1234"; GitDirty = $false }
$rollbackBuild = [pscustomobject]@{ GitCommit = "rollback5678"; GitDirty = $false }
$oldInstanceId = "instance-old"

# Old registration is stale for replacement readiness.
$decision = Get-RunnerReadinessDecision `
    -Observation (New-RunnerObservation -InstanceId $oldInstanceId -Commit $candidateBuild.GitCommit) `
    -ExpectedClientId "msi" `
    -ExpectedBuild $candidateBuild `
    -DisallowedAgentInstanceIds @($oldInstanceId)
Assert-Equal "not_ready" $decision.State "old agent_instance_id was accepted"
Assert-Equal "stale_agent_instance_id" $decision.Reason "old instance diagnostic changed"
$decision = Get-RunnerReadinessDecision `
    -Observation (New-RunnerObservation -InstanceId $oldInstanceId -Commit $candidateBuild.GitCommit -Compatibility "version_mismatch") `
    -ExpectedClientId "msi" `
    -ExpectedBuild $candidateBuild `
    -DisallowedAgentInstanceIds @($oldInstanceId) `
    -AllowVersionMismatch
Assert-Equal "not_ready" $decision.State "version override weakened stale instance fencing"
Assert-Equal "stale_agent_instance_id" $decision.Reason "stale instance override diagnostic changed"

# Fresh exact candidate is Ready even when source differs from Server.
$freshCandidate = New-RunnerObservation -InstanceId "instance-new" -Commit $candidateBuild.GitCommit
$decision = Get-RunnerReadinessDecision -Observation $freshCandidate -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -DisallowedAgentInstanceIds @($oldInstanceId)
Assert-Equal "ready" $decision.State "fresh exact candidate was not Ready"
Assert-Equal "exact_fresh_build_ready" $decision.Reason "compatible readiness reason changed"

# Package version mismatch is strict by default and only the explicit rolling-upgrade
# override may accept it. Source alignment remains diagnostic-only.
$versionMismatch = New-RunnerObservation -InstanceId "instance-version-mismatch" -Commit $candidateBuild.GitCommit -Compatibility "version_mismatch"
$decision = Get-RunnerReadinessDecision -Observation $versionMismatch -ExpectedClientId "msi" -ExpectedBuild $candidateBuild
Assert-Equal "mismatch" $decision.State "version mismatch was accepted without opt-in"
Assert-Equal "runner_version_mismatch" $decision.Reason "version mismatch diagnostic changed"
$decision = Get-RunnerReadinessDecision -Observation $versionMismatch -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -AllowVersionMismatch
Assert-Equal "ready" $decision.State "explicit rolling-upgrade version mismatch was rejected"
Assert-Equal "version_mismatch_allowed_for_rolling_upgrade" $decision.Reason "rolling-upgrade override was not explicit in evidence"

$missingCompatibility = New-RunnerObservation -InstanceId "instance-missing-compatibility" -Commit $candidateBuild.GitCommit -OmitCompatibility
$decision = Get-RunnerReadinessDecision -Observation $missingCompatibility -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -AllowVersionMismatch
Assert-Equal "mismatch" $decision.State "missing compatibility facts were accepted"
Assert-Equal "runner_compatibility_status_unavailable" $decision.Reason "missing compatibility diagnostic changed"

$unknownCompatibility = New-RunnerObservation -InstanceId "instance-unknown-compatibility" -Commit $candidateBuild.GitCommit -Compatibility "future_unknown"
$decision = Get-RunnerReadinessDecision -Observation $unknownCompatibility -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -AllowVersionMismatch
Assert-Equal "mismatch" $decision.State "unknown compatibility status was accepted"
Assert-Equal "runner_compatibility_status_unknown" $decision.Reason "unknown compatibility diagnostic changed"

# Fresh wrong source identities must never become Ready.
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-wrong-commit" -Commit "wrong1234567") -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -DisallowedAgentInstanceIds @($oldInstanceId)
Assert-Equal "mismatch" $decision.State "wrong commit was accepted"
Assert-Equal "unexpected_build_commit" $decision.Reason "wrong commit diagnostic changed"
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-wrong-dirty" -Commit $candidateBuild.GitCommit -Dirty $true) -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -DisallowedAgentInstanceIds @($oldInstanceId)
Assert-Equal "mismatch" $decision.State "wrong dirty state was accepted"
Assert-Equal "unexpected_build_dirty_state" $decision.Reason "wrong dirty diagnostic changed"
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-wrong-commit-override" -Commit "wrong1234567" -Compatibility "version_mismatch") -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -AllowVersionMismatch
Assert-Equal "mismatch" $decision.State "version override weakened exact commit identity"
Assert-Equal "unexpected_build_commit" $decision.Reason "wrong commit override diagnostic changed"
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-wrong-dirty-override" -Commit $candidateBuild.GitCommit -Dirty $true -Compatibility "version_mismatch") -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -AllowVersionMismatch
Assert-Equal "mismatch" $decision.State "version override weakened dirty identity"
Assert-Equal "unexpected_build_dirty_state" $decision.Reason "wrong dirty override diagnostic changed"

# Disconnected state remains transitional/not Ready.
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-disconnected" -Commit $candidateBuild.GitCommit -Connected $false) -ExpectedClientId "msi" -ExpectedBuild $candidateBuild
Assert-Equal "not_ready" $decision.State "disconnected Runner was accepted"
Assert-Equal "control_plane_runner_not_connected" $decision.Reason "disconnected diagnostic changed"
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-disconnected-override" -Commit $candidateBuild.GitCommit -Connected $false -Compatibility "version_mismatch") -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -AllowVersionMismatch
Assert-Equal "not_ready" $decision.State "version override weakened connected-state readiness"
Assert-Equal "control_plane_runner_not_connected" $decision.Reason "disconnected override diagnostic changed"

# Explicit protocol incompatibility is fatal mismatch even with the version-only override.
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-incompatible" -Commit $candidateBuild.GitCommit -Compatibility "capability_mismatch") -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -AllowVersionMismatch
Assert-Equal "mismatch" $decision.State "unsupported protocol was accepted"
Assert-Equal "runner_protocol_incompatible" $decision.Reason "protocol mismatch diagnostic changed"

$null = Assert-Throws {
    Assert-PreReplacementRunnerObservation -Observation $versionMismatch -ExpectedClientId "msi" -ExpectedBuild $candidateBuild
} "Pre-replacement Runner build identity does not match"
$acceptedPreReplacement = Assert-PreReplacementRunnerObservation -Observation $versionMismatch -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -AllowVersionMismatch
Assert-Equal "instance-version-mismatch" $acceptedPreReplacement.agent_instance_id "pre-replacement override did not reach readiness policy"

$waitStart = [DateTime]::Parse("2026-08-24T00:00:00Z").ToUniversalTime()
$allowedWait = Wait-RunnerControlPlaneReadiness `
    -Observe { param([int]$RequestTimeoutMilliseconds) $versionMismatch } `
    -ExpectedClientId "msi" `
    -ExpectedBuild $candidateBuild `
    -DeadlineUtc $waitStart.AddSeconds(1) `
    -AllowVersionMismatch `
    -UtcNow { $waitStart }
Assert-Equal "ready" $allowedWait.Decision.State "wait helper dropped version mismatch override"
Assert-Equal "version_mismatch_allowed_for_rolling_upgrade" $allowedWait.Decision.Reason "wait helper lost explicit override evidence"

# Rollback reconciliation uses rollback build, not candidate build, and excludes
# both the pre-replacement and already-observed candidate instances.
$rollbackObservation = New-RunnerObservation -InstanceId "instance-rollback" -Commit $rollbackBuild.GitCommit
$decision = Get-RunnerReadinessDecision -Observation $rollbackObservation -ExpectedClientId "msi" -ExpectedBuild $rollbackBuild -DisallowedAgentInstanceIds @($oldInstanceId, "instance-new")
Assert-Equal "ready" $decision.State "exact rollback build was not Ready"
$decision = Get-RunnerReadinessDecision -Observation $rollbackObservation -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -DisallowedAgentInstanceIds @($oldInstanceId, "instance-new")
Assert-Equal "mismatch" $decision.State "rollback build incorrectly satisfied candidate expectation"

# Rollback stop tolerates only the exact captured identity exiting concurrently
# after Task disable; a still-live identity preserves P1a fail-closed behavior.
$identityFixture = [pscustomobject]@{ Id = 123; CreationTime = [uint64]456 }
$script:liveChecks = @($true, $false)
$stopOutcome = Stop-CapturedPrimaryRunnerForRollback `
    -Identity $identityFixture `
    -IsLive { param($Identity) $next = $script:liveChecks[0]; $script:liveChecks = @($script:liveChecks | Select-Object -Skip 1); return $next } `
    -StopExact { param($Identity) throw "exact role no longer provable" }
Assert-Equal "exited_during_stop" $stopOutcome "concurrent supervisor exit aborted rollback"

$stopOutcome = Stop-CapturedPrimaryRunnerForRollback `
    -Identity $identityFixture `
    -IsLive { param($Identity) return $false } `
    -StopExact { param($Identity) throw "stop should not run" }
Assert-Equal "already_exited" $stopOutcome "already-exited rollback identity was not accepted"

$null = Assert-Throws {
    Stop-CapturedPrimaryRunnerForRollback `
        -Identity $identityFixture `
        -IsLive { param($Identity) return $true } `
        -StopExact { param($Identity) throw "exact role no longer provable" }
} "exact role no longer provable"

# Absolute deadline: transient progress never resets the original deadline, and
# each control-plane request receives at most the remaining deadline budget.
$script:fakeNow = [DateTime]::Parse("2026-08-24T00:00:00Z").ToUniversalTime()
$deadline = $script:fakeNow.AddMilliseconds(1000)
$script:probeCount = 0
$script:requestTimeouts = @()
$observe = {
    param([int]$RequestTimeoutMilliseconds)
    $script:probeCount++
    $script:requestTimeouts += $RequestTimeoutMilliseconds
    New-RunnerObservation -InstanceId $oldInstanceId -Commit $candidateBuild.GitCommit
}
$now = { $script:fakeNow }
$sleep = { param([int]$Milliseconds) $script:fakeNow = $script:fakeNow.AddMilliseconds($Milliseconds) }
$null = Assert-Throws {
    Wait-RunnerControlPlaneReadiness `
        -Observe $observe `
        -ExpectedClientId "msi" `
        -ExpectedBuild $candidateBuild `
        -DeadlineUtc $deadline `
        -DisallowedAgentInstanceIds @($oldInstanceId) `
        -PollIntervalMilliseconds 250 `
        -UtcNow $now `
        -Sleep $sleep
} "Runner readiness timeout"
Assert-Equal $deadline $script:fakeNow "readiness wait exceeded or reset its absolute deadline"
Assert-Equal 4 $script:probeCount "absolute deadline produced unexpected probe count"
Assert-True (($script:requestTimeouts -join ',') -eq '1000,750,500,250') "request timeout did not track remaining absolute deadline"

# The real deployment entrypoint must expose and explicitly thread the opt-in to
# pre-replacement, candidate, and rollback readiness. Static AST inspection keeps
# this test side-effect free while proving the option is not dead wiring.
$deployPath = Join-Path $PSScriptRoot "deploy_windows_runner_dogfood.ps1"
$deployTokens = $null
$deployParseErrors = $null
$deployAst = [System.Management.Automation.Language.Parser]::ParseFile($deployPath, [ref]$deployTokens, [ref]$deployParseErrors)
Assert-Equal 0 @($deployParseErrors).Count "deployment helper parser errors"
$allowVersionParameters = @($deployAst.ParamBlock.Parameters | Where-Object { $_.Name.VariablePath.UserPath -eq 'AllowVersionMismatch' })
Assert-Equal 1 $allowVersionParameters.Count "deployment helper does not expose exactly one AllowVersionMismatch opt-in"
$deployReadinessCalls = @($deployAst.FindAll({
    param($Node)
    if ($Node -isnot [System.Management.Automation.Language.CommandAst]) { return $false }
    $name = $Node.GetCommandName()
    return $name -eq 'Assert-PreReplacementRunnerObservation' -or $name -eq 'Wait-RunnerControlPlaneReadiness'
}, $true))
Assert-Equal 3 $deployReadinessCalls.Count "deployment helper readiness callsite count changed"
foreach ($call in $deployReadinessCalls) {
    Assert-True ($call.Extent.Text -match '-AllowVersionMismatch:\$AllowVersionMismatch') "deployment readiness callsite does not explicitly thread AllowVersionMismatch: $($call.GetCommandName())"
}

# Operator profile lookup carries only the token FILE PATH; token contents never
# enter the helper projection or command diagnostics.
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("webcodex-readiness-test-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot | Out-Null
try {
    $configPath = Join-Path $tempRoot "agent.toml"
    $tokenPath = Join-Path $tempRoot "webcodex-user-token"
    $secret = "wc_pat_must_not_leak_from_profile_0123456789"
    Set-Content -LiteralPath $configPath -Encoding UTF8 -Value @('client_id = "msi-test"', 'server_url = "https://runtime.example"')
    Set-Content -LiteralPath $tokenPath -Encoding ASCII -NoNewline -Value $secret
    $primary = [pscustomobject]@{
        CommandLine = '"C:\fake\webcodex-runner.exe" --config "' + $configPath + '"'
    }
    $profile = Get-RunnerOperatorProfile -PrimaryIdentity $primary
    Assert-Equal "msi-test" $profile.ClientId "operator profile client_id mismatch"
    Assert-Equal "https://runtime.example" $profile.ServerUrl "operator profile Server URL mismatch"
    Assert-Equal $tokenPath $profile.TokenFile "operator profile token-file path mismatch"
    $projection = $profile | ConvertTo-Json -Compress
    Assert-True (-not $projection.Contains($secret)) "operator profile leaked credential contents"
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output "Windows Runner readiness focused tests passed."
