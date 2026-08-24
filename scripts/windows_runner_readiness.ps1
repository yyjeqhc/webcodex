# Focused control-plane readiness helpers for Windows Runner replacement.
# Authentication remains inside the existing `webcodex ops runner` CLI path;
# this script only passes an existing user-token file path and consumes safe JSON.

function Get-RunnerBuildIdentity {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Runner binary does not exist: $Path"
    }
    $output = @(& $Path --version 2>&1)
    if ($LASTEXITCODE -ne 0 -or $output.Count -ne 1) {
        throw "Runner build identity unavailable for $Path"
    }
    $text = [string]$output[0]
    $pattern = '^webcodex-runner\s+(?<version>\S+)\s+\(commit\s+(?<commit>[0-9A-Fa-f]+),\s+dirty=(?<dirty>true|false)(?:,\s+built_at=(?<built_at>[^)]+))?\)$'
    if ($text -notmatch $pattern) {
        throw "Runner build identity unavailable for $Path"
    }
    return [pscustomobject]@{
        Version = [string]$matches['version']
        GitCommit = [string]$matches['commit']
        GitDirty = [System.Convert]::ToBoolean([string]$matches['dirty'])
        BuiltAt = if ($matches['built_at']) { [string]$matches['built_at'] } else { $null }
        Display = $text
    }
}

function Get-RunnerOperatorProfile {
    param([Parameter(Mandatory = $true)]$PrimaryIdentity)

    $argv = @(ConvertFrom-WindowsCommandLine -CommandLine ([string]$PrimaryIdentity.CommandLine))
    $configPath = $null
    for ($i = 0; $i -lt $argv.Count; $i++) {
        if ($argv[$i] -eq '--config' -and $i + 1 -lt $argv.Count) {
            $configPath = [string]$argv[$i + 1]
            break
        }
        if ($argv[$i].StartsWith('--config=', [System.StringComparison]::Ordinal)) {
            $configPath = $argv[$i].Substring('--config='.Length)
            break
        }
    }
    if (-not $configPath -or -not (Test-Path -LiteralPath $configPath -PathType Leaf)) {
        throw "Primary Runner config path is unavailable"
    }

    $clientId = $null
    $serverUrl = $null
    foreach ($line in Get-Content -LiteralPath $configPath) {
        if (-not $clientId -and $line -match '^\s*client_id\s*=\s*"([^"]+)"\s*$') {
            $clientId = [string]$matches[1]
        } elseif (-not $serverUrl -and $line -match '^\s*server_url\s*=\s*"([^"]+)"\s*$') {
            $serverUrl = [string]$matches[1]
        }
    }
    if (-not $clientId -or -not $serverUrl) {
        throw "Primary Runner config does not provide exact client_id and server_url"
    }
    $tokenFile = Join-Path (Split-Path -Parent $configPath) 'webcodex-user-token'
    if (-not (Test-Path -LiteralPath $tokenFile -PathType Leaf)) {
        throw "Operator user-token file is unavailable: $tokenFile"
    }
    return [pscustomobject]@{
        ClientId = $clientId
        ServerUrl = $serverUrl.TrimEnd('/')
        TokenFile = $tokenFile
        ConfigPath = $configPath
    }
}

function Get-RunnerControlPlaneObservation {
    param(
        [Parameter(Mandatory = $true)][string]$WebCodexCliPath,
        [Parameter(Mandatory = $true)][string]$ServerUrl,
        [Parameter(Mandatory = $true)][string]$TokenFile,
        [Parameter(Mandatory = $true)][string]$ClientId,
        [ValidateRange(1, 30000)][int]$RequestTimeoutMilliseconds = 5000
    )

    if (-not (Test-Path -LiteralPath $WebCodexCliPath -PathType Leaf)) {
        throw "WebCodex operator CLI does not exist: $WebCodexCliPath"
    }
    $raw = @(& $WebCodexCliPath ops runner --client-id $ClientId --server-url $ServerUrl --token-file $TokenFile --request-timeout-ms $RequestTimeoutMilliseconds --json --strict 2>&1)
    $exitCode = $LASTEXITCODE
    try {
        $response = ($raw -join "`n") | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw "Control-plane Runner observation returned invalid JSON; WebCodexCliPath must point to a CLI that supports 'ops runner'"
    }
    if ($exitCode -ne 0) {
        $reason = @($response.blocking_reasons | Select-Object -First 1)
        if ($reason.Count -eq 0) { $reason = @("operator_query_failed") }
        throw "Control-plane Runner observation failed: $($reason[0])"
    }
    if (-not $response.summary -or $response.summary.client_id -ne $ClientId) {
        throw "Control-plane Runner observation did not return exact client_id $ClientId"
    }
    return $response.summary
}

function Get-RunnerReadinessDecision {
    param(
        [Parameter(Mandatory = $true)]$Observation,
        [Parameter(Mandatory = $true)][string]$ExpectedClientId,
        [Parameter(Mandatory = $true)]$ExpectedBuild,
        [string[]]$DisallowedAgentInstanceIds = @()
    )

    if (-not $Observation -or $Observation.client_id -ne $ExpectedClientId) {
        return [pscustomobject]@{ State = 'not_ready'; Reason = 'exact_client_not_observed' }
    }
    if ($Observation.connected -ne $true) {
        return [pscustomobject]@{ State = 'not_ready'; Reason = 'control_plane_runner_not_connected' }
    }
    $instanceId = [string]$Observation.agent_instance_id
    if ([string]::IsNullOrWhiteSpace($instanceId)) {
        return [pscustomobject]@{ State = 'not_ready'; Reason = 'agent_instance_id_unavailable' }
    }
    if (@($DisallowedAgentInstanceIds | Where-Object { $_ -eq $instanceId }).Count -gt 0) {
        return [pscustomobject]@{ State = 'not_ready'; Reason = 'stale_agent_instance_id'; AgentInstanceId = $instanceId }
    }
    if (-not $Observation.build -or [string]::IsNullOrWhiteSpace([string]$Observation.build.git_commit) -or $null -eq $Observation.build.git_dirty) {
        return [pscustomobject]@{ State = 'mismatch'; Reason = 'observed_build_identity_unavailable'; AgentInstanceId = $instanceId }
    }
    if ([string]$Observation.build.git_commit -ne [string]$ExpectedBuild.GitCommit) {
        return [pscustomobject]@{
            State = 'mismatch'; Reason = 'unexpected_build_commit'; AgentInstanceId = $instanceId
            ExpectedCommit = [string]$ExpectedBuild.GitCommit; ObservedCommit = [string]$Observation.build.git_commit
        }
    }
    if ([bool]$Observation.build.git_dirty -ne [bool]$ExpectedBuild.GitDirty) {
        return [pscustomobject]@{
            State = 'mismatch'; Reason = 'unexpected_build_dirty_state'; AgentInstanceId = $instanceId
            ExpectedDirty = [bool]$ExpectedBuild.GitDirty; ObservedDirty = [bool]$Observation.build.git_dirty
        }
    }
    if ([string]$Observation.compatibility_status -eq 'capability_mismatch') {
        return [pscustomobject]@{ State = 'mismatch'; Reason = 'runner_protocol_incompatible'; AgentInstanceId = $instanceId }
    }
    return [pscustomobject]@{ State = 'ready'; Reason = 'exact_fresh_build_ready'; AgentInstanceId = $instanceId }
}

function Assert-PreReplacementRunnerObservation {
    param(
        [Parameter(Mandatory = $true)]$Observation,
        [Parameter(Mandatory = $true)][string]$ExpectedClientId,
        [Parameter(Mandatory = $true)]$ExpectedBuild
    )

    if (-not $Observation -or $Observation.client_id -ne $ExpectedClientId) {
        throw "Pre-replacement Runner not observable for exact client_id $ExpectedClientId"
    }
    if ($Observation.connected -ne $true) {
        throw "Pre-replacement Runner not connected for exact client_id $ExpectedClientId"
    }
    if ([string]::IsNullOrWhiteSpace([string]$Observation.agent_instance_id)) {
        throw "Pre-replacement Runner agent_instance_id is unavailable"
    }
    $decision = Get-RunnerReadinessDecision -Observation $Observation -ExpectedClientId $ExpectedClientId -ExpectedBuild $ExpectedBuild
    if ($decision.State -ne 'ready') {
        $details = "reason=$($decision.Reason) expected_commit=$($ExpectedBuild.GitCommit) observed_commit=$($Observation.build.git_commit) expected_dirty=$($ExpectedBuild.GitDirty) observed_dirty=$($Observation.build.git_dirty)"
        throw "Pre-replacement Runner build identity does not match the installed rollback source: $details"
    }
    return $Observation
}

function Stop-CapturedPrimaryRunnerForRollback {
    param(
        [Parameter(Mandatory = $true)]$Identity,
        [scriptblock]$IsLive = { param($Captured) Test-CapturedProcessIdentityLive -Identity $Captured },
        [scriptblock]$StopExact = { param($Captured) Stop-CapturedPrimaryRunner -Identity $Captured }
    )

    if (-not (& $IsLive $Identity)) {
        return 'already_exited'
    }
    try {
        & $StopExact $Identity
        return 'terminated'
    } catch {
        # Disabling the Scheduled Task asks the existing supervisor to stop its
        # Runner too. If that concurrent stop wins, no termination effect remains
        # to perform. Only accept that race when the exact captured PID+creation
        # identity is now dead; a still-live identity keeps the P1a failure closed.
        if (-not (& $IsLive $Identity)) {
            return 'exited_during_stop'
        }
        throw
    }
}

function Wait-RunnerControlPlaneReadiness {
    param(
        [Parameter(Mandatory = $true)][scriptblock]$Observe,
        [Parameter(Mandatory = $true)][string]$ExpectedClientId,
        [Parameter(Mandatory = $true)]$ExpectedBuild,
        [Parameter(Mandatory = $true)][DateTime]$DeadlineUtc,
        [string[]]$DisallowedAgentInstanceIds = @(),
        [ValidateRange(1, 5000)][int]$PollIntervalMilliseconds = 250,
        [switch]$FailOnBuildMismatch,
        [scriptblock]$UtcNow = { [DateTime]::UtcNow },
        [scriptblock]$Sleep = { param([int]$Milliseconds) Start-Sleep -Milliseconds $Milliseconds }
    )

    $lastReason = 'control_plane_runner_not_observed'
    $lastObservation = $null
    while ($true) {
        $now = & $UtcNow
        if ($now -ge $DeadlineUtc) { break }
        $remainingMs = [int][Math]::Max(1, [Math]::Floor(($DeadlineUtc - $now).TotalMilliseconds))
        $requestTimeoutMs = [Math]::Min(5000, $remainingMs)
        try {
            $lastObservation = & $Observe $requestTimeoutMs
            $decision = Get-RunnerReadinessDecision -Observation $lastObservation -ExpectedClientId $ExpectedClientId -ExpectedBuild $ExpectedBuild -DisallowedAgentInstanceIds $DisallowedAgentInstanceIds
            $lastReason = [string]$decision.Reason
            if ($decision.State -eq 'ready') {
                return [pscustomobject]@{ Observation = $lastObservation; Decision = $decision }
            }
            if ($decision.State -eq 'mismatch' -and $FailOnBuildMismatch) {
                $details = "reason=$($decision.Reason) expected_commit=$($ExpectedBuild.GitCommit) observed_commit=$($lastObservation.build.git_commit) expected_dirty=$($ExpectedBuild.GitDirty) observed_dirty=$($lastObservation.build.git_dirty)"
                throw "Fresh Runner instance has unexpected build: $details"
            }
        } catch {
            if ($_.Exception.Message.StartsWith('Fresh Runner instance has unexpected build:')) { throw }
            $lastReason = "control_plane_observation_failed: $($_.Exception.Message)"
        }

        $now = & $UtcNow
        if ($now -ge $DeadlineUtc) { break }
        $remainingMs = [int][Math]::Max(0, [Math]::Floor(($DeadlineUtc - $now).TotalMilliseconds))
        $delayMs = [Math]::Min($PollIntervalMilliseconds, $remainingMs)
        if ($delayMs -gt 0) { & $Sleep $delayMs }
    }

    throw "Runner readiness timeout for $ExpectedClientId; last_reason=$lastReason"
}
