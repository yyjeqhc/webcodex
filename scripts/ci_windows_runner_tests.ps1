$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# The Runner test harness is one binary. These namespaces own real child
# processes, PowerShell, fake MCP/LSP executables, process-tree cleanup, or
# similarly tight lifecycle deadlines. Keep them mutually serial on hosted
# Windows while allowing the rest of the Runner suite to use libtest's normal
# parallelism. The same filters are used for --skip and for the serial reruns,
# so this partition changes scheduling only; it does not remove test coverage.
$serialFilters = @(
    'tests::dispatch_file::',
    'tests::dispatch_shell::',
    'tests::profile_process_lifecycle::',
    'tests::shell_config::',
    'tests::shell_job_execution::',
    'tests::shell_job_tree::',
    'tests::shell_profiles::',
    'tests::shell_job_native_exe_nonzero_exit_code_is_preserved',
    'tests::shell_job_unicode_stdout_stderr_env_and_cwd',
    'tests::prepared_profile_unicode_env_round_trip_and_unicode_init_path',
    'job_manager_tests::',
    'webcodex_runner::detached_job::tests::',
    'webcodex_runner::external_tools::tests::',
    'webcodex_runner::lsp::navigation_tests::',
    'webcodex_runner::lsp::supervisor::tests::',
    'webcodex_runner::mcp_gateway::tests::',
    'webcodex_runner::projects::git_lifecycle_tests::',
    'webcodex_runner::shell::runner_lifecycle_tests::',
    'webcodex_runner::ssh::windows_tests::',
    'webcodex_runner::validation::execute::tests::',
    'webcodex_runner::validation::tests::'
)

function Invoke-CargoTest {
    param([string[]]$CargoArgs)

    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo exited with code $LASTEXITCODE"
    }
}

$timer = [System.Diagnostics.Stopwatch]::StartNew()
$parallelArgs = @('test', '--locked', '-p', 'webcodex-runner', '--')
foreach ($filter in $serialFilters) {
    $parallelArgs += '--skip'
    $parallelArgs += $filter
}

Write-Host 'Running Windows Runner logic tests with default libtest parallelism'
Invoke-CargoTest -CargoArgs $parallelArgs

Write-Host 'Running Windows Runner process/lifecycle partitions serially'
foreach ($filter in $serialFilters) {
    Write-Host "serial filter: $filter"
    $serialArgs = @(
        'test', '--locked', '-p', 'webcodex-runner', $filter,
        '--', '--test-threads=1'
    )
    Invoke-CargoTest -CargoArgs $serialArgs
}

Write-Host 'Running Windows Computer tests with default libtest parallelism'
Invoke-CargoTest -CargoArgs @('test', '--locked', '-p', 'webcodex-computer')

$timer.Stop()
Write-Host ("Partitioned Windows Runner + Computer tests completed in {0:N1}s" -f $timer.Elapsed.TotalSeconds)
