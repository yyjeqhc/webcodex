# Dogfood-only Windows Runner deployment helper.
#
# This intentionally keeps the existing interactive Scheduled Task because
# computer_observe needs the logged-in desktop session. It does not create or
# modify credentials, agent configuration, or Scheduled Task definitions.
#
# The deployment contract is:
#   verify candidate -> disable task -> stop exact old runner -> replace with
#   rollback -> re-enable/start task -> verify the exact new image stays alive.
# Any failure after replacement restores the previous binary and restarts it.
# The operator must still verify Control-plane re-registration after this local
# handoff and redeploy the retained rollback binary if that external check fails.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Candidate,

    [string]$RunnerPath = "$env:USERPROFILE\.local\bin\webcodex-runner.exe",
    [string]$TaskName = "WebCodex MSI Dogfood Runner",
    [string]$TaskPath = "\",

    [ValidateRange(1, 120)]
    [int]$StopTimeoutSecs = 20,

    [ValidateRange(1, 120)]
    [int]$StartTimeoutSecs = 20
)

$ErrorActionPreference = "Stop"

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

function Get-ExactRunnerProcesses {
    param([Parameter(Mandatory = $true)][string]$ExactPath)

    $normalized = [System.IO.Path]::GetFullPath($ExactPath)
    return @(Get-Process -Name "webcodex-runner" -ErrorAction SilentlyContinue | Where-Object {
        try {
            $_.Path -and ([System.IO.Path]::GetFullPath($_.Path) -ieq $normalized)
        } catch {
            $false
        }
    })
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

$candidateIdentity = Get-RunnerIdentity -Path $Candidate
if (-not (Test-Path -LiteralPath $RunnerPath -PathType Leaf)) {
    throw "Existing Runner is required so deployment has a concrete rollback binary: $RunnerPath"
}
$previousIdentity = Get-RunnerIdentity -Path $RunnerPath

# Copy first so the source may be a build directory, network path, or even the
# current RunnerPath. The staged image is fully verified before the old process
# is touched.
$stagedPath = Join-Path $RunnerDir ("webcodex-runner.{0}.new.exe" -f [guid]::NewGuid().ToString("N"))
$rollbackPath = "$RunnerPath.rollback"
$failedPath = "$RunnerPath.failed"
$replacementInstalled = $false
$rollbackAvailable = $false

try {
    Copy-Item -LiteralPath $Candidate -Destination $stagedPath -ErrorAction Stop
    $stagedIdentity = Get-RunnerIdentity -Path $stagedPath
    if ($stagedIdentity -ne $candidateIdentity) {
        throw "Staged Runner identity differs from the candidate"
    }

    # Prevent the Task Scheduler's RestartOnFailure policy from racing the
    # replacement after we intentionally terminate the old Runner.
    Disable-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath | Out-Null

    foreach ($process in Get-ExactRunnerProcesses -ExactPath $RunnerPath) {
        Stop-Process -Id $process.Id -Force -ErrorAction Stop
    }
    Wait-Until -TimeoutSecs $StopTimeoutSecs -FailureMessage "Old Runner process did not exit before replacement" -Condition {
        (Get-ExactRunnerProcesses -ExactPath $RunnerPath).Count -eq 0
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
    Wait-Until -TimeoutSecs $StartTimeoutSecs -FailureMessage "New Runner process did not start" -Condition {
        (Get-ExactRunnerProcesses -ExactPath $RunnerPath).Count -eq 1
    }

    # Catch immediate startup failures before declaring the local handoff good.
    Start-Sleep -Seconds 3
    if ((Get-ExactRunnerProcesses -ExactPath $RunnerPath).Count -ne 1) {
        throw "New Runner did not remain alive after startup"
    }

    # Successful handoff: remove stale per-deployment staging images but retain
    # one concrete rollback binary for the next operator action.
    Get-ChildItem -LiteralPath $RunnerDir -Filter "webcodex-runner.*.new.exe" -File -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue

    Write-Output "Windows Runner dogfood local handoff succeeded."
    Write-Output "  candidate: $candidateIdentity"
    if ($previousIdentity) {
        Write-Output "  previous:  $previousIdentity"
        Write-Output "  rollback:  $rollbackPath"
    }
    Write-Output "  task:      $TaskPath$TaskName"
    Write-Output "  runner:    $RunnerPath"
} catch {
    $deploymentError = $_

    try {
        Disable-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath -ErrorAction SilentlyContinue | Out-Null
        foreach ($process in Get-ExactRunnerProcesses -ExactPath $RunnerPath) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        }
        Wait-Until -TimeoutSecs $StopTimeoutSecs -FailureMessage "Runner did not stop during rollback" -Condition {
            (Get-ExactRunnerProcesses -ExactPath $RunnerPath).Count -eq 0
        }

        if ($rollbackAvailable -and (Test-Path -LiteralPath $rollbackPath -PathType Leaf)) {
            if (Test-Path -LiteralPath $RunnerPath -PathType Leaf) {
                Move-Item -LiteralPath $RunnerPath -Destination $failedPath -Force
            }
            Move-Item -LiteralPath $rollbackPath -Destination $RunnerPath
            $replacementInstalled = $false
            $rollbackAvailable = $false
        }

        Enable-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath -ErrorAction SilentlyContinue | Out-Null
        if (Test-Path -LiteralPath $RunnerPath -PathType Leaf) {
            Start-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath -ErrorAction SilentlyContinue
            Wait-Until -TimeoutSecs $StartTimeoutSecs -FailureMessage "Rollback Runner did not restart" -Condition {
                (Get-ExactRunnerProcesses -ExactPath $RunnerPath).Count -eq 1
            }
        }
    } catch {
        Write-Warning "Rollback/restart also encountered an error: $($_.Exception.Message)"
    }

    throw $deploymentError
} finally {
    if (Test-Path -LiteralPath $stagedPath) {
        Remove-Item -LiteralPath $stagedPath -Force -ErrorAction SilentlyContinue
    }
}
