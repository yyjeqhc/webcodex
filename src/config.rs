use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub addr: String,
    pub data_dir: PathBuf,
    pub token: Option<String>,
    pub max_text_size: usize,
    pub max_file_size: usize,
    pub codex: CodexConfig,
    pub oauth2: OAuth2Config,
}

/// Server-side QUIC agent transport configuration. Sourced from
/// `WEBCODEX_QUIC_*` env vars, mirroring the project's env-var config pattern.
/// Kept as a standalone struct (not embedded in [`Config`]) so existing
/// `Config { ... }` test literals and constructors are untouched. The listener
/// is **off by default**; operators must explicitly set
/// `WEBCODEX_QUIC_ENABLED=true` and provide a cert/key.
///
/// This is a custom WebCodex QUIC stream transport, NOT HTTP/3. Nginx is
/// not involved; the server listens on UDP directly. The cert/key paths are
/// NOT hardcoded to production Let's Encrypt paths — they are read from env so
/// dev/staging/prod can differ. Paths are validated at listener startup so a
/// missing cert produces a clear runtime error.
#[derive(Debug, Clone)]
pub struct QuicServerConfig {
    pub enabled: bool,
    pub listen: String,
    pub cert: PathBuf,
    pub key: PathBuf,
    pub alpn: String,
}

/// Non-sensitive server-side QUIC listener status exposed through
/// `runtime_status`. This intentionally carries only operator-safe fields:
/// listen address, ALPN, started flag, and a short sanitized error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuicRuntimeStatus {
    pub enabled: bool,
    pub listen: String,
    pub alpn: String,
    pub listener_started: bool,
    pub last_error: Option<String>,
}

impl QuicServerConfig {
    /// Parse from `WEBCODEX_QUIC_*` env vars. Disabled by default.
    pub fn from_env() -> Self {
        Self {
            enabled: env_flag("WEBCODEX_QUIC_ENABLED").unwrap_or(false),
            listen: std::env::var("WEBCODEX_QUIC_LISTEN")
                .unwrap_or_else(|_| "0.0.0.0:8443".to_string()),
            cert: std::env::var("WEBCODEX_QUIC_CERT")
                .map(PathBuf::from)
                .unwrap_or_default(),
            key: std::env::var("WEBCODEX_QUIC_KEY")
                .map(PathBuf::from)
                .unwrap_or_default(),
            alpn: std::env::var("WEBCODEX_QUIC_ALPN")
                .unwrap_or_else(|_| "webcodex-agent/1".to_string()),
        }
    }

    /// Validate that the required cert/key paths are present and readable when
    /// the listener is enabled. Returns a clear error naming the missing field;
    /// never reads or returns file *contents* (in particular, never the key).
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if self.cert.as_os_str().is_empty() {
            return Err("WEBCODEX_QUIC_CERT is not set; QUIC listener requires a cert path".into());
        }
        if self.key.as_os_str().is_empty() {
            return Err("WEBCODEX_QUIC_KEY is not set; QUIC listener requires a key path".into());
        }
        if !self.cert.exists() {
            return Err(format!(
                "WEBCODEX_QUIC_CERT path does not exist: {}",
                self.cert.display()
            ));
        }
        if !self.key.exists() {
            return Err(format!(
                "WEBCODEX_QUIC_KEY path does not exist: {}",
                self.key.display()
            ));
        }
        if self.alpn.trim().is_empty() {
            return Err("WEBCODEX_QUIC_ALPN cannot be empty".into());
        }
        Ok(())
    }

    pub fn runtime_status(&self) -> QuicRuntimeStatus {
        QuicRuntimeStatus {
            enabled: self.enabled,
            listen: self.listen.clone(),
            alpn: self.alpn.clone(),
            listener_started: false,
            last_error: None,
        }
    }
}

impl QuicRuntimeStatus {
    pub fn mark_started(&mut self) {
        self.listener_started = true;
        self.last_error = None;
    }

    pub fn mark_error(&mut self, error: impl AsRef<str>) {
        self.listener_started = false;
        self.last_error = Some(sanitize_quic_runtime_error(error.as_ref()));
    }
}

pub fn sanitize_quic_runtime_error(error: &str) -> String {
    let compact = error
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    let lower = compact.to_ascii_lowercase();

    if lower.contains("webcodex_quic_cert is not set") {
        return "WEBCODEX_QUIC_CERT is not set; QUIC listener requires a cert path".to_string();
    }
    if lower.contains("webcodex_quic_key is not set") {
        return "WEBCODEX_QUIC_KEY is not set; QUIC listener requires a key path".to_string();
    }
    if lower.contains("webcodex_quic_cert") && lower.contains("does not exist") {
        return "WEBCODEX_QUIC_CERT path does not exist".to_string();
    }
    if lower.contains("webcodex_quic_key") && lower.contains("does not exist") {
        return "WEBCODEX_QUIC_KEY path does not exist".to_string();
    }
    if lower.contains("quic cert") {
        if lower.contains("no certificates") {
            return "QUIC cert contains no certificates".to_string();
        }
        if lower.contains("parse") {
            return "failed to parse QUIC cert".to_string();
        }
        if lower.contains("open") {
            return "failed to open QUIC cert".to_string();
        }
    }
    if lower.contains("quic key") || lower.contains("private key") {
        if lower.contains("no private key") {
            return "QUIC key contains no private key".to_string();
        }
        if lower.contains("parse") {
            return "failed to parse QUIC key".to_string();
        }
        if lower.contains("open") {
            return "failed to open QUIC key".to_string();
        }
    }

    compact.chars().take(240).collect()
}

impl Default for QuicServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: "0.0.0.0:8443".to_string(),
            cert: PathBuf::new(),
            key: PathBuf::new(),
            alpn: "webcodex-agent/1".to_string(),
        }
    }
}

/// Codex CLI execution configuration, sourced from `CODEX_*` env vars.
///
/// Codex is an optional advanced local dependency for external workflows. The
/// WebCodex runtime itself serves `read_file`, `git_status`, `git_diff`,
/// `apply_patch`, and `run_shell` through the agent registry.
#[derive(Debug, Clone)]
pub struct CodexConfig {
    /// Path/name of the Codex CLI binary. Default `codex`.
    pub bin: String,
    /// Approval mode passed via `--approval-mode`. Default is **empty**
    /// (disabled): no `--approval-mode` flag is emitted. This keeps the runtime
    /// compatible with Codex CLI builds that do not understand the flag. Set
    /// `CODEX_APPROVAL_MODE` (e.g. `full-auto`, `suggest`) to enable it.
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
            approval_mode: String::new(),
            default_timeout_secs: 3600,
            max_prompt_bytes: 100_000,
            allowed_extra_args: Vec::new(),
        }
    }
}

impl CodexConfig {
    pub fn from_env() -> Self {
        let bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string());
        // CODEX_APPROVAL_MODE defaults to empty (disabled). An empty/blank
        // value, or the sentinels none/off/disabled, mean "do not pass
        // --approval-mode" so the runtime works with Codex CLI builds that do
        // not support the flag.
        let approval_mode = std::env::var("CODEX_APPROVAL_MODE")
            .unwrap_or_default()
            .trim()
            .to_string();
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
    pub(crate) legacy: bool,
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
        legacy: false,
    })
}

fn load_env_file_candidate(path: &Path, legacy: bool) -> Result<EnvFileLoad, String> {
    let mut loaded = load_env_file(path)?;
    loaded.legacy = legacy;
    Ok(loaded)
}

pub(crate) fn env_flag(key: &str) -> Option<bool> {
    let value = std::env::var(key).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Gated tool-request lifecycle tracing (`WEBCODEX_TOOL_REQUEST_TRACE`).
///
/// Default **false**. When false, MCP/API handlers must not serialize response
/// bodies solely to measure size, and no lifecycle events are emitted.
pub(crate) fn tool_request_trace_enabled() -> bool {
    env_flag("WEBCODEX_TOOL_REQUEST_TRACE").unwrap_or(false)
}

/// Experimental MCP `tools/list` compact schemas switch.
///
/// When true, MCP discovery omits `outputSchema` only (keeps name, description,
/// inputSchema, annotations). Default false — production behavior unchanged.
/// Invalid / unset values follow `env_flag` and default to false.
pub(crate) fn mcp_compact_schemas_enabled() -> bool {
    env_flag("WEBCODEX_MCP_COMPACT_SCHEMAS").unwrap_or(false)
}

/// Experimental GPT Action response compact switch.
///
/// When true, `POST /api/tools/call` (callRuntimeTool) may shrink selected
/// success payloads after tool execution — especially `start_coding_task` —
/// to reduce ChatGPT continuation friction. Default **false**: full responses
/// unchanged. Does **not** affect MCP, tools/list, OpenAPI schemas, tool
/// execution, session ledger, or permission decisions.
pub(crate) fn action_compact_responses_enabled() -> bool {
    env_flag("WEBCODEX_ACTION_COMPACT_RESPONSES").unwrap_or(false)
}

pub(crate) fn load_startup_env_files() -> Result<Vec<EnvFileLoad>, String> {
    if let Ok(path) = std::env::var("WEBCODEX_ENV_FILE") {
        return Ok(vec![load_env_file_candidate(Path::new(&path), false)?]);
    }
    let candidates = [
        (PathBuf::from("./webcodex.env"), false),
        (PathBuf::from("/opt/webcodex/webcodex.env"), false),
        (PathBuf::from("/etc/webcodex/webcodex.env"), false),
    ];
    let mut loaded = Vec::new();
    for (path, legacy) in candidates {
        if path.exists() {
            loaded.push(load_env_file_candidate(&path, legacy)?);
        }
    }
    Ok(loaded)
}

/// OAuth2 server configuration, sourced from `WEBCODEX_OAUTH2_*` env vars.
///
/// Embedded in [`Config`] but **disabled by default**, so it does not change
/// existing runtime behavior unless explicitly enabled with
/// `WEBCODEX_OAUTH2_ENABLED=true`.
///
/// The first OAuth2 implementation uses opaque DB-backed tokens. JWT/JWKS/OIDC
/// can be added later as an extension.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct OAuth2Config {
    /// Public issuer URL for `/.well-known/*` metadata. Defaults to
    /// `WEBCODEX_OAUTH2_ISSUER`, falling back to `WEBCODEX_PUBLIC_URL`.
    pub issuer: Option<String>,
    /// Whether the OAuth2 subsystem is active. Default `false`.
    pub enabled: bool,
    /// Access token time-to-live in seconds. Default `3600` (1 hour).
    pub access_token_ttl_secs: i64,
    /// Refresh token time-to-live in seconds. Default `2592000` (30 days).
    pub refresh_token_ttl_secs: i64,
    /// Authorization code time-to-live in seconds. Default `300` (5 minutes).
    pub authorization_code_ttl_secs: i64,
    /// Whether PKCE (S256) is required for authorization code flows. Default
    /// `true`.
    pub require_pkce: bool,
    /// Whether the public shared-key OAuth bridge authorize flow is enabled.
    /// Default `false`.
    pub shared_key_bridge_enabled: bool,
}

impl Default for OAuth2Config {
    fn default() -> Self {
        Self {
            issuer: None,
            enabled: false,
            access_token_ttl_secs: 3600,
            refresh_token_ttl_secs: 2_592_000,
            authorization_code_ttl_secs: 300,
            require_pkce: true,
            shared_key_bridge_enabled: false,
        }
    }
}

impl OAuth2Config {
    pub fn from_env() -> Self {
        let enabled = env_flag("WEBCODEX_OAUTH2_ENABLED").unwrap_or(false);
        // OAuth2-specific issuer takes precedence over the generic public URL.
        let issuer = std::env::var("WEBCODEX_OAUTH2_ISSUER")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                std::env::var("WEBCODEX_PUBLIC_URL")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
            });
        let access_token_ttl_secs = std::env::var("WEBCODEX_OAUTH2_ACCESS_TOKEN_TTL_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(3600);
        let refresh_token_ttl_secs = std::env::var("WEBCODEX_OAUTH2_REFRESH_TOKEN_TTL_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(2_592_000);
        let authorization_code_ttl_secs = std::env::var("WEBCODEX_OAUTH2_AUTH_CODE_TTL_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(300);
        let require_pkce = env_flag("WEBCODEX_OAUTH2_REQUIRE_PKCE").unwrap_or(true);
        let shared_key_bridge_enabled =
            env_flag("WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE").unwrap_or(false);
        Self {
            issuer,
            enabled,
            access_token_ttl_secs,
            refresh_token_ttl_secs,
            authorization_code_ttl_secs,
            require_pkce,
            shared_key_bridge_enabled,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            addr: std::env::var("WEBCODEX_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            data_dir: std::env::var("WEBCODEX_DATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./data")),
            token: std::env::var("WEBCODEX_TOKEN").ok(),
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::from_env(),
            oauth2: OAuth2Config::from_env(),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("webcodex.db")
    }

    pub fn session_ledger_path(&self) -> PathBuf {
        if self.data_dir.is_absolute() {
            return self.data_dir.join("sessions.json");
        }
        runtime_state_dir().join("sessions.json")
    }

    pub fn runtime_state_dir(&self) -> PathBuf {
        if self.data_dir.is_absolute() {
            return self.data_dir.clone();
        }
        runtime_state_dir()
    }

    pub fn uploads_dir(&self) -> PathBuf {
        self.data_dir.join("uploads")
    }

    pub fn is_auth_enabled(&self) -> bool {
        self.token.is_some()
    }

    pub fn validate_token(&self, token: &str) -> bool {
        self.token
            .as_ref()
            .map(|t| constant_time_eq(t.as_bytes(), token.as_bytes()))
            .unwrap_or(false)
    }
}

pub(crate) fn runtime_state_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path).join("webcodex");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/state/webcodex");
    }
    std::env::temp_dir().join("webcodex")
}

pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let max_len = a.len().max(b.len());
    let mut diff = a.len() ^ b.len();
    for i in 0..max_len {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        diff |= (av ^ bv) as usize;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quic_server_config_defaults_to_disabled() {
        let cfg = QuicServerConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.listen, "0.0.0.0:8443");
        assert_eq!(cfg.alpn, "webcodex-agent/1");
        // A disabled config is always valid (no cert/key required).
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn quic_server_config_enabled_requires_cert_and_key_paths() {
        // Missing cert path.
        let cfg = QuicServerConfig {
            enabled: true,
            listen: "0.0.0.0:8443".to_string(),
            cert: PathBuf::new(),
            key: PathBuf::from("/tmp/key.pem"),
            alpn: "webcodex-agent/1".to_string(),
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("WEBCODEX_QUIC_CERT"), "err was: {err}");

        // Missing key path.
        let cfg = QuicServerConfig {
            enabled: true,
            listen: "0.0.0.0:8443".to_string(),
            cert: PathBuf::from("/tmp/cert.pem"),
            key: PathBuf::new(),
            alpn: "webcodex-agent/1".to_string(),
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("WEBCODEX_QUIC_KEY"), "err was: {err}");
    }

    #[test]
    fn quic_server_config_enabled_rejects_nonexistent_paths() {
        let cfg = QuicServerConfig {
            enabled: true,
            listen: "0.0.0.0:8443".to_string(),
            cert: PathBuf::from("/definitely/does/not/exist/cert.pem"),
            key: PathBuf::from("/definitely/does/not/exist/key.pem"),
            alpn: "webcodex-agent/1".to_string(),
        };
        let err = cfg.validate().unwrap_err();
        // Names the missing path without dumping file contents.
        assert!(err.contains("does not exist"), "err was: {err}");
        assert!(!err.contains("BEGIN PRIVATE KEY"));
        assert!(!err.contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn quic_runtime_error_sanitizer_removes_cert_and_key_paths() {
        let cert = sanitize_quic_runtime_error(
            "WEBCODEX_QUIC_CERT path does not exist: /etc/letsencrypt/live/example/fullchain.pem",
        );
        assert_eq!(cert, "WEBCODEX_QUIC_CERT path does not exist");
        assert!(!cert.contains("/etc/letsencrypt"));

        let key = sanitize_quic_runtime_error(
            "failed to parse QUIC key /etc/letsencrypt/live/example/privkey.pem: bad pem",
        );
        assert_eq!(key, "failed to parse QUIC key");
        assert!(!key.contains("privkey.pem"));
    }

    #[test]
    fn quic_server_config_from_env_disabled_by_default() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_QUIC_ENABLED");
        std::env::remove_var("WEBCODEX_QUIC_LISTEN");
        std::env::remove_var("WEBCODEX_QUIC_CERT");
        std::env::remove_var("WEBCODEX_QUIC_KEY");
        std::env::remove_var("WEBCODEX_QUIC_ALPN");
        let cfg = QuicServerConfig::from_env();
        assert!(!cfg.enabled);
        assert_eq!(cfg.listen, "0.0.0.0:8443");
        assert_eq!(cfg.alpn, "webcodex-agent/1");
    }

    #[test]
    fn codex_config_defaults() {
        let cfg = CodexConfig::default();
        assert_eq!(cfg.bin, "codex");
        // Default approval mode is empty (disabled): no --approval-mode flag.
        assert_eq!(cfg.approval_mode, "");
        assert_eq!(cfg.default_timeout_secs, 3600);
        assert_eq!(cfg.max_prompt_bytes, 100_000);
        assert!(cfg.allowed_extra_args.is_empty());
    }

    #[test]
    fn constant_time_eq_matches_byte_equality() {
        assert!(constant_time_eq(b"secret123", b"secret123"));
        assert!(!constant_time_eq(b"secret123", b"secret124"));
        assert!(!constant_time_eq(b"secret123", b"secret1234"));
        assert!(!constant_time_eq(b"secret123", b""));
    }

    #[test]
    fn codex_config_from_env_uses_defaults_when_unset() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        // Clear CODEX_* env vars so we get deterministic defaults.
        std::env::remove_var("CODEX_BIN");
        std::env::remove_var("CODEX_APPROVAL_MODE");
        std::env::remove_var("CODEX_DEFAULT_TIMEOUT_SECS");
        std::env::remove_var("CODEX_MAX_PROMPT_BYTES");
        std::env::remove_var("CODEX_ALLOWED_EXTRA_ARGS");

        let cfg = CodexConfig::from_env();
        assert_eq!(cfg.bin, "codex");
        // Unset CODEX_APPROVAL_MODE means disabled (empty), not full-auto.
        assert_eq!(cfg.approval_mode, "");
        assert_eq!(cfg.default_timeout_secs, 3600);
        assert_eq!(cfg.max_prompt_bytes, 100_000);
        assert!(cfg.allowed_extra_args.is_empty());
    }

    #[test]
    fn codex_config_from_env_parses_overrides() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
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
    fn codex_config_from_env_trims_approval_mode_whitespace() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("CODEX_APPROVAL_MODE", "  suggest  ");
        let cfg = CodexConfig::from_env();
        assert_eq!(cfg.approval_mode, "suggest");

        // An unset/blank value normalizes to empty (disabled). The disabled
        // sentinels (none/off/disabled) are recognized later by
        // build_codex_command, so the config keeps the trimmed token.
        std::env::set_var("CODEX_APPROVAL_MODE", "   ");
        let cfg = CodexConfig::from_env();
        assert_eq!(cfg.approval_mode, "");

        std::env::remove_var("CODEX_APPROVAL_MODE");
    }

    #[test]
    fn codex_config_from_env_ignores_invalid_numeric_values() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("CODEX_DEFAULT_TIMEOUT_SECS", "not-a-number");
        std::env::set_var("CODEX_MAX_PROMPT_BYTES", "-5");

        let cfg = CodexConfig::from_env();
        assert_eq!(cfg.default_timeout_secs, 3600);
        assert_eq!(cfg.max_prompt_bytes, 100_000);

        std::env::remove_var("CODEX_DEFAULT_TIMEOUT_SECS");
        std::env::remove_var("CODEX_MAX_PROMPT_BYTES");
    }

    #[test]
    fn oauth2_config_defaults_to_disabled() {
        let _env = crate::auth::AuthEnvGuard::auth_required();
        std::env::remove_var("WEBCODEX_OAUTH2_ENABLED");
        std::env::remove_var("WEBCODEX_PUBLIC_URL");
        std::env::remove_var("WEBCODEX_OAUTH2_ISSUER");
        std::env::remove_var("WEBCODEX_OAUTH2_ACCESS_TOKEN_TTL_SECS");
        std::env::remove_var("WEBCODEX_OAUTH2_REFRESH_TOKEN_TTL_SECS");
        std::env::remove_var("WEBCODEX_OAUTH2_AUTH_CODE_TTL_SECS");
        std::env::remove_var("WEBCODEX_OAUTH2_REQUIRE_PKCE");
        std::env::remove_var("WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE");

        let cfg = OAuth2Config::from_env();
        assert!(!cfg.enabled);
        assert!(cfg.issuer.is_none());
        assert_eq!(cfg.access_token_ttl_secs, 3600);
        assert_eq!(cfg.refresh_token_ttl_secs, 2_592_000);
        assert_eq!(cfg.authorization_code_ttl_secs, 300);
        assert!(cfg.require_pkce);
        assert!(!cfg.shared_key_bridge_enabled);
    }

    #[test]
    fn oauth2_config_from_env_parses_overrides() {
        let env = crate::auth::AuthEnvGuard::new();
        std::env::set_var("WEBCODEX_OAUTH2_ENABLED", "true");
        std::env::set_var("WEBCODEX_OAUTH2_ISSUER", "https://example.com");
        std::env::set_var("WEBCODEX_OAUTH2_ACCESS_TOKEN_TTL_SECS", "1800");
        std::env::set_var("WEBCODEX_OAUTH2_REFRESH_TOKEN_TTL_SECS", "86400");
        std::env::set_var("WEBCODEX_OAUTH2_AUTH_CODE_TTL_SECS", "600");
        std::env::set_var("WEBCODEX_OAUTH2_REQUIRE_PKCE", "false");
        env.enable_oauth2_shared_key_bridge();

        let cfg = OAuth2Config::from_env();
        assert!(cfg.enabled);
        assert_eq!(cfg.issuer.as_deref(), Some("https://example.com"));
        assert_eq!(cfg.access_token_ttl_secs, 1800);
        assert_eq!(cfg.refresh_token_ttl_secs, 86400);
        assert_eq!(cfg.authorization_code_ttl_secs, 600);
        assert!(!cfg.require_pkce);
        assert!(cfg.shared_key_bridge_enabled);

        std::env::remove_var("WEBCODEX_OAUTH2_ENABLED");
        std::env::remove_var("WEBCODEX_OAUTH2_ISSUER");
        std::env::remove_var("WEBCODEX_OAUTH2_ACCESS_TOKEN_TTL_SECS");
        std::env::remove_var("WEBCODEX_OAUTH2_REFRESH_TOKEN_TTL_SECS");
        std::env::remove_var("WEBCODEX_OAUTH2_AUTH_CODE_TTL_SECS");
        std::env::remove_var("WEBCODEX_OAUTH2_REQUIRE_PKCE");
    }

    #[test]
    fn oauth2_config_issuer_prefers_oauth2_issuer_over_public_url() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_PUBLIC_URL", "https://pub.example.com");
        std::env::set_var("WEBCODEX_OAUTH2_ISSUER", "https://issuer.example.com");

        let cfg = OAuth2Config::from_env();
        assert_eq!(cfg.issuer.as_deref(), Some("https://issuer.example.com"));

        std::env::remove_var("WEBCODEX_OAUTH2_ISSUER");
        // Falls back to WEBCODEX_PUBLIC_URL when OAUTH2_ISSUER is absent.
        let cfg = OAuth2Config::from_env();
        assert_eq!(cfg.issuer.as_deref(), Some("https://pub.example.com"));

        std::env::remove_var("WEBCODEX_PUBLIC_URL");
    }

    #[test]
    fn codex_config_allowed_extra_args_ignores_empty_entries() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("CODEX_ALLOWED_EXTRA_ARGS", " --verbose , , --json ");

        let cfg = CodexConfig::from_env();
        assert_eq!(cfg.allowed_extra_args, vec!["--verbose", "--json"]);

        std::env::remove_var("CODEX_ALLOWED_EXTRA_ARGS");
    }

    #[test]
    fn load_startup_env_files_explicit_path_loads_webcodex_env() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let new_file = dir.path().join("webcodex.env");
        std::fs::write(&new_file, "WEBCODEX_TOKEN=new\n").unwrap();
        std::env::set_var("WEBCODEX_ENV_FILE", &new_file);
        std::env::remove_var("WEBCODEX_TOKEN");

        let loads = load_startup_env_files().unwrap();
        assert_eq!(loads.len(), 1);
        assert_eq!(loads[0].path, new_file);
        assert_eq!(std::env::var("WEBCODEX_TOKEN").unwrap(), "new");

        std::env::remove_var("WEBCODEX_ENV_FILE");
    }
    #[test]
    fn mcp_compact_schemas_defaults_off() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
        assert!(!mcp_compact_schemas_enabled());
    }

    #[test]
    fn mcp_compact_schemas_true_enables() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "true");
        assert!(mcp_compact_schemas_enabled());
        std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "1");
        assert!(mcp_compact_schemas_enabled());
        std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "false");
        assert!(!mcp_compact_schemas_enabled());
        std::env::set_var("WEBCODEX_MCP_COMPACT_SCHEMAS", "maybe");
        // Invalid values are treated as unset by env_flag -> default false.
        assert!(!mcp_compact_schemas_enabled());
        std::env::remove_var("WEBCODEX_MCP_COMPACT_SCHEMAS");
    }

    #[test]
    fn action_compact_responses_defaults_off() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_ACTION_COMPACT_RESPONSES");
        assert!(!action_compact_responses_enabled());
    }

    #[test]
    fn action_compact_responses_true_enables() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_ACTION_COMPACT_RESPONSES", "true");
        assert!(action_compact_responses_enabled());
        std::env::set_var("WEBCODEX_ACTION_COMPACT_RESPONSES", "1");
        assert!(action_compact_responses_enabled());
        std::env::set_var("WEBCODEX_ACTION_COMPACT_RESPONSES", "yes");
        assert!(action_compact_responses_enabled());
        std::env::set_var("WEBCODEX_ACTION_COMPACT_RESPONSES", "false");
        assert!(!action_compact_responses_enabled());
        std::env::set_var("WEBCODEX_ACTION_COMPACT_RESPONSES", "maybe");
        assert!(!action_compact_responses_enabled());
        std::env::remove_var("WEBCODEX_ACTION_COMPACT_RESPONSES");
    }
    #[test]
    fn tool_request_trace_defaults_off() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_TOOL_REQUEST_TRACE");
        assert!(!tool_request_trace_enabled());
    }

    #[test]
    fn tool_request_trace_true_enables() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_TOOL_REQUEST_TRACE", "true");
        assert!(tool_request_trace_enabled());
        std::env::set_var("WEBCODEX_TOOL_REQUEST_TRACE", "false");
        assert!(!tool_request_trace_enabled());
        std::env::set_var("WEBCODEX_TOOL_REQUEST_TRACE", "maybe");
        assert!(!tool_request_trace_enabled());
        std::env::remove_var("WEBCODEX_TOOL_REQUEST_TRACE");
    }
}
