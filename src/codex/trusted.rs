//! Trusted raw command execution: safety guardrails for multi-line shell scripts.
//!
//! This module provides the validation and safety-check helpers used by the
//! trusted-script job path (see `codex::jobs::create_local_job` with
//! `trusted_script_text`). Execution writes the script to
//! `.codex/jobs/<job_id>/script.sh` and runs `bash <script.sh>`; this module
//! only validates script content and blocks dangerous patterns.
//!
//! Exports:
//! - `validate_trusted_script`: validates multi-line script content
//! - `check_denylist`: checks for blocked dangerous commands
//! - `check_secret_read`: checks for attempts to read sensitive files
//! - `check_background_escape`: checks for nohup/&/disown

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub(super) const TRUSTED_MAX_SCRIPT_LEN: usize = 32_000;

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate that a trusted script is non-empty, within length limits, and
/// doesn't contain NUL bytes.
pub fn validate_trusted_script(script: &str) -> Result<(), String> {
    let trimmed = script.trim();
    if trimmed.is_empty() {
        return Err("trusted script cannot be empty".to_string());
    }
    if script.contains('\0') {
        return Err("trusted script cannot contain NUL bytes".to_string());
    }
    if script.len() > TRUSTED_MAX_SCRIPT_LEN {
        return Err(format!(
            "trusted script is too long; maximum is {} bytes, got {}",
            TRUSTED_MAX_SCRIPT_LEN,
            script.len()
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Denylist check
// ---------------------------------------------------------------------------

/// Dangerous command patterns that are always blocked in trusted mode.
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    "rm -rf ~/*",
    "mkfs",
    "dd if=",
    "dd of=/dev",
    ":(){ :|:& };:",
    "chmod -R 777 /",
    "chown -R ",
    "systemctl",
    "service nginx",
    "service apache",
    "docker system prune",
    "git push",
    "git fetch",
    // Prevent modifying system daemons
    "nginx -s",
    "docker rm",
    "docker rmi",
];

/// Check if script contains dangerous patterns. Returns Some(error_message) if blocked.
pub fn check_denylist(script: &str) -> Option<String> {
    let lower = script.to_ascii_lowercase();
    for pattern in DANGEROUS_PATTERNS {
        if lower.contains(pattern) {
            return Some(format!(
                "blocked by denylist: command contains dangerous pattern '{}'",
                pattern
            ));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Secret read check
// ---------------------------------------------------------------------------

/// Patterns that indicate reading sensitive file content.
const SECRET_READ_PATTERNS: &[&str] = &[
    ".env",
    ".pem",
    "id_rsa",
    "id_ed25519",
    ".key",
    "secrets.",
    "secrets/",
    "token",
];

/// File extensions that are always sensitive.
const SENSITIVE_EXTENSIONS: &[&str] = &[".pem", ".key"];

/// Check if a script attempts to read sensitive file content.
/// Returns Some(error_message) if blocked.
pub fn check_secret_read(script: &str) -> Option<String> {
    let lower = script.to_ascii_lowercase();
    // Look for cat/grep/head/tail/less/more followed by sensitive file references
    let read_commands = [
        "cat ",
        "grep ",
        "head ",
        "tail ",
        "less ",
        "more ",
        "jq ",
        "python -c",
        "python3 -c",
    ];
    for cmd in &read_commands {
        for line in lower.lines() {
            if line.contains(cmd) {
                // Check if the line references sensitive files
                for secret in SECRET_READ_PATTERNS {
                    if line.contains(secret) {
                        // Allow ls to see filenames but not content
                        if line.trim().starts_with("ls ") || line.trim().starts_with("find ") {
                            continue;
                        }
                        return Some(format!(
                            "blocked: appears to read sensitive file content (pattern '{}'). Use ls to see filenames but not content.",
                            secret
                        ));
                    }
                }
                // Check sensitive extensions
                for ext in SENSITIVE_EXTENSIONS {
                    if line.contains(ext) {
                        if line.trim().starts_with("ls ") || line.trim().starts_with("find ") {
                            continue;
                        }
                        return Some(format!(
                            "blocked: appears to read sensitive file content (extension '{}'). Use ls to see filenames but not content.",
                            ext
                        ));
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Background escape check
// ---------------------------------------------------------------------------

/// Check if a script tries to escape job management via nohup/disown/background &.
/// Returns Some(error_message) if blocked.
pub fn check_background_escape(script: &str) -> Option<String> {
    for line in script.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        // nohup anywhere in line
        if lower.contains("nohup") {
            return Some(
                "blocked: 'nohup' is not allowed; use runJobOp for long-running tasks".to_string(),
            );
        }
        // disown anywhere in line
        if lower.contains("disown") {
            return Some(
                "blocked: 'disown' is not allowed; use runJobOp for long-running tasks".to_string(),
            );
        }
        // trailing & that is not && or part of a valid construct
        // Simple heuristic: if line ends with & and not &&, and not inside a comment
        if !trimmed.starts_with('#') && trimmed.ends_with('&') && !trimmed.ends_with("&&") {
            return Some(
                "blocked: background '&' is not allowed; use runJobOp for async execution"
                    .to_string(),
            );
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_trusted_script_accepts_normal() {
        assert!(validate_trusted_script("echo hello").is_ok());
        assert!(validate_trusted_script("line1\nline2\nline3").is_ok());
    }

    #[test]
    fn validate_trusted_script_rejects_empty() {
        assert!(validate_trusted_script("").is_err());
        assert!(validate_trusted_script("   ").is_err());
    }

    #[test]
    fn validate_trusted_script_rejects_nul() {
        assert!(validate_trusted_script("echo\0hello").is_err());
    }

    #[test]
    fn validate_trusted_script_rejects_too_long() {
        let long = "a".repeat(TRUSTED_MAX_SCRIPT_LEN + 1);
        assert!(validate_trusted_script(&long).is_err());
    }

    #[test]
    fn check_denylist_blocks_dangerous() {
        assert!(check_denylist("rm -rf /").is_some());
        assert!(check_denylist("rm -rf /*").is_some());
        assert!(check_denylist("mkfs.ext4 /dev/sda1").is_some());
        assert!(check_denylist("dd if=/dev/zero of=/dev/sda").is_some());
        assert!(check_denylist("systemctl restart nginx").is_some());
        assert!(check_denylist("git push origin main").is_some());
        assert!(check_denylist("docker system prune -a").is_some());
    }

    #[test]
    fn check_denylist_allows_safe() {
        assert!(check_denylist("echo hello").is_none());
        assert!(check_denylist("cargo test").is_none());
        assert!(check_denylist("rm -rf target/").is_none());
        assert!(check_denylist("git status").is_none());
        assert!(check_denylist("grep -RIn foo src/").is_none());
    }

    #[test]
    fn check_secret_read_blocks_env() {
        assert!(check_secret_read("cat .env").is_some());
        assert!(check_secret_read("cat config/.env.local").is_some());
        assert!(check_secret_read("grep secret .env").is_some());
        assert!(check_secret_read("cat id_rsa").is_some());
        assert!(check_secret_read("cat server.pem").is_some());
        assert!(check_secret_read("head -5 secrets.json").is_some());
    }

    #[test]
    fn check_secret_read_allows_ls() {
        assert!(check_secret_read("ls .env").is_none());
        assert!(check_secret_read("find . -name '*.pem'").is_none());
    }

    #[test]
    fn check_secret_read_allows_normal() {
        assert!(check_secret_read("cat src/main.rs").is_none());
        assert!(check_secret_read("grep foo README.md").is_none());
    }

    #[test]
    fn check_background_escape_blocks_nohup() {
        assert!(check_background_escape("nohup python train.py").is_some());
    }

    #[test]
    fn check_background_escape_blocks_disown() {
        assert!(check_background_escape("disown %1").is_some());
    }

    #[test]
    fn check_background_escape_blocks_bg_ampersand() {
        assert!(check_background_escape("sleep 100 &").is_some());
    }

    #[test]
    fn check_background_escape_allows_logical_and() {
        assert!(check_background_escape("cargo fmt && cargo test").is_none());
    }

    #[test]
    fn check_background_escape_allows_normal() {
        assert!(check_background_escape("echo hello").is_none());
        assert!(check_background_escape("grep -RIn foo src/").is_none());
    }
}
