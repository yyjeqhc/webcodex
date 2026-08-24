# Dogfood-only Windows Runner deployment helper.
#
# This intentionally keeps the existing interactive Scheduled Task because
# computer_observe needs the logged-in desktop session. It does not create or
# modify credentials, agent configuration, or Scheduled Task definitions.
#
# The deployment contract is:
#   verify candidate build -> prove exact pre-replacement Server identity ->
#   P1a exact local replacement -> prove a fresh candidate instance/build Ready.
# Any failure after destructive replacement begins performs bounded rollback and
# must prove a fresh rollback instance/build Ready before recovery is considered
# established. The Server binary/service is never modified by this helper.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Candidate,

    [string]$RunnerPath = "$env:USERPROFILE\.local\bin\webcodex-runner.exe",
    [string]$WebCodexCliPath = "$env:USERPROFILE\.local\bin\webcodex.exe",
    [string]$TaskName = "WebCodex MSI Dogfood Runner",
    [string]$TaskPath = "\",

    [ValidateRange(1, 120)]
    [int]$StopTimeoutSecs = 20,

    [ValidateRange(1, 120)]
    [int]$StartTimeoutSecs = 20,

    [ValidateRange(1, 300)]
    [int]$ReadinessTimeoutSecs = 30
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "windows_runner_process_identity.ps1")
. (Join-Path $PSScriptRoot "windows_runner_readiness.ps1")

function Get-RunnerIdentity {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Runner binary does not exist: $Path"
    }
    $output = @(& $Path --version 2>&1)
    if ($LASTEXITCODE -ne 0 -or $output.Count -ne 1) {
        throw "Runner version probe failed for $Path"
    }
    $identity = [string]$output[0]
    if (-not $identity.StartsWith("webcodex-runner ")) {
        throw "Runner version probe returned an unexpected identity for $Path"
    }
    return $identity
}

function Wait-Until {
    param(
        [Parameter(Mandatory = $true)][scriptblock]$Condition,
        [Parameter(Mandatory = $true)][int]$TimeoutSecs,
        [Parameter(Mandatory = $true)][string]$FailureMessage
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSecs)
    do {
        if (& $Condition) {
            return
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw $FailureMessage
}

$Candidate = [System.IO.Path]::GetFullPath($Candidate)
$RunnerPath = [System.IO.Path]::GetFullPath($RunnerPath)
$RunnerDir = Split-Path -Parent $RunnerPath
if (-not (Test-Path -LiteralPath $RunnerDir -PathType Container)) {
    throw "Runner directory does not exist: $RunnerDir"
}

# Validate the task before touching the running Runner.
$task = Get-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath -ErrorAction Stop
$taskWasEnabled = $task.State -ne "Disabled"
if (-not $taskWasEnabled) {
    throw "Scheduled Task is already disabled: $TaskPath$TaskName"
}

$candidateBuild = Get-RunnerBuildIdentity -Path $Candidate
$candidateIdentity = Get-RunnerIdentity -Path $Candidate
if (-not (Test-Path -LiteralPath $RunnerPath -PathType Leaf)) {
    throw "Existing Runner is required so deployment has a concrete rollback binary: $RunnerPath"
}
$previousBuild = Get-RunnerBuildIdentity -Path $RunnerPath
$previousIdentity = Get-RunnerIdentity -Path $RunnerPath
$oldPrimary = Get-ExactlyOnePrimaryRunner -ExactPath $RunnerPath
$operatorProfile = Get-RunnerOperatorProfile -PrimaryIdentity $oldPrimary
$preObservation = Get-RunnerControlPlaneObservation `
    -WebCodexCliPath $WebCodexCliPath `
    -ServerUrl $operatorProfile.ServerUrl `
    -TokenFile $operatorProfile.TokenFile `
    -ClientId $operatorProfile.ClientId `
    -RequestTimeoutMilliseconds ([Math]::Min(5000, $ReadinessTimeoutSecs * 1000))
$null = Assert-PreReplacementRunnerObservation `
    -Observation $preObservation `
    -ExpectedClientId $operatorProfile.ClientId `
    -ExpectedBuild $previousBuild
$oldAgentInstanceId = [string]$preObservation.agent_instance_id

# Copy first so the source may be a build directory, network path, or even the
# current RunnerPath. The staged image is fully verified before the old process
# is touched.
$stagedPath = Join-Path $RunnerDir ("webcodex-runner.{0}.new.exe" -f [guid]::NewGuid().ToString("N"))
$rollbackPath = "$RunnerPath.rollback"
$failedPath = "$RunnerPath.failed"
$replacementInstalled = $false
$rollbackAvailable = $false
$candidateObservedInstanceIds = @()
$candidateReadyObservation = $null

try {
    Copy-Item -LiteralPath $Candidate -Destination $stagedPath -ErrorAction Stop
    $stagedIdentity = Get-RunnerIdentity -Path $stagedPath
    if ($stagedIdentity -ne $candidateIdentity) {
        throw "Staged Runner identity differs from the candidate"
    }
    $stagedBuild = Get-RunnerBuildIdentity -Path $stagedPath
    if ($stagedBuild.GitCommit -ne $candidateBuild.GitCommit -or $stagedBuild.GitDirty -ne $candidateBuild.GitDirty) {
        throw "Staged Runner build identity differs from the candidate"
    }

    # Prevent the Task Scheduler's RestartOnFailure policy from racing the
    # replacement after we intentionally terminate the old Runner.
    Disable-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath | Out-Null

    Stop-CapturedPrimaryRunner -Identity $oldPrimary
    Wait-Until -TimeoutSecs $StopTimeoutSecs -FailureMessage "Old Runner process identity did not exit before replacement" -Condition {
        -not (Test-CapturedProcessIdentityLive -Identity $oldPrimary)
    }
    Wait-Until -TimeoutSecs $StopTimeoutSecs -FailureMessage "Scheduled Task wrapper did not stop before replacement" -Condition {
        (Get-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath).State -ne "Running"
    }

    if (Test-Path -LiteralPath $rollbackPath) {
        Remove-Item -LiteralPath $rollbackPath -Force
    }
    if (Test-Path -LiteralPath $failedPath) {
        Remove-Item -LiteralPath $failedPath -Force
    }
    if (Test-Path -LiteralPath $RunnerPath -PathType Leaf) {
        Move-Item -LiteralPath $RunnerPath -Destination $rollbackPath
        $rollbackAvailable = $true
    }
    Move-Item -LiteralPath $stagedPath -Destination $RunnerPath
    $replacementInstalled = $true

    $installedIdentity = Get-RunnerIdentity -Path $RunnerPath
    if ($installedIdentity -ne $candidateIdentity) {
        throw "Installed Runner identity differs from the verified candidate"
    }

    Enable-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath | Out-Null
    Start-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath
    $newPrimary = $null
    Wait-Until -TimeoutSecs $StartTimeoutSecs -FailureMessage "New primary Runner process did not start" -Condition {
        $matches = @(Get-PrimaryRunnerProcesses -ExactPath $RunnerPath)
        if ($matches.Count -eq 1) {
            $script:newPrimary = $matches[0]
            return $true
        }
        return $false
    }

    # Catch immediate startup failures before declaring the local handoff good.
    Start-Sleep -Seconds 3
    $null = Assert-CapturedPrimaryRunnerIdentity -Identity $newPrimary
    if ((Get-PrimaryRunnerProcesses -ExactPath $RunnerPath).Count -ne 1) {
        throw "New Runner did not remain the exactly-one primary Runner after startup"
    }

    $candidateObserve = {
        param([int]$RequestTimeoutMilliseconds)
        $observation = Get-RunnerControlPlaneObservation `
            -WebCodexCliPath $WebCodexCliPath `
            -ServerUrl $operatorProfile.ServerUrl `
            -TokenFile $operatorProfile.TokenFile `
            -ClientId $operatorProfile.ClientId `
            -RequestTimeoutMilliseconds $RequestTimeoutMilliseconds
        $instanceId = [string]$observation.agent_instance_id
        if (-not [string]::IsNullOrWhiteSpace($instanceId) -and
            $instanceId -ne $oldAgentInstanceId -and
            $script:candidateObservedInstanceIds -notcontains $instanceId) {
            $script:candidateObservedInstanceIds += $instanceId
        }
        return $observation
    }
    $candidateReady = Wait-RunnerControlPlaneReadiness `
        -Observe $candidateObserve `
        -ExpectedClientId $operatorProfile.ClientId `
        -ExpectedBuild $candidateBuild `
        -DeadlineUtc ([DateTime]::UtcNow.AddSeconds($ReadinessTimeoutSecs)) `
        -DisallowedAgentInstanceIds @($oldAgentInstanceId) `
        -FailOnBuildMismatch
    $candidateReadyObservation = $candidateReady.Observation
    $null = Assert-CapturedPrimaryRunnerIdentity -Identity $newPrimary

    # Successful handoff: remove stale per-deployment staging images but retain
    # one concrete rollback binary for the next operator action.
    Get-ChildItem -LiteralPath $RunnerDir -Filter "webcodex-runner.*.new.exe" -File -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue

    Write-Output "Windows Runner dogfood replacement readiness succeeded."
    Write-Output "  client_id:             $($operatorProfile.ClientId)"
    Write-Output "  old_agent_instance_id: $oldAgentInstanceId"
    Write-Output "  new_agent_instance_id: $($candidateReadyObservation.agent_instance_id)"
    Write-Output "  expected_build:        commit=$($candidateBuild.GitCommit) dirty=$($candidateBuild.GitDirty)"
    Write-Output "  observed_build:        commit=$($candidateReadyObservation.build.git_commit) dirty=$($candidateReadyObservation.build.git_dirty)"
    Write-Output "  candidate:             $candidateIdentity"
    Write-Output "  previous:              $previousIdentity"
    Write-Output "  rollback:              $rollbackPath"
    Write-Output "  task:                  $TaskPath$TaskName"
    Write-Output "  runner:                $RunnerPath"
} catch {
    $deploymentError = $_
    $rollbackFailure = $null
    $rollbackReadyObservation = $null

    try {
        Disable-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath -ErrorAction SilentlyContinue | Out-Null

        # Capture any fresh candidate instance already visible before stopping it,
        # so that a stale candidate registration cannot satisfy rollback readiness.
        try {
            $beforeRollback = Get-RunnerControlPlaneObservation `
                -WebCodexCliPath $WebCodexCliPath `
                -ServerUrl $operatorProfile.ServerUrl `
                -TokenFile $operatorProfile.TokenFile `
                -ClientId $operatorProfile.ClientId `
                -RequestTimeoutMilliseconds ([Math]::Min(2000, $ReadinessTimeoutSecs * 1000))
            $beforeRollbackInstanceId = [string]$beforeRollback.agent_instance_id
            if (-not [string]::IsNullOrWhiteSpace($beforeRollbackInstanceId) -and
                $beforeRollbackInstanceId -ne $oldAgentInstanceId -and
                $script:candidateObservedInstanceIds -notcontains $beforeRollbackInstanceId) {
                $script:candidateObservedInstanceIds += $beforeRollbackInstanceId
            }
        } catch {
            # Best-effort only: the bounded rollback readiness proof below remains authoritative.
        }

        $rollbackStopTarget = $null
        $rollbackMatches = @(Get-PrimaryRunnerProcesses -ExactPath $RunnerPath)
        if ($rollbackMatches.Count -gt 1) {
            throw "Multiple primary Runners found during rollback: $($rollbackMatches.Count)"
        }
        if ($rollbackMatches.Count -eq 1) {
            $rollbackStopTarget = $rollbackMatches[0]
            Stop-CapturedPrimaryRunner -Identity $rollbackStopTarget
            Wait-Until -TimeoutSecs $StopTimeoutSecs -FailureMessage "Runner identity did not stop during rollback" -Condition {
                -not (Test-CapturedProcessIdentityLive -Identity $rollbackStopTarget)
            }
        }

        if ($rollbackAvailable -and (Test-Path -LiteralPath $rollbackPath -PathType Leaf)) {
            if (Test-Path -LiteralPath $RunnerPath -PathType Leaf) {
                Move-Item -LiteralPath $RunnerPath -Destination $failedPath -Force
            }
            Move-Item -LiteralPath $rollbackPath -Destination $RunnerPath
            $replacementInstalled = $false
            $rollbackAvailable = $false
        }
        if (-not (Test-Path -LiteralPath $RunnerPath -PathType Leaf)) {
            throw "Rollback binary is unavailable after restore"
        }
        $restoredBuild = Get-RunnerBuildIdentity -Path $RunnerPath
        if ($restoredBuild.GitCommit -ne $previousBuild.GitCommit -or $restoredBuild.GitDirty -ne $previousBuild.GitDirty) {
            throw "Restored rollback binary build identity differs from the pre-replacement Runner"
        }

        Enable-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath -ErrorAction Stop | Out-Null
        Start-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath -ErrorAction Stop
        $rollbackPrimary = $null
        Wait-Until -TimeoutSecs $StartTimeoutSecs -FailureMessage "Rollback primary Runner did not restart" -Condition {
            $matches = @(Get-PrimaryRunnerProcesses -ExactPath $RunnerPath)
            if ($matches.Count -eq 1) {
                $script:rollbackPrimary = $matches[0]
                return $true
            }
            return $false
        }
        $null = Assert-CapturedPrimaryRunnerIdentity -Identity $rollbackPrimary

        $rollbackObserve = {
            param([int]$RequestTimeoutMilliseconds)
            Get-RunnerControlPlaneObservation `
                -WebCodexCliPath $WebCodexCliPath `
                -ServerUrl $operatorProfile.ServerUrl `
                -TokenFile $operatorProfile.TokenFile `
                -ClientId $operatorProfile.ClientId `
                -RequestTimeoutMilliseconds $RequestTimeoutMilliseconds
        }
        $rollbackDisallowedIds = @($oldAgentInstanceId) + @($candidateObservedInstanceIds)
        $rollbackReady = Wait-RunnerControlPlaneReadiness `
            -Observe $rollbackObserve `
            -ExpectedClientId $operatorProfile.ClientId `
            -ExpectedBuild $previousBuild `
            -DeadlineUtc ([DateTime]::UtcNow.AddSeconds($ReadinessTimeoutSecs)) `
            -DisallowedAgentInstanceIds $rollbackDisallowedIds
        $rollbackReadyObservation = $rollbackReady.Observation
        $null = Assert-CapturedPrimaryRunnerIdentity -Identity $rollbackPrimary
    } catch {
        $rollbackFailure = $_.Exception.Message
        Write-Warning "Rollback/restart readiness failed: $rollbackFailure"
    }

    if ($rollbackFailure) {
        throw "Deployment failed: $($deploymentError.Exception.Message). Rollback outcome uncertain / rollback readiness failed: $rollbackFailure"
    }
    Write-Warning "Deployment failed, but rollback readiness was proven for agent_instance_id=$($rollbackReadyObservation.agent_instance_id) commit=$($rollbackReadyObservation.build.git_commit) dirty=$($rollbackReadyObservation.build.git_dirty)"
    throw $deploymentError
} finally {
    if (Test-Path -LiteralPath $stagedPath) {
        Remove-Item -LiteralPath $stagedPath -Force -ErrorAction SilentlyContinue
    }
}
