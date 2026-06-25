use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub addr: String,
    pub data_dir: PathBuf,
    pub token: Option<String>,
    pub enable_ssh: bool,
    pub max_text_size: usize,
    pub max_file_size: usize,
    pub codex: CodexConfig,
}

/// Codex CLI execution configuration, sourced from `CODEX_*` env vars.
#[derive(Debug, Clone)]
pub struct CodexConfig {
    /// Path/name of the Codex CLI binary. Default `codex`.
    pub bin: String,
    /// Approval mode passed via `--approval-mode`. Default `full-auto`.
    pub approval_mode: String,
    /// Default job timeout in seconds. Default `3600`.
    pub default_timeout_secs: i64,
    /// Maximum prompt size in bytes. Default `100000`.
    pub max_prompt_bytes: usize,
    /// Allowlist of accepted `extra_args`. Empty means no extra args allowed.
    pub allowed_extra_args: Vec<String>,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            bin: "codex".to_string(),
            approval_mode: "full-auto".to_string(),
            default_timeout_secs: 3600,
            max_prompt_bytes: 100_000,
            allowed_extra_args: Vec::new(),
        }
    }
}

impl CodexConfig {
    pub fn from_env() -> Self {
        let bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string());
        let approval_mode =
            std::env::var("CODEX_APPROVAL_MODE").unwrap_or_else(|_| "full-auto".to_string());
        let default_timeout_secs = std::env::var("CODEX_DEFAULT_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(3600);
        let max_prompt_bytes = std::env::var("CODEX_MAX_PROMPT_BYTES")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(100_000);
        let allowed_extra_args = std::env::var("CODEX_ALLOWED_EXTRA_ARGS")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            bin,
            approval_mode,
            default_timeout_secs,
            max_prompt_bytes,
            allowed_extra_args,
        }
    }

    /// Returns true if `arg` is in the configured allowlist.
    pub fn is_extra_arg_allowed(&self, arg: &str) -> bool {
        self.allowed_extra_args.iter().any(|allowed| allowed == arg)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EnvFileLoad {
    pub(crate) path: PathBuf,
    pub(crate) loaded_count: usize,
}

pub(crate) fn parse_env_file_line(line: &str) -> Option<Result<(String, String), String>> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line).trim();
    let Some((key, value)) = line.split_once('=') else {
        return Some(Err("missing '='".to_string()));
    };
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Some(Err(format!("invalid env key '{}'", key)));
    }
    let value = value.trim();
    let value = if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    };
    Some(Ok((key.to_string(), value)))
}

fn load_env_file(path: &Path) -> Result<EnvFileLoad, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read env file {}: {}", path.display(), e))?;
    let mut loaded_count = 0;
    for (idx, line) in content.lines().enumerate() {
        let Some(parsed) = parse_env_file_line(line) else {
            continue;
        };
        let (key, value) = parsed.map_err(|e| {
            format!(
                "failed to parse env file {} line {}: {}",
                path.display(),
                idx + 1,
                e
            )
        })?;
        if std::env::var_os(&key).is_none() {
            std::env::set_var(&key, value);
            loaded_count += 1;
        }
    }
    Ok(EnvFileLoad {
        path: path.to_path_buf(),
        loaded_count,
    })
}

pub(crate) fn load_startup_env_files() -> Result<Vec<EnvFileLoad>, String> {
    if let Ok(path) = std::env::var("DROP_ENV_FILE") {
        return Ok(vec![load_env_file(Path::new(&path))?]);
    }
    let candidates = [
        PathBuf::from("./private-drop.env"),
        PathBuf::from("/opt/private-drop/private-drop.env"),
        PathBuf::from("/etc/private-drop/private-drop.env"),
    ];
    let mut loaded = Vec::new();
    for path in candidates {
        if path.exists() {
            loaded.push(load_env_file(&path)?);
        }
    }
    Ok(loaded)
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            addr: std::env::var("DROP_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            data_dir: std::env::var("DROP_DATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./data")),
            token: std::env::var("DROP_TOKEN").ok(),
            enable_ssh: env_flag("DROP_ENABLE_SSH").unwrap_or(false),
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::from_env(),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("drop.db")
    }

    pub fn uploads_dir(&self) -> PathBuf {
        self.data_dir.join("uploads")
    }

    pub fn is_auth_enabled(&self) -> bool {
        self.token.is_some()
    }

    pub fn is_ssh_enabled(&self) -> bool {
        self.enable_ssh
    }

    pub fn validate_token(&self, token: &str) -> bool {
        self.token.as_ref().map(|t| t == token).unwrap_or(false)
    }
}

fn env_flag(key: &str) -> Option<bool> {
    let value = std::env::var(key).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_config_defaults() {
        let cfg = CodexConfig::default();
        assert_eq!(cfg.bin, "codex");
        assert_eq!(cfg.approval_mode, "full-auto");
        assert_eq!(cfg.default_timeout_secs, 3600);
        assert_eq!(cfg.max_prompt_bytes, 100_000);
        assert!(cfg.allowed_extra_args.is_empty());
    }

    #[test]
    fn codex_config_from_env_uses_defaults_when_unset() {
        // Clear CODEX_* env vars so we get deterministic defaults.
        std::env::remove_var("CODEX_BIN");
        std::env::remove_var("CODEX_APPROVAL_MODE");
        std::env::remove_var("CODEX_DEFAULT_TIMEOUT_SECS");
        std::env::remove_var("CODEX_MAX_PROMPT_BYTES");
        std::env::remove_var("CODEX_ALLOWED_EXTRA_ARGS");

        let cfg = CodexConfig::from_env();
        assert_eq!(cfg.bin, "codex");
        assert_eq!(cfg.approval_mode, "full-auto");
        assert_eq!(cfg.default_timeout_secs, 3600);
        assert_eq!(cfg.max_prompt_bytes, 100_000);
        assert!(cfg.allowed_extra_args.is_empty());
    }

    #[test]
    fn codex_config_from_env_parses_overrides() {
        std::env::set_var("CODEX_BIN", "/usr/local/bin/codex");
        std::env::set_var("CODEX_APPROVAL_MODE", "suggest");
        std::env::set_var("CODEX_DEFAULT_TIMEOUT_SECS", "600");
        std::env::set_var("CODEX_MAX_PROMPT_BYTES", "2048");
        std::env::set_var("CODEX_ALLOWED_EXTRA_ARGS", "--verbose, --json, --no-color");

        let cfg = CodexConfig::from_env();
        assert_eq!(cfg.bin, "/usr/local/bin/codex");
        assert_eq!(cfg.approval_mode, "suggest");
        assert_eq!(cfg.default_timeout_secs, 600);
        assert_eq!(cfg.max_prompt_bytes, 2048);
        assert_eq!(
            cfg.allowed_extra_args,
            vec!["--verbose", "--json", "--no-color"]
        );
        assert!(cfg.is_extra_arg_allowed("--verbose"));
        assert!(cfg.is_extra_arg_allowed("--json"));
        assert!(!cfg.is_extra_arg_allowed("--danger"));

        // Restore defaults.
        std::env::remove_var("CODEX_BIN");
        std::env::remove_var("CODEX_APPROVAL_MODE");
        std::env::remove_var("CODEX_DEFAULT_TIMEOUT_SECS");
        std::env::remove_var("CODEX_MAX_PROMPT_BYTES");
        std::env::remove_var("CODEX_ALLOWED_EXTRA_ARGS");
    }

    #[test]
    fn codex_config_from_env_ignores_invalid_numeric_values() {
        std::env::set_var("CODEX_DEFAULT_TIMEOUT_SECS", "not-a-number");
        std::env::set_var("CODEX_MAX_PROMPT_BYTES", "-5");

        let cfg = CodexConfig::from_env();
        assert_eq!(cfg.default_timeout_secs, 3600);
        assert_eq!(cfg.max_prompt_bytes, 100_000);

        std::env::remove_var("CODEX_DEFAULT_TIMEOUT_SECS");
        std::env::remove_var("CODEX_MAX_PROMPT_BYTES");
    }

    #[test]
    fn codex_config_allowed_extra_args_ignores_empty_entries() {
        std::env::set_var("CODEX_ALLOWED_EXTRA_ARGS", " --verbose , , --json ");

        let cfg = CodexConfig::from_env();
        assert_eq!(cfg.allowed_extra_args, vec!["--verbose", "--json"]);

        std::env::remove_var("CODEX_ALLOWED_EXTRA_ARGS");
    }
}
