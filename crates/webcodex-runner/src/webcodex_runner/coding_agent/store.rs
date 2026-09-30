//! Bounded durable Run records and atomic state-file publication.

use super::*;

pub(super) const STORE_SCHEMA_VERSION: u32 = 1;
const STORE_FILE: &str = "state.json";
const STORE_MAX_BYTES: usize = 64 * 1024;
pub(super) const STORE_RETENTION_SECS: i64 = 15 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DurableDispatchPhase {
    BeforePromptBarrier,
    PromptDispatchMayHaveOccurred,
    Terminal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DurableRunRecord {
    pub(super) schema_version: u32,
    pub(super) run_id: String,
    pub(super) intent_fingerprint: String,
    pub(super) authority_fingerprint: String,
    pub(super) runtime_project_id: String,
    pub(super) provider_id: String,
    pub(super) provider_instance_id: String,
    pub(super) state: CodingAgentRunState,
    pub(super) execution_state: CodingAgentExecutionState,
    pub(super) dispatch_phase: DurableDispatchPhase,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) terminal: Option<CodingAgentTerminal>,
}

impl DurableRunRecord {
    pub(super) fn snapshot(&self, observation_revision: u64) -> CodingAgentRunSnapshot {
        CodingAgentRunSnapshot {
            run_id: self.run_id.clone(),
            intent_fingerprint: self.intent_fingerprint.clone(),
            authority_fingerprint: self.authority_fingerprint.clone(),
            runtime_project_id: self.runtime_project_id.clone(),
            provider_id: self.provider_id.clone(),
            provider_instance_id: self.provider_instance_id.clone(),
            state: self.state.clone(),
            execution_state: self.execution_state,
            observation_revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
            terminal: self.terminal.clone(),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TerminalWriteGateState {
    reached: bool,
    released: bool,
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct TerminalWriteGate {
    state: Mutex<TerminalWriteGateState>,
    changed: Condvar,
}

#[cfg(test)]
impl TerminalWriteGate {
    fn block_writer(&self) {
        let mut state = self.state.lock().unwrap();
        state.reached = true;
        self.changed.notify_all();
        while !state.released {
            state = self.changed.wait(state).unwrap();
        }
    }

    pub(super) fn wait_until_reached(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = self.state.lock().unwrap();
        while !state.reached {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "terminal durable write gate was never reached"
            );
            let (next, _) = self.changed.wait_timeout(state, remaining).unwrap();
            state = next;
        }
    }

    pub(super) fn release(&self) {
        let mut state = self.state.lock().unwrap();
        state.released = true;
        self.changed.notify_all();
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct DurableRunStoreTestControl {
    fail_next_terminal_writes: std::sync::atomic::AtomicUsize,
    terminal_write_gate: Mutex<Option<Arc<TerminalWriteGate>>>,
}

#[derive(Debug, Clone)]
pub(super) struct DurableRunStore {
    pub(super) root: PathBuf,
    #[cfg(test)]
    test_control: Arc<DurableRunStoreTestControl>,
}

impl DurableRunStore {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            #[cfg(test)]
            test_control: Arc::new(DurableRunStoreTestControl::default()),
        }
    }

    pub(super) fn default_root(client_id: &str, server_url: &str) -> Result<PathBuf, String> {
        let server_url = server_url.trim().trim_end_matches('/');
        if client_id.trim().is_empty() || server_url.is_empty() {
            return Err("ACP durable store requires non-empty Runner identity".to_string());
        }
        let mut hasher = Sha256::new();
        hasher.update(b"webcodex-coding-agent-store-runner-v1\0");
        hasher.update(client_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(server_url.as_bytes());
        let namespace = format!("{:x}", hasher.finalize());
        Ok(
            webcodex_runner_config::paths::default_client_state_base_dir()?
                .join("runner-coding-agent-runs-v1")
                .join(namespace),
        )
    }

    fn run_dir(&self, run_id: &str) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(run_id.as_bytes());
        self.root.join(format!("{:x}", hasher.finalize()))
    }

    pub(super) fn state_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join(STORE_FILE)
    }

    pub(super) fn read(&self, run_id: &str) -> Result<Option<DurableRunRecord>, String> {
        let path = self.state_path(run_id);
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return match fs::symlink_metadata(self.run_dir(run_id)) {
                    Ok(_) => Err(
                        "ACP Run state is missing from an existing Run state directory".to_string(),
                    ),
                    Err(dir_error) if dir_error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(dir_error) => {
                        Err(format!("failed to inspect ACP Run state dir: {dir_error}"))
                    }
                };
            }
            Err(error) => return Err(format!("failed to open ACP Run state: {error}")),
        };
        let len = file
            .metadata()
            .map_err(|error| format!("failed to inspect ACP Run state: {error}"))?
            .len() as usize;
        if len == 0 || len > STORE_MAX_BYTES {
            return Err("ACP Run state has invalid bounded size".to_string());
        }
        let mut bytes = Vec::with_capacity(len);
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("failed to read ACP Run state: {error}"))?;
        let record: DurableRunRecord =
            serde_json::from_slice(&bytes).map_err(|_| "ACP Run state is malformed".to_string())?;
        validate_durable_record(&record)?;
        if record.run_id != run_id
            || self.run_dir(&record.run_id) != path.parent().unwrap_or(Path::new(""))
        {
            return Err("ACP Run state identity mismatch".to_string());
        }
        Ok(Some(record))
    }

    pub(super) fn write(&self, record: &DurableRunRecord) -> Result<(), String> {
        validate_durable_record(record)?;
        let bytes =
            serde_json::to_vec(record).map_err(|_| "failed to encode ACP Run state".to_string())?;
        if bytes.len() > STORE_MAX_BYTES {
            return Err("ACP Run state exceeds durable bound".to_string());
        }
        #[cfg(test)]
        if record.dispatch_phase == DurableDispatchPhase::Terminal {
            if let Some(gate) = self
                .test_control
                .terminal_write_gate
                .lock()
                .unwrap()
                .clone()
            {
                gate.block_writer();
            }
            if self
                .test_control
                .fail_next_terminal_writes
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                    (remaining > 0).then(|| remaining - 1)
                })
                .is_ok()
            {
                return Err("injected ACP terminal durable write failure".to_string());
            }
        }
        let dir = self.run_dir(&record.run_id);
        fs::create_dir_all(&dir)
            .map_err(|error| format!("failed to create ACP Run state dir: {error}"))?;
        let temp = dir.join(format!("state.{}.tmp", Uuid::new_v4().simple()));
        let state_path = self.state_path(&record.run_id);
        let result = (|| {
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|error| format!("failed to create ACP Run temp state: {error}"))?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| format!("failed to persist ACP Run state: {error}"))?;
            drop(file);
            publish_state_file(&temp, &state_path)?;
            sync_parent(&dir)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    pub(super) fn scan(&self) -> Result<Vec<DurableRunRecord>, String> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("failed to list ACP Run state: {error}")),
        };
        let mut records = Vec::new();
        let mut entry_count = 0usize;
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("failed to inspect ACP Run state: {error}"))?;
            let ty = entry
                .file_type()
                .map_err(|error| format!("failed to inspect ACP Run state: {error}"))?;
            if !ty.is_dir() || ty.is_symlink() {
                return Err("ACP Run state root contains an unexpected entry".to_string());
            }
            entry_count = entry_count.saturating_add(1);
            if entry_count > CODING_AGENT_MAX_INVENTORY_RUNS {
                return Err("ACP Run durable state exceeds bounded record count".to_string());
            }
            let path = entry.path().join(STORE_FILE);
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::error!(state_path = %path.display(), error = %error, "ACP Run durable state unavailable during recovery; preserving tombstone");
                    continue;
                }
            };
            if bytes.is_empty() || bytes.len() > STORE_MAX_BYTES {
                tracing::error!(state_path = %path.display(), "ACP Run durable state has invalid bounded size; preserving tombstone");
                continue;
            }
            let record: DurableRunRecord = match serde_json::from_slice(&bytes) {
                Ok(record) => record,
                Err(_) => {
                    tracing::error!(state_path = %path.display(), "ACP Run durable state is malformed; preserving tombstone");
                    continue;
                }
            };
            if let Err(error) = validate_durable_record(&record) {
                tracing::error!(state_path = %path.display(), error = %error, "ACP Run durable state is invalid; preserving tombstone");
                continue;
            }
            if self.run_dir(&record.run_id) != entry.path() {
                tracing::error!(state_path = %path.display(), "ACP Run durable state directory identity mismatch; preserving tombstone");
                continue;
            }
            records.push(record);
        }
        records.sort_by(|a, b| a.run_id.cmp(&b.run_id));
        Ok(records)
    }

    pub(super) fn remove(&self, run_id: &str) {
        let _ = fs::remove_dir_all(self.run_dir(run_id));
    }

    #[cfg(test)]
    pub(super) fn fail_next_terminal_writes(&self, count: usize) {
        self.test_control
            .fail_next_terminal_writes
            .store(count, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(super) fn set_terminal_write_gate(&self, gate: Option<Arc<TerminalWriteGate>>) {
        *self.test_control.terminal_write_gate.lock().unwrap() = gate;
    }
}

#[cfg(unix)]
fn publish_state_file(temp: &Path, state_path: &Path) -> Result<(), String> {
    fs::rename(temp, state_path)
        .map_err(|error| format!("failed to publish ACP Run state: {error}"))
}

#[cfg(windows)]
fn publish_state_file(temp: &Path, state_path: &Path) -> Result<(), String> {
    let from = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let to = state_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let retry_deadline = Instant::now() + Duration::from_millis(500);
    loop {
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } != 0
        {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        let retryable = matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33));
        if !retryable || Instant::now() >= retry_deadline {
            return Err(format!("failed to publish ACP Run state: {error}"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(not(any(unix, windows)))]
fn publish_state_file(_temp: &Path, _state_path: &Path) -> Result<(), String> {
    Err("ACP Run durable state is unsupported on this platform".to_string())
}

fn sync_parent(_dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(_dir)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("failed to sync ACP Run state dir: {error}"))?;
    }
    Ok(())
}

fn validate_durable_record(record: &DurableRunRecord) -> Result<(), String> {
    if record.schema_version != STORE_SCHEMA_VERSION {
        return Err("ACP Run durable record schema is invalid".to_string());
    }
    validate_coding_agent_run_snapshot(&record.snapshot(0))
        .map_err(|error| format!("ACP Run durable snapshot is invalid: {error}"))?;
    if (record.dispatch_phase == DurableDispatchPhase::Terminal) != record.state.terminal() {
        return Err("ACP Run durable phase/state terminal truth is inconsistent".to_string());
    }
    Ok(())
}

pub(super) fn durable_record_from_snapshot(
    run_id: &str,
    snapshot: &CodingAgentRunSnapshot,
    phase: DurableDispatchPhase,
) -> Result<DurableRunRecord, String> {
    if snapshot.run_id != run_id {
        return Err("ACP Run identity changed unexpectedly".to_string());
    }
    Ok(DurableRunRecord {
        schema_version: STORE_SCHEMA_VERSION,
        run_id: snapshot.run_id.clone(),
        intent_fingerprint: snapshot.intent_fingerprint.clone(),
        authority_fingerprint: snapshot.authority_fingerprint.clone(),
        runtime_project_id: snapshot.runtime_project_id.clone(),
        provider_id: snapshot.provider_id.clone(),
        provider_instance_id: snapshot.provider_instance_id.clone(),
        state: snapshot.state.clone(),
        execution_state: snapshot.execution_state,
        dispatch_phase: phase,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
        terminal: snapshot.terminal.clone(),
    })
}
