//! Bounded durable state, recovery and conditional retention.
use super::*;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Debug, Clone)]
pub(crate) struct DetachedJobStore {
    pub(super) root: PathBuf,
}

#[derive(Debug)]
pub(super) enum PrepareOutcome {
    First(DetachedJobRecord),
    Existing(DetachedJobRecord),
}

impl DetachedJobStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Return the durable state root for one stable Runner identity on one Server.
    ///
    /// The per-user state base can host Runners connected to different Servers,
    /// while `client_id` is only server-local identity. Bind the detached
    /// namespace to the normalized Server endpoint plus stable `client_id` so
    /// unrelated Runners do not share job ids or the bounded record quota.
    /// Local config/project path spelling is deliberately excluded: moving a
    /// profile or restarting it with an equivalent path must not orphan the
    /// supervisor-owned state that the Server still attributes to this Runner.
    pub(crate) fn default_root_for_runner(
        client_id: &str,
        server_url: &str,
    ) -> Result<PathBuf, String> {
        validate_identity("client_id", client_id, 128)?;
        let server_url = server_url.trim().trim_end_matches('/');
        if server_url.is_empty() {
            return Err("detached Job profile server_url cannot be empty".to_string());
        }
        let mut hasher = Sha256::new();
        hasher.update(b"webcodex-detached-store-runner-v1\0");
        hasher.update(client_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(server_url.as_bytes());
        let namespace = format!("{:x}", hasher.finalize());
        Ok(
            webcodex_runner_config::paths::default_client_state_base_dir()?
                .join("runner-detached-jobs-v1")
                .join(namespace),
        )
    }

    pub(super) fn job_dir(&self, job_id: &str) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(job_id.as_bytes());
        let digest = format!("{:x}", hasher.finalize());
        self.root.join(digest)
    }

    #[cfg(test)]
    pub(super) fn state_path_for_job(&self, job_id: &str) -> PathBuf {
        self.job_dir(job_id).join(STATE_FILE)
    }

    pub(crate) fn read(&self, job_id: &str) -> Result<DetachedJobRecord, String> {
        let job_dir = self.job_dir(job_id);
        let record = read_state_record_locked(&job_dir)?;
        if record.job_id != job_id {
            return Err("detached Job state job_id does not match its lookup identity".to_string());
        }
        Ok(record)
    }

    pub(crate) fn scan_for_client(
        &self,
        client_id: &str,
    ) -> Result<Vec<DetachedJobRecord>, String> {
        validate_identity("client_id", client_id, 128)?;
        match fs::symlink_metadata(&self.root) {
            Ok(_) => reject_symlink_or_non_dir(&self.root, "detached Job state root")?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(format!(
                    "failed to inspect detached Job state root {}: {error}",
                    self.root.display()
                ))
            }
        }
        let _root_lock = exclusive_lock(&self.root.join(ROOT_LOCK_FILE), true)?;
        let mut records = Vec::new();
        let mut job_dirs = 0usize;
        for entry in fs::read_dir(&self.root)
            .map_err(|error| format!("failed to list detached Job state root: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("failed to inspect detached Job state: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("failed to inspect detached Job state entry: {error}"))?;
            if file_type.is_symlink() {
                return Err("detached Job state root contains a symlink entry".to_string());
            }
            if !file_type.is_dir() {
                if entry.file_name() == ROOT_LOCK_FILE {
                    continue;
                }
                return Err(
                    "detached Job state root contains an unexpected non-directory entry"
                        .to_string(),
                );
            }
            job_dirs = job_dirs.saturating_add(1);
            if job_dirs > DETACHED_STATE_MAX_RECORDS {
                return Err(format!(
                    "detached Job state root exceeds {DETACHED_STATE_MAX_RECORDS} records"
                ));
            }
            let record = read_state_record_locked(&entry.path())?;
            if self.job_dir(&record.job_id) != entry.path() {
                return Err(
                    "detached Job state directory does not match its job identity".to_string(),
                );
            }
            if record.client_id == client_id {
                records.push(record);
            }
        }
        records.sort_by(|left, right| left.job_id.cmp(&right.job_id));
        Ok(records)
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn reconcile_after_runner_restart(
        &self,
        record: DetachedJobRecord,
    ) -> Result<Option<DetachedJobRecord>, String> {
        validate_record(&record)?;
        if record.phase == DetachedJobPhase::Terminal {
            return Ok(Some(record));
        }
        if record.ownership_accepted_at_unix_ms.is_none() {
            // A replacement Runner must never turn a pre-accept residue into a
            // respawn. Prepared has no supervisor and is therefore definitely
            // not_started. SupervisorStarted may still be draining the old
            // Runner's one-shot handoff pipe; while that exact supervisor is live
            // we retain and observe the same record. Only after its exact native
            // identity/lifetime lock proves dead may recovery persist
            // handoff_failed. This closes the narrow accept-byte race without
            // making the residue invisible.
            if record.phase == DetachedJobPhase::Prepared {
                let execution_id = record.execution_id.clone();
                return self
                    .update(&record.job_id, &execution_id, |current| {
                        set_terminal(
                            current,
                            "handoff_failed",
                            None,
                            Some("Runner restarted before detached ownership acceptance"),
                            current.created_at_unix_ms,
                        );
                        Ok(())
                    })
                    .map(Some);
            }
            let supervisor = record.supervisor.as_ref().ok_or_else(|| {
                "pre-accept detached Job is missing supervisor identity".to_string()
            })?;
            if detached_process_identity_is_live(
                &self.job_dir(&record.job_id).join(SUPERVISOR_LOCK_FILE),
                supervisor,
            )? {
                return Ok(Some(record));
            }
            let refreshed = self.read(&record.job_id)?;
            if refreshed.phase == DetachedJobPhase::Terminal
                || refreshed.ownership_accepted_at_unix_ms.is_some()
            {
                return Ok(Some(refreshed));
            }
            let execution_id = refreshed.execution_id.clone();
            return self
                .update(&refreshed.job_id, &execution_id, |current| {
                    set_terminal(
                        current,
                        "handoff_failed",
                        None,
                        Some("detached supervisor exited before ownership acceptance"),
                        current.created_at_unix_ms,
                    );
                    Ok(())
                })
                .map(Some);
        }
        let supervisor = record
            .supervisor
            .as_ref()
            .ok_or_else(|| "accepted detached Job is missing supervisor identity".to_string())?;
        if detached_process_identity_is_live(
            &self.job_dir(&record.job_id).join(SUPERVISOR_LOCK_FILE),
            supervisor,
        )? {
            return Ok(Some(record));
        }
        let refreshed = self.read(&record.job_id)?;
        if refreshed.phase == DetachedJobPhase::Terminal {
            return Ok(Some(refreshed));
        }
        let started_at = refreshed
            .payload_started_at_unix_ms
            .or(refreshed.ownership_accepted_at_unix_ms)
            .unwrap_or(refreshed.created_at_unix_ms);
        let execution_id = refreshed.execution_id.clone();
        self.update(&refreshed.job_id, &execution_id, |current| {
            if current.phase != DetachedJobPhase::Terminal {
                set_terminal(
                    current,
                    "supervisor_lost",
                    None,
                    Some(
                        "detached supervisor is no longer live after Runner restart reconciliation",
                    ),
                    started_at,
                );
            }
            Ok(())
        })
        .map(Some)
    }

    pub(crate) fn request_stop(
        &self,
        job_id: &str,
        execution_id: &str,
    ) -> Result<DetachedJobRecord, String> {
        self.update(job_id, execution_id, |record| {
            if record.ownership_accepted_at_unix_ms.is_none() {
                return Err(
                    "detached Job cannot be stopped before ownership acceptance".to_string()
                );
            }
            record.stop_requested = true;
            Ok(())
        })
    }

    pub(super) fn reclaim_expired_terminal_records_locked(
        &self,
        now_unix_ms: i64,
    ) -> Result<usize, String> {
        let mut retained = 0usize;
        for entry in fs::read_dir(&self.root).map_err(|error| {
            format!("failed to list detached Job state root for reclamation: {error}")
        })? {
            let entry = entry.map_err(|error| {
                format!("failed to inspect detached Job state during reclamation: {error}")
            })?;
            let file_type = entry.file_type().map_err(|error| {
                format!("failed to inspect detached Job state entry during reclamation: {error}")
            })?;
            if file_type.is_symlink() {
                return Err("detached Job reclamation found a symlink state entry".to_string());
            }
            if !file_type.is_dir() {
                if entry.file_name() == ROOT_LOCK_FILE {
                    continue;
                }
                return Err(
                    "detached Job reclamation found an unexpected non-directory state entry"
                        .to_string(),
                );
            }
            let job_dir = entry.path();
            let initial = read_state_record_locked(&job_dir)?;
            if self.job_dir(&initial.job_id) != job_dir {
                return Err(
                    "detached Job reclamation state directory does not match durable job identity"
                        .to_string(),
                );
            }
            let terminal_expired = initial.terminal.as_ref().is_some_and(|terminal| {
                now_unix_ms.saturating_sub(terminal.completed_at_unix_ms) >= TERMINAL_RETENTION_MS
            });
            if !terminal_expired {
                retained = retained.saturating_add(1);
                continue;
            }
            self.reclaim_terminal_job_dir_locked(&job_dir, &initial, now_unix_ms)?;
        }
        Ok(retained)
    }

    pub(super) fn reclaim_terminal_job_dir_locked(
        &self,
        job_dir: &Path,
        expected: &DetachedJobRecord,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        reject_symlink_or_non_dir(job_dir, "detached Job reclamation directory")?;
        let state_lock_path = job_dir.join(STATE_LOCK_FILE);
        let state_lock = exclusive_lock(&state_lock_path, true)?;
        let current: DetachedJobRecord = read_json_bounded(
            &job_dir.join(STATE_FILE),
            DETACHED_STATE_MAX_BYTES,
            "detached Job state",
        )?;
        validate_record(&current)?;
        if current.job_id != expected.job_id || current.execution_id != expected.execution_id {
            return Err("detached Job reclamation identity changed under lock".to_string());
        }
        let terminal = current
            .terminal
            .as_ref()
            .ok_or_else(|| "detached Job reclamation refuses an active state record".to_string())?;
        if now_unix_ms.saturating_sub(terminal.completed_at_unix_ms) < TERMINAL_RETENTION_MS {
            return Err("detached Job reclamation retention window changed under lock".to_string());
        }
        let mut removable = Vec::new();
        for child in fs::read_dir(job_dir).map_err(|error| {
            format!("failed to list detached Job directory for reclamation: {error}")
        })? {
            let child = child.map_err(|error| {
                format!("failed to inspect detached Job reclamation child: {error}")
            })?;
            let file_type = child.file_type().map_err(|error| {
                format!("failed to inspect detached Job reclamation child type: {error}")
            })?;
            if file_type.is_symlink() || !file_type.is_file() {
                return Err(
                    "detached Job reclamation found a symlink or non-file child; refusing deletion"
                        .to_string(),
                );
            }
            let name = child.file_name();
            let known = name == STATE_FILE
                || name == STATE_LOCK_FILE
                || name == STATE_TEMP_FILE
                || name == SUPERVISOR_LOCK_FILE
                || name == TREE_LOCK_FILE;
            if !known {
                return Err(
                    "detached Job reclamation found an unexpected state child; refusing deletion"
                        .to_string(),
                );
            }
            removable.push(child.path());
        }
        for path in removable {
            fs::remove_file(&path).map_err(|error| {
                format!(
                    "failed to remove expired detached Job state {}: {error}",
                    path.display()
                )
            })?;
        }
        // The exact state lock remains open through remove_dir. A racing updater
        // can only fail closed against the disappearing path; it cannot turn a
        // terminal record back into an active execution.
        fs::remove_dir(job_dir).map_err(|error| {
            format!(
                "failed to remove expired detached Job directory {}: {error}",
                job_dir.display()
            )
        })?;
        drop(state_lock);
        sync_directory(&self.root)?;
        Ok(())
    }

    pub(super) fn prepare(&self, request: &DetachedStartRequest) -> Result<PrepareOutcome, String> {
        validate_start_request(request)?;
        ensure_private_dir(&self.root)?;
        let _root_lock = exclusive_lock(&self.root.join(ROOT_LOCK_FILE), true)?;
        let job_dir = self.job_dir(&request.job_id);
        match fs::symlink_metadata(&job_dir) {
            Ok(_) => {
                reject_symlink_or_non_dir(&job_dir, "detached Job directory")?;
                let existing = self.read(&request.job_id)?;
                validate_existing_request(&existing, request)?;
                return Ok(PrepareOutcome::Existing(existing));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to inspect detached Job state directory before prepare: {error}"
                ))
            }
        }
        let retained = self.reclaim_expired_terminal_records_locked(unix_ms())?;
        if retained >= DETACHED_STATE_MAX_RECORDS {
            return Err(format!(
                "detached Job state root is full; maximum is {DETACHED_STATE_MAX_RECORDS} records"
            ));
        }
        fs::create_dir(&job_dir)
            .map_err(|error| format!("failed to create detached Job state directory: {error}"))?;
        set_private_dir_permissions(&job_dir)?;

        let record = DetachedJobRecord {
            schema_version: DETACHED_STATE_SCHEMA_VERSION,
            job_id: request.job_id.clone(),
            execution_id: execution_id_for(request),
            request_id: request.request_id.clone(),
            client_id: request.client_id.clone(),
            runner_instance_id: request.runner_instance_id.clone(),
            context: request.context.clone(),
            phase: DetachedJobPhase::Prepared,
            // JobManager has already projected agent_queued at sequence 1 before
            // detached durable handoff starts. Reserve that sequence so every
            // durable transition, including a pre-accept failure, is strictly
            // newer than the public queued snapshot.
            update_seq: 1,
            stop_requested: false,
            created_at_unix_ms: unix_ms(),
            supervisor_started_at_unix_ms: None,
            ownership_accepted_at_unix_ms: None,
            payload_started_at_unix_ms: None,
            supervisor: None,
            tree_leader: None,
            stdout: DetachedOutputState::default(),
            stderr: DetachedOutputState::default(),
            terminal: None,
        };
        validate_record(&record)?;
        if let Err(error) =
            atomic_write_json(&job_dir.join(STATE_FILE), &record, DETACHED_STATE_MAX_BYTES)
        {
            let _ = fs::remove_dir(&job_dir);
            return Err(error);
        }
        Ok(PrepareOutcome::First(record))
    }

    pub(super) fn update<F>(
        &self,
        job_id: &str,
        execution_id: &str,
        update: F,
    ) -> Result<DetachedJobRecord, String>
    where
        F: FnOnce(&mut DetachedJobRecord) -> Result<(), String>,
    {
        let job_dir = self.job_dir(job_id);
        reject_symlink_or_non_dir(&job_dir, "detached Job directory")?;
        let _guard = exclusive_lock(&job_dir.join(STATE_LOCK_FILE), true)?;
        let mut record: DetachedJobRecord = read_json_bounded(
            &job_dir.join(STATE_FILE),
            DETACHED_STATE_MAX_BYTES,
            "detached Job state",
        )?;
        validate_record(&record)?;
        if record.job_id != job_id || record.execution_id != execution_id {
            return Err("detached Job state identity mismatch".to_string());
        }
        let previous = record.clone();
        if previous.phase == DetachedJobPhase::Terminal {
            return Ok(previous);
        }
        update(&mut record)?;
        if record == previous {
            return Ok(previous);
        }
        record.update_seq = previous
            .update_seq
            .checked_add(1)
            .ok_or_else(|| "detached Job update sequence overflow".to_string())?;
        validate_transition(&previous, &record)?;
        validate_record(&record)?;
        atomic_write_json(&job_dir.join(STATE_FILE), &record, DETACHED_STATE_MAX_BYTES)?;
        Ok(record)
    }
}

pub(super) fn reject_symlink_or_non_dir(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label} {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("{label} must be a real directory, not a symlink"));
    }
    Ok(())
}

pub(super) fn read_state_record_locked(job_dir: &Path) -> Result<DetachedJobRecord, String> {
    reject_symlink_or_non_dir(job_dir, "detached Job directory")?;
    // Durable updates replace STATE_FILE atomically, which intentionally changes
    // its inode. Serialize pathname revalidation with those writers so a trusted
    // atomic replacement cannot be mistaken for same-user path tampering. The
    // O_NOFOLLOW + dev/inode checks in read_json_bounded remain authoritative.
    let _guard = exclusive_lock(&job_dir.join(STATE_LOCK_FILE), true)?;
    let record: DetachedJobRecord = read_json_bounded(
        &job_dir.join(STATE_FILE),
        DETACHED_STATE_MAX_BYTES,
        "detached Job state",
    )?;
    validate_record(&record)?;
    Ok(record)
}

pub(super) fn read_json_bounded<T: for<'de> Deserialize<'de>>(
    path: &Path,
    max_bytes: usize,
    label: &str,
) -> Result<T, String> {
    #[cfg(unix)]
    let bytes = {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|error| format!("failed to open {label} {}: {error}", path.display()))?;
        validate_open_regular_file(&file, path, label)?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("failed to inspect open {label}: {error}"))?;
        if metadata.len() > max_bytes as u64 {
            return Err(format!("{label} exceeds {max_bytes} bytes"));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        Read::by_ref(&mut file)
            .take((max_bytes + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
        if bytes.len() > max_bytes {
            return Err(format!("{label} exceeds {max_bytes} bytes"));
        }
        bytes
    };
    #[cfg(not(unix))]
    let bytes = {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("failed to inspect {label} {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("{label} must be a regular non-symlink file"));
        }
        if metadata.len() > max_bytes as u64 {
            return Err(format!("{label} exceeds {max_bytes} bytes"));
        }
        fs::read(path)
            .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?
    };
    serde_json::from_slice(&bytes).map_err(|error| format!("corrupt {label}: {error}"))
}
