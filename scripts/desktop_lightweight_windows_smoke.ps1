# Requires Windows/WebView2, MSVC and the Desktop dev server on its configured URL.
# Builds only a dedicated test host, never installs Desktop or starts a Runtime.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $manifest = Join-Path $root 'apps/desktop/src-tauri/src/desktop_shell/native-smoke.manifest'
    $logs = Join-Path $root 'target/desktop-lightweight-947'
    New-Item -ItemType Directory -Force $logs | Out-Null
    # --profile test builds the library's test harness. Extra rustc flags apply
    # only to that selected target, not dependencies or the shipped executable.
    $messages = & cargo rustc --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --profile test --message-format=json -- `
        --cfg desktop_native_smoke '-C' 'link-arg=/MANIFEST:EMBED' '-C' "link-arg=/MANIFESTINPUT:$manifest"
    $compileCode = $LASTEXITCODE
    $hostExe = $null
    foreach ($line in $messages) {
        try { $entry = $line | ConvertFrom-Json } catch { continue }
        if ($entry.reason -eq 'compiler-message' -and $entry.message.rendered) { Write-Host $entry.message.rendered }
        if ($entry.reason -eq 'compiler-artifact' -and $entry.target.name -eq 'webcodex_desktop_lib' -and $entry.profile.test -and $entry.executable) {
            $hostExe = $entry.executable
        }
    }
    if ($compileCode -ne 0) { throw 'Native smoke test-host compilation failed' }
    if (!$hostExe) { throw 'Cargo did not return an actual library test executable' }
    & $hostExe --exact desktop_native_smoke::desktop_real_process_windows_lightweight_smoke --ignored --nocapture --test-threads=1 2>&1 |
        Tee-Object (Join-Path $logs 'native-smoke.log')
    if ($LASTEXITCODE -ne 0) { throw "Native smoke failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
}
