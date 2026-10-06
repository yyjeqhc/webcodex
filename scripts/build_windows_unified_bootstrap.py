#!/usr/bin/env python3
"""Build the outer Windows unified installer around a Tauri NSIS payload."""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import shutil
import subprocess
from pathlib import Path

import prepare_windows_unified_installer as candidate


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def nsis_quote(path: Path) -> str:
    value = str(path.resolve()).replace("/", "\\")
    if any(ch in value for ch in ('"', "$", "\r", "\n")):
        raise ValueError("NSIS path contains unsupported characters")
    return f'"{value}"'


def candidate_hash_command(expected_sha256: str) -> str:
    # Keep the candidate path out of PowerShell source. Use .NET file/crypto APIs
    # directly instead of PowerShell provider cmdlets so exact-path hashing does
    # not depend on provider parsing or hosted-runner cmdlet behavior.
    script = (
        "$ErrorActionPreference='Stop'; try { "
        "$s=[System.IO.File]::OpenRead($env:WEBCODEX_INSTALLER_CANDIDATE_CLI); try { "
        "$sha=[System.Security.Cryptography.SHA256]::Create(); try { "
        "$h=([System.BitConverter]::ToString($sha.ComputeHash($s))).Replace('-','').ToLowerInvariant() "
        "} finally { $sha.Dispose() } "
        "} finally { $s.Dispose() }; "
        f"if($h -ne '{expected_sha256}') {{ exit 1 }}; exit 0 "
        "} catch { exit 1 }"
    )
    return base64.b64encode(script.encode("utf-16le")).decode("ascii")


def render(candidate_dir: Path, inner_installer: Path, output: Path, platform: str) -> str:
    manifest, files = candidate.validate_candidate(candidate_dir, platform)
    candidate.require_guarded_bootstrap_contract(manifest)
    inner = inner_installer.resolve(strict=True)
    if inner.read_bytes()[:2] != b"MZ":
        raise ValueError("inner Tauri installer is not a Windows executable")
    target = "x86_64" if platform == "win32-x64" else "aarch64"
    lines = [
        'Unicode true',
        '!include "LogicLib.nsh"',
        '!include "FileFunc.nsh"',
        '!include "WinMessages.nsh"',
        '!include "StrFunc.nsh"',
        '${Using:StrFunc} StrStr',
        'Name "WebCodex Unified Installer"',
        f'OutFile {nsis_quote(output)}',
        'RequestExecutionLevel user',
        'InstallDir "$LOCALAPPDATA\\WebCodex Desktop"',
        'Var WebCodexInstallDir',
        'Var WebCodexEnvironmentDir',
        'Var WebCodexExisting',
        'Var WebCodexCandidate',
        'Var WebCodexTrustedCLI',
        'Var WebCodexPackageUpgrade',
        'Var WebCodexForeignServices',
        'Var WebCodexOldDesktop',
        'Var WebCodexUpgradePrepared',
        'Var WebCodexUpgradeTargetFile',
        'Var WebCodexBoundOperation',
        '',
        'Function .onInit',
        '  StrCpy $WebCodexUpgradeTargetFile ""',
        '  StrCpy $WebCodexBoundOperation ""',
        '  SetRegView 64',
        '  StrCpy $WebCodexEnvironmentDir ""',
        '  ${GetParameters} $R0',
        '  ClearErrors',
        '  ${GetOptions} $R0 "/ENVIRONMENTDIR=" $WebCodexEnvironmentDir',
        '  ${If} ${Errors}',
        '    StrCpy $WebCodexEnvironmentDir ""',
        '  ${EndIf}',
        '  ClearErrors',
        '  ${GetOptions} $R0 "/UPGRADETARGET=" $WebCodexUpgradeTargetFile',
        '  ${If} ${Errors}',
        '    StrCpy $WebCodexUpgradeTargetFile ""',
        '  ${ElseIf} $WebCodexUpgradeTargetFile == ""',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ${If} $WebCodexUpgradeTargetFile != ""',
        '  ${AndIf} $WebCodexEnvironmentDir == ""',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ReadRegStr $WebCodexInstallDir HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\WebCodex Desktop" "InstallLocation"',
        '  ${If} $WebCodexInstallDir == ""',
        '    StrCpy $WebCodexInstallDir "$LOCALAPPDATA\\WebCodex Desktop"',
        '  ${Else}',
        '    StrCpy $0 $WebCodexInstallDir 1',
        '    ${If} $0 == \'"\'',
        '      StrCpy $WebCodexInstallDir $WebCodexInstallDir -1 1',
        '    ${EndIf}',
        '  ${EndIf}',
        'FunctionEnd',
        '',
        'Section "-WebCodexUnifiedBootstrap"',
        '  StrCpy $WebCodexExisting 0',
        '  StrCpy $WebCodexPackageUpgrade 0',
        '  StrCpy $WebCodexForeignServices 0',
        '  ReadRegStr $R3 HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\WebCodex Desktop" "InstallLocation"',
        '  ${If} $R3 != ""',
        '    StrCpy $WebCodexExisting 1',
        '  ${EndIf}',
        '  ${If} ${FileExists} "$WebCodexInstallDir\\WebCodex.exe"',
        '  ${OrIf} ${FileExists} "$WebCodexInstallDir\\webcodex-runtime\\webcodex.exe"',
        '    StrCpy $WebCodexExisting 1',
        '  ${EndIf}',
        '  SetRegView 64',
        '  StrCpy $R1 0',
        'webcodex_service_scan:',
        '  EnumRegKey $R0 HKLM "SYSTEM\\CurrentControlSet\\Services" $R1',
        '  ${If} $R0 == ""',
        '    Goto webcodex_services_checked',
        '  ${EndIf}',
        '  StrCpy $R2 $R0 8',
        '  ${If} $R2 == "WebCodex"',
        '    StrCpy $WebCodexForeignServices 1',
        '  ${EndIf}',
        '  IntOp $R1 $R1 + 1',
        '  Goto webcodex_service_scan',
        'webcodex_services_checked:',
        '  InitPluginsDir',
        '  StrCpy $WebCodexCandidate "$PLUGINSDIR\\WebCodexUpgradeCandidate"',
        '  SetOutPath "$WebCodexCandidate"',
        f'  File /oname=source-manifest.json {nsis_quote(candidate_dir / "source-manifest.json")}',
        f'  File /oname=SHA256SUMS {nsis_quote(candidate_dir / "SHA256SUMS")}',
        '  SetOutPath "$WebCodexCandidate\\artifacts\\bin"',
    ]
    for name in candidate.BINARIES:
        lines.append(f'  File /oname={name}.exe {nsis_quote(files[name])}')
    lines.extend([
        '  System::Call \'kernel32::SetEnvironmentVariableW(w "WEBCODEX_INSTALLER_CANDIDATE_CLI", w "$WebCodexCandidate\\artifacts\\bin\\webcodex.exe") i .r0\'',
        '  ${If} $0 == 0',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        f'  ExecWait \'"$WINDIR\\System32\\WindowsPowerShell\\v1.0\\powershell.exe" -NoProfile -NonInteractive -EncodedCommand {candidate_hash_command(manifest["artifacts"]["webcodex"]["sha256"])}\' $0',
        '  ${If} $0 != 0',
        '    MessageBox MB_ICONSTOP "WebCodex candidate CLI failed its manifest-bound SHA-256 check."',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  StrCpy $WebCodexTrustedCLI "$WebCodexCandidate\\artifacts\\bin\\webcodex.exe"',
        '  ${If} $WebCodexEnvironmentDir == ""',
        '    nsExec::ExecToStack \'"$WebCodexTrustedCLI" environment installer-classify --expected-runtime-dir "$WebCodexInstallDir\\webcodex-runtime" --json\'',
        '  ${Else}',
        '    ${If} $WebCodexUpgradeTargetFile != ""',
        '      nsExec::ExecToStack \'"$WebCodexTrustedCLI" environment installer-classify --expected-runtime-dir "$WebCodexInstallDir\\webcodex-runtime" --environment-dir "$WebCodexEnvironmentDir" --upgrade-target-file "$WebCodexUpgradeTargetFile" --json\'',
        '    ${Else}',
        '    nsExec::ExecToStack \'"$WebCodexTrustedCLI" environment installer-classify --expected-runtime-dir "$WebCodexInstallDir\\webcodex-runtime" --environment-dir "$WebCodexEnvironmentDir" --json\'',
        '    ${EndIf}',
        '  ${EndIf}',
        '  Pop $0',
        '  Pop $1',
        'webcodex_classification_trim:',
        '  StrCpy $R0 $1 1 -1',
        '  ${If} $R0 == "$\\r"',
        '  ${OrIf} $R0 == "$\\n"',
        '    StrCpy $1 $1 -1',
        '    Goto webcodex_classification_trim',
        '  ${EndIf}',
        '  ${If} $0 != 0',
        '    MessageBox MB_ICONSTOP "The installed package identity or Environment ownership could not be verified. Quit the old Desktop and stop its Runtime before retrying."',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ${If} $WebCodexUpgradeTargetFile != ""',
        '  ${AndIf} $1 != \'{"kind":"environment"}\'',
        '    MessageBox MB_ICONSTOP "The selected Environment is no longer available. Guarded installation cannot fall back to another installation mode."',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ${If} $1 == \'{"kind":"environment"}\'',
        '    ${If} $WebCodexUpgradeTargetFile == ""',
        '    StrCpy $WebCodexTrustedCLI "$WebCodexInstallDir\\webcodex-runtime\\webcodex.exe"',
        '    ${EndIf}',
        '  ${ElseIf} $1 == \'{"kind":"legacy"}\'',
        '    StrCpy $WebCodexPackageUpgrade 1',
        '  ${ElseIf} $1 == \'{"kind":"unconfigured"}\'',
        '    StrCpy $WebCodexPackageUpgrade 1',
        '  ${ElseIf} $1 != \'{"kind":"fresh"}\'',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ${If} $WebCodexForeignServices == 1',
        '  ${AndIf} $1 != \'{"kind":"environment"}\'',
        '    MessageBox MB_ICONSTOP "Windows contains WebCodex services without a matching Environment owner. Recover their original ownership before upgrading."',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ${If} $WebCodexExisting == 1',
        '  ${AndIf} $WebCodexUpgradeTargetFile == ""',
        '    ExecWait \'"$WebCodexTrustedCLI" environment installer-verify-same --candidate-dir "$WebCodexCandidate" --expected-runtime-dir "$WebCodexInstallDir\\webcodex-runtime" --json\' $0',
        '    ${If} $0 == 0',
        '      Goto webcodex_bootstrap_done',
        '    ${EndIf}',
        '  ${EndIf}',
        '  ${If} $WebCodexPackageUpgrade == 1',
        '    Call WebCodexPackagePreflight',
        '  ${Else}',
        '  ${If} $WebCodexEnvironmentDir == ""',
        '    ExecWait \'"$WebCodexTrustedCLI" environment upgrade-preflight --candidate-dir "$WebCodexCandidate" --json\' $0',
        '  ${Else}',
        '    ExecWait \'"$WebCodexTrustedCLI" environment upgrade-preflight --candidate-dir "$WebCodexCandidate" --environment-dir "$WebCodexEnvironmentDir" --json\' $0',
        '  ${EndIf}',
        '  ${EndIf}',
        '  ${If} $0 != 0',
        '    MessageBox MB_ICONSTOP "WebCodex upgrade preflight rejected this installer."',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ${If} $WebCodexUpgradeTargetFile != ""',
        '    nsExec::ExecToStack \'"$WebCodexTrustedCLI" environment upgrade-prepare --candidate-dir "$WebCodexCandidate" --environment-dir "$WebCodexEnvironmentDir" --upgrade-target-file "$WebCodexUpgradeTargetFile" --operation-id-output\'',
        '    Pop $0',
        '    Pop $WebCodexBoundOperation',
        'webcodex_operation_trim:',
        '    StrCpy $R0 $WebCodexBoundOperation 1 -1',
        '    ${If} $R0 == "$\\r"',
        '    ${OrIf} $R0 == "$\\n"',
        '      StrCpy $WebCodexBoundOperation $WebCodexBoundOperation -1',
        '      Goto webcodex_operation_trim',
        '    ${EndIf}',
        '    StrLen $R0 $WebCodexBoundOperation',
        '    ${If} $0 != 0',
        '    ${OrIf} $R0 != 36',
        '      StrCpy $WebCodexBoundOperation ""',
        '      SetErrorLevel 1',
        '      Abort',
        '    ${EndIf}',
        '  StrCpy $WebCodexUpgradePrepared 1',
        '  ${Else}',
        '  StrCpy $WebCodexUpgradePrepared 1',
        '  ${If} $WebCodexPackageUpgrade == 1',
        '    Call WebCodexPackagePrepare',
        '  ${Else}',
        '  ${If} $WebCodexEnvironmentDir == ""',
        '    ExecWait \'"$WebCodexTrustedCLI" environment upgrade-prepare --candidate-dir "$WebCodexCandidate" --json\' $0',
        '  ${Else}',
        '    ExecWait \'"$WebCodexTrustedCLI" environment upgrade-prepare --candidate-dir "$WebCodexCandidate" --environment-dir "$WebCodexEnvironmentDir" --json\' $0',
        '  ${EndIf}',
        '  ${EndIf}',
        '  ${EndIf}',
        '  ${If} $0 != 0',
        '    Call WebCodexRollback',
        '    MessageBox MB_ICONSTOP "WebCodex could not prepare the upgrade; installation was stopped."',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ${If} $WebCodexExisting == 1',
        '    ${If} $WebCodexPackageUpgrade == 1',
        '      Call WebCodexPackageVerify',
        '    ${Else}',
        '    ${If} $WebCodexEnvironmentDir == ""',
        '      ExecWait \'"$WebCodexTrustedCLI" environment installer-verify --candidate-dir "$WebCodexCandidate" --expected-runtime-dir "$WebCodexInstallDir\\webcodex-runtime" --json\' $0',
        '    ${Else}',
        '      ${If} $WebCodexUpgradeTargetFile != ""',
        '        ExecWait \'"$WebCodexTrustedCLI" environment installer-verify --candidate-dir "$WebCodexCandidate" --expected-runtime-dir "$WebCodexInstallDir\\webcodex-runtime" --environment-dir "$WebCodexEnvironmentDir" --upgrade-target-file "$WebCodexUpgradeTargetFile" --operation-id "$WebCodexBoundOperation" --json\' $0',
        '      ${Else}',
        '      ExecWait \'"$WebCodexTrustedCLI" environment installer-verify --candidate-dir "$WebCodexCandidate" --expected-runtime-dir "$WebCodexInstallDir\\webcodex-runtime" --environment-dir "$WebCodexEnvironmentDir" --json\' $0',
        '      ${EndIf}',
        '    ${EndIf}',
        '    ${EndIf}',
        '    ${If} $0 != 0',
        '      Call WebCodexRollback',
        '      MessageBox MB_ICONSTOP "Prepared owner receipt or runtime target does not match this installation. No files were replaced."',
        '      SetErrorLevel 1',
        '      Abort',
        '    ${EndIf}',
        '  ${EndIf}',
        '  ${If} ${FileExists} "$WebCodexInstallDir\\WebCodex.exe"',
        '    StrCpy $WebCodexOldDesktop "$PLUGINSDIR\\WebCodexDesktop.backup.exe"',
        '    CopyFiles /SILENT "$WebCodexInstallDir\\WebCodex.exe" "$WebCodexOldDesktop"',
        '  ${EndIf}',
        '  SetOutPath "$PLUGINSDIR"',
        f'  File /oname=WebCodexTauriPayload.exe {nsis_quote(inner)}',
        '  ExecWait \'"$PLUGINSDIR\\WebCodexTauriPayload.exe" /S /UPDATE /D=$WebCodexInstallDir\' $0',
        '  ${If} $0 != 0',
        '    Call WebCodexRollback',
        '    MessageBox MB_ICONSTOP "WebCodex installation failed; restoration of the selected operation was requested. Verify its recovery status before retrying."',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ${If} $WebCodexPackageUpgrade == 1',
        '    Call WebCodexPackageFinish',
        '  ${Else}',
        '  ${If} $WebCodexEnvironmentDir == ""',
        '    ExecWait \'"$WebCodexInstallDir\\webcodex-runtime\\webcodex.exe" environment upgrade-finish --json\' $0',
        '  ${Else}',
        '    ${If} $WebCodexUpgradeTargetFile != ""',
        '      ExecWait \'"$WebCodexTrustedCLI" environment upgrade-finish --environment-dir "$WebCodexEnvironmentDir" --upgrade-target-file "$WebCodexUpgradeTargetFile" --operation-id "$WebCodexBoundOperation" --json\' $0',
        '    ${Else}',
        '    ExecWait \'"$WebCodexInstallDir\\webcodex-runtime\\webcodex.exe" environment upgrade-finish --environment-dir "$WebCodexEnvironmentDir" --json\' $0',
        '    ${EndIf}',
        '  ${EndIf}',
        '  ${EndIf}',
        '  ${If} $0 != 0',
        '    Call WebCodexRollback',
        '    MessageBox MB_ICONSTOP "WebCodex could not verify the installed upgrade; rollback was requested."',
        '    SetErrorLevel 1',
        '    Abort',
        '  ${EndIf}',
        '  ReadRegStr $0 HKCU "Environment" "Path"',
        '  StrCpy $1 "$WebCodexInstallDir\\webcodex-runtime"',
        '  ${StrStr} $2 "$0" "$1"',
        '  ${If} $2 == ""',
        '    ${If} $0 == ""',
        '      WriteRegStr HKCU "Environment" "Path" "$1"',
        '    ${Else}',
        '      WriteRegStr HKCU "Environment" "Path" "$1;$0"',
        '    ${EndIf}',
        '  ${EndIf}',
        '  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000',
        '  StrCpy $WebCodexUpgradePrepared 0',
        'webcodex_bootstrap_done:',
        'SectionEnd',
        '',
        'Function WebCodexRollback',
        '  ${If} $WebCodexUpgradePrepared == 1',
        '    ${If} $WebCodexPackageUpgrade == 1',
        '      Call WebCodexPackageRollback',
        '    ${Else}',
        '    ${If} $WebCodexEnvironmentDir == ""',
        '      ExecWait \'"$WebCodexTrustedCLI" environment upgrade-rollback --json\' $0',
        '    ${Else}',
        '      ${If} $WebCodexUpgradeTargetFile != ""',
        '        ExecWait \'"$WebCodexTrustedCLI" environment upgrade-rollback --environment-dir "$WebCodexEnvironmentDir" --upgrade-target-file "$WebCodexUpgradeTargetFile" --operation-id "$WebCodexBoundOperation" --json\' $0',
        '      ${Else}',
        '      ExecWait \'"$WebCodexTrustedCLI" environment upgrade-rollback --environment-dir "$WebCodexEnvironmentDir" --json\' $0',
        '      ${EndIf}',
        '    ${EndIf}',
        '    ${EndIf}',
        '    ${If} $WebCodexPackageUpgrade == 0',
        '    ${AndIf} $0 == 0',
        '    ${AndIf} ${FileExists} "$WebCodexOldDesktop"',
        '      CopyFiles /SILENT "$WebCodexOldDesktop" "$WebCodexInstallDir\\WebCodex.exe"',
        '    ${EndIf}',
        '    ${If} $0 != 0',
        '      MessageBox MB_ICONSTOP "WebCodex rollback failed; the previous program backup remains available."',
        '    ${EndIf}',
        '  ${EndIf}',
        '  StrCpy $WebCodexUpgradePrepared 0',
        'FunctionEnd',
        '',
        'Function .onInstFailed',
        '  Call WebCodexRollback',
        'FunctionEnd',
        'Function .onUserAbort',
        '  Call WebCodexRollback',
        'FunctionEnd',
        '',
    ])
    for step in ("preflight", "prepare", "verify", "finish", "rollback"):
        arguments = ' --candidate-dir "$WebCodexCandidate"' if step in ("preflight", "prepare", "verify") else ""
        command = f'"$WebCodexCandidate\\artifacts\\bin\\webcodex.exe" environment package-upgrade-{step}{arguments} --expected-runtime-dir "$WebCodexInstallDir\\webcodex-runtime"'
        lines.extend([
            f"Function WebCodexPackage{step.title()}",
            '  ${If} $WebCodexEnvironmentDir == ""',
            f"    ExecWait '{command} --json' $0",
            '  ${Else}',
            f"    ExecWait '{command} --environment-dir \"$WebCodexEnvironmentDir\" --json' $0",
            '  ${EndIf}',
            'FunctionEnd',
            '',
        ])
    return "\n".join(lines)


def build(args: argparse.Namespace) -> dict:
    if args.output.exists():
        raise ValueError(f"output already exists: {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    source = render(args.candidate_dir, args.inner_installer, args.output, args.platform)
    script = args.output.with_suffix(".nsi")
    script.write_text(source, encoding="utf-8", newline="\n")
    compiler = args.makensis or shutil.which("makensis")
    if not compiler:
        raise ValueError("makensis is required to build the Windows bootstrap installer")
    result = subprocess.run([compiler, "/V3", str(script)], stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False, timeout=180)
    if result.returncode:
        args.output.unlink(missing_ok=True)
        raise ValueError("makensis failed to compile the outer Windows installer")
    manifest = json.loads((args.candidate_dir / "source-manifest.json").read_text(encoding="utf-8"))
    provenance = {
        "schema_version": 1,
        "guarded_handoff_version": 1,
        "platform": args.platform,
        "version": manifest["version"],
        "source_sha": manifest["source_sha"],
        "workflow_run_id": manifest["source_workflow_run_id"],
        "workflow_ref": manifest["source_workflow_ref"],
        "candidate_manifest_sha256": sha256(args.candidate_dir / "source-manifest.json"),
        "inner_installer_sha256": sha256(args.inner_installer),
        "installer_sha256": sha256(args.output),
    }
    sidecar = args.output.with_suffix(args.output.suffix + ".provenance.json")
    sidecar.write_text(json.dumps(provenance, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return provenance


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-dir", type=Path, required=True)
    parser.add_argument("--inner-installer", type=Path, required=True)
    parser.add_argument("--platform", choices=tuple(candidate.TARGETS), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--makensis")
    args = parser.parse_args()
    try:
        print(json.dumps(build(args), sort_keys=True))
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        raise SystemExit(f"Windows unified bootstrap build failed: {exc}") from exc
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
