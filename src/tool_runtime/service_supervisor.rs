//! Narrow authenticated protocol between WebPi Server and its standalone parent.
//!
//! The protocol deliberately has no executable/argv/path mutation surface. A
//! scoped Server call can only request one fixed lifecycle action (`restart`)
//! bound to a durable deployment receipt and revision. The parent owns process
//! control and returns a signed result that a restarted Server can reconcile.

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const SUPERVISOR_TOKEN_ENV: &str = "WEBPI_SUPERVISOR_TOKEN";
pub(crate) const SUPERVISOR_CONTROL_DIR_ENV: &str = "WEBPI_SUPERVISOR_CONTROL_DIR";
pub(crate) const SUPERVISOR_CLIENT_ID_ENV: &str = "WEBPI_SUPERVISOR_CLIENT_ID";
pub(crate) const SUPERVISOR_CANDIDATE_ROOT_ENV: &str = "WEBPI_SUPERVISOR_CANDIDATE_ROOT";
pub(crate) const SUPERVISOR_STAGING_ROOT_ENV: &str = "WEBPI_SUPERVISOR_STAGING_ROOT";
pub(crate) const SUPERVISOR_BACKUP_ROOT_ENV: &str = "WEBPI_SUPERVISOR_BACKUP_ROOT";
pub(crate) const SUPERVISOR_INSTALLED_RUNTIME_ENV: &str = "WEBPI_SUPERVISOR_INSTALLED_RUNTIME";
const REQUEST_FILE: &str = "request.json";
const REQUEST_LOCK_FILE: &str = "request.lock";
const MAX_ENVELOPE_BYTES: usize = 16 * 1024;
const TOKEN_HEX_LEN: usize = 64;
const EXECUTE_AFTER_MS: u64 = 1_500;
const PROTOCOL_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SupervisorProtocolError {
    Unavailable,
    InvalidConfiguration,
    TargetMismatch,
    RequestBusy,
    Io,
    InvalidResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupervisorScheduleOutcome {
    Scheduled,
    AlreadyScheduled,
}

impl SupervisorProtocolError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Unavailable => "supervisor_unavailable",
            Self::InvalidConfiguration => "supervisor_invalid_configuration",
            Self::TargetMismatch => "supervisor_target_mismatch",
            Self::RequestBusy => "supervisor_request_busy",
            Self::Io => "supervisor_io_error",
            Self::InvalidResult => "supervisor_invalid_result",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupervisorRestartResult {
    pub(crate) status: String,
    pub(crate) receipt_revision: u64,
    pub(crate) completed_at_ms: i64,
    pub(crate) error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupervisorDeployArtifact {
    pub(crate) name: String,
    pub(crate) sha256: String,
    pub(crate) size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupervisorDeployResult {
    pub(crate) status: String,
    pub(crate) receipt_revision: u64,
    pub(crate) completed_at_ms: i64,
    pub(crate) error_code: Option<String>,
    pub(crate) backup_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupervisorRollbackResult {
    pub(crate) status: String,
    pub(crate) receipt_revision: u64,
    pub(crate) completed_at_ms: i64,
    pub(crate) error_code: Option<String>,
    pub(crate) backup_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload: String,
    mac: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestartResultPayload {
    version: u64,
    action: String,
    receipt_id: String,
    receipt_revision: u64,
    status: String,
    completed_at_ms: i64,
    error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeployResultPayload {
    version: u64,
    action: String,
    receipt_id: String,
    receipt_revision: u64,
    status: String,
    completed_at_ms: i64,
    error_code: Option<String>,
    backup_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RollbackResultPayload {
    version: u64,
    action: String,
    receipt_id: String,
    receipt_revision: u64,
    status: String,
    completed_at_ms: i64,
    error_code: Option<String>,
    backup_id: Option<String>,
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn valid_token(token: &str) -> bool {
    token.len() == TOKEN_HEX_LEN && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut normalized = [0_u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha256::digest(key);
        normalized[..digest.len()].copy_from_slice(&digest);
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; BLOCK];
    let mut outer_pad = [0x5c_u8; BLOCK];
    for index in 0..BLOCK {
        inner_pad[index] ^= normalized[index];
        outer_pad[index] ^= normalized[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn hex_digest(value: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in value {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn constant_time_hex_eq(expected: &str, actual: &str) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    expected
        .bytes()
        .zip(actual.bytes())
        .fold(0_u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

fn supervisor_context_from_env() -> Result<(PathBuf, String), SupervisorProtocolError> {
    let control = std::env::var(SUPERVISOR_CONTROL_DIR_ENV)
        .ok()
        .map(PathBuf::from)
        .ok_or(SupervisorProtocolError::Unavailable)?;
    let token = std::env::var(SUPERVISOR_TOKEN_ENV)
        .ok()
        .filter(|value| valid_token(value))
        .ok_or(SupervisorProtocolError::Unavailable)?;
    if !control.is_absolute() || !control.is_dir() {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    Ok((control, token))
}

fn valid_client_id(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 128 && !value.chars().any(char::is_control)
}

pub(crate) fn managed_client_id() -> Option<String> {
    std::env::var(SUPERVISOR_CLIENT_ID_ENV)
        .ok()
        .filter(|value| valid_client_id(value))
}

pub(crate) fn available_for_client(client_id: &str) -> bool {
    supervisor_context_from_env().is_ok() && managed_client_id().as_deref() == Some(client_id)
}

pub(crate) fn available() -> bool {
    supervisor_context_from_env().is_ok() && managed_client_id().is_some()
}

fn absolute_env_path(name: &str) -> Result<PathBuf, SupervisorProtocolError> {
    let value = std::env::var(name).map_err(|_| SupervisorProtocolError::Unavailable)?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    Ok(path)
}

fn metadata_is_linklike(path: &Path) -> Result<bool, SupervisorProtocolError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| SupervisorProtocolError::Io)?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn sha256_file(path: &Path) -> Result<String, SupervisorProtocolError> {
    let mut file = fs::File::open(path).map_err(|_| SupervisorProtocolError::Io)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| SupervisorProtocolError::Io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn deployment_environment_probe(
    candidate_id: &str,
    artifacts: &[SupervisorDeployArtifact],
) -> Result<Value, SupervisorProtocolError> {
    if candidate_id.is_empty()
        || candidate_id.len() > 64
        || !candidate_id.bytes().enumerate().all(|(index, byte)| {
            (index > 0 || byte.is_ascii_alphanumeric())
                && (byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let candidate_root = absolute_env_path(SUPERVISOR_CANDIDATE_ROOT_ENV)?;
    let staging_root = absolute_env_path(SUPERVISOR_STAGING_ROOT_ENV)?;
    let backup_root = absolute_env_path(SUPERVISOR_BACKUP_ROOT_ENV)?;
    let installed_root = absolute_env_path(SUPERVISOR_INSTALLED_RUNTIME_ENV)?;
    let candidate_root =
        fs::canonicalize(&candidate_root).map_err(|_| SupervisorProtocolError::Io)?;
    let candidate_component = candidate_root.join(candidate_id);
    let candidate_dir = candidate_component.join("dogfood");
    if metadata_is_linklike(&candidate_component)? || metadata_is_linklike(&candidate_dir)? {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let candidate_dir = fs::canonicalize(candidate_dir).map_err(|_| SupervisorProtocolError::Io)?;
    if !candidate_dir.starts_with(&candidate_root) {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let required = ["webpi.exe", "webpi-server.exe", "webpi-runner.exe"];
    if artifacts.len() != required.len() {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut verified = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        if !required.contains(&artifact.name.as_str())
            || !seen.insert(artifact.name.as_str())
            || artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || artifact.size_bytes == 0
        {
            return Err(SupervisorProtocolError::InvalidConfiguration);
        }
        let path = candidate_dir.join(&artifact.name);
        if metadata_is_linklike(&path)? {
            return Err(SupervisorProtocolError::InvalidConfiguration);
        }
        let metadata = fs::metadata(&path).map_err(|_| SupervisorProtocolError::Io)?;
        if !metadata.is_file() || metadata.len() != artifact.size_bytes {
            return Err(SupervisorProtocolError::InvalidResult);
        }
        let digest = sha256_file(&path)?;
        if !constant_time_hex_eq(&digest, &artifact.sha256.to_ascii_lowercase()) {
            return Err(SupervisorProtocolError::InvalidResult);
        }
        verified.push(json!({
            "name": artifact.name,
            "size_bytes": artifact.size_bytes,
            "sha256_verified": true,
        }));
    }
    let installed_ready = installed_root.is_dir()
        && required
            .iter()
            .all(|name| installed_root.join(name).is_file());
    let staging_parent_ready = staging_root.parent().is_some_and(Path::is_dir);
    let backup_parent_ready = backup_root.parent().is_some_and(Path::is_dir);
    Ok(json!({
        "candidate_id": candidate_id,
        "candidate_artifacts_verified": verified,
        "installed_runtime_present": installed_ready,
        "staging_parent_present": staging_parent_ready,
        "backup_parent_present": backup_parent_ready,
    }))
}

pub(crate) fn rollback_environment_probe(
    backup_id: &str,
) -> Result<Value, SupervisorProtocolError> {
    if backup_id.is_empty()
        || backup_id.len() > 128
        || !backup_id.bytes().enumerate().all(|(index, byte)| {
            (index > 0 || byte.is_ascii_alphanumeric())
                && (byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let backup_root = absolute_env_path(SUPERVISOR_BACKUP_ROOT_ENV)?;
    let installed_root = absolute_env_path(SUPERVISOR_INSTALLED_RUNTIME_ENV)?;
    let backup_root = fs::canonicalize(&backup_root).map_err(|_| SupervisorProtocolError::Io)?;
    let backup_dir_unresolved = backup_root.join(backup_id);
    if metadata_is_linklike(&backup_dir_unresolved)? {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let backup_dir =
        fs::canonicalize(&backup_dir_unresolved).map_err(|_| SupervisorProtocolError::Io)?;
    if !backup_dir.starts_with(&backup_root) {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let manifest_path = backup_dir.join("manifest.json");
    if metadata_is_linklike(&manifest_path)? {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let manifest_bytes = fs::read(&manifest_path).map_err(|_| SupervisorProtocolError::Io)?;
    if manifest_bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| SupervisorProtocolError::InvalidResult)?;
    let object = manifest
        .as_object()
        .ok_or(SupervisorProtocolError::InvalidResult)?;
    if object.get("version").and_then(Value::as_u64) != Some(1)
        || object.get("backup_id").and_then(Value::as_str) != Some(backup_id)
        || !object
            .get("receipt_id")
            .and_then(Value::as_str)
            .is_some_and(|value| {
                value.starts_with(webcodex_store::DEPLOYMENT_RECEIPT_ID_PREFIX) && value.len() <= 96
            })
        || object
            .get("receipt_revision")
            .and_then(Value::as_u64)
            .is_none_or(|revision| revision == 0)
    {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    let required = ["webpi.exe", "webpi-server.exe", "webpi-runner.exe"];
    let artifacts = object
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or(SupervisorProtocolError::InvalidResult)?;
    if artifacts.len() != required.len() {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut verified = Vec::with_capacity(artifacts.len());
    for item in artifacts {
        let item = item
            .as_object()
            .ok_or(SupervisorProtocolError::InvalidResult)?;
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .ok_or(SupervisorProtocolError::InvalidResult)?;
        let digest = item
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or(SupervisorProtocolError::InvalidResult)?;
        let size = item
            .get("size_bytes")
            .and_then(Value::as_u64)
            .ok_or(SupervisorProtocolError::InvalidResult)?;
        if !required.contains(&name)
            || !seen.insert(name)
            || digest.len() != 64
            || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            || size == 0
        {
            return Err(SupervisorProtocolError::InvalidResult);
        }
        let path = backup_dir.join(name);
        if metadata_is_linklike(&path)? {
            return Err(SupervisorProtocolError::InvalidConfiguration);
        }
        let metadata = fs::metadata(&path).map_err(|_| SupervisorProtocolError::Io)?;
        if !metadata.is_file() || metadata.len() != size {
            return Err(SupervisorProtocolError::InvalidResult);
        }
        let actual = sha256_file(&path)?;
        if !constant_time_hex_eq(&actual, &digest.to_ascii_lowercase()) {
            return Err(SupervisorProtocolError::InvalidResult);
        }
        verified.push(json!({
            "name": name,
            "size_bytes": size,
            "sha256_verified": true,
        }));
    }
    if seen.len() != required.len() {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    let installed_runtime_present = installed_root.is_dir()
        && required
            .iter()
            .all(|name| installed_root.join(name).is_file());
    Ok(json!({
        "backup_id": backup_id,
        "backup_artifacts_verified": verified,
        "installed_runtime_present": installed_runtime_present,
    }))
}

fn restart_payload(
    receipt_id: &str,
    receipt_revision: u64,
    requested_at_ms: i64,
) -> Result<String, SupervisorProtocolError> {
    if !receipt_id.starts_with(webcodex_store::DEPLOYMENT_RECEIPT_ID_PREFIX)
        || receipt_id.len() > 96
        || !receipt_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || receipt_revision == 0
    {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let mut payload = BTreeMap::<String, Value>::new();
    payload.insert("action".into(), json!("restart"));
    payload.insert("execute_after_ms".into(), json!(EXECUTE_AFTER_MS));
    payload.insert("receipt_id".into(), json!(receipt_id));
    payload.insert("receipt_revision".into(), json!(receipt_revision));
    payload.insert("requested_at_ms".into(), json!(requested_at_ms));
    payload.insert("version".into(), json!(PROTOCOL_VERSION));
    serde_json::to_string(&payload).map_err(|_| SupervisorProtocolError::InvalidConfiguration)
}

fn deploy_payload(
    receipt_id: &str,
    receipt_revision: u64,
    requested_at_ms: i64,
    candidate_id: &str,
    artifacts: &[SupervisorDeployArtifact],
) -> Result<String, SupervisorProtocolError> {
    if !receipt_id.starts_with(webcodex_store::DEPLOYMENT_RECEIPT_ID_PREFIX)
        || receipt_id.len() > 96
        || !receipt_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || receipt_revision == 0
        || candidate_id.is_empty()
        || candidate_id.len() > 64
        || !candidate_id.bytes().enumerate().all(|(index, byte)| {
            (index > 0 || byte.is_ascii_alphanumeric())
                && (byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let required = ["webpi.exe", "webpi-server.exe", "webpi-runner.exe"];
    if artifacts.len() != required.len() {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut artifact_values = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        if !required.contains(&artifact.name.as_str())
            || !seen.insert(artifact.name.as_str())
            || artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || artifact.size_bytes == 0
        {
            return Err(SupervisorProtocolError::InvalidConfiguration);
        }
        let mut item = BTreeMap::<String, Value>::new();
        item.insert("name".into(), json!(artifact.name));
        item.insert("sha256".into(), json!(artifact.sha256.to_ascii_lowercase()));
        item.insert("size_bytes".into(), json!(artifact.size_bytes));
        artifact_values.push(json!(item));
    }
    if seen.len() != required.len() {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let mut payload = BTreeMap::<String, Value>::new();
    payload.insert("action".into(), json!("deploy"));
    payload.insert("artifacts".into(), Value::Array(artifact_values));
    payload.insert("candidate_id".into(), json!(candidate_id));
    payload.insert("execute_after_ms".into(), json!(EXECUTE_AFTER_MS));
    payload.insert("receipt_id".into(), json!(receipt_id));
    payload.insert("receipt_revision".into(), json!(receipt_revision));
    payload.insert("requested_at_ms".into(), json!(requested_at_ms));
    payload.insert("version".into(), json!(PROTOCOL_VERSION));
    serde_json::to_string(&payload).map_err(|_| SupervisorProtocolError::InvalidConfiguration)
}

fn rollback_payload(
    receipt_id: &str,
    receipt_revision: u64,
    requested_at_ms: i64,
    backup_id: &str,
) -> Result<String, SupervisorProtocolError> {
    if !receipt_id.starts_with(webcodex_store::DEPLOYMENT_RECEIPT_ID_PREFIX)
        || receipt_id.len() > 96
        || !receipt_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || receipt_revision == 0
        || backup_id.is_empty()
        || backup_id.len() > 128
        || !backup_id.bytes().enumerate().all(|(index, byte)| {
            (index > 0 || byte.is_ascii_alphanumeric())
                && (byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let mut payload = BTreeMap::<String, Value>::new();
    payload.insert("action".into(), json!("rollback"));
    payload.insert("backup_id".into(), json!(backup_id));
    payload.insert("execute_after_ms".into(), json!(EXECUTE_AFTER_MS));
    payload.insert("receipt_id".into(), json!(receipt_id));
    payload.insert("receipt_revision".into(), json!(receipt_revision));
    payload.insert("requested_at_ms".into(), json!(requested_at_ms));
    payload.insert("version".into(), json!(PROTOCOL_VERSION));
    serde_json::to_string(&payload).map_err(|_| SupervisorProtocolError::InvalidConfiguration)
}

fn envelope_json(payload: String, token: &str) -> Result<Vec<u8>, SupervisorProtocolError> {
    let mac = hex_digest(&hmac_sha256(token.as_bytes(), payload.as_bytes()));
    serde_json::to_vec(&json!({"payload": payload, "mac": mac}))
        .map_err(|_| SupervisorProtocolError::InvalidConfiguration)
}

fn request_identity_fingerprint(identity: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webpi.supervisor.request-identity.v1\0");
    hasher.update(identity.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn request_in_flight(
    action: &str,
    receipt_id: &str,
    receipt_revision: u64,
) -> Result<bool, SupervisorProtocolError> {
    if !matches!(action, "restart" | "deploy" | "rollback")
        || !receipt_id.starts_with(webcodex_store::DEPLOYMENT_RECEIPT_ID_PREFIX)
        || receipt_id.len() > 96
        || receipt_revision == 0
    {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let (control_dir, _) = supervisor_context_from_env()?;
    let identity = format!("{action}:{receipt_id}:{receipt_revision}");
    let expected = request_identity_fingerprint(&identity);
    match fs::read_to_string(control_dir.join(REQUEST_LOCK_FILE)) {
        Ok(existing) => Ok(existing.trim() == expected),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(SupervisorProtocolError::Io),
    }
}

fn atomic_create_request(
    control_dir: &Path,
    identity: &str,
    bytes: &[u8],
) -> Result<SupervisorScheduleOutcome, SupervisorProtocolError> {
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    let destination = control_dir.join(REQUEST_FILE);
    let lock_path = control_dir.join(REQUEST_LOCK_FILE);
    let fingerprint = request_identity_fingerprint(identity);
    let mut lock = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return match fs::read_to_string(&lock_path) {
                Ok(existing) if existing.trim() == fingerprint => {
                    Ok(SupervisorScheduleOutcome::AlreadyScheduled)
                }
                _ => Err(SupervisorProtocolError::RequestBusy),
            };
        }
        Err(_) => return Err(SupervisorProtocolError::Io),
    };
    if writeln!(lock, "{fingerprint}").is_err() || lock.sync_all().is_err() {
        drop(lock);
        let _ = fs::remove_file(&lock_path);
        return Err(SupervisorProtocolError::Io);
    }
    if destination.exists() {
        drop(lock);
        let _ = fs::remove_file(&lock_path);
        return Err(SupervisorProtocolError::RequestBusy);
    }
    let temp = control_dir.join(format!(".request.{}.tmp", std::process::id()));
    // The exclusive request lock guarantees there is no concurrent writer in
    // this supervisor epoch, so a stale same-PID temp can be safely discarded.
    let _ = fs::remove_file(&temp);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|_| SupervisorProtocolError::Io)?;
        file.write_all(bytes)
            .map_err(|_| SupervisorProtocolError::Io)?;
        file.sync_all().map_err(|_| SupervisorProtocolError::Io)?;
        drop(file);
        fs::rename(&temp, &destination).map_err(|_| SupervisorProtocolError::Io)
    })();
    let _ = fs::remove_file(&temp);
    drop(lock);
    if result.is_err() {
        let _ = fs::remove_file(&lock_path);
    }
    result.map(|()| SupervisorScheduleOutcome::Scheduled)
}

pub(crate) fn schedule_restart(
    client_id: &str,
    receipt_id: &str,
    receipt_revision: u64,
) -> Result<SupervisorScheduleOutcome, SupervisorProtocolError> {
    if !available_for_client(client_id) {
        return Err(SupervisorProtocolError::TargetMismatch);
    }
    let (control_dir, token) = supervisor_context_from_env()?;
    let payload = restart_payload(receipt_id, receipt_revision, unix_millis())?;
    let envelope = envelope_json(payload, &token)?;
    let identity = format!("restart:{receipt_id}:{receipt_revision}");
    atomic_create_request(&control_dir, &identity, &envelope)
}

pub(crate) fn schedule_deploy(
    client_id: &str,
    receipt_id: &str,
    receipt_revision: u64,
    candidate_id: &str,
    artifacts: &[SupervisorDeployArtifact],
) -> Result<SupervisorScheduleOutcome, SupervisorProtocolError> {
    if !available_for_client(client_id) {
        return Err(SupervisorProtocolError::TargetMismatch);
    }
    let (control_dir, token) = supervisor_context_from_env()?;
    let payload = deploy_payload(
        receipt_id,
        receipt_revision,
        unix_millis(),
        candidate_id,
        artifacts,
    )?;
    let envelope = envelope_json(payload, &token)?;
    let identity = format!("deploy:{receipt_id}:{receipt_revision}");
    atomic_create_request(&control_dir, &identity, &envelope)
}

pub(crate) fn schedule_rollback(
    client_id: &str,
    receipt_id: &str,
    receipt_revision: u64,
    backup_id: &str,
) -> Result<SupervisorScheduleOutcome, SupervisorProtocolError> {
    if !available_for_client(client_id) {
        return Err(SupervisorProtocolError::TargetMismatch);
    }
    let (control_dir, token) = supervisor_context_from_env()?;
    let payload = rollback_payload(receipt_id, receipt_revision, unix_millis(), backup_id)?;
    let envelope = envelope_json(payload, &token)?;
    let identity = format!("rollback:{receipt_id}:{receipt_revision}");
    atomic_create_request(&control_dir, &identity, &envelope)
}

fn result_path(control_dir: &Path, receipt_id: &str) -> Result<PathBuf, SupervisorProtocolError> {
    if !receipt_id.starts_with(webcodex_store::DEPLOYMENT_RECEIPT_ID_PREFIX)
        || receipt_id.len() > 96
        || !receipt_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(SupervisorProtocolError::InvalidConfiguration);
    }
    Ok(control_dir.join(format!("result-{receipt_id}.json")))
}

fn verify_envelope(bytes: &[u8], token: &str) -> Result<String, SupervisorProtocolError> {
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|_| SupervisorProtocolError::InvalidResult)?;
    if envelope.payload.len() > MAX_ENVELOPE_BYTES || envelope.mac.len() != 64 {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    let expected = hex_digest(&hmac_sha256(token.as_bytes(), envelope.payload.as_bytes()));
    if !constant_time_hex_eq(&expected, &envelope.mac.to_ascii_lowercase()) {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    Ok(envelope.payload)
}

pub(crate) fn read_restart_result(
    receipt_id: &str,
    receipt_revision: u64,
) -> Result<Option<SupervisorRestartResult>, SupervisorProtocolError> {
    let (control_dir, token) = supervisor_context_from_env()?;
    let path = result_path(&control_dir, receipt_id)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(SupervisorProtocolError::Io),
    };
    let payload = verify_envelope(&bytes, &token)?;
    let result: RestartResultPayload =
        serde_json::from_str(&payload).map_err(|_| SupervisorProtocolError::InvalidResult)?;
    if result.version != PROTOCOL_VERSION
        || result.action != "restart"
        || result.receipt_id != receipt_id
        || result.receipt_revision != receipt_revision
        || !matches!(result.status.as_str(), "succeeded" | "failed")
        || result.error_code.as_deref().is_some_and(|code| {
            code.is_empty()
                || code.len() > 128
                || !code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
        })
    {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    Ok(Some(SupervisorRestartResult {
        status: result.status,
        receipt_revision: result.receipt_revision,
        completed_at_ms: result.completed_at_ms,
        error_code: result.error_code,
    }))
}

fn parse_deploy_result_payload(
    payload: &str,
    receipt_id: &str,
    receipt_revision: u64,
) -> Result<SupervisorDeployResult, SupervisorProtocolError> {
    let result: DeployResultPayload =
        serde_json::from_str(payload).map_err(|_| SupervisorProtocolError::InvalidResult)?;
    let valid_error = result.error_code.as_deref().is_none_or(|code| {
        !code.is_empty()
            && code.len() <= 128
            && code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
    });
    let valid_backup = result.backup_id.as_deref().is_none_or(|backup| {
        !backup.is_empty()
            && backup.len() <= 128
            && backup
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    });
    if result.version != PROTOCOL_VERSION
        || result.action != "deploy"
        || result.receipt_id != receipt_id
        || result.receipt_revision != receipt_revision
        || !matches!(
            result.status.as_str(),
            "succeeded" | "failed" | "rolled_back" | "outcome_unknown"
        )
        || !valid_error
        || !valid_backup
        || (matches!(result.status.as_str(), "succeeded" | "rolled_back")
            && result.backup_id.is_none())
    {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    Ok(SupervisorDeployResult {
        status: result.status,
        receipt_revision: result.receipt_revision,
        completed_at_ms: result.completed_at_ms,
        error_code: result.error_code,
        backup_id: result.backup_id,
    })
}

fn parse_rollback_result_payload(
    payload: &str,
    receipt_id: &str,
    receipt_revision: u64,
) -> Result<SupervisorRollbackResult, SupervisorProtocolError> {
    let result: RollbackResultPayload =
        serde_json::from_str(payload).map_err(|_| SupervisorProtocolError::InvalidResult)?;
    let valid_error = result.error_code.as_deref().is_none_or(|code| {
        !code.is_empty()
            && code.len() <= 128
            && code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
    });
    let valid_backup = result.backup_id.as_deref().is_none_or(|backup| {
        !backup.is_empty()
            && backup.len() <= 128
            && backup
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    });
    if result.version != PROTOCOL_VERSION
        || result.action != "rollback"
        || result.receipt_id != receipt_id
        || result.receipt_revision != receipt_revision
        || !matches!(
            result.status.as_str(),
            "succeeded" | "failed" | "outcome_unknown"
        )
        || !valid_error
        || !valid_backup
        || (matches!(result.status.as_str(), "succeeded" | "outcome_unknown")
            && result.backup_id.is_none())
    {
        return Err(SupervisorProtocolError::InvalidResult);
    }
    Ok(SupervisorRollbackResult {
        status: result.status,
        receipt_revision: result.receipt_revision,
        completed_at_ms: result.completed_at_ms,
        error_code: result.error_code,
        backup_id: result.backup_id,
    })
}

pub(crate) fn read_rollback_result(
    receipt_id: &str,
    receipt_revision: u64,
) -> Result<Option<SupervisorRollbackResult>, SupervisorProtocolError> {
    let (control_dir, token) = supervisor_context_from_env()?;
    let path = result_path(&control_dir, receipt_id)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(SupervisorProtocolError::Io),
    };
    let payload = verify_envelope(&bytes, &token)?;
    Ok(Some(parse_rollback_result_payload(
        &payload,
        receipt_id,
        receipt_revision,
    )?))
}

pub(crate) fn remove_rollback_result(receipt_id: &str) {
    remove_restart_result(receipt_id);
}

pub(crate) fn read_deploy_result(
    receipt_id: &str,
    receipt_revision: u64,
) -> Result<Option<SupervisorDeployResult>, SupervisorProtocolError> {
    let (control_dir, token) = supervisor_context_from_env()?;
    let path = result_path(&control_dir, receipt_id)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(SupervisorProtocolError::Io),
    };
    let payload = verify_envelope(&bytes, &token)?;
    Ok(Some(parse_deploy_result_payload(
        &payload,
        receipt_id,
        receipt_revision,
    )?))
}

pub(crate) fn remove_deploy_result(receipt_id: &str) {
    remove_restart_result(receipt_id);
}

pub(crate) fn remove_restart_result(receipt_id: &str) {
    let Ok((control_dir, _)) = supervisor_context_from_env() else {
        return;
    };
    if let Ok(path) = result_path(&control_dir, receipt_id) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_lock_prevents_concurrent_or_duplicate_supervisor_requests() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            atomic_create_request(
                dir.path(),
                "restart:receipt-a:3",
                br#"{"payload":"x","mac":"y"}"#
            )
            .unwrap(),
            SupervisorScheduleOutcome::Scheduled
        );
        assert!(dir.path().join(REQUEST_FILE).is_file());
        assert!(dir.path().join(REQUEST_LOCK_FILE).is_file());
        assert_eq!(
            atomic_create_request(
                dir.path(),
                "restart:receipt-b:3",
                br#"{"payload":"second","mac":"y"}"#
            ),
            Err(SupervisorProtocolError::RequestBusy)
        );
    }

    #[test]
    fn exact_supervisor_request_replay_is_distinguished_from_other_busy_request() {
        let dir = tempfile::tempdir().unwrap();
        let first = atomic_create_request(
            dir.path(),
            "restart:receipt-a:7",
            br#"{"payload":"first","mac":"a"}"#,
        )
        .unwrap();
        assert_eq!(first, SupervisorScheduleOutcome::Scheduled);
        let replay = atomic_create_request(
            dir.path(),
            "restart:receipt-a:7",
            br#"{"payload":"different-timestamp","mac":"b"}"#,
        )
        .unwrap();
        assert_eq!(replay, SupervisorScheduleOutcome::AlreadyScheduled);
        assert_eq!(
            atomic_create_request(
                dir.path(),
                "restart:receipt-a:8",
                br#"{"payload":"other","mac":"c"}"#,
            ),
            Err(SupervisorProtocolError::RequestBusy)
        );
    }

    #[test]
    fn hmac_matches_python_supervisor_protocol_vector() {
        let token = "ab".repeat(32);
        let payload =
            restart_payload("wc_deploy_1234567890abcdef1234567890abcdef", 3, 10_000).unwrap();
        assert_eq!(
            payload,
            r#"{"action":"restart","execute_after_ms":1500,"receipt_id":"wc_deploy_1234567890abcdef1234567890abcdef","receipt_revision":3,"requested_at_ms":10000,"version":1}"#
        );
        assert_eq!(
            hex_digest(&hmac_sha256(token.as_bytes(), payload.as_bytes())),
            "9a44d942f34597d408ab08ac2f3dcf9c70ce9eae0372258154e62fbecb65617d"
        );
    }

    #[test]
    fn deploy_hmac_matches_python_supervisor_protocol_vector() {
        let token = "ab".repeat(32);
        let artifacts = vec![
            SupervisorDeployArtifact {
                name: "webpi.exe".to_string(),
                sha256: "11".repeat(32),
                size_bytes: 101,
            },
            SupervisorDeployArtifact {
                name: "webpi-server.exe".to_string(),
                sha256: "22".repeat(32),
                size_bytes: 202,
            },
            SupervisorDeployArtifact {
                name: "webpi-runner.exe".to_string(),
                sha256: "33".repeat(32),
                size_bytes: 303,
            },
        ];
        let payload = deploy_payload(
            "wc_deploy_1234567890abcdef1234567890abcdef",
            4,
            10_000,
            "candidate-1",
            &artifacts,
        )
        .unwrap();
        assert_eq!(
            payload,
            r#"{"action":"deploy","artifacts":[{"name":"webpi.exe","sha256":"1111111111111111111111111111111111111111111111111111111111111111","size_bytes":101},{"name":"webpi-server.exe","sha256":"2222222222222222222222222222222222222222222222222222222222222222","size_bytes":202},{"name":"webpi-runner.exe","sha256":"3333333333333333333333333333333333333333333333333333333333333333","size_bytes":303}],"candidate_id":"candidate-1","execute_after_ms":1500,"receipt_id":"wc_deploy_1234567890abcdef1234567890abcdef","receipt_revision":4,"requested_at_ms":10000,"version":1}"#
        );
        assert_eq!(
            hex_digest(&hmac_sha256(token.as_bytes(), payload.as_bytes())),
            "d58f007e2669d454ad014b561543dd68794a4320e0297a9a23262794070f7f8d"
        );
    }

    #[test]
    fn rollback_hmac_matches_python_supervisor_protocol_vector() {
        let token = "ab".repeat(32);
        let payload = rollback_payload(
            "wc_deploy_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            2,
            10_000,
            "backup-source-r4",
        )
        .unwrap();
        assert_eq!(
            payload,
            r#"{"action":"rollback","backup_id":"backup-source-r4","execute_after_ms":1500,"receipt_id":"wc_deploy_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","receipt_revision":2,"requested_at_ms":10000,"version":1}"#
        );
        assert_eq!(
            hex_digest(&hmac_sha256(token.as_bytes(), payload.as_bytes())),
            "f8e835c17cf992de6b7971403baf7ef4c46453e2d7a2a110e72e11d9155a2a94"
        );
        assert_eq!(
            rollback_payload(
                "wc_deploy_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                2,
                10_000,
                "../backup"
            ),
            Err(SupervisorProtocolError::InvalidConfiguration)
        );
    }

    #[test]
    fn deploy_payload_rejects_incomplete_or_unsafe_candidate_identity() {
        let artifacts = vec![SupervisorDeployArtifact {
            name: "webpi.exe".to_string(),
            sha256: "11".repeat(32),
            size_bytes: 101,
        }];
        assert_eq!(
            deploy_payload(
                "wc_deploy_1234567890abcdef1234567890abcdef",
                4,
                10_000,
                "candidate-1",
                &artifacts
            ),
            Err(SupervisorProtocolError::InvalidConfiguration)
        );
        let complete = vec![
            SupervisorDeployArtifact {
                name: "webpi.exe".into(),
                sha256: "11".repeat(32),
                size_bytes: 1,
            },
            SupervisorDeployArtifact {
                name: "webpi-server.exe".into(),
                sha256: "22".repeat(32),
                size_bytes: 2,
            },
            SupervisorDeployArtifact {
                name: "webpi-runner.exe".into(),
                sha256: "33".repeat(32),
                size_bytes: 3,
            },
        ];
        assert_eq!(
            deploy_payload(
                "wc_deploy_1234567890abcdef1234567890abcdef",
                4,
                10_000,
                "../escape",
                &complete
            ),
            Err(SupervisorProtocolError::InvalidConfiguration)
        );
    }

    #[test]
    fn deploy_result_requires_backup_for_success_and_rollback() {
        let receipt = "wc_deploy_1234567890abcdef1234567890abcdef";
        let succeeded = serde_json::json!({
            "version": 1,
            "action": "deploy",
            "receipt_id": receipt,
            "receipt_revision": 4,
            "status": "succeeded",
            "completed_at_ms": 12_000,
            "error_code": null,
            "backup_id": "backup-123"
        })
        .to_string();
        let parsed = parse_deploy_result_payload(&succeeded, receipt, 4).unwrap();
        assert_eq!(parsed.status, "succeeded");
        assert_eq!(parsed.backup_id.as_deref(), Some("backup-123"));

        let missing_backup = serde_json::json!({
            "version": 1,
            "action": "deploy",
            "receipt_id": receipt,
            "receipt_revision": 4,
            "status": "rolled_back",
            "completed_at_ms": 12_000,
            "error_code": "candidate_start_failed",
            "backup_id": null
        })
        .to_string();
        assert_eq!(
            parse_deploy_result_payload(&missing_backup, receipt, 4),
            Err(SupervisorProtocolError::InvalidResult)
        );
    }

    #[test]
    fn deploy_result_rejects_wrong_revision_and_unsafe_backup_but_allows_failed_without_backup() {
        let receipt = "wc_deploy_1234567890abcdef1234567890abcdef";
        let failed = serde_json::json!({
            "version": 1,
            "action": "deploy",
            "receipt_id": receipt,
            "receipt_revision": 4,
            "status": "failed",
            "completed_at_ms": 12_000,
            "error_code": "candidate_hash_mismatch",
            "backup_id": null
        })
        .to_string();
        assert_eq!(
            parse_deploy_result_payload(&failed, receipt, 4)
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            parse_deploy_result_payload(&failed, receipt, 5),
            Err(SupervisorProtocolError::InvalidResult)
        );

        let unsafe_backup = serde_json::json!({
            "version": 1,
            "action": "deploy",
            "receipt_id": receipt,
            "receipt_revision": 4,
            "status": "succeeded",
            "completed_at_ms": 12_000,
            "error_code": null,
            "backup_id": "../escape"
        })
        .to_string();
        assert_eq!(
            parse_deploy_result_payload(&unsafe_backup, receipt, 4),
            Err(SupervisorProtocolError::InvalidResult)
        );
    }

    #[test]
    fn rollback_environment_probe_verifies_backup_before_cutover() {
        let mut env = crate::test_support::TestEnvGuard::new();
        let temp = tempfile::tempdir().unwrap();
        let backup_root = temp.path().join("backups");
        let installed_root = temp.path().join("installed");
        let backup_id = "backup-source-r4";
        let backup_dir = backup_root.join(backup_id);
        std::fs::create_dir_all(&backup_dir).unwrap();
        std::fs::create_dir_all(&installed_root).unwrap();
        env.set(SUPERVISOR_BACKUP_ROOT_ENV, &backup_root);
        env.set(SUPERVISOR_INSTALLED_RUNTIME_ENV, &installed_root);

        let mut artifacts = Vec::new();
        for (index, name) in ["webpi.exe", "webpi-server.exe", "webpi-runner.exe"]
            .into_iter()
            .enumerate()
        {
            let bytes = format!("backup-{name}-{index}").into_bytes();
            let backup_file = backup_dir.join(name);
            std::fs::write(&backup_file, &bytes).unwrap();
            std::fs::write(installed_root.join(name), b"installed").unwrap();
            artifacts.push(json!({
                "name": name,
                "sha256": sha256_file(&backup_file).unwrap(),
                "size_bytes": bytes.len(),
            }));
        }
        std::fs::write(
            backup_dir.join("manifest.json"),
            serde_json::to_vec(&json!({
                "version": 1,
                "backup_id": backup_id,
                "receipt_id": "wc_deploy_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "receipt_revision": 4,
                "artifacts": artifacts,
            }))
            .unwrap(),
        )
        .unwrap();

        let ready = rollback_environment_probe(backup_id).unwrap();
        assert_eq!(ready["installed_runtime_present"], true);
        assert_eq!(
            ready["backup_artifacts_verified"]
                .as_array()
                .expect("verified artifacts")
                .len(),
            3
        );

        std::fs::write(backup_dir.join("webpi.exe"), b"tampered").unwrap();
        assert_eq!(
            rollback_environment_probe(backup_id),
            Err(SupervisorProtocolError::InvalidResult)
        );
        assert_eq!(
            rollback_environment_probe("../escape"),
            Err(SupervisorProtocolError::InvalidConfiguration)
        );
    }

    #[test]
    fn rollback_result_requires_safety_backup_for_success_and_unknown() {
        let receipt = "wc_deploy_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let succeeded = serde_json::json!({
            "version": 1,
            "action": "rollback",
            "receipt_id": receipt,
            "receipt_revision": 2,
            "status": "succeeded",
            "completed_at_ms": 12_000,
            "error_code": null,
            "backup_id": "backup-safety-r2"
        })
        .to_string();
        let parsed = parse_rollback_result_payload(&succeeded, receipt, 2).unwrap();
        assert_eq!(parsed.status, "succeeded");
        assert_eq!(parsed.backup_id.as_deref(), Some("backup-safety-r2"));

        let unknown_without_backup = serde_json::json!({
            "version": 1,
            "action": "rollback",
            "receipt_id": receipt,
            "receipt_revision": 2,
            "status": "outcome_unknown",
            "completed_at_ms": 12_000,
            "error_code": "rollback_recovery_failed",
            "backup_id": null
        })
        .to_string();
        assert_eq!(
            parse_rollback_result_payload(&unknown_without_backup, receipt, 2),
            Err(SupervisorProtocolError::InvalidResult)
        );

        let failed_without_backup = serde_json::json!({
            "version": 1,
            "action": "rollback",
            "receipt_id": receipt,
            "receipt_revision": 2,
            "status": "failed",
            "completed_at_ms": 12_000,
            "error_code": "rollback_backup_validation_failed",
            "backup_id": null
        })
        .to_string();
        assert_eq!(
            parse_rollback_result_payload(&failed_without_backup, receipt, 2)
                .unwrap()
                .status,
            "failed"
        );
    }

    #[test]
    fn result_mac_comparison_rejects_differences() {
        assert!(constant_time_hex_eq("abcd", "abcd"));
        assert!(!constant_time_hex_eq("abcd", "abce"));
        assert!(!constant_time_hex_eq("abcd", "abc"));
    }
}
