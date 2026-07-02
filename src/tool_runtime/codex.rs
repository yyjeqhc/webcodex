#![allow(dead_code)]

use serde_json::Value;

use super::helpers::{read_json, shell_escape_simple};
use super::types::ToolResult;
use super::ToolRuntime;
use crate::config::CodexConfig;

pub(crate) fn build_codex_command(
    codex: &CodexConfig,
    prompt: &str,
    approval_mode: Option<&str>,
    extra_args: Option<Vec<String>>,
) -> Result<String, String> {
    validate_cli_arg(&codex.bin, "CODEX_BIN")?;
    // Resolve the effective approval mode. An explicit request value wins over
    // the config default. Empty/blank, none, off, and disabled all mean "do not
    // pass --approval-mode" so the runtime stays compatible with Codex CLI
    // builds that do not support the flag.
    let resolved_approval_mode = match approval_mode {
        Some(v) => v.trim().to_string(),
        None => codex.approval_mode.clone(),
    };
    if resolved_approval_mode.contains('\0') {
        return Err("approval_mode cannot contain NUL bytes".to_string());
    }
    let approval_disabled = is_approval_mode_disabled(&resolved_approval_mode);
    let extra_args = extra_args.unwrap_or_default();
    if extra_args.len() > 32 {
        return Err("extra_args may contain at most 32 arguments".to_string());
    }
    for (idx, arg) in extra_args.iter().enumerate() {
        validate_cli_arg(arg, &format!("extra_args[{}]", idx))?;
        if !codex.is_extra_arg_allowed(arg) {
            return Err(format!(
                "extra_args[{}] '{}' is not in CODEX_ALLOWED_EXTRA_ARGS allowlist",
                idx, arg
            ));
        }
    }
    let mut parts = vec![shell_escape_simple(&codex.bin)];
    if !approval_disabled {
        parts.push("--approval-mode".to_string());
        parts.push(shell_escape_simple(&resolved_approval_mode));
    }
    for arg in &extra_args {
        parts.push(shell_escape_simple(arg));
    }
    parts.push(shell_escape_simple(prompt));
    Ok(parts.join(" "))
}

/// Returns true when an approval-mode value means "do not pass --approval-mode".
/// Empty/whitespace, `none`, `off`, and `disabled` (case-insensitive) disable
/// the flag so the runtime works with Codex CLI builds that lack it.
pub(crate) fn is_approval_mode_disabled(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    v.is_empty() || v == "none" || v == "off" || v == "disabled"
}

pub(crate) fn validate_cli_arg(value: &str, field: &str) -> Result<(), String> {
    if value.contains('\0') {
        return Err(format!("{} cannot contain NUL bytes", field));
    }
    if value.trim().is_empty() {
        return Err(format!("{} cannot be empty", field));
    }
    Ok(())
}

impl ToolRuntime {
    pub(crate) async fn run_codex(
        &self,
        project: String,
        prompt: String,
        approval_mode: Option<String>,
        timeout_secs: Option<i64>,
        cwd: Option<String>,
        extra_args: Option<Vec<String>>,
    ) -> ToolResult {
        if prompt.trim().is_empty() {
            return ToolResult::err("prompt cannot be empty");
        }
        if prompt.contains('\0') {
            return ToolResult::err("prompt cannot contain NUL bytes");
        }
        if prompt.len() > self.codex.max_prompt_bytes {
            return ToolResult::err(format!(
                "prompt is too large; maximum is {} bytes",
                self.codex.max_prompt_bytes
            ));
        }
        if let Some(mode) = approval_mode.as_deref() {
            if mode.contains('\0') {
                return ToolResult::err("approval_mode cannot contain NUL bytes");
            }
        }
        let project_for_output = project.clone();
        let command =
            match build_codex_command(&self.codex, &prompt, approval_mode.as_deref(), extra_args) {
                Ok(command) => command,
                Err(e) => return ToolResult::err(e),
            };
        let result = self
            .run_job(
                project,
                command,
                timeout_secs.or(Some(self.codex.default_timeout_secs)),
                cwd,
            )
            .await;
        if !result.success {
            return result;
        }
        let mut output = result.output;
        if let Some(obj) = output.as_object_mut() {
            obj.insert("kind".to_string(), Value::String("codex".to_string()));
            obj.insert("project".to_string(), Value::String(project_for_output));
            obj.insert(
                "status_endpoint".to_string(),
                Value::String("/api/jobs/status".to_string()),
            );
            obj.insert(
                "log_endpoint".to_string(),
                Value::String("/api/jobs/log".to_string()),
            );
            if let Some(job_id) = obj.get("job_id").and_then(Value::as_str) {
                if let Some(record) = self.local_jobs.lock().await.get(job_id).cloned() {
                    let mut meta = read_json(record.dir.join("metadata.json"));
                    if let Some(meta_obj) = meta.as_object_mut() {
                        meta_obj.insert("kind".to_string(), Value::String("codex".to_string()));
                    }
                    let _ = std::fs::write(
                        record.dir.join("metadata.json"),
                        serde_json::to_string_pretty(&meta).unwrap_or_default(),
                    );
                }
            }
        }
        ToolResult::ok(output)
    }
}
