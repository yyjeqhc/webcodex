use crate::projects::{ProjectConfig, ProjectsConfig, ProjectsState, SshConfig};
use salvo::prelude::*;
mod agent_exec;
mod artifact;
mod capabilities;
mod command_request;
mod command_workflow;
mod context;
mod doctor;
mod edit;
mod git;
mod hooks;
mod jobs;
mod patch;
mod remote_edit;
mod report;
mod security;
mod shell;
mod source;
mod trusted;
mod types;
mod url_security;
mod workflow;
pub use artifact::codex_artifact;
pub use capabilities::codex_projects;
#[cfg(test)]
use command_request::*;
pub use command_request::{
    codex_check, codex_command, codex_command_approve, codex_command_reject, codex_command_request,
    codex_command_request_batch, codex_command_request_op, codex_command_request_raw,
    codex_command_requests,
};
use context::*;
pub use context::{codex_context, codex_context_batch};
pub use doctor::codex_project_doctor;
pub use edit::codex_edit;
use edit::*;
pub use git::codex_git;
#[cfg(test)]
use git::*;
pub use hooks::codex_project_hook;
pub use jobs::codex_job;
pub use patch::codex_apply_patch;
use remote_edit::*;
pub use report::codex_report;
pub use security::is_sensitive_path;
use shell::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use types::*;
#[cfg(test)]
use url_security::*;
pub use workflow::codex_project_workflow;
// =============================================================================
// Request / Response types
// =============================================================================

// =============================================================================
// Constants
// =============================================================================

pub(super) const MAX_OUTPUT_LEN: usize = 50_000;
pub(super) const CHECK_TIMEOUT_SECS: u64 = 300;

// =============================================================================
// SSH stubs (removed in v2 — SSH executor no longer supported)
// =============================================================================

pub(super) const SSH_DISABLED_MESSAGE: &str = "SSH removed in v2";

pub(super) fn is_ssh_enabled(_depot: &Depot) -> bool {
    false
}

pub(super) fn ssh_disabled_error() -> String {
    SSH_DISABLED_MESSAGE.to_string()
}

pub(in crate::codex) fn ensure_ssh_enabled(
    _depot: &Depot,
    proj: &ProjectConfig,
) -> Result<(), String> {
    if proj.is_ssh() {
        return Err(ssh_disabled_error());
    }
    Ok(())
}

/// SSH context stubs — return errors since SSH is removed in v2.
pub(super) fn ssh_overview(
    _proj: &ProjectConfig,
    project_name: &str,
    _ssh_config: Option<&crate::projects::SshConfig>,
) -> types::ContextResponse {
    types::ContextResponse {
        success: false,
        project: project_name.to_string(),
        mode: "overview".to_string(),
        content: None,
        items: None,
        truncated: false,
        error: Some("SSH executor removed in v2".to_string()),
    }
}

pub(super) fn ssh_tree(
    _proj: &ProjectConfig,
    project_name: &str,
    _depth: usize,
    _max_files: usize,
    _ssh_config: Option<&crate::projects::SshConfig>,
) -> types::ContextResponse {
    types::ContextResponse {
        success: false,
        project: project_name.to_string(),
        mode: "tree".to_string(),
        content: None,
        items: None,
        truncated: false,
        error: Some("SSH executor removed in v2".to_string()),
    }
}

pub(super) fn ssh_search(
    _proj: &ProjectConfig,
    project_name: &str,
    _query: &str,
    _ssh_config: Option<&crate::projects::SshConfig>,
) -> types::ContextResponse {
    types::ContextResponse {
        success: false,
        project: project_name.to_string(),
        mode: "search".to_string(),
        content: None,
        items: None,
        truncated: false,
        error: Some("SSH executor removed in v2".to_string()),
    }
}

pub(super) fn ssh_grep_context(
    _proj: &ProjectConfig,
    project_name: &str,
    _path: &str,
    _query: Option<&str>,
    _ssh_config: Option<&crate::projects::SshConfig>,
) -> types::ContextResponse {
    types::ContextResponse {
        success: false,
        project: project_name.to_string(),
        mode: "grep_context".to_string(),
        content: None,
        items: None,
        truncated: false,
        error: Some("SSH executor removed in v2".to_string()),
    }
}

pub(super) fn ssh_read_file(
    _proj: &ProjectConfig,
    project_name: &str,
    _path: &str,
    _start_line: Option<usize>,
    _max_lines: Option<usize>,
    _ssh_config: Option<&crate::projects::SshConfig>,
) -> types::ContextResponse {
    types::ContextResponse {
        success: false,
        project: project_name.to_string(),
        mode: "read_file".to_string(),
        content: None,
        items: None,
        truncated: false,
        error: Some("SSH executor removed in v2".to_string()),
    }
}

pub(super) fn try_ssh_context_batch_once(
    _proj: &ProjectConfig,
    _project_name: &str,
    _requests: &[types::ContextBatchItem],
    _ssh_config: Option<&crate::projects::SshConfig>,
) -> Option<(
    Vec<types::ContextBatchResultMetadata>,
    types::ContextBatchResponse,
    Vec<bool>,
    u32,
)> {
    None
}

pub(super) fn ssh_context_batch_error_results(
    project_name: &str,
    requests: &[types::ContextBatchItem],
    error: String,
) -> (
    Vec<types::ContextBatchResultMetadata>,
    types::ContextBatchResponse,
    Vec<bool>,
    u32,
) {
    let results = requests
        .iter()
        .enumerate()
        .map(|(i, item)| types::ContextBatchResultMetadata {
            request_index: i,
            mode: format!("{:?}", item.mode),
            path: item.path.clone(),
            fingerprint: None,
            unchanged: false,
            file_size: None,
            modified_unix_ms: None,
            total_lines: None,
        })
        .collect();
    let response = types::ContextBatchResponse {
        success: false,
        project: project_name.to_string(),
        results: Vec::new(),
        duration_ms: 0,
        ssh_calls: 0,
        error: Some(error),
        preflight_rejected: None,
        estimated_chars: None,
        max_allowed_chars: None,
        max_allowed_items: None,
        project_is_ssh: None,
        suggestion: None,
        warnings: Vec::new(),
        result_metadata: Vec::new(),
        cache_hits: None,
        recommended_next_action: None,
        action_budget_hint: None,
    };
    (results, response, vec![false; requests.len()], 0)
}

// =============================================================================
// Helpers
// =============================================================================

pub(super) fn get_projects(depot: &Depot) -> Option<Arc<ProjectsConfig>> {
    depot
        .obtain::<Arc<ProjectsState>>()
        .ok()
        .and_then(|state| state.config.clone())
}

pub(super) fn get_projects_load_error(depot: &Depot) -> Option<String> {
    depot
        .obtain::<Arc<ProjectsState>>()
        .ok()
        .and_then(|state| state.load_error.clone())
}

pub(super) fn get_projects_config_path(depot: &Depot) -> Option<String> {
    depot
        .obtain::<Arc<ProjectsState>>()
        .ok()
        .map(|state| state.config_path.clone())
}

pub(super) fn truncate_string(s: String, max_len: usize) -> (String, bool) {
    if s.len() <= max_len {
        (s, false)
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        (s[..end].to_string(), true)
    }
}

// =============================================================================
// Command helpers
// =============================================================================

/// Run a command in the project directory (local only).
pub(super) fn run_project_cmd(
    proj: &ProjectConfig,
    cmd: &str,
    timeout_secs: u64,
) -> (i32, String, String, u64) {
    run_command(cmd, &proj.root(), timeout_secs)
}

// =============================================================================
// Agent context
// =============================================================================

pub(super) fn agent_context_shell_fragment() -> String {
    let files = AGENT_CONTEXT_FILES
        .iter()
        .map(|f| shell_escape(f))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        " printf '# Agent context\\n\\nLoaded project rules and memory files for alignment before planning or editing.\\n'; for f in {}; do printf '\\n## %s\\n\\n' \"$f\"; if test -f \"$f\"; then sed -n '1,240p' -- \"$f\"; else printf '(missing)\\n'; fi; done;",
        files
    )
}

// =============================================================================
// Trusted async shell job helpers
// =============================================================================

// =============================================================================
// Handlers
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sensitive_path_variants() {
        assert!(is_sensitive_path(".env"));
        assert!(is_sensitive_path(".env.local"));
        assert!(is_sensitive_path("secret.pem"));
        assert!(is_sensitive_path("id_rsa"));
        assert!(is_sensitive_path(".git/config"));
        assert!(!is_sensitive_path("src/main.rs"));
        assert!(!is_sensitive_path("README.md"));
    }

    #[test]
    fn test_validate_command_name_accepts_safe_ids() {
        assert!(validate_command_name("clippy").is_ok());
        assert!(validate_command_name("doc.build-1").is_ok());
    }

    #[test]
    fn test_validate_command_name_rejects_shell_like_text() {
        assert!(validate_command_name("").is_err());
        assert!(validate_command_name("cargo test").is_err());
        assert!(validate_command_name("test;rm").is_err());
        assert!(validate_command_name(&"a".repeat(101)).is_err());
    }

    #[test]
    fn test_get_project_command_returns_configured_command() {
        let mut commands = HashMap::new();
        commands.insert("smoke".to_string(), "echo ok".to_string());
        let proj = ProjectConfig {
            path: "/tmp/project".to_string(),
            executor: crate::projects::Executor::Local,
            host: None,
            ssh_hosts: Vec::new(),
            user: None,
            client_id: None,
            allow_patch: true,
            allow_command_requests: true,
            allow_raw_command_requests: true,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands,
            hooks: HashMap::new(),
        };
        assert_eq!(get_project_command(&proj, "smoke").unwrap(), "echo ok");
        assert!(get_project_command(&proj, "missing").is_err());
    }

    #[test]
    fn test_git_command_status_and_diff_are_fixed() {
        let status = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::Status,
            paths: vec![],
            message: None,
            checkpoint_id: None,
        };
        assert_eq!(
            git_command_for_request(&status).unwrap(),
            "git status --short"
        );
        let diff = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::Diff,
            paths: vec!["src/main.rs".to_string()],
            message: None,
            checkpoint_id: None,
        };
        assert_eq!(
            git_command_for_request(&diff).unwrap(),
            "git diff -- 'src/main.rs'"
        );
    }

    #[test]
    fn test_git_checkpoint_commands_are_fixed() {
        let checkpoint = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::Checkpoint,
            paths: vec![],
            message: None,
            checkpoint_id: Some("before-edit".to_string()),
        };
        let cmd = git_command_for_request(&checkpoint).unwrap();
        assert!(cmd.contains("mkdir -p .codex/checkpoints"));
        assert!(cmd.contains("git diff --binary"));
        assert!(cmd.contains(".codex/checkpoints/before-edit.patch"));
        assert!(cmd.contains("checkpoint_id"));

        let rollback = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::RollbackToCheckpoint,
            paths: vec![],
            message: None,
            checkpoint_id: Some("before-edit".to_string()),
        };
        let cmd = git_command_for_request(&rollback).unwrap();
        assert!(cmd.contains("git apply -R"));
        assert!(cmd.contains("git apply --whitespace=nowarn"));
        assert!(cmd.contains("rolled_back_to_checkpoint"));

        let missing_id = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::RollbackToCheckpoint,
            paths: vec![],
            message: None,
            checkpoint_id: None,
        };
        assert!(git_command_for_request(&missing_id).is_err());
    }

    #[test]
    fn test_git_command_commit_is_fixed_and_no_verify() {
        let request = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::Commit,
            paths: vec!["src/main.rs".to_string()],
            message: Some("Add feature".to_string()),
            checkpoint_id: None,
        };
        let cmd = git_command_for_request(&request).unwrap();
        assert!(cmd.contains("git add -- 'src/main.rs'"));
        assert!(cmd.contains("git diff --cached --quiet -- 'src/main.rs'"));
        assert!(cmd.contains("No staged changes to commit"));
        assert!(cmd.contains("git commit -m 'Add feature' --no-verify"));
    }

    #[test]
    fn test_git_command_commit_rejects_bad_message() {
        let request = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::Commit,
            paths: vec!["src/main.rs".to_string()],
            message: Some("bad\nmessage".to_string()),
            checkpoint_id: None,
        };
        assert!(git_command_for_request(&request).is_err());
    }

    #[test]
    fn test_raw_command_validation_rejects_high_risk_tokens() {
        assert!(validate_raw_command_text("echo ok").is_ok());
        assert!(validate_raw_command_text("git status --short").is_ok());
        assert!(validate_raw_command_text("git push origin main").is_err());
        assert!(validate_raw_command_text("sudo systemctl restart nginx").is_err());
        assert!(validate_raw_command_text("rm -rf target").is_err());
        assert!(validate_raw_command_text("echo one\necho two").is_err());
    }

    #[test]
    fn test_git_command_amend_is_fixed_and_no_verify() {
        let request = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::CommitAmendNoEdit,
            paths: vec!["src/codex.rs".to_string()],
            message: None,
            checkpoint_id: None,
        };
        let cmd = git_command_for_request(&request).unwrap();
        assert!(cmd.contains("git add -- 'src/codex.rs'"));
        assert!(cmd.contains("git diff --cached --quiet -- 'src/codex.rs'"));
        assert!(cmd.contains("No staged changes to amend"));
        assert!(cmd.contains("git commit --amend --no-edit --no-verify"));
    }

    #[test]
    fn test_git_paths_reject_too_many_paths() {
        let paths = (0..=MAX_GIT_PATHS)
            .map(|i| format!("src/file{i}.rs"))
            .collect::<Vec<_>>();
        let err = validate_git_paths(&paths).unwrap_err();
        assert!(err.contains("too many paths"));
        assert!(err.contains("50"));
    }

    #[test]
    fn test_git_paths_reject_too_long_path() {
        let long_path = format!("src/{}.rs", "a".repeat(MAX_GIT_PATH_LEN));
        let err = validate_git_paths(&[long_path]).unwrap_err();
        assert!(err.contains("path is too long"));
        assert!(err.contains("512"));
    }

    #[test]
    fn test_git_command_rejects_sensitive_paths() {
        let request = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::Add,
            paths: vec![".env".to_string()],
            message: None,
            checkpoint_id: None,
        };
        assert!(git_command_for_request(&request).is_err());
    }

    #[test]
    fn test_git_mutating_commands_require_paths() {
        let request = GitRequest {
            project: "p".to_string(),
            operation: GitOperation::CommitAmendNoEdit,
            paths: vec![],
            message: None,
            checkpoint_id: None,
        };
        assert!(git_command_for_request(&request).is_err());
    }

    #[test]
    fn test_invalid_read_file_ranges_return_errors() {
        assert!(validate_read_file_range(0, 10).is_err());
        assert!(validate_read_file_range(1, 0).is_err());
        assert!(validate_read_file_range(1, MAX_READ_FILE_LIMIT + 1).is_err());
        assert!(validate_read_file_range(usize::MAX, 2).is_err());
    }

    #[test]
    fn test_local_executor_is_default() {
        let proj = ProjectConfig {
            path: "/tmp/test".to_string(),
            executor: crate::projects::Executor::default(),
            host: None,
            ssh_hosts: Vec::new(),
            user: None,
            client_id: None,
            allow_patch: false,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        };
        assert!(!proj.is_ssh());
    }

    // =========================================================================
    // Edit unit tests
    // =========================================================================

    #[test]
    fn test_replace_nth_single_match() {
        let result = replace_nth("hello world", "world", "rust", None).unwrap();
        assert_eq!(result, "hello rust");
    }

    #[test]
    fn test_replace_nth_no_match() {
        let result = replace_nth("hello world", "xyz", "abc", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_replace_nth_empty_old() {
        let result = replace_nth("hello", "", "x", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_replace_nth_multiple_no_occurrence() {
        let result = replace_nth("aXbXc", "X", "Y", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("2 times"));
    }

    #[test]
    fn test_replace_nth_multiple_with_occurrence() {
        let result = replace_nth("aXbXc", "X", "Y", Some(2)).unwrap();
        assert_eq!(result, "aXbYc");
    }

    #[test]
    fn test_replace_nth_occurrence_zero() {
        let result = replace_nth("abc", "a", "b", Some(0));
        assert!(result.is_err());
    }

    #[test]
    fn test_replace_nth_occurrence_too_large() {
        let result = replace_nth("abc", "a", "b", Some(5));
        assert!(result.is_err());
    }

    #[test]
    fn test_replace_line_range_basic() {
        let content = "line1\nline2\nline3\n";
        let result = replace_line_range(content, 2, 2, "new2\n").unwrap();
        assert_eq!(result, "line1\nnew2\nline3\n");
    }

    #[test]
    fn test_replace_line_range_multi() {
        let content = "line1\nline2\nline3\nline4\n";
        let result = replace_line_range(content, 2, 3, "replaced\n").unwrap();
        assert_eq!(result, "line1\nreplaced\nline4\n");
    }

    #[test]
    fn test_replace_line_range_invalid_start() {
        let content = "line1\nline2\n";
        let result = replace_line_range(content, 0, 1, "x");
        assert!(result.is_err());
    }

    #[test]
    fn test_replace_line_range_exceeds() {
        let content = "line1\n";
        let result = replace_line_range(content, 1, 5, "x");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_no_mixed_edit_kinds_rejects_same_path_text_binary() {
        let edits = vec![
            EditOperation::WriteFile {
                path: "docs/diagram.bin".to_string(),
                content: "text".to_string(),
                allow_overwrite: true,
            },
            EditOperation::WriteBinaryFile {
                path: "docs/diagram.bin".to_string(),
                base64_content: "AAE=".to_string(),
                allow_overwrite: true,
            },
        ];
        let err = validate_no_mixed_edit_kinds(&edits).unwrap_err();
        assert!(err.contains("cannot mix text and binary edits for the same path"));
    }

    #[test]
    fn test_validate_no_mixed_edit_kinds_allows_same_path_same_kind() {
        let edits = vec![
            EditOperation::WriteBinaryFile {
                path: "docs/diagram.bin".to_string(),
                base64_content: "AAE=".to_string(),
                allow_overwrite: true,
            },
            EditOperation::WriteBinaryFile {
                path: "docs/diagram.bin".to_string(),
                base64_content: "AQI=".to_string(),
                allow_overwrite: true,
            },
        ];
        assert!(validate_no_mixed_edit_kinds(&edits).is_ok());
    }

    #[test]
    fn test_read_binary_from_upload_accepts_project_relative_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("upload.bin");
        std::fs::write(&source, [1_u8, 2, 3, 4]).unwrap();
        let bytes = read_binary_from_upload(dir.path(), "upload.bin", "docs/out.bin").unwrap();
        assert_eq!(bytes, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_read_binary_from_upload_rejects_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_binary_from_upload(dir.path(), "../secret.bin", "docs/out.bin").unwrap_err();
        assert!(err.contains("traversal"));
    }

    #[test]
    fn test_validate_source_url_rejects_localhost() {
        let err = validate_source_url("http://localhost:8080/file.png").unwrap_err();
        assert!(err.contains("not allowed"));
        let err = validate_source_url("http://127.0.0.1/file.png").unwrap_err();
        assert!(err.contains("blocked private/local"));
    }

    #[test]
    fn test_validate_source_url_rejects_non_http() {
        let err = validate_source_url("file:///tmp/file.png").unwrap_err();
        assert!(err.contains("http or https"));
    }

    #[test]
    fn test_validate_source_url_allows_chatgpt_estuary_content() {
        let url = validate_source_url("https://chatgpt.com/backend-api/estuary/content?id=file_abc123&ts=1&p=fsns&cid=1&sig=abc&v=0").unwrap();
        assert_eq!(url.host_str(), Some("chatgpt.com"));
        assert_eq!(url.path(), "/backend-api/estuary/content");
    }

    #[test]
    fn test_chatgpt_estuary_allowlist_rejects_non_estuary_path() {
        let url = reqwest::Url::parse("https://chatgpt.com/api/not-estuary?id=file_abc123&sig=abc")
            .unwrap();
        assert!(!is_allowed_chatgpt_estuary_url(&url));
    }

    #[test]
    fn test_decode_binary_artifact_accepts_small_base64() {
        let bytes = decode_binary_artifact("AAECAw==", "docs/pixel.bin").unwrap();
        assert_eq!(bytes, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_decode_binary_artifact_rejects_invalid_base64() {
        let err = decode_binary_artifact("not valid base64!", "docs/pixel.bin").unwrap_err();
        assert!(err.contains("Invalid base64"));
    }

    #[test]
    fn test_simple_binary_diff_mentions_sizes() {
        let diff = simple_binary_diff("docs/pixel.bin", Some(2), 4);
        assert!(diff.contains("Binary files"));
        assert!(diff.contains("old size: 2"));
        assert!(diff.contains("new size: 4"));
    }

    #[test]
    fn test_validate_edit_path_rejects_env() {
        assert!(validate_edit_path(".env").is_err());
        assert!(validate_edit_path("config/.env").is_err());
    }

    #[test]
    fn test_validate_edit_path_rejects_traversal() {
        assert!(validate_edit_path("../evil.txt").is_err());
        assert!(validate_edit_path("src/../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_edit_path_rejects_absolute() {
        assert!(validate_edit_path("/etc/passwd").is_err());
    }

    #[test]
    fn test_validate_edit_path_rejects_target() {
        assert!(validate_edit_path("target/debug/binary").is_err());
    }

    #[test]
    fn test_validate_edit_path_allows_normal() {
        assert!(validate_edit_path("src/main.rs").is_ok());
        assert!(validate_edit_path("README.md").is_ok());
        assert!(validate_edit_path(".gitignore").is_ok());
    }

    #[test]
    fn test_validate_edit_path_rejects_git_dir() {
        assert!(validate_edit_path(".git/config").is_err());
        assert!(validate_edit_path(".git/hooks/pre-commit").is_err());
    }

    // =========================================================================
    // SSH edit safety tests
    // =========================================================================

    #[test]
    fn test_shell_escape_no_injection() {
        // Verify that shell_escape properly wraps in single quotes
        // Input: '; rm -rf /; echo '
        // Expected output: '\'''; rm -rf /; echo '\''
        // The outer single quotes prevent shell interpretation of the content
        let dangerous = "'; rm -rf /; echo '";
        let escaped = shell_escape(dangerous);
        // Should start and end with single quote
        assert!(escaped.starts_with('\''));
        assert!(escaped.ends_with('\''));
        // Should contain the escaped single-quote sequence ('\'' means: end quote, literal quote, start quote)
        assert!(escaped.contains("'\\''"));
        // The escaped form should be: '\''  '; rm -rf /; echo '  '\''
        // which is safe because the dangerous content is inside single quotes
    }

    // =========================================================================
    // Remote python3 script local run test
    // =========================================================================

    #[test]
    fn test_remote_edit_script_replace_text_local() {
        // Run the embedded python3 script locally to verify it works
        let tmp = tempfile::tempdir().unwrap_or_else(|_| {
            // fallback if tempfile not available
            let d = std::path::PathBuf::from("/tmp/private-drop-test-script");
            let _ = std::fs::create_dir_all(&d);
            // Return a wrapper
            tempfile::TempDir::new_in(&d).unwrap()
        });
        let root = tmp.path();
        std::fs::write(root.join("test.txt"), "hello world\n").unwrap();

        let request = serde_json::json!({
            "dry_run": false,
            "edits": [{
                "type": "replace_text",
                "path": "test.txt",
                "old_text": "world",
                "new_text": "rust"
            }]
        });

        let mut child = std::process::Command::new("python3")
            .arg("-c")
            .arg(REMOTE_EDIT_SCRIPT)
            .arg(root.to_str().unwrap())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to spawn python3");

        if let Some(ref mut stdin) = child.stdin {
            use std::io::Write;
            stdin.write_all(request.to_string().as_bytes()).unwrap();
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "Script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["changed_files"][0], "test.txt");
        assert!(result["diff"].as_str().unwrap().contains("-hello world"));
        assert!(result["diff"].as_str().unwrap().contains("+hello rust"));
        // Verify the file was actually modified
        let content = std::fs::read_to_string(root.join("test.txt")).unwrap();
        assert_eq!(content, "hello rust\n");
    }

    #[test]
    fn test_remote_edit_script_dry_run_local() {
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap_or_else(|_| {
            let d = std::path::PathBuf::from("/tmp/private-drop-test-dry");
            let _ = std::fs::create_dir_all(&d);
            tempfile::TempDir::new_in(&d).unwrap()
        });
        let root = tmp.path();
        std::fs::write(root.join("test.txt"), "original content\n").unwrap();

        let request = serde_json::json!({
            "dry_run": true,
            "edits": [{
                "type": "replace_text",
                "path": "test.txt",
                "old_text": "original",
                "new_text": "changed"
            }]
        });

        let mut child = std::process::Command::new("python3")
            .arg("-c")
            .arg(REMOTE_EDIT_SCRIPT)
            .arg(root.to_str().unwrap())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to spawn python3");

        if let Some(ref mut stdin) = child.stdin {
            stdin.write_all(request.to_string().as_bytes()).unwrap();
        }
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["success"], true);
        assert!(result["diff"].as_str().unwrap().contains("-original"));
        assert!(result["diff"].as_str().unwrap().contains("+changed"));
        // Verify the file was NOT modified (dry_run)
        let content = std::fs::read_to_string(root.join("test.txt")).unwrap();
        assert_eq!(content, "original content\n");
    }

    #[test]
    fn test_remote_edit_script_rejects_env() {
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap_or_else(|_| {
            let d = std::path::PathBuf::from("/tmp/private-drop-test-env");
            let _ = std::fs::create_dir_all(&d);
            tempfile::TempDir::new_in(&d).unwrap()
        });
        let root = tmp.path();

        let request = serde_json::json!({
            "dry_run": false,
            "edits": [{
                "type": "replace_text",
                "path": ".env",
                "old_text": "x",
                "new_text": "y"
            }]
        });

        let mut child = std::process::Command::new("python3")
            .arg("-c")
            .arg(REMOTE_EDIT_SCRIPT)
            .arg(root.to_str().unwrap())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to spawn python3");

        if let Some(ref mut stdin) = child.stdin {
            stdin.write_all(request.to_string().as_bytes()).unwrap();
        }
        let output = child.wait_with_output().unwrap();
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("sensitive"));
    }
}

pub(super) fn apply_edit_request_with_metrics(
    projects: &ProjectsConfig,
    proj: &ProjectConfig,
    body: &EditRequest,
    operation: &'static str,
) -> EditResponse {
    let edit_start = Instant::now();
    let response = local_apply_project_edit(proj, body);
    tracing::info!(
        target: "codex.metrics",
        operation = operation,
        project = %body.project,
        executor = "local",
        success = response.success,
        dry_run = body.dry_run,
        edit_count = body.edits.len(),
        changed_files = response.changed_files.len(),
        duration_ms = edit_start.elapsed().as_millis() as u64,
        ssh_calls = 0,
        control_master = false,
        "codex_edit_completed"
    );
    response
}

#[cfg(test)]
mod ssh_command_tests {
    use super::*;

    fn command_args(command: &std::process::Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect()
    }

    // --- Context batch preflight tests ---

    fn make_batch_request(
        items: Vec<ContextBatchItem>,
        max_total_chars: usize,
    ) -> ContextBatchRequest {
        ContextBatchRequest {
            project: "test".to_string(),
            requests: items,
            max_total_chars,
        }
    }

    #[test]
    fn preflight_local_small_batch_passes() {
        let req = make_batch_request(
            vec![
                ContextBatchItem {
                    mode: ContextMode::Overview,
                    path: None,
                    query: None,
                    if_fingerprint: None,
                    start_line: 1,
                    limit: 200,
                    max_depth: default_tree_max_depth(),
                },
                ContextBatchItem {
                    mode: ContextMode::ReadFile,
                    path: Some("README.md".to_string()),
                    query: None,
                    if_fingerprint: None,
                    start_line: 1,
                    limit: 50,
                    max_depth: default_tree_max_depth(),
                },
            ],
            60_000,
        );
        let result = context::preflight_context_batch(&req, false, "test");
        assert!(result.is_ok(), "Small local batch should pass preflight");
    }

    #[test]
    fn preflight_rejects_max_total_chars_over_hard_limit() {
        let req = make_batch_request(
            vec![ContextBatchItem {
                mode: ContextMode::Overview,
                path: None,
                query: None,
                if_fingerprint: None,
                start_line: 1,
                limit: 200,
                max_depth: default_tree_max_depth(),
            }],
            200_000, // exceeds PREFLIGHT_MAX_TOTAL_CHARS (180_000)
        );
        let result = context::preflight_context_batch(&req, false, "test");
        assert!(
            result.is_err(),
            "Should reject max_total_chars over hard limit"
        );
        let resp = result.unwrap_err();
        assert!(!resp.success);
        assert_eq!(resp.preflight_rejected, Some(true));
        assert!(resp.error.as_ref().unwrap().contains("too large"));
        assert!(resp.suggestion.is_some());
        assert!(resp.max_allowed_chars.is_some());
    }

    #[test]
    fn preflight_ssh_rejects_too_many_items() {
        let items: Vec<ContextBatchItem> = (0..13)
            .map(|_| ContextBatchItem {
                mode: ContextMode::ReadFile,
                path: Some("file.txt".to_string()),
                query: None,
                if_fingerprint: None,
                start_line: 1,
                limit: 50,
                max_depth: default_tree_max_depth(),
            })
            .collect();
        let req = make_batch_request(items, 60_000);
        let result = context::preflight_context_batch(&req, true, "test");
        assert!(result.is_err(), "Should reject SSH batch with >12 items");
        let resp = result.unwrap_err();
        assert!(!resp.success);
        assert_eq!(resp.preflight_rejected, Some(true));
        assert_eq!(resp.project_is_ssh, Some(true));
        assert!(resp.max_allowed_items.is_some());
        assert!(resp.suggestion.as_ref().unwrap().contains("SSH"));
    }

    #[test]
    fn preflight_ssh_small_batch_passes() {
        let items: Vec<ContextBatchItem> = (0..6)
            .map(|_| ContextBatchItem {
                mode: ContextMode::ReadFile,
                path: Some("file.txt".to_string()),
                query: None,
                if_fingerprint: None,
                start_line: 1,
                limit: 50,
                max_depth: default_tree_max_depth(),
            })
            .collect();
        let req = make_batch_request(items, 60_000);
        let result = context::preflight_context_batch(&req, true, "test");
        assert!(
            result.is_ok(),
            "SSH batch with 6 items should pass preflight"
        );
    }

    #[test]
    fn preflight_rejects_large_read_file_limit_on_ssh() {
        let req = make_batch_request(
            vec![ContextBatchItem {
                mode: ContextMode::ReadFile,
                path: Some("big.rs".to_string()),
                query: None,
                if_fingerprint: None,
                start_line: 1,
                limit: 1200, // exceeds PREFLIGHT_MAX_READ_FILE_LIMIT (800)
                max_depth: default_tree_max_depth(),
            }],
            60_000,
        );
        let result = context::preflight_context_batch(&req, true, "test");
        assert!(
            result.is_err(),
            "Should reject SSH read_file with limit > 800"
        );
        let resp = result.unwrap_err();
        assert!(resp.error.as_ref().unwrap().contains("read_file limit"));
    }

    #[test]
    fn preflight_local_git_diff_plus_many_reads_warns() {
        // git_diff estimates 40k, 5 read_file(limit=400) each estimates 48k = 240k total
        let mut items = vec![ContextBatchItem {
            mode: ContextMode::GitDiff,
            path: None,
            query: None,
            if_fingerprint: None,
            start_line: 1,
            limit: 200,
            max_depth: default_tree_max_depth(),
        }];
        for _ in 0..5 {
            items.push(ContextBatchItem {
                mode: ContextMode::ReadFile,
                path: Some("file.rs".to_string()),
                query: None,
                if_fingerprint: None,
                start_line: 1,
                limit: 400,
                max_depth: default_tree_max_depth(),
            });
        }
        // max_total_chars = 60k but estimate ≈ 40k + 5*48k = 280k → 3x budget
        let req = make_batch_request(items, 60_000);
        let result = context::preflight_context_batch(&req, false, "test");
        assert!(
            result.is_ok(),
            "Local git_diff + many read_file should warn and rely on truncation"
        );
        let warnings = result.unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("Estimated output")),
            "expected truncation warning, got {:?}",
            warnings
        );
    }

    #[test]
    fn preflight_rejection_contains_suggestion() {
        let req = make_batch_request(
            vec![ContextBatchItem {
                mode: ContextMode::Overview,
                path: None,
                query: None,
                if_fingerprint: None,
                start_line: 1,
                limit: 200,
                max_depth: default_tree_max_depth(),
            }],
            200_000,
        );
        let result = context::preflight_context_batch(&req, true, "test");
        let resp = result.unwrap_err();
        assert!(resp.suggestion.is_some());
        assert!(!resp.suggestion.as_ref().unwrap().is_empty());
        assert_eq!(resp.preflight_rejected, Some(true));
        assert!(resp.estimated_chars.is_some());
        assert!(resp.max_allowed_chars.is_some());
        assert_eq!(resp.project_is_ssh, Some(true));
    }

    #[test]
    fn preflight_local_large_batch_warns() {
        // 25 items on local → should get a warning but still pass
        let items: Vec<ContextBatchItem> = (0..25)
            .map(|_| ContextBatchItem {
                mode: ContextMode::ReadFile,
                path: Some("f.txt".to_string()),
                query: None,
                if_fingerprint: None,
                start_line: 1,
                limit: 50,
                max_depth: default_tree_max_depth(),
            })
            .collect();
        let req = make_batch_request(items, 120_000);
        let result = context::preflight_context_batch(&req, false, "test");
        assert!(result.is_ok(), "Local 25 items should pass (not SSH)");
        let warnings = result.unwrap();
        assert!(!warnings.is_empty(), "Should have warning about batch size");
        assert!(
            warnings[0].contains("splitting")
                || warnings[0].contains("Splitting")
                || warnings[0].contains("batches")
        );
    }
}

#[cfg(test)]
mod trusted_command_tests {
    use super::trusted::*;
    use super::*;
    use crate::codex::jobs::{
        build_script_job_command, build_trusted_script_content, build_trusted_script_job_command,
        create_local_job,
    };

    fn make_local_proj() -> ProjectConfig {
        ProjectConfig {
            path: std::env::temp_dir().to_string_lossy().to_string(),
            executor: crate::projects::Executor::Local,
            host: None,
            ssh_hosts: Vec::new(),
            user: None,
            client_id: None,
            allow_patch: true,
            allow_command_requests: true,
            allow_raw_command_requests: true,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    // --- Test 1: create_trusted_raw_and_approve still works for multi-line ---

    #[test]
    fn trusted_raw_multiline_script_executes() {
        let proj = make_local_proj();
        let script = "echo hello\necho world";
        let wrapped = build_trusted_wrapper(script);
        let (code, stdout, stderr, _duration) = run_project_cmd(&proj, &wrapped, 30);
        assert_eq!(code, 0, "stderr: {}", stderr);
        assert!(stdout.contains("hello"), "stdout: {}", stdout);
        assert!(stdout.contains("world"), "stdout: {}", stdout);
    }

    #[test]
    fn trusted_raw_cwd_is_project_root() {
        let proj = make_local_proj();
        let script = "pwd";
        let wrapped = build_trusted_wrapper(script);
        let (code, stdout, stderr, _duration) = run_project_cmd(&proj, &wrapped, 30);
        assert_eq!(code, 0, "stderr: {}", stderr);
        assert!(
            stdout.contains(&proj.path),
            "stdout should contain project root, got: {}",
            stdout
        );
    }

    // --- Test 2: trusted script command does NOT produce the old broken pattern ---

    #[test]
    fn trusted_script_command_does_not_use_quoted_script() {
        // The OLD broken pattern was: set -euo pipefail; '<escaped_script>'
        // The NEW correct pattern is: bash .codex/jobs/<job_id>/script.sh
        let job_id = "test-job-123";
        let cmd = build_trusted_script_job_command(job_id);
        // Must NOT contain the old pattern of single-quoting the whole script
        assert!(
            !cmd.contains("set -euo pipefail; '"),
            "command should NOT use the old broken pattern, got: {}",
            cmd
        );
        // Must point to the script.sh file
        assert!(
            cmd.contains("script.sh"),
            "command should reference script.sh, got: {}",
            cmd
        );
        assert!(
            cmd.contains(job_id),
            "command should contain job_id, got: {}",
            cmd
        );
        assert!(
            cmd.contains("bash"),
            "command should use bash to execute the script, got: {}",
            cmd
        );
    }

    // --- Test 3: script.sh content includes shebang, set -euo pipefail, and original script ---

    #[test]
    fn trusted_script_content_has_shebang_and_safety() {
        let content = build_trusted_script_content("echo hello\necho world");
        assert!(
            content.starts_with("#!/usr/bin/env bash\n"),
            "script should start with shebang, got: {}",
            content
        );
        assert!(
            content.contains("set -euo pipefail"),
            "script should contain set -euo pipefail, got: {}",
            content
        );
        assert!(
            content.contains("echo hello"),
            "script should contain original script text, got: {}",
            content
        );
        assert!(
            content.contains("echo world"),
            "script should contain original script text, got: {}",
            content
        );
    }

    // --- Test 4: Local trusted script job actually runs and produces output ---

    #[test]
    fn local_trusted_script_job_executes_and_produces_output() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = ProjectConfig {
            path: tmp.path().to_string_lossy().to_string(),
            executor: crate::projects::Executor::Local,
            host: None,
            ssh_hosts: Vec::new(),
            user: None,
            client_id: None,
            allow_patch: true,
            allow_command_requests: true,
            allow_raw_command_requests: true,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        };
        // Create .codex/jobs dir so the job can be created
        std::fs::create_dir_all(tmp.path().join(".codex/jobs")).unwrap();

        let script_text = "echo hello_from_trusted_job";
        let result = create_local_job(
            &proj,
            "test-project",
            "goal-test",
            "", // placeholder for trusted_script_text mode
            None,
            Some("trusted_script".to_string()),
            None,
            None,
            Some("test reason".to_string()),
            60,
            Some(script_text),
        );
        assert!(result.is_ok(), "job creation should succeed: {:?}", result);
        let job = result.unwrap();
        assert_eq!(job.kind, Some("trusted_script".to_string()));

        // Wait for the job to finish
        let mut attempts = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let dir = proj.root().join(".codex/jobs").join(&job.job_id);
            let status =
                std::fs::read_to_string(dir.join("status")).unwrap_or_else(|_| "running".into());
            if status != "running" || attempts > 50 {
                break;
            }
            attempts += 1;
        }

        // Check that the job produced output
        let dir = proj.root().join(".codex/jobs").join(&job.job_id);
        let stdout = std::fs::read_to_string(dir.join("stdout.log")).unwrap_or_default();
        assert!(
            stdout.contains("hello_from_trusted_job"),
            "stdout should contain script output, got: {}",
            stdout
        );

        // Verify script.sh exists and has proper content
        let script_content = std::fs::read_to_string(dir.join("script.sh")).unwrap_or_default();
        assert!(
            script_content.contains("#!/usr/bin/env bash"),
            "script.sh should have shebang"
        );
        assert!(
            script_content.contains("set -euo pipefail"),
            "script.sh should have set -euo pipefail"
        );
        assert!(
            script_content.contains("echo hello_from_trusted_job"),
            "script.sh should contain original script"
        );

        // Verify command references script.sh
        assert!(
            job.command.contains("script.sh"),
            "job command should reference script.sh, got: {}",
            job.command
        );
    }

    // --- Test 5: script_text without trusted=true is rejected ---
    // (This is tested at the handler level, but we test the validation function)

    #[test]
    fn trusted_raw_stdout_is_truncated() {
        let result = build_trusted_result(
            0,
            100,
            "/tmp",
            &"a".repeat(100_000),
            &"b".repeat(50_000),
            "summary",
            None,
            false,
        );
        assert!(result.stdout_truncated);
        assert!(result.stderr_truncated);
    }

    // --- Test 6: Denylist / secret / background checks still work ---

    #[test]
    fn dangerous_command_blocked_by_denylist() {
        assert!(check_denylist("rm -rf /").is_some());
        assert!(check_denylist("mkfs.ext4 /dev/sda1").is_some());
        assert!(check_denylist("systemctl restart nginx").is_some());
        assert!(check_denylist("git push origin main").is_some());
        assert!(check_denylist("docker system prune -af").is_some());
    }

    #[test]
    fn git_push_blocked_by_denylist() {
        assert!(check_denylist("git push").is_some());
        assert!(check_denylist("git push origin main").is_some());
        assert!(check_denylist("git push --force").is_some());
    }

    #[test]
    fn env_content_read_blocked() {
        assert!(check_secret_read("cat .env").is_some());
        assert!(check_secret_read("cat id_rsa").is_some());
        assert!(check_secret_read("cat server.pem").is_some());
    }

    #[test]
    fn nohup_disown_background_ampersand_rejected() {
        assert!(check_background_escape("nohup python train.py").is_some());
        assert!(check_background_escape("disown %1").is_some());
        assert!(check_background_escape("sleep 100 &").is_some());
    }

    // --- Test 7: Job create response is lightweight ---

    #[test]
    fn job_create_response_is_lightweight() {
        let response = types::JobOpResponse {
            success: true,
            op: "create".to_string(),
            job_id: Some("job-1".to_string()),
            job_ids: vec!["job-1".to_string()],
            job: None,
            jobs: Vec::new(),
            stdout_tail: None,
            stderr_tail: None,
            summary_markdown: None,
            error: None,
            log_total_lines: None,
            next_cursor: None,
            metadata_only: None,
            logs_included: None,
            warnings: Vec::new(),
            recommended_next_action: None,
            action_budget_hint: None,
        };
        assert_eq!(response.stdout_tail, None);
        assert_eq!(response.stderr_tail, None);
        assert_eq!(response.summary_markdown, None);
    }

    // --- Test 8: OpenAPI schema still has trusted fields ---

    #[test]
    fn openapi_schema_contains_trusted_descriptions() {
        let spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();

        let op_enum: Vec<String> = spec["components"]["schemas"]["CommandRequestOpRequest"]
            ["properties"]["op"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(
            op_enum.contains(&"create_trusted_raw".to_string()),
            "op enum should contain 'create_trusted_raw', got: {:?}",
            op_enum
        );
        assert!(
            op_enum.contains(&"create_trusted_raw_and_approve".to_string()),
            "op enum should contain 'create_trusted_raw_and_approve', got: {:?}",
            op_enum
        );

        let cr_props = &spec["components"]["schemas"]["CommandRequestOpRequest"]["properties"];
        assert!(!cr_props["script_text"].is_null());
        assert!(!cr_props["timeout_secs"].is_null());
        assert!(!cr_props["response_mode"].is_null());

        let job_props = &spec["components"]["schemas"]["JobOpRequest"]["properties"];
        assert!(!job_props["script_text"].is_null());
        assert!(!job_props["trusted"].is_null());

        let resp_props = &spec["components"]["schemas"]["CommandRequestOpResponse"]["properties"];
        assert!(!resp_props["trusted_result"].is_null());
    }

    // --- Test 9: Old create_raw and script_path behavior unchanged ---

    #[test]
    fn old_create_raw_behavior_unchanged() {
        assert!(validate_raw_command_text("echo ok").is_ok());
        assert!(validate_raw_command_text("git status --short").is_ok());
        assert!(validate_raw_command_text("git push").is_err());
        assert!(validate_raw_command_text("sudo rm -rf /").is_err());
        assert!(validate_raw_command_text("echo one\necho two").is_err());
    }

    #[test]
    fn old_run_job_op_script_path_behavior_unchanged() {
        let result = build_script_job_command("scripts/test.sh", &[]);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert!(cmd.contains("scripts/test.sh"));
        assert!(cmd.contains("bash"));
    }
}
