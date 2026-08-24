# Focused Windows process identity helpers for Runner deployment/lifecycle scripts.
# CreationTime is the raw FILETIME returned by GetProcessTimes, matching the
# authoritative detached Job PID-reuse fencing in webcodex-runner.

if (-not ("WebCodex.WindowsProcessIdentity" -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace WebCodex {
    public sealed class ProcessIdentityHandle : SafeHandleZeroOrMinusOneIsInvalid {
        private ProcessIdentityHandle() : base(true) {}
        protected override bool ReleaseHandle() { return CloseHandle(handle); }
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool CloseHandle(IntPtr handle);
    }

    public static class WindowsProcessIdentity {
        private const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x1000;
        private const uint SYNCHRONIZE = 0x00100000;
        private const uint PROCESS_TERMINATE = 0x0001;
        private const uint WAIT_OBJECT_0 = 0x00000000;
        private const uint WAIT_TIMEOUT = 0x00000102;

        [StructLayout(LayoutKind.Sequential)]
        private struct FILETIME {
            public uint Low;
            public uint High;
        }

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern ProcessIdentityHandle OpenProcess(uint access, bool inherit, uint pid);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetProcessTimes(ProcessIdentityHandle process, out FILETIME creation, out FILETIME exit, out FILETIME kernel, out FILETIME user);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint WaitForSingleObject(ProcessIdentityHandle handle, uint milliseconds);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool TerminateProcess(ProcessIdentityHandle process, uint exitCode);

        public static ulong GetCreationTime(uint pid) {
            using (ProcessIdentityHandle handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, false, pid)) {
                if (handle == null || handle.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error());
                FILETIME creation, exit, kernel, user;
                if (!GetProcessTimes(handle, out creation, out exit, out kernel, out user)) throw new Win32Exception(Marshal.GetLastWin32Error());
                return ((ulong)creation.High << 32) | creation.Low;
            }
        }

        public static bool TryGetCreationTime(uint pid, out ulong creationTime) {
            creationTime = 0;
            using (ProcessIdentityHandle handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, false, pid)) {
                if (handle == null || handle.IsInvalid) {
                    int error = Marshal.GetLastWin32Error();
                    if (error == 87) return false;
                    throw new Win32Exception(error);
                }
                FILETIME creation, exit, kernel, user;
                if (!GetProcessTimes(handle, out creation, out exit, out kernel, out user)) throw new Win32Exception(Marshal.GetLastWin32Error());
                creationTime = ((ulong)creation.High << 32) | creation.Low;
                return true;
            }
        }

        public static bool IsLive(uint pid, ulong expectedCreationTime) {
            using (ProcessIdentityHandle handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, false, pid)) {
                if (handle == null || handle.IsInvalid) {
                    int error = Marshal.GetLastWin32Error();
                    if (error == 87) return false;
                    throw new Win32Exception(error);
                }
                FILETIME creation, exit, kernel, user;
                if (!GetProcessTimes(handle, out creation, out exit, out kernel, out user)) throw new Win32Exception(Marshal.GetLastWin32Error());
                ulong current = ((ulong)creation.High << 32) | creation.Low;
                if (current != expectedCreationTime) return false;
                uint wait = WaitForSingleObject(handle, 0);
                if (wait == WAIT_TIMEOUT) return true;
                if (wait == WAIT_OBJECT_0) return false;
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
        }

        public static bool CreationIdentityMatches(ulong captured, ulong current) {
            return captured == current;
        }

        public static void TerminateExact(uint pid, ulong expectedCreationTime) {
            using (ProcessIdentityHandle handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE | PROCESS_TERMINATE, false, pid)) {
                if (handle == null || handle.IsInvalid) {
                    int error = Marshal.GetLastWin32Error();
                    if (error == 87) throw new InvalidOperationException("process exited before effect");
                    throw new Win32Exception(error);
                }
                FILETIME creation, exit, kernel, user;
                if (!GetProcessTimes(handle, out creation, out exit, out kernel, out user)) throw new Win32Exception(Marshal.GetLastWin32Error());
                ulong current = ((ulong)creation.High << 32) | creation.Low;
                if (current != expectedCreationTime) throw new InvalidOperationException("process creation identity mismatch before effect");
                uint wait = WaitForSingleObject(handle, 0);
                if (wait == WAIT_OBJECT_0) throw new InvalidOperationException("process exited before effect");
                if (wait != WAIT_TIMEOUT) throw new Win32Exception(Marshal.GetLastWin32Error());
                if (!TerminateProcess(handle, 1)) throw new Win32Exception(Marshal.GetLastWin32Error());
            }
        }
    }
}
'@
}

function ConvertFrom-WindowsCommandLine {
    param([Parameter(Mandatory = $true)][string]$CommandLine)

    # PowerShell's parser is not Windows argv parsing. Use the framework's exact
    # CommandLineToArgvW binding through a tiny on-demand type.
    if (-not ("WebCodex.CommandLine" -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace WebCodex {
    public static class CommandLine {
        [DllImport("shell32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr CommandLineToArgvW(string commandLine, out int argc);
        [DllImport("kernel32.dll")]
        private static extern IntPtr LocalFree(IntPtr memory);
        public static string[] Parse(string commandLine) {
            int argc;
            IntPtr argv = CommandLineToArgvW(commandLine, out argc);
            if (argv == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            try {
                string[] result = new string[argc];
                for (int i = 0; i < argc; i++) {
                    IntPtr value = Marshal.ReadIntPtr(argv, i * IntPtr.Size);
                    result[i] = Marshal.PtrToStringUni(value);
                }
                return result;
            } finally { LocalFree(argv); }
        }
    }
}
'@
    }
    return @([WebCodex.CommandLine]::Parse($CommandLine))
}

function Test-PrimaryRunnerArguments {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    # CommandLine normally includes the executable as argv[0], but do not rely on
    # that representation: any current/future internal Runner token anywhere in
    # parsed argv is non-primary.
    foreach ($argument in @($Arguments)) {
        if ($argument.StartsWith("--webcodex-internal-", [System.StringComparison]::Ordinal)) {
            return $false
        }
    }
    return $true
}

function Get-PrimaryRunnerProcesses {
    param([Parameter(Mandatory = $true)][string]$ExactPath)

    $normalized = [System.IO.Path]::GetFullPath($ExactPath)
    $records = @(Get-CimInstance Win32_Process -Filter "Name = 'webcodex-runner.exe'" -ErrorAction Stop)
    $matches = @()
    foreach ($record in $records) {
        if (-not $record.ExecutablePath) { continue }
        try {
            if ([System.IO.Path]::GetFullPath([string]$record.ExecutablePath) -ine $normalized) { continue }
        } catch {
            continue
        }
        if (-not $record.CommandLine) {
            throw "Unable to classify same-path Runner PID $($record.ProcessId): command line is unavailable"
        }
        $argv = @(ConvertFrom-WindowsCommandLine -CommandLine ([string]$record.CommandLine))
        if (-not (Test-PrimaryRunnerArguments -Arguments $argv)) { continue }
        $creation = [uint64]0
        if (-not [WebCodex.WindowsProcessIdentity]::TryGetCreationTime([uint32]$record.ProcessId, [ref]$creation)) {
            # A stale CIM row for a process that already exited is not live identity.
            continue
        }
        $matches += [pscustomobject]@{
            Id = [uint32]$record.ProcessId
            CreationTime = [uint64]$creation
            Path = $normalized
            CommandLine = [string]$record.CommandLine
        }
    }
    return @($matches)
}

function Get-ExactlyOnePrimaryRunner {
    param([Parameter(Mandatory = $true)][string]$ExactPath)

    $matches = @(Get-PrimaryRunnerProcesses -ExactPath $ExactPath)
    if ($matches.Count -eq 0) { throw "No primary Runner found at $ExactPath" }
    if ($matches.Count -ne 1) { throw "Multiple primary Runners found at ${ExactPath}: $($matches.Count)" }
    return $matches[0]
}

function Test-CapturedProcessIdentityLive {
    param([Parameter(Mandatory = $true)]$Identity)
    return [WebCodex.WindowsProcessIdentity]::IsLive([uint32]$Identity.Id, [uint64]$Identity.CreationTime)
}

function Assert-CapturedPrimaryRunnerIdentity {
    param([Parameter(Mandatory = $true)]$Identity)

    $currentCreation = [uint64]0
    if (-not [WebCodex.WindowsProcessIdentity]::TryGetCreationTime([uint32]$Identity.Id, [ref]$currentCreation)) {
        throw "Primary Runner process exited before effect: PID $($Identity.Id)"
    }
    if ($currentCreation -ne [uint64]$Identity.CreationTime) {
        throw "Primary Runner process creation identity mismatch before effect: PID $($Identity.Id)"
    }
    if (-not (Test-CapturedProcessIdentityLive -Identity $Identity)) {
        throw "Primary Runner process exited before effect: PID $($Identity.Id)"
    }
    $current = @(Get-PrimaryRunnerProcesses -ExactPath $Identity.Path | Where-Object {
        $_.Id -eq $Identity.Id -and $_.CreationTime -eq $Identity.CreationTime
    })
    if ($current.Count -ne 1) {
        throw "Primary Runner identity revalidation failed for PID $($Identity.Id): exact primary role is no longer provable"
    }
    return $current[0]
}

function Stop-CapturedPrimaryRunner {
    param([Parameter(Mandatory = $true)]$Identity)

    # Role is revalidated first; TerminateExact then reopens the exact PID and
    # checks creation FILETIME on the same handle used for the termination effect.
    # A process exit/PID reuse in between therefore fails closed before effect.
    $null = Assert-CapturedPrimaryRunnerIdentity -Identity $Identity
    [WebCodex.WindowsProcessIdentity]::TerminateExact([uint32]$Identity.Id, [uint64]$Identity.CreationTime)
}
