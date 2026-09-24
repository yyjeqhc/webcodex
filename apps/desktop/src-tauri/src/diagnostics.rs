//! Diagnostics are explicit allowlist projections, never a redacted dump of
//! configuration, tool payloads, user source, or arbitrary process output.
use crate::error::{DesktopError, DesktopResult};
use crate::runtime_selection::RuntimeSettings;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use webcodex_core::desktop_runtime_contract::MachineBuildInfo;

const MAX_ENV_BYTES: u64 = 256 * 1024;
const TRACE_KEY: &str = "WEBCODEX_TOOL_REQUEST_TRACE";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceMode {
    #[default]
    Off,
    Metadata,
    Full,
}
impl TraceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Metadata => "metadata",
            Self::Full => "full",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "false" | "0" | "no" => Some(Self::Off),
            "metadata" | "true" | "1" | "on" | "yes" => Some(Self::Metadata),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceSettings {
    pub mode: TraceMode,
    pub effective_mode: Option<TraceMode>,
    pub revision: String,
    pub available: bool,
    pub restart_required: bool,
    pub can_restart: bool,
    pub error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceUpdate {
    pub mode: TraceMode,
    pub expected_revision: String,
    pub confirm_full: bool,
    pub restart: bool,
    pub confirm_interrupt: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigurationRecovery {
    pub reason_code: Option<String>,
    pub backup_available: bool,
    pub primary_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    AppData,
    ServerConfiguration,
    TraceDirectory,
    RuntimeDirectory,
    RuntimeConsole,
    Documentation,
    Github,
    ReportIssue,
    Contributing,
    DesktopDevelopment,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticSnapshot {
    pub schema_version: u16,
    pub observed_at_ms: u64,
    pub trace: TraceSettings,
    pub configuration: ConfigurationRecovery,
    pub resources: Vec<ResourceKind>,
    pub can_copy_console_credential: bool,
    pub credential_copy_fence: Option<String>,
    pub report: Value,
    pub markdown: String,
}

pub fn diagnostic_error(code: &str) -> DesktopError {
    DesktopError::new(
        code,
        "The diagnostic action could not be completed safely",
        "Refresh Diagnostics and check that Desktop owns the selected local Runtime.",
    )
}

fn read_env(path: &Path) -> DesktopResult<String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| diagnostic_error("server_environment_unavailable"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_ENV_BYTES {
        return Err(diagnostic_error("server_environment_invalid"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| diagnostic_error("server_environment_unavailable"))?
        .take(MAX_ENV_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| diagnostic_error("server_environment_unavailable"))?;
    if bytes.len() > MAX_ENV_BYTES as usize {
        return Err(diagnostic_error("server_environment_invalid"));
    }
    String::from_utf8(bytes).map_err(|_| diagnostic_error("server_environment_invalid"))
}

fn entry(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    Some((key, value.trim()))
}

fn scalar(value: &str) -> Option<&str> {
    if let Some(rest) = value.strip_prefix('"') {
        return rest
            .split_once('"')
            .filter(|(_, tail)| tail.trim().is_empty() || tail.trim().starts_with('#'))
            .map(|(v, _)| v);
    }
    if let Some(rest) = value.strip_prefix('\'') {
        return rest
            .split_once('\'')
            .filter(|(_, tail)| tail.trim().is_empty() || tail.trim().starts_with('#'))
            .map(|(v, _)| v);
    }
    Some(value.split(" #").next().unwrap_or(value).trim())
}

pub fn env_value(path: &Path, key: &str) -> DesktopResult<Option<String>> {
    // Only called with fixed non-secret diagnostic location keys, never a
    // WebView-supplied name. No env values are returned through IPC.
    if !matches!(
        key,
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR" | "WEBCODEX_DATA" | TRACE_KEY
    ) {
        return Err(diagnostic_error("diagnostic_key_not_allowed"));
    }
    let text = read_env(path)?;
    let values: Vec<_> = text
        .lines()
        .filter_map(entry)
        .filter(|(name, _)| *name == key)
        .collect();
    if values.len() > 1 {
        return Err(diagnostic_error("server_environment_duplicate_key"));
    }
    Ok(values
        .first()
        .and_then(|(_, v)| scalar(v))
        .map(str::to_string))
}

pub fn inspect_trace(path: &Path, can_restart: bool) -> DesktopResult<TraceSettings> {
    let text = read_env(path)?;
    let values: Vec<_> = text
        .lines()
        .filter_map(entry)
        .filter(|(key, _)| *key == TRACE_KEY)
        .collect();
    let mode = values
        .first()
        .and_then(|(_, v)| scalar(v))
        .and_then(TraceMode::parse);
    let error = if values.len() > 1 {
        Some("server_environment_duplicate_key")
    } else if !values.is_empty() && mode.is_none() {
        Some("trace_mode_invalid")
    } else {
        None
    };
    Ok(TraceSettings {
        mode: mode.unwrap_or_default(),
        effective_mode: None,
        revision: format!("{:x}", Sha256::digest(text.as_bytes())),
        available: true,
        restart_required: false,
        can_restart,
        error_code: error.map(str::to_string),
    })
}

pub fn update_trace(
    path: &Path,
    mode: TraceMode,
    expected_revision: &str,
    confirm_full: bool,
) -> DesktopResult<TraceSettings> {
    if mode == TraceMode::Full && !confirm_full {
        return Err(diagnostic_error("full_trace_confirmation_required"));
    }
    let text = read_env(path)?;
    let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
    if digest != expected_revision {
        return Err(diagnostic_error("server_environment_changed"));
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<&str> = text
        .lines()
        .filter(|line| !entry(line).is_some_and(|(key, _)| key == TRACE_KEY))
        .collect();
    let setting = format!("{TRACE_KEY}={}", mode.as_str());
    lines.push(&setting);
    let next = format!("{}{newline}", lines.join(newline));
    if next.len() > MAX_ENV_BYTES as usize {
        return Err(diagnostic_error("server_environment_invalid"));
    }
    let path_for_check = path.to_path_buf();
    crate::state::write_atomic_file_with_hook(path, next.as_bytes(), |_| {
        let current = read_env(&path_for_check)
            .map_err(|_| std::io::Error::other("environment unavailable"))?;
        if format!("{:x}", Sha256::digest(current.as_bytes())) != digest {
            return Err(std::io::Error::other("environment changed"));
        }
        Ok(())
    })
    .map_err(|_| diagnostic_error("server_environment_write_unconfirmed"))?;
    let mut result = inspect_trace(path, false)?;
    result.restart_required = true;
    Ok(result)
}

pub fn trace_directory(path: &Path) -> Option<PathBuf> {
    let location = env_value(path, "WEBCODEX_TOOL_REQUEST_TRACE_DIR")
        .ok()
        .flatten()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env_value(path, "WEBCODEX_DATA")
                .ok()
                .flatten()
                .map(|v| PathBuf::from(v).join("tool-request-traces"))
        })?;
    (location.is_absolute() && location.is_dir()).then_some(location)
}

pub fn console_url(server_url: &str) -> DesktopResult<String> {
    let mut url =
        url::Url::parse(server_url).map_err(|_| diagnostic_error("runtime_console_unavailable"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !matches!(
            url.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
        )
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(diagnostic_error("runtime_console_unavailable"));
    }
    url.set_path("/runtime");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

pub fn safe_build(info: &MachineBuildInfo) -> Value {
    // Reports never repeat arbitrary semantic-version build metadata or an
    // operator-defined prerelease suffix that could encode a private value.
    let version = semver::Version::parse(&info.version)
        .ok()
        .map(|v| format!("{}.{}.{}", v.major, v.minor, v.patch));
    let commit = info.git_commit.as_deref().filter(|value| {
        value.len() >= 7 && value.len() <= 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
    });
    let target = (info.target.len() <= 128
        && info
            .target
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-+.".contains(&b))
        && ["apple-darwin", "windows", "linux"]
            .iter()
            .any(|os| info.target.contains(os)))
    .then_some(&info.target);
    let architecture = matches!(
        info.architecture.as_str(),
        "aarch64" | "x86_64" | "x86" | "arm" | "arm64" | "riscv64"
    )
    .then_some(&info.architecture);
    json!({"binary":match info.binary.as_str(){"webcodex"|"webcodex-server"|"webcodex-runner"|"webcodex-desktop"=>info.binary.as_str(),_=>"unknown"},
        "version":version,"git_commit":commit,"git_dirty":info.git_dirty,"target":target,"architecture":architecture,
        "desktop_runtime_contract":info.desktop_runtime_contract})
}

fn bounded_code(value: Option<&Value>) -> Option<String> {
    let value = value?.as_str()?;
    (value.len() <= 96
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && ![
            "token",
            "secret",
            "password",
            "authorization",
            "credential",
            "api_key",
        ]
        .iter()
        .any(|word| value.contains(word)))
    .then(|| value.into())
}

pub fn continuation(detail: &Value, now: u64) -> Option<Value> {
    let latest = detail
        .get("activity")?
        .as_array()?
        .iter()
        .filter(|event| event.get("meaningful").and_then(Value::as_bool) == Some(true))
        .max_by_key(|event| {
            event
                .get("ended_at_ms")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        })?;
    let observed = latest.get("request_observed_at_ms").and_then(Value::as_u64);
    let handed = latest
        .get("response_handed_at_ms")
        .and_then(Value::as_u64)
        .filter(|h| observed.is_some_and(|o| *h >= o));
    let streaming = latest.get("response_streaming").and_then(Value::as_bool) == Some(true);
    let execution = match latest.get("status").and_then(Value::as_str) {
        Some("succeeded" | "completed" | "success") => "completed",
        Some("failed" | "error") => "failed",
        Some("cancelled") => "cancelled",
        _ => "unknown",
    };
    // Canonical next_call_gap_ms is attached to the ARRIVING event and measures
    // the gap from the preceding completed handoff. It is not evidence that a
    // later call followed this event. Never turn that incoming gap into a false
    // continuation-success claim. Pending requests do not declare meaningfulness.
    let next_observed = handed.and_then(|handed| {
        detail
            .get("activity")
            .and_then(Value::as_array)
            .and_then(|rows| {
                rows.iter()
                    .filter(|row| row.get("meaningful").and_then(Value::as_bool) == Some(true))
                    .filter_map(|row| {
                        row.get("request_observed_at_ms")
                            .or_else(|| row.get("started_at_ms"))
                            .and_then(Value::as_u64)
                    })
                    .filter(|started| *started > handed)
                    .min()
            })
    });
    Some(
        json!({"tool_name":bounded_code(latest.get("tool_name")),"execution":execution,
        "response_handoff":if handed.is_none(){"not_confirmed"}else if streaming{"stream_started"}else{"handler_returned"},
        "request_observed_at_ms":observed,"response_handed_at_ms":handed,
        "service_ms":latest.get("service_ms").and_then(Value::as_u64),
        "previous_response_gap_ms":latest.get("next_call_gap_ms").and_then(Value::as_u64),
        "next_call_gap_ms":next_observed.zip(handed).map(|(started,handed)|started.saturating_sub(handed)),
        "next_meaningful_call":if next_observed.is_some(){"observed"}else{"not_observed"},
        "elapsed_ms":handed.or_else(||latest.get("ended_at_ms").and_then(Value::as_u64)).map(|timestamp|now.saturating_sub(timestamp)),
        "window_transition_kind":bounded_code(latest.get("window_transition_kind")),
        "active_request_count":detail.get("active_count").and_then(Value::as_u64),
        "history_partial":detail.get("activity_truncated").and_then(Value::as_bool).unwrap_or(true),
        "interpretation":"WebCodex observations only. Gaps may include network, Host scheduling, model inference, user actions, or other unobservable time; handoff is not proof of model receipt."}),
    )
}

pub fn report(
    snapshot: &crate::models::DesktopStateSnapshot,
    runtime: &RuntimeSettings,
    runner: Option<&Value>,
    last_call: Option<Value>,
    trace: &TraceSettings,
    activity: &[crate::activity::ActivityEntry],
    permissions: Value,
) -> Value {
    let selected = runtime.selected.as_ref();
    let runtime_builds: Vec<_> = selected
        .into_iter()
        .flat_map(|s| s.binaries.iter())
        .filter_map(|b| b.metadata.as_ref())
        .map(safe_build)
        .collect();
    let capabilities = runner.and_then(|r| r.get("capabilities"));
    let mut cu = serde_json::Map::new();
    for name in [
        "computer_observe",
        "computer_control",
        "computer_snapshot_region",
        "computer_accessibility_observe",
        "computer_accessibility_action",
    ] {
        cu.insert(
            name.into(),
            capabilities
                .and_then(|c| c.get(name))
                .and_then(Value::as_bool)
                .map(Value::Bool)
                .unwrap_or(Value::Null),
        );
    }
    let safe_activity:Vec<_>=activity.iter().rev().take(25).map(|entry| json!({"sequence":entry.sequence,"timestamp_ms":entry.timestamp_ms,"level":entry.level,"event_kind":entry.event_kind})).collect();
    let connection_health:Vec<_>=snapshot.connections.profiles.iter().take(16).map(|p|json!({"lifecycle":p.runtime.lifecycle,"health":p.runtime.health,"ready":p.runtime.ready,"error_code":p.runtime.last_error})).collect();
    let mut desktop = webcodex_core::build_info::machine_build_info("webcodex-desktop");
    desktop.version = env!("CARGO_PKG_VERSION").into();
    json!({"schema_version":1,"desktop":safe_build(&desktop),
        "selected_runtime":{"source":match runtime.source {crate::runtime_selection::RuntimeSource::Bundled=>"bundled",_=>"custom"},
            "selection_revision":runtime.selection_revision,"binaries":runtime_builds,
            "desktop_protocol_compatibility":selected.map(|s|s.compatibility),
            "server_runner_protocol_compatibility":runner.and_then(|r|bounded_code(r.get("protocol_compatibility"))),
            "build_alignment":selected.map(|s|s.build_alignment),"runner_build_alignment":runner.and_then(|r|bounded_code(r.get("build_alignment")))},
        "runtime_health":{"server":snapshot.readiness.server,"runner":snapshot.readiness.runner,"project":snapshot.readiness.project,
            "connection":snapshot.readiness.exposure,"runtime_ready":snapshot.readiness.runtime_ready,
            "connections_running":snapshot.connections.running,"connections_needing_attention":snapshot.connections.needs_attention,
            "connections":connection_health,"last_observed_chatgpt_activity_at_ms":snapshot.chatgpt_activity.as_ref().and_then(|a|a.last_meaningful_activity_at_ms)},
        "diagnostics":{"trace_mode":trace.mode,"effective_trace_mode":trace.effective_mode,"restart_required":trace.restart_required,
            "current_operation":snapshot.current_operation.as_ref().map(|o|o.kind),
            "configuration_issue":snapshot.configuration_issue,"runtime_issue":runtime.unavailable_code},
        "computer_use":{"desktop_permissions":permissions,"runner_advertised_capabilities":cu},
        "last_webcodex_call":last_call,"activity":safe_activity,
        "excluded":["credentials","environment contents","Runner config contents","project files","tool input/result payloads","clipboard","raw stdout/stderr"]})
}

pub fn report_markdown(report: &Value) -> String {
    format!("# WebCodex diagnostic report\n\nProtocol and capabilities determine compatibility. Build revisions are diagnostic identity; modified builds remain the operator's responsibility.\n\nThis report contains allowlisted observations only, not project code, credentials, configuration contents, or request/result payloads.\n\n```json\n{}\n```\n",serde_json::to_string_pretty(report).unwrap_or_else(|_|"{}".into()))
}

/// A deterministic ZIP containing a fixed set of bounded projections. It never
/// walks a directory or reads any operator file for export.
pub fn support_zip(report: &Value) -> DesktopResult<Vec<u8>> {
    let json_bytes =
        serde_json::to_vec_pretty(report).map_err(|_| diagnostic_error("support_bundle_failed"))?;
    let markdown = report_markdown(report).into_bytes();
    let activity = serde_json::to_vec(&report["activity"])
        .map_err(|_| diagnostic_error("support_bundle_failed"))?;
    let files = [
        ("diagnostic-report.json", json_bytes),
        ("diagnostic-report.md", markdown),
        ("activity-safe.json", activity),
    ];
    let mut output = Vec::new();
    let mut central = Vec::new();
    fn u16le(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    fn u32le(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for (name, data) in &files {
        if data.len() > 1024 * 1024 {
            return Err(diagnostic_error("support_bundle_too_large"));
        }
        let offset = output.len() as u32;
        let size = data.len() as u32;
        let mut crc = !0u32;
        for byte in data {
            crc ^= *byte as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320u32 & (0u32.wrapping_sub(crc & 1)));
            }
        }
        crc = !crc;
        u32le(&mut output, 0x04034b50);
        u16le(&mut output, 20);
        u16le(&mut output, 0);
        u16le(&mut output, 0);
        u16le(&mut output, 0);
        u16le(&mut output, 33);
        u32le(&mut output, crc);
        u32le(&mut output, size);
        u32le(&mut output, size);
        u16le(&mut output, name.len() as u16);
        u16le(&mut output, 0);
        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(data);
        u32le(&mut central, 0x02014b50);
        u16le(&mut central, 20);
        u16le(&mut central, 20);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 33);
        u32le(&mut central, crc);
        u32le(&mut central, size);
        u32le(&mut central, size);
        u16le(&mut central, name.len() as u16);
        for _ in 0..4 {
            u16le(&mut central, 0);
        }
        u32le(&mut central, 0);
        u32le(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = output.len() as u32;
    let central_size = central.len() as u32;
    output.extend(central);
    u32le(&mut output, 0x06054b50);
    u16le(&mut output, 0);
    u16le(&mut output, 0);
    u16le(&mut output, files.len() as u16);
    u16le(&mut output, files.len() as u16);
    u32le(&mut output, central_size);
    u32le(&mut output, central_offset);
    u16le(&mut output, 0);
    Ok(output)
}

pub fn export_support(path: &Path, report: &Value) -> DesktopResult<()> {
    if !path.is_absolute() || path.extension().and_then(|e| e.to_str()) != Some("zip") {
        return Err(diagnostic_error("support_bundle_path_invalid"));
    }
    let bytes = support_zip(report)?;
    // No silent overwrite of any existing file, including links. The native
    // chooser can select a fresh name; project files are never included.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| diagnostic_error("support_bundle_file_exists_or_unavailable"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| diagnostic_error("support_bundle_write_unconfirmed"))
}

#[cfg(test)]
mod tests;
