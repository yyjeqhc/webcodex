use serde_json::{json, Value};
use std::time::Duration;

use super::types::ToolResult;
use super::ToolRuntime;
use crate::shell_protocol::ShellRunRequest;

pub(crate) fn parse_changed_files_from_patch(patch: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            if let Some(b_pos) = line.rfind(" b/") {
                let file = &line[b_pos + 3..];
                if !files.iter().any(|f: &String| f == file) {
                    files.push(file.to_string());
                }
            }
            continue;
        }
        for prefix in ["+++ b/", "--- a/"] {
            if let Some(file) = line.strip_prefix(prefix) {
                if file != "/dev/null" && !files.iter().any(|f: &String| f == file) {
                    files.push(file.to_string());
                }
            }
        }
    }
    files
}

pub(crate) fn validate_patch_file_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("patch path cannot be empty".to_string());
    }
    if path.starts_with('/') {
        return Err(format!("Absolute paths are not allowed: {}", path));
    }
    if path.contains("..") {
        return Err(format!("Path traversal (..) is not allowed: {}", path));
    }
    let sensitive = [".env", ".env.local", "secret.pem", "id_rsa", ".git/config"];
    if sensitive.iter().any(|s| path.contains(s)) {
        return Err(format!("Cannot modify sensitive path: {}", path));
    }
    Ok(())
}

/// Maximum accepted patch size for `validate_patch`, in bytes. Kept
/// conservative to bound memory use and the agent stdin payload size. The
/// patch is sent to the agent as stdin for `git apply`; larger patches should
/// be split.
/// This is a preflight-only bound; it does not affect `apply_patch`.
pub(crate) const MAX_VALIDATE_PATCH_BYTES: usize = 256 * 1024; // 256 KiB

/// Hard-reject patch file paths that would escape the project boundary during
/// `validate_patch` preflight. Unlike `validate_patch_file_path` (used by the
/// real `apply_patch`), this does **not** reject sensitive filenames — those
/// are reported as `warnings` instead so the caller can still see the dry-run
/// result. Only absolute paths, `..` traversal, and NUL bytes are hard
/// rejects, ensuring the preflight never escapes the project root.
pub(crate) fn validate_preflight_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("patch path cannot be empty".to_string());
    }
    if path.starts_with('/') {
        return Err(format!("Absolute paths are not allowed: {}", path));
    }
    if path.contains("..") {
        return Err(format!("Path traversal (..) is not allowed: {}", path));
    }
    if path.contains('\0') {
        return Err("NUL byte in patch path is not allowed".to_string());
    }
    Ok(())
}

/// Sensitive path components that `validate_patch` should warn about (but not
/// hard-reject). The preflight still runs; the caller sees the warning and can
/// decide whether to proceed with `apply_patch`. Matching is case-insensitive
/// substring so it catches `foo/.env`, `agent.toml.bak`, `target/debug`, etc.
pub(crate) fn sensitive_path_warnings(path: &str) -> Vec<String> {
    let lower = path.to_lowercase();
    let sensitive = [
        "agent.toml",
        "webcodex.env",
        ".env",
        "projects.d",
        ".git",
        "target",
        "node_modules",
    ];
    let mut warnings = Vec::new();
    for name in sensitive {
        if lower.contains(name) {
            warnings.push(format!(
                "patch touches sensitive path component '{}': {}",
                name, path
            ));
        }
    }
    warnings
}

impl ToolRuntime {
    pub(crate) async fn apply_patch(&self, project: String, patch: String) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.allow_patch() {
            return ToolResult::err("Patch is not allowed for this project");
        }
        if patch.is_empty() {
            return ToolResult::err("Patch cannot be empty");
        }
        if patch.contains('\0') {
            return ToolResult::err("Patch contains NUL byte");
        }
        let changed = parse_changed_files_from_patch(&patch);
        if changed.is_empty() {
            return ToolResult::err("Patch does not declare any changed files");
        }
        for file in &changed {
            if let Err(e) = validate_patch_file_path(file) {
                return ToolResult::err(e);
            }
        }
        // ---- Agent routing ----
        // apply_patch mutates the worktree through the owning agent only. The
        // server never reads or writes the agent project filesystem directly,
        // and server-configured legacy projects are not a supported runtime
        // surface for this tool (consistent with `validate_patch`). The patch
        // payload always travels over `ShellRunRequest.stdin`; the command
        // string is a fixed `git apply` invocation and never contains patch
        // content, `echo <patch>`, heredocs, or a `cd` prefix — the working
        // directory is supplied via the shell request `cwd` field.
        if !proj.is_agent() {
            return ToolResult::err(
                "apply_patch requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let (check_req_id, check_rx) = match self
            .shell_clients
            .enqueue_run(
                ShellRunRequest {
                    client_id: client_id.clone(),
                    cwd: Some(proj.path.clone()),
                    command: "git apply --check - && echo OK".to_string(),
                    stdin: Some(patch.clone()),
                    timeout_secs: 60,
                    wait_timeout_secs: 62,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(e),
        };
        let check_result = tokio::time::timeout(Duration::from_secs(64), check_rx).await;
        match check_result {
            Ok(Ok(resp)) if resp.exit_code != Some(0) => {
                return ToolResult::ok(json!({
                    "success": false,
                    "changed_files": changed,
                    "stdout": resp.stdout,
                    "stderr": resp.stderr,
                    "error": "git apply --check failed",
                }));
            }
            Err(_) => {
                self.shell_clients.cancel_request(&check_req_id).await;
                return ToolResult::err("timed out during patch validation");
            }
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&check_req_id).await;
                return ToolResult::err("patch validation request dropped");
            }
            _ => {}
        }
        let (apply_req_id, apply_rx) = match self
            .shell_clients
            .enqueue_run(
                ShellRunRequest {
                    client_id,
                    cwd: Some(proj.path.clone()),
                    command: "git apply -".to_string(),
                    stdin: Some(patch.clone()),
                    timeout_secs: 60,
                    wait_timeout_secs: 62,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(e),
        };
        match tokio::time::timeout(Duration::from_secs(64), apply_rx).await {
            Ok(Ok(resp)) => {
                let success = resp.exit_code == Some(0);
                ToolResult::ok(json!({
                    "success": success,
                    "changed_files": changed,
                    "stdout": resp.stdout,
                    "stderr": resp.stderr,
                }))
            }
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&apply_req_id).await;
                ToolResult::err("apply request dropped")
            }
            Err(_) => {
                self.shell_clients.cancel_request(&apply_req_id).await;
                ToolResult::err("timed out applying patch")
            }
        }
    }

    pub(crate) async fn apply_patch_checked(
        &self,
        project: String,
        patch: String,
        deny_sensitive_paths: Option<bool>,
    ) -> ToolResult {
        let deny = deny_sensitive_paths.unwrap_or(true);
        let validate = self
            .validate_patch(project.clone(), patch.clone(), Some(deny))
            .await;
        if !validate.success {
            return validate;
        }
        let can_apply = validate.output["can_apply"].as_bool().unwrap_or(false);
        if !can_apply {
            return ToolResult::ok(json!({
                "applied": false,
                "validate": validate.output,
                "apply": Value::Null,
                "diff_summary": Value::Null,
            }));
        }
        let apply = self.apply_patch(project.clone(), patch).await;
        if !apply.success {
            return ToolResult::ok(json!({
                "applied": false,
                "validate": validate.output,
                "apply": apply,
                "diff_summary": Value::Null,
            }));
        }
        let diff_summary = self.git_diff_summary(project).await;
        ToolResult::ok(json!({
            "applied": apply.output["success"].as_bool().unwrap_or(false),
            "validate": validate.output,
            "apply": apply.output,
            "diff_summary": diff_summary.output,
        }))
    }

    pub(crate) async fn validate_patch(
        &self,
        project: String,
        patch: String,
        deny_sensitive_paths: Option<bool>,
    ) -> ToolResult {
        // ---- Input validation (before any project resolution) ----
        if patch.is_empty() {
            return ToolResult::err("Patch cannot be empty");
        }
        if patch.contains('\0') {
            return ToolResult::err("Patch contains NUL byte");
        }
        if patch.len() > MAX_VALIDATE_PATCH_BYTES {
            return ToolResult::err(format!(
                "Patch too large ({} bytes); maximum is {} bytes",
                patch.len(),
                MAX_VALIDATE_PATCH_BYTES
            ));
        }

        // ---- Project resolution (agent-registered only) ----
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.allow_patch() {
            return ToolResult::err("Patch is not allowed for this project");
        }
        // ---- Patch path analysis ----
        let affected = parse_changed_files_from_patch(&patch);
        if affected.is_empty() {
            return ToolResult::err("Patch does not declare any changed files");
        }
        // Hard-reject paths that escape the project boundary. Collect warnings
        // for sensitive filenames. Callers can request a hard policy block so
        // full-auto loops do not accidentally ignore sensitive-path warnings.
        let mut warnings: Vec<String> = Vec::new();
        for file in &affected {
            if let Err(e) = validate_preflight_path(file) {
                return ToolResult::err(e);
            }
            warnings.extend(sensitive_path_warnings(file));
        }
        warnings.sort();
        warnings.dedup();
        let deny_sensitive = deny_sensitive_paths.unwrap_or(false);
        if deny_sensitive && !warnings.is_empty() {
            return ToolResult::ok(json!({
                "can_apply": false,
                "policy_blocked": true,
                "affected_files": affected,
                "stat": Value::Null,
                "stdout": Value::Null,
                "stderr": "sensitive path policy blocked patch preflight",
                "warnings": warnings,
            }));
        }

        // ---- Agent routing ----
        // validate_patch must run through the owning agent; the server never
        // reads the agent project filesystem directly, and server-configured
        // legacy projects are not a supported runtime surface for this tool.
        if !proj.is_agent() {
            return ToolResult::err(
                "validate_patch requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        // ---- 1) git apply --check (read-only applicability test) ----
        let (check_req_id, check_rx) = match self
            .shell_clients
            .enqueue_run(
                ShellRunRequest {
                    client_id: client_id.clone(),
                    cwd: Some(proj.path.clone()),
                    command: "git apply --check -".to_string(),
                    stdin: Some(patch.clone()),
                    timeout_secs: 60,
                    wait_timeout_secs: 62,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(e),
        };
        let check_resp = match tokio::time::timeout(Duration::from_secs(64), check_rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&check_req_id).await;
                return ToolResult::err("patch validation request dropped");
            }
            Err(_) => {
                self.shell_clients.cancel_request(&check_req_id).await;
                return ToolResult::err("timed out during patch validation");
            }
        };
        let can_apply = check_resp.exit_code == Some(0);
        let check_stdout = check_resp.stdout.clone();
        let check_stderr = check_resp.stderr.clone();

        // ---- 2) git apply --stat (read-only summary) ----
        // `--stat` only parses the diff and prints a summary; it does not
        // check applicability and does not write files. It works regardless
        // of `can_apply`, so the caller always gets a summary.
        let (stat_req_id, stat_rx) = match self
            .shell_clients
            .enqueue_run(
                ShellRunRequest {
                    client_id,
                    cwd: Some(proj.path.clone()),
                    command: "git apply --stat -".to_string(),
                    stdin: Some(patch.clone()),
                    timeout_secs: 60,
                    wait_timeout_secs: 62,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(e),
        };
        let stat_resp = match tokio::time::timeout(Duration::from_secs(64), stat_rx).await {
            Ok(Ok(resp)) => resp.stdout,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&stat_req_id).await;
                None
            }
            Err(_) => {
                self.shell_clients.cancel_request(&stat_req_id).await;
                None
            }
        };

        // ---- Structured result ----
        // ToolResult.success reflects whether the *validation* ran cleanly;
        // `can_apply` reports whether the patch would apply. A non-applicable
        // patch is a normal preflight outcome (success=true, can_apply=false),
        // not a tool error, so the agent loop can read it and regenerate.
        ToolResult::ok(json!({
            "can_apply": can_apply,
            "policy_blocked": false,
            "affected_files": affected,
            "stat": stat_resp,
            "stdout": check_stdout,
            "stderr": check_stderr,
            "warnings": warnings,
        }))
    }
}
