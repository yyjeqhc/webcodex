use super::*;
use crate::validation_bridge::{
    failure_kinds, value_contains_absolute_path_leak, ValidationBridgeOptions,
    ValidationBridgeRequest, MAX_VALIDATION_STDOUT_BYTES, VALIDATION_BRIDGE_PROTOCOL_VERSION,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn typecheck_request(project_id: &str) -> ValidationBridgeRequest {
    ValidationBridgeRequest {
        protocol_version: VALIDATION_BRIDGE_PROTOCOL_VERSION,
        adapter_id: "pyright".into(),
        language: "python".into(),
        validation_kind: "typecheck".into(),
        project_id: project_id.into(),
        cwd: None,
        targets: vec![],
        timeout_secs: 30,
        options: ValidationBridgeOptions::default(),
    }
}

fn write_fake_pyright(bin_dir: &std::path::Path, script_body: &str) -> PathBuf {
    fs::create_dir_all(bin_dir).unwrap();
    let path = bin_dir.join("pyright");
    fs::write(&path, script_body).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

fn with_path<T>(bin_dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    with_path_mode(bin_dir, true, f)
}

/// When `prepend` is false, PATH is *only* `bin_dir` so real tools cannot leak in.
fn with_path_mode<T>(bin_dir: &std::path::Path, prepend: bool, f: impl FnOnce() -> T) -> T {
    let old = std::env::var_os("PATH");
    let joined = if prepend {
        let mut paths = vec![bin_dir.to_path_buf()];
        if let Some(old) = old.as_ref() {
            paths.extend(std::env::split_paths(old));
        }
        std::env::join_paths(paths).unwrap()
    } else {
        bin_dir.as_os_str().to_os_string()
    };
    std::env::set_var("PATH", &joined);
    std::env::remove_var("WEBCODEX_PYRIGHT");
    let result = f();
    match old {
        Some(v) => std::env::set_var("PATH", v),
        None => std::env::remove_var("PATH"),
    }
    result
}

#[test]
fn registry_exposes_pyright_only_for_now() {
    assert_eq!(registered_adapter_ids(), vec!["pyright"]);
    let meta = adapter_metadata("pyright").unwrap();
    assert_eq!(meta.language, "python");
    assert_eq!(meta.validation_kind, "typecheck");
}

#[test]
fn unknown_adapter_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let mut req = typecheck_request("demo");
    req.adapter_id = "does-not-exist".into();
    let err = execute_validation_at_root(tmp.path(), &req, 120).unwrap_err();
    assert!(!err.success);
    assert_eq!(
        err.error.as_ref().unwrap().code,
        failure_kinds::ADAPTER_NOT_FOUND
    );
}

#[test]
fn language_mismatch_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let mut req = typecheck_request("demo");
    req.language = "typescript".into();
    let err = execute_validation_at_root(tmp.path(), &req, 120).unwrap_err();
    assert_eq!(
        err.error.as_ref().unwrap().code,
        failure_kinds::LANGUAGE_ADAPTER_MISMATCH
    );
}

#[test]
fn absolute_cwd_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let mut req = typecheck_request("demo");
    req.cwd = Some("/etc".into());
    let err = execute_validation_at_root(tmp.path(), &req, 120).unwrap_err();
    assert_eq!(
        err.error.as_ref().unwrap().code,
        failure_kinds::INVALID_ARGUMENTS
    );
}

#[test]
fn path_traversal_target_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let mut req = typecheck_request("demo");
    req.targets = vec!["../secret.py".into()];
    let err = execute_validation_at_root(tmp.path(), &req, 120).unwrap_err();
    assert_eq!(
        err.error.as_ref().unwrap().code,
        failure_kinds::INVALID_ARGUMENTS
    );
}

#[test]
fn end_to_end_fake_pyright_success_and_diagnostics() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    fs::create_dir_all(root.join("src")).unwrap();
    let file = root.join("src/app.py");
    fs::write(&file, "x: int = 'nope'\n").unwrap();
    let abs = fs::canonicalize(&file).unwrap();
    let abs_json = abs.to_string_lossy().replace('\\', "\\\\");

    let bin = tempfile::tempdir().unwrap();
    let script = format!(
        r#"#!/bin/sh
cat <<'EOF'
{{
  "version": "1.1.382",
  "generalDiagnostics": [
    {{
      "file": "{abs_json}",
      "severity": "error",
      "message": "Type mismatch",
      "rule": "reportAssignmentType",
      "range": {{
        "start": {{ "line": 0, "character": 0 }},
        "end": {{ "line": 0, "character": 1 }}
      }}
    }}
  ],
  "summary": {{
    "filesAnalyzed": 1,
    "errorCount": 1,
    "warningCount": 0,
    "informationCount": 0,
    "timeInSec": 0.01
  }}
}}
EOF
exit 1
"#
    );
    write_fake_pyright(bin.path(), &script);

    let response = with_path(bin.path(), || {
        execute_validation_at_root(root, &typecheck_request("demo"), 120).unwrap()
    });

    assert!(response.command_started);
    assert!(!response.success);
    assert_eq!(
        response.failure_kind.as_deref(),
        Some(failure_kinds::COMPILE_ERROR)
    );
    assert_eq!(response.adapter_id, "pyright");
    assert_eq!(response.language, "python");
    assert_eq!(response.validation_kind, "typecheck");
    let diags = response.diagnostics.as_ref().unwrap();
    assert_eq!(diags.diagnostics.len(), 1);
    assert_eq!(diags.diagnostics[0].file.as_deref(), Some("src/app.py"));
    assert_eq!(diags.diagnostics[0].line, Some(1));
    assert_eq!(diags.diagnostics[0].column, Some(1));
    assert_eq!(diags.diagnostics[0].severity, "error");
    assert_eq!(diags.summary_error_count, Some(1));

    let encoded = serde_json::to_value(&response).unwrap();
    assert!(
        !value_contains_absolute_path_leak(&encoded),
        "response leaked absolute path: {encoded}"
    );
    let raw = serde_json::to_string(&response).unwrap();
    assert!(!raw.contains("generalDiagnostics"));
    assert!(!raw.contains(abs.to_str().unwrap()));
    // Project files unchanged.
    assert_eq!(fs::read_to_string(&file).unwrap(), "x: int = 'nope'\n");
}

#[test]
fn end_to_end_exit_zero_no_diagnostics_is_success() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    fs::write(root.join("ok.py"), "x = 1\n").unwrap();
    let bin = tempfile::tempdir().unwrap();
    write_fake_pyright(
        bin.path(),
        r#"#!/bin/sh
cat <<'EOF'
{
  "version": "1.1.382",
  "generalDiagnostics": [],
  "summary": {
    "filesAnalyzed": 1,
    "errorCount": 0,
    "warningCount": 0,
    "informationCount": 0,
    "timeInSec": 0.01
  }
}
EOF
exit 0
"#,
    );
    let response = with_path(bin.path(), || {
        execute_validation_at_root(root, &typecheck_request("demo"), 120).unwrap()
    });
    assert!(response.success);
    assert!(response.command_started);
    assert!(response.failure_kind.is_none());
    assert_eq!(
        response.diagnostics.as_ref().unwrap().summary_error_count,
        Some(0)
    );
}

#[test]
fn fake_pyright_missing_reports_tool_unavailable() {
    let project = tempfile::tempdir().unwrap();
    let empty_bin = tempfile::tempdir().unwrap();
    // PATH with empty dir only — no pyright (do not prepend system PATH).
    let response = with_path_mode(empty_bin.path(), false, || {
        execute_validation_at_root(project.path(), &typecheck_request("demo"), 120).unwrap()
    });
    assert!(!response.command_started);
    assert!(!response.tool_available);
    assert_eq!(
        response.failure_kind.as_deref(),
        Some(failure_kinds::TOOL_UNAVAILABLE)
    );
}

#[test]
fn oversized_stdout_is_not_parsed() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    let bin = tempfile::tempdir().unwrap();
    let over = MAX_VALIDATION_STDOUT_BYTES + 8192;
    // Pure shell: print 64-byte chunks until past the hard capture cap.
    let script = format!(
        r#"#!/bin/sh
i=0
while [ "$i" -lt {over} ]; do
  printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
  i=$((i+64))
done
exit 0
"#
    );
    write_fake_pyright(bin.path(), &script);
    let response = with_path(bin.path(), || {
        execute_validation_at_root(root, &typecheck_request("demo"), 30).unwrap()
    });
    assert!(response.command_started, "{response:?}");
    assert!(
        response.stdout_capped,
        "expected stdout cap; got failure_kind={:?} message={:?} bytes={}",
        response.failure_kind, response.message, response.stdout_bytes
    );
    assert_eq!(
        response.failure_kind.as_deref(),
        Some(failure_kinds::OUTPUT_TOO_LARGE)
    );
    assert!(response.diagnostics.is_none());
}

#[test]
fn malformed_json_is_structured_failure() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    let bin = tempfile::tempdir().unwrap();
    write_fake_pyright(
        bin.path(),
        r#"#!/bin/sh
echo 'not-json'
exit 1
"#,
    );
    let response = with_path(bin.path(), || {
        execute_validation_at_root(root, &typecheck_request("demo"), 120).unwrap()
    });
    assert!(response.command_started);
    assert_eq!(
        response.failure_kind.as_deref(),
        Some(failure_kinds::MALFORMED_OUTPUT)
    );
    assert!(!response.success);
}

#[test]
fn unicode_paths_and_messages_are_preserved_relative() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    fs::create_dir_all(root.join("源")).unwrap();
    let file = root.join("源/测试.py");
    fs::write(&file, "x = 1\n").unwrap();
    let abs = fs::canonicalize(&file).unwrap();
    let abs_json = abs.to_string_lossy().replace('\\', "\\\\");
    let bin = tempfile::tempdir().unwrap();
    let script = format!(
        r#"#!/bin/sh
cat <<'EOF'
{{
  "generalDiagnostics": [
    {{
      "file": "{abs_json}",
      "severity": "information",
      "message": "你好 world",
      "rule": "reportGeneralTypeIssues",
      "range": {{
        "start": {{ "line": 0, "character": 0 }},
        "end": {{ "line": 0, "character": 1 }}
      }}
    }}
  ],
  "summary": {{ "errorCount": 0, "warningCount": 0, "informationCount": 1 }}
}}
EOF
exit 0
"#
    );
    write_fake_pyright(bin.path(), &script);
    let response = with_path(bin.path(), || {
        execute_validation_at_root(root, &typecheck_request("demo"), 120).unwrap()
    });
    assert!(response.success); // information only → no errors
    let diag = &response.diagnostics.as_ref().unwrap().diagnostics[0];
    assert_eq!(diag.file.as_deref(), Some("源/测试.py"));
    assert_eq!(diag.message, "你好 world");
    assert_eq!(diag.severity, "information");
}
