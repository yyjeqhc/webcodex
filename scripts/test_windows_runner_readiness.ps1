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
        [string]$Compatibility = "compatible"
    )
    [pscustomobject]@{
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

# Fresh exact candidate is Ready even when source differs from Server.
$freshCandidate = New-RunnerObservation -InstanceId "instance-new" -Commit $candidateBuild.GitCommit
$decision = Get-RunnerReadinessDecision -Observation $freshCandidate -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -DisallowedAgentInstanceIds @($oldInstanceId)
Assert-Equal "ready" $decision.State "fresh exact candidate was not Ready"

# Fresh wrong source identities must never become Ready.
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-wrong-commit" -Commit "wrong1234567") -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -DisallowedAgentInstanceIds @($oldInstanceId)
Assert-Equal "mismatch" $decision.State "wrong commit was accepted"
Assert-Equal "unexpected_build_commit" $decision.Reason "wrong commit diagnostic changed"
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-wrong-dirty" -Commit $candidateBuild.GitCommit -Dirty $true) -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -DisallowedAgentInstanceIds @($oldInstanceId)
Assert-Equal "mismatch" $decision.State "wrong dirty state was accepted"
Assert-Equal "unexpected_build_dirty_state" $decision.Reason "wrong dirty diagnostic changed"

# Disconnected state remains transitional/not Ready.
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-disconnected" -Commit $candidateBuild.GitCommit -Connected $false) -ExpectedClientId "msi" -ExpectedBuild $candidateBuild
Assert-Equal "not_ready" $decision.State "disconnected Runner was accepted"
Assert-Equal "control_plane_runner_not_connected" $decision.Reason "disconnected diagnostic changed"

# Explicit protocol incompatibility is fatal mismatch; source alignment alone is not.
$decision = Get-RunnerReadinessDecision -Observation (New-RunnerObservation -InstanceId "instance-incompatible" -Commit $candidateBuild.GitCommit -Compatibility "capability_mismatch") -ExpectedClientId "msi" -ExpectedBuild $candidateBuild
Assert-Equal "mismatch" $decision.State "unsupported protocol was accepted"

# Rollback reconciliation uses rollback build, not candidate build, and excludes
# both the pre-replacement and already-observed candidate instances.
$rollbackObservation = New-RunnerObservation -InstanceId "instance-rollback" -Commit $rollbackBuild.GitCommit
$decision = Get-RunnerReadinessDecision -Observation $rollbackObservation -ExpectedClientId "msi" -ExpectedBuild $rollbackBuild -DisallowedAgentInstanceIds @($oldInstanceId, "instance-new")
Assert-Equal "ready" $decision.State "exact rollback build was not Ready"
$decision = Get-RunnerReadinessDecision -Observation $rollbackObservation -ExpectedClientId "msi" -ExpectedBuild $candidateBuild -DisallowedAgentInstanceIds @($oldInstanceId, "instance-new")
Assert-Equal "mismatch" $decision.State "rollback build incorrectly satisfied candidate expectation"

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
