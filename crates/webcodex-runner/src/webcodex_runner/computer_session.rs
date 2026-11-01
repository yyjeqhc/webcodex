//! Narrow, local Computer broker for a persistent Runner in a service session.
//! The login-session helper has no Server connection, token, shell, or generic
//! Runner request decoder. Its only authority is the closed Computer operation.

#[cfg(any(target_os = "macos", windows, test))]
use super::computer::handle_computer_operation_with_runtime;
use super::{err_cmd, CommandResult};
#[cfg(any(target_os = "macos", windows, test))]
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
#[cfg(any(target_os = "macos", windows, test))]
use webcodex_computer::{ComputerConfig, ComputerRuntime};
#[cfg(any(target_os = "macos", windows, test))]
use webcodex_core::artifact_policy::MAX_MCP_IMAGE_BYTES;
use webcodex_core::runner_operation::RunnerComputerOperation;
#[cfg(any(target_os = "macos", windows, test))]
use webcodex_core::runner_operation::RunnerComputerOperationKind;

#[cfg(any(target_os = "macos", windows, test))]
const REQUEST_MAX_BYTES: usize = 160 * 1024;
#[cfg(any(target_os = "macos", windows, test))]
const RESPONSE_MAX_BYTES: usize = 6 * 1024 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_millis(750);
const MAX_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct ClientConfig {
    #[cfg(any(target_os = "macos", windows))]
    dir: PathBuf,
    runner_epoch: String,
    helper_epoch: Arc<Mutex<Option<String>>>,
    control_epoch: Arc<Mutex<Option<String>>>,
}

static CLIENT: OnceLock<ClientConfig> = OnceLock::new();
static REPORTED_AVAILABILITY: Mutex<Option<bool>> = Mutex::new(None);
static REPORTED_CONTROL_EPOCH: Mutex<Option<String>> = Mutex::new(None);
static SESSION_INVALIDATION: AtomicU64 = AtomicU64::new(0);
// The helper is never granted Computer authority through an older Server that
// cannot echo and honor runtime session availability at registration.
static SERVER_AVAILABILITY_CONTRACT: AtomicBool = AtomicBool::new(false);

pub(crate) fn registration_echo_confirms_contract(echo: Option<bool>) -> bool {
    echo.is_some()
}

pub(crate) fn set_server_availability_contract(confirmed: bool) {
    if CLIENT.get().is_some() {
        SERVER_AVAILABILITY_CONTRACT.store(confirmed, Ordering::Release);
        if !confirmed {
            if let Ok(mut known) = CLIENT.get().unwrap().helper_epoch.lock() {
                *known = None;
            }
            // A replacement Server registration starts with unavailable state.
            // Re-publish live facts even if the helper itself did not change.
            if let Ok(mut reported) = REPORTED_AVAILABILITY.lock() {
                *reported = None;
            }
            if let Ok(mut reported) = REPORTED_CONTROL_EPOCH.lock() {
                *reported = None;
            }
        }
    }
}

/// Configure only a persistent Runner, before its first registration or dispatch.
/// Transient Runner invocations keep the existing in-process Computer runtime.
pub(crate) fn configure(dir: PathBuf, runner_epoch: String) -> Result<(), String> {
    if !dir.is_absolute() {
        return Err("computer session directory must be absolute".to_string());
    }
    if dir.exists() {
        validate_dir(&dir, false)?;
    }
    if runner_epoch.len() < 16 || runner_epoch.len() > 128 || !runner_epoch.is_ascii() {
        return Err("invalid computer session Runner epoch".to_string());
    }
    CLIENT
        .set(ClientConfig {
            #[cfg(any(target_os = "macos", windows))]
            dir,
            runner_epoch,
            helper_epoch: Arc::new(Mutex::new(None)),
            control_epoch: Arc::new(Mutex::new(None)),
        })
        .map_err(|_| "computer session already configured".to_string())
}

pub(crate) fn configured() -> bool {
    CLIENT.get().is_some()
}

pub(crate) fn control_epoch() -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    if let Some(client) = CLIENT.get() {
        // A probe refreshes the local revision even when availability is unchanged.
        let _ = availability();
        client
            .control_epoch
            .lock()
            .ok()
            .and_then(|epoch| epoch.clone())
    } else {
        super::computer::local_control_epoch()
    }
}

pub(crate) fn publish_control_epoch_update(publish: impl FnOnce(Option<String>) -> bool) {
    publish_epoch_update(&REPORTED_CONTROL_EPOCH, control_epoch, publish);
}

fn publish_epoch_update(
    reported: &Mutex<Option<String>>,
    current: impl FnOnce() -> Option<String>,
    publish: impl FnOnce(Option<String>) -> bool,
) {
    // Snapshot, enqueue and commit the report under one lock. A second publisher
    // must not enqueue an older snapshot after a newer one while leaving the
    // reported marker at the newer epoch, which would suppress its repair.
    // Registration resets use this same mutex and cannot be lost mid-publication.
    let Ok(mut reported) = reported.lock() else {
        return;
    };
    let update = current().filter(|epoch| reported.as_ref() != Some(epoch));
    if publish(update.clone()) {
        if let Some(epoch) = update {
            *reported = Some(epoch);
        }
    }
}

pub(crate) fn availability() -> Option<bool> {
    CLIENT.get().map(|config| {
        if !SERVER_AVAILABILITY_CONTRACT.load(Ordering::Acquire) {
            return false;
        }
        match exchange(
            config,
            WireRequest::Probe {
                runner_epoch: config.runner_epoch.clone(),
            },
            PROBE_TIMEOUT,
        ) {
            Ok(WireResponse::Ready {
                available: true,
                helper_epoch,
                control_epoch,
            }) => {
                if let Ok(mut known) = config.control_epoch.lock() {
                    *known = control_epoch;
                }
                let Ok(mut known) = config.helper_epoch.lock() else {
                    return false;
                };
                let changed = known.as_ref().is_some_and(|old| old != &helper_epoch);
                *known = Some(helper_epoch);
                !changed
            }
            Ok(WireResponse::Ready {
                available: false, ..
            }) => {
                if let Ok(mut known) = config.helper_epoch.lock() {
                    *known = None;
                }
                false
            }
            _ => false,
        }
    })
}

pub(crate) fn changed_availability() -> Option<bool> {
    let current = availability()?;
    (REPORTED_AVAILABILITY.lock().ok()?.as_ref() != Some(&current)).then_some(current)
}

pub(crate) fn mark_availability_reported(value: bool) {
    if let Ok(mut reported) = REPORTED_AVAILABILITY.lock() {
        *reported = Some(value);
    }
}

pub(crate) fn dispatch(operation: &RunnerComputerOperation) -> CommandResult {
    let start = Instant::now();
    let Some(config) = CLIENT.get() else {
        return err_cmd(
            start,
            "not_started: computer session broker is not configured".to_string(),
        );
    };
    if !SERVER_AVAILABILITY_CONTRACT.load(Ordering::Acquire) {
        return err_cmd(
            start,
            "not_started: Server cannot confirm Computer session availability".to_string(),
        );
    }
    let helper_epoch = match config
        .helper_epoch
        .lock()
        .ok()
        .and_then(|known| known.clone())
    {
        Some(epoch) => epoch,
        None => {
            if availability() != Some(true) {
                return err_cmd(
                    start,
                    "not_started: computer session is unavailable".to_string(),
                );
            }
            let Some(epoch) = config
                .helper_epoch
                .lock()
                .ok()
                .and_then(|known| known.clone())
            else {
                return err_cmd(
                    start,
                    "not_started: computer session epoch is unavailable".to_string(),
                );
            };
            epoch
        }
    };
    // The wire kind is closed here and again in the helper. The same bounded
    // payload validated by the Runner's normal request decoder is forwarded.
    let request = WireRequest::Execute {
        runner_epoch: config.runner_epoch.clone(),
        helper_epoch,
        kind: operation.kind.wire_kind().to_string(),
        payload: operation.payload.clone(),
        timeout_secs: operation.timeout_secs.min(MAX_OPERATION_TIMEOUT.as_secs()),
        control_epoch: operation.control_epoch.clone(),
    };
    let timeout = Duration::from_secs(operation.timeout_secs.max(1)).min(MAX_OPERATION_TIMEOUT);
    match exchange(config, request, timeout) {
        Ok(WireResponse::Result { result }) => result.into_command_result(),
        Ok(WireResponse::Rejected { error }) => err_cmd(start, error),
        _ => err_cmd(start, "outcome_unknown: computer session helper connection was lost; inspect current state before retrying an effect".to_string()),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum WireRequest {
    Probe {
        runner_epoch: String,
    },
    Execute {
        runner_epoch: String,
        helper_epoch: String,
        kind: String,
        payload: String,
        timeout_secs: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        control_epoch: Option<String>,
    },
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum WireResponse {
    Ready {
        available: bool,
        helper_epoch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        control_epoch: Option<String>,
    },
    Result {
        result: WireResult,
    },
    Rejected {
        error: String,
    },
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WireResult {
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    duration_ms: Option<u64>,
    error: Option<String>,
}

impl WireResult {
    #[cfg(any(target_os = "macos", windows, test))]
    fn from_command_result(value: CommandResult) -> Self {
        Self {
            exit_code: value.exit_code,
            stdout: value.stdout,
            stderr: value.stderr,
            duration_ms: value.duration_ms,
            error: value.error,
        }
    }
    fn into_command_result(self) -> CommandResult {
        CommandResult {
            exit_code: self.exit_code,
            stdout: self.stdout,
            stderr: self.stderr,
            duration_ms: self.duration_ms,
            error: self.error,
        }
    }
}

#[cfg(any(target_os = "macos", windows, test))]
struct HelperState {
    runner_epoch: Option<String>,
    runner_pid: Option<u32>,
    retired_runner_epochs: Vec<String>,
    helper_epoch: String,
    runtime: ComputerRuntime,
    observed_invalidation: u64,
}

#[cfg(any(target_os = "macos", windows, test))]
impl HelperState {
    fn new() -> Self {
        Self {
            runner_epoch: None,
            runner_pid: None,
            retired_runner_epochs: Vec::new(),
            helper_epoch: uuid::Uuid::new_v4().to_string(),
            runtime: ComputerRuntime::new(ComputerConfig {
                max_encoded_image_bytes: MAX_MCP_IMAGE_BYTES,
            })
            .with_local_control(super::computer::computer_runtime().local_control().clone()),
            observed_invalidation: SESSION_INVALIDATION.load(Ordering::Acquire),
        }
    }
    fn handle(&mut self, request: WireRequest, peer_pid: u32, available: bool) -> WireResponse {
        let invalidation = SESSION_INVALIDATION.load(Ordering::Acquire);
        if invalidation != self.observed_invalidation {
            self.reset();
            self.observed_invalidation = invalidation;
        }
        let epoch = match &request {
            WireRequest::Probe { runner_epoch } | WireRequest::Execute { runner_epoch, .. } => {
                runner_epoch
            }
        };
        if epoch.len() < 16 || epoch.len() > 128 || !epoch.is_ascii() {
            return WireResponse::Rejected {
                error: "not_started: invalid computer session Runner epoch".to_string(),
            };
        }
        if self
            .retired_runner_epochs
            .iter()
            .any(|retired| retired == epoch)
            || (self.runner_epoch.as_deref() == Some(epoch) && self.runner_pid != Some(peer_pid))
        {
            return WireResponse::Rejected {
                error: "not_started: stale computer session Runner identity".to_string(),
            };
        }
        if !available {
            // A lock/logout invalidates every opaque ID. Reset before returning,
            // including on a probe, so unlocking never resurrects stale IDs.
            self.reset();
            return match request {
                WireRequest::Probe { .. } => WireResponse::Ready {
                    available: false,
                    helper_epoch: self.helper_epoch.clone(),
                    control_epoch: self
                        .runtime
                        .local_control()
                        .snapshot()
                        .ok()
                        .map(|s| s.epoch),
                },
                WireRequest::Execute { .. } => WireResponse::Rejected {
                    error: "not_started: login session is locked or inactive".to_string(),
                },
            };
        }
        if self.runner_epoch.as_deref() != Some(epoch) {
            if let Some(retired) = self.runner_epoch.take() {
                self.retired_runner_epochs.push(retired);
                if self.retired_runner_epochs.len() > 64 {
                    self.retired_runner_epochs.remove(0);
                }
            }
            self.reset();
            self.runner_epoch = Some(epoch.clone());
            self.runner_pid = Some(peer_pid);
        }
        match request {
            WireRequest::Probe { .. } => WireResponse::Ready {
                available: true,
                helper_epoch: self.helper_epoch.clone(),
                control_epoch: self
                    .runtime
                    .local_control()
                    .snapshot()
                    .ok()
                    .map(|s| s.epoch),
            },
            WireRequest::Execute {
                helper_epoch,
                kind,
                payload,
                timeout_secs,
                control_epoch,
                ..
            } => {
                if helper_epoch != self.helper_epoch {
                    return WireResponse::Rejected { error: "not_started: computer session helper epoch changed; reacquire Computer IDs".to_string() };
                }
                let Some(kind) = RunnerComputerOperationKind::from_wire(&kind) else {
                    return WireResponse::Rejected {
                        error: "invalid_request: unsupported computer operation".to_string(),
                    };
                };
                if payload.len()
                    > webcodex_core::runner_protocol::shell_computer_request_payload_max_bytes(
                        kind.wire_kind(),
                    )
                    || payload.contains('\0')
                    || serde_json::from_str::<serde_json::Value>(&payload).is_err()
                    || timeout_secs == 0
                    || timeout_secs > MAX_OPERATION_TIMEOUT.as_secs()
                {
                    return WireResponse::Rejected {
                        error: "invalid_request: malformed or unbounded computer operation"
                            .to_string(),
                    };
                }
                let operation = RunnerComputerOperation {
                    control_epoch,
                    kind,
                    payload,
                    timeout_secs,
                };
                WireResponse::Result {
                    result: WireResult::from_command_result(
                        handle_computer_operation_with_runtime(&self.runtime, &operation),
                    ),
                }
            }
        }
    }
    fn reset(&mut self) {
        let control = self.runtime.local_control().clone();
        let _ = control.invalidate();
        self.helper_epoch = uuid::Uuid::new_v4().to_string();
        self.runtime = ComputerRuntime::new(ComputerConfig {
            max_encoded_image_bytes: MAX_MCP_IMAGE_BYTES,
        })
        .with_local_control(control);
    }
}

#[cfg(any(target_os = "macos", windows, test))]
fn read_frame(stream: &mut impl Read, max: usize) -> Result<Vec<u8>, String> {
    let mut length = [0; 4];
    stream.read_exact(&mut length).map_err(|e| e.to_string())?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > max {
        return Err("computer session frame exceeds bound".to_string());
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

#[cfg(any(target_os = "macos", windows, test))]
fn write_frame(stream: &mut impl Write, bytes: &[u8], max: usize) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > max {
        return Err("computer session frame exceeds bound".to_string());
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(bytes).map_err(|e| e.to_string())
}

fn validate_dir(dir: &Path, helper: bool) -> Result<(), String> {
    #[cfg(not(unix))]
    let _ = helper;
    let meta =
        std::fs::symlink_metadata(dir).map_err(|e| format!("computer session directory: {e}"))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("computer session directory must be a real directory".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o077 != 0 || (helper && meta.uid() != unsafe { libc::geteuid() }) {
            return Err("computer session directory has unsafe owner or permissions".to_string());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod platform;
#[cfg(windows)]
mod platform;
#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use super::*;
    pub(super) fn exchange(
        _: &ClientConfig,
        _: WireRequest,
        _: Duration,
    ) -> Result<WireResponse, String> {
        Err("unsupported platform".to_string())
    }
    pub(super) fn run_helper(_: &Path) -> Result<(), String> {
        Err("unsupported_platform: computer session helper requires macOS or Windows".to_string())
    }
    pub(super) fn native_session_available() -> bool {
        false
    }
}

use platform::{exchange, native_session_available};

pub(crate) fn run_helper(dir: &Path) -> Result<(), String> {
    validate_dir(dir, true)?;
    #[cfg(windows)]
    super::computer::configure_local_control(dir)?;
    std::thread::Builder::new()
        .name("computer-session-monitor".to_string())
        .spawn(|| {
            let mut was_available = native_session_available();
            loop {
                std::thread::sleep(Duration::from_millis(200));
                let available = native_session_available();
                if was_available && !available {
                    SESSION_INVALIDATION.fetch_add(1, Ordering::AcqRel);
                }
                was_available = available;
            }
        })
        .map_err(|e| format!("computer session monitor: {e}"))?;
    platform::run_helper(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_epoch_publication_serializes_snapshot_enqueue_and_record() {
        use std::sync::mpsc;
        let reported = Arc::new(Mutex::new(None));
        let current = Arc::new(AtomicU64::new(1));
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        let first = {
            let reported = reported.clone();
            let current = current.clone();
            let out = out_tx.clone();
            std::thread::spawn(move || {
                publish_epoch_update(
                    &reported,
                    || {
                        assert!(matches!(
                            reported.try_lock(),
                            Err(std::sync::TryLockError::WouldBlock)
                        ));
                        Some(current.load(Ordering::Acquire).to_string())
                    },
                    |epoch| {
                        entered_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        out.send(epoch).unwrap();
                        true
                    },
                );
            })
        };
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // The enqueue callback is paused. Its version must remain protected
        // until the actual enqueue and successful-report record both finish.
        let publication_locked = matches!(
            reported.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        );
        current.store(2, Ordering::Release);
        let second = {
            let reported = reported.clone();
            let current = current.clone();
            std::thread::spawn(move || {
                publish_epoch_update(
                    &reported,
                    || Some(current.load(Ordering::Acquire).to_string()),
                    |epoch| {
                        out_tx.send(epoch).unwrap();
                        true
                    },
                );
            })
        };
        release_tx.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
        assert!(
            publication_locked,
            "epoch publication released its lock before enqueue"
        );
        assert_eq!(
            out_rx.try_iter().collect::<Vec<_>>(),
            [Some("1".into()), Some("2".into())]
        );
        assert_eq!(reported.lock().unwrap().as_deref(), Some("2"));
        publish_epoch_update(
            &reported,
            || Some("2".into()),
            |epoch| {
                assert!(epoch.is_none());
                true
            },
        );
    }

    #[test]
    fn control_epoch_publication_retries_failed_enqueue_without_marking_reported() {
        let reported = Mutex::new(Some("old".into()));
        publish_epoch_update(
            &reported,
            || Some("current".into()),
            |epoch| {
                assert_eq!(epoch.as_deref(), Some("current"));
                false
            },
        );
        assert_eq!(reported.lock().unwrap().as_deref(), Some("old"));
        publish_epoch_update(
            &reported,
            || Some("current".into()),
            |epoch| {
                assert_eq!(epoch.as_deref(), Some("current"));
                true
            },
        );
        assert_eq!(reported.lock().unwrap().as_deref(), Some("current"));
        // An unavailable epoch must still allow unrelated provider metadata,
        // without claiming that the previous control epoch was superseded.
        publish_epoch_update(
            &reported,
            || None,
            |epoch| {
                assert!(epoch.is_none());
                true
            },
        );
        assert_eq!(reported.lock().unwrap().as_deref(), Some("current"));
    }

    #[test]
    fn old_server_without_availability_echo_cannot_authorize_brokered_computer() {
        assert!(!registration_echo_confirms_contract(None));
        assert!(registration_echo_confirms_contract(Some(false)));
        assert!(registration_echo_confirms_contract(Some(true)));
    }
    #[test]
    fn persistent_runner_rejects_computer_until_server_echoes_availability() {
        // Configuration and registration facts belong to the whole Runner
        // process. Keep this fixture out of other loopback transport sessions.
        crate::tests::IsolatedEnv::new().run("computer-session-availability", || {
            let dir = tempfile::tempdir().unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
                    .unwrap();
            }
            configure(
                dir.path().to_path_buf(),
                "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into(),
            )
            .unwrap();
            mark_availability_reported(true);
            *REPORTED_CONTROL_EPOCH.lock().unwrap() = Some("previous-registration".into());
            set_server_availability_contract(false);
            assert!(REPORTED_AVAILABILITY.lock().unwrap().is_none());
            assert!(REPORTED_CONTROL_EPOCH.lock().unwrap().is_none());
            assert_eq!(availability(), Some(false));
            let result = dispatch(&RunnerComputerOperation {
                control_epoch: None,
                kind: RunnerComputerOperationKind::AccessibilityStatus,
                payload: "{}".into(),
                timeout_secs: 1,
            });
            assert!(result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("Server cannot confirm"));
        });
    }
    #[test]
    fn wire_limits_cover_max_computer_payload_and_image() {
        assert!(
            REQUEST_MAX_BYTES
                >= webcodex_core::runner_protocol::SHELL_COMPUTER_CLIPBOARD_WRITE_PAYLOAD_MAX_BYTES
        );
        assert!(RESPONSE_MAX_BYTES > MAX_MCP_IMAGE_BYTES);
    }
    #[test]
    fn frames_reject_oversize_before_allocation() {
        assert!(read_frame(&mut (20_u32.to_be_bytes().as_slice()), 10).is_err());
        let mut bytes = Vec::new();
        assert!(write_frame(&mut bytes, &[0; 11], 10).is_err());
    }

    #[test]
    fn helper_binds_runner_and_helper_epochs_before_dispatch() {
        let mut state = HelperState::new();
        let runner_a = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".to_string();
        let runner_b = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb".to_string();
        let first = state.handle(
            WireRequest::Probe {
                runner_epoch: runner_a.clone(),
            },
            100,
            true,
        );
        let WireResponse::Ready {
            available: true,
            helper_epoch,
            ..
        } = first
        else {
            panic!("probe failed")
        };
        let rejected = state.handle(
            WireRequest::Execute {
                control_epoch: None,
                runner_epoch: runner_a.clone(),
                helper_epoch: "stale-helper".to_string(),
                kind: "computer_read_clipboard".to_string(),
                payload: "{}".to_string(),
                timeout_secs: 1,
            },
            100,
            true,
        );
        assert!(matches!(rejected, WireResponse::Rejected { .. }));
        let unknown_kind = state.handle(
            WireRequest::Execute {
                control_epoch: None,
                runner_epoch: runner_a.clone(),
                helper_epoch,
                kind: "shell".to_string(),
                payload: "{}".to_string(),
                timeout_secs: 1,
            },
            100,
            true,
        );
        assert!(matches!(unknown_kind, WireResponse::Rejected { .. }));
        assert!(matches!(
            state.handle(
                WireRequest::Probe {
                    runner_epoch: runner_b
                },
                101,
                true
            ),
            WireResponse::Ready {
                available: true,
                ..
            }
        ));
        assert!(matches!(
            state.handle(
                WireRequest::Probe {
                    runner_epoch: runner_a
                },
                100,
                true
            ),
            WireResponse::Rejected { .. }
        ));
    }
}
