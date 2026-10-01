//! Store responsibility of the existing trace subsystem.
use super::*;

// Ordinary writes update accounting incrementally. A full recursive scan exists
// only to discover out-of-process drift, so keep it deliberately low-frequency.
pub(super) const TRACE_STORE_RECONCILE_INTERVAL: Duration = Duration::from_secs(15 * 60);

pub(super) static TRACE_IO_STATE: OnceLock<Mutex<TraceStoreAccounting>> = OnceLock::new();

#[derive(Debug, Clone)]
pub(super) struct TraceDirInfo {
    pub(super) path: PathBuf,
    pub(super) bytes: u64,
    pub(super) modified: SystemTime,
    pub(super) evictable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TraceStoreConfig {
    pub(super) root: PathBuf,
    pub(super) retention: Duration,
    pub(super) budget: u64,
}

#[derive(Debug, Default)]
pub(super) struct TraceStoreAccounting {
    pub(super) config: Option<TraceStoreConfig>,
    pub(super) initialized: bool,
    pub(super) total_bytes: u64,
    pub(super) traces: HashMap<String, TraceDirInfo>,
    pub(super) last_reconcile: Option<Instant>,
    #[cfg(test)]
    pub(super) filesystem_scans: u64,
}

impl TraceStoreAccounting {
    pub(super) fn invalidate(&mut self) {
        self.initialized = false;
        self.last_reconcile = None;
    }
}

pub(super) fn trace_root() -> PathBuf {
    crate::config::tool_request_trace_dir()
}

pub(super) fn trace_retention() -> Duration {
    Duration::from_secs(crate::config::tool_request_trace_retention_hours().saturating_mul(60 * 60))
}

pub(super) fn trace_budget() -> u64 {
    crate::config::tool_request_trace_max_total_bytes()
}

pub(super) fn trace_store_config() -> TraceStoreConfig {
    TraceStoreConfig {
        root: trace_root(),
        retention: trace_retention(),
        budget: trace_budget(),
    }
}

pub(super) fn trace_io_state() -> &'static Mutex<TraceStoreAccounting> {
    TRACE_IO_STATE.get_or_init(|| Mutex::new(TraceStoreAccounting::default()))
}

pub(super) fn require_private_regular_file(
    path: &Path,
    kind: &'static str,
) -> Result<fs::Metadata, TraceReadError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            TraceReadError::new(
                "trace_not_found",
                "trace data is unavailable or has expired",
            )
        } else {
            TraceReadError::new(kind, "trace storage could not be inspected safely")
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(TraceReadError::new(
            kind,
            "trace storage failed the regular-file safety check",
        ));
    }
    Ok(metadata)
}

pub(super) fn require_private_directory(
    path: &Path,
    not_found_kind: &'static str,
) -> Result<(), TraceReadError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            TraceReadError::new(not_found_kind, "trace data is unavailable or has expired")
        } else {
            TraceReadError::new(
                "trace_store_unavailable",
                "trace storage could not be inspected safely",
            )
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(TraceReadError::new(
            "trace_corrupt",
            "trace storage failed the directory safety check",
        ));
    }
    Ok(())
}

pub(super) fn directory_stats(path: &Path) -> io::Result<(u64, SystemTime)> {
    let metadata = fs::metadata(path)?;
    let mut bytes = if metadata.is_file() {
        metadata.len()
    } else {
        0
    };
    let mut modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let (child_bytes, child_modified) = directory_stats(&entry.path())?;
            bytes = bytes.saturating_add(child_bytes);
            if child_modified > modified {
                modified = child_modified;
            }
        }
    }
    Ok((bytes, modified))
}

pub(super) const TRACE_OWNER_MARKER: &str = ".webcodex-tool-trace";

pub(super) fn trace_dir_owned_by_store(path: &Path, active_trace_id: &str) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name == active_trace_id || path.join(TRACE_OWNER_MARKER).is_file()
}

pub(super) fn trace_dirs(root: &Path, active_trace_id: &str) -> io::Result<Vec<TraceDirInfo>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        if !trace_dir_owned_by_store(&path, active_trace_id) {
            continue;
        }
        let (bytes, modified) = directory_stats(&path)?;
        let evictable = path.join(TRACE_OWNER_MARKER).is_file();
        dirs.push(TraceDirInfo {
            path,
            bytes,
            modified,
            evictable,
        });
    }
    Ok(dirs)
}

pub(super) fn create_private_trace_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(super) fn ensure_trace_owner_marker(trace_dir: &Path) -> io::Result<()> {
    let marker = trace_dir.join(TRACE_OWNER_MARKER);
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(marker)?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

pub(super) fn open_private_append(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path)?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

pub(super) fn remove_accounted_trace(accounting: &mut TraceStoreAccounting, trace_id: &str) {
    if let Some(info) = accounting.traces.remove(trace_id) {
        accounting.total_bytes = accounting.total_bytes.saturating_sub(info.bytes);
    }
}

pub(super) fn scan_trace_store(
    accounting: &mut TraceStoreAccounting,
    config: &TraceStoreConfig,
    active_trace_id: &str,
) -> io::Result<(HashMap<String, TraceDirInfo>, u64)> {
    #[cfg(test)]
    {
        accounting.filesystem_scans = accounting.filesystem_scans.saturating_add(1);
    }

    let previous = accounting
        .config
        .as_ref()
        .is_some_and(|previous| previous.root == config.root)
        .then(|| accounting.traces.clone());
    let mut traces = HashMap::new();
    let mut total_bytes = 0_u64;
    for info in trace_dirs(&config.root, active_trace_id)? {
        let Some(trace_id) = info
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        total_bytes = total_bytes.saturating_add(info.bytes);
        traces.insert(trace_id, info);
    }

    // A failed cleanup can remove the owner marker before remove_dir_all reports
    // failure. Keep any process-known residue in the budget until it disappears;
    // never make it evictable without a current owner marker.
    if let Some(previous) = previous {
        for (trace_id, mut info) in previous {
            if traces.contains_key(&trace_id) || !info.path.exists() {
                continue;
            }
            match directory_stats(&info.path) {
                Ok((bytes, modified)) => {
                    info.bytes = bytes;
                    info.modified = modified;
                    info.evictable = false;
                    total_bytes = total_bytes.saturating_add(bytes);
                    traces.insert(trace_id, info);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }

    Ok((traces, total_bytes))
}

pub(super) fn prune_expired_traces(
    accounting: &mut TraceStoreAccounting,
    config: &TraceStoreConfig,
    active_trace_id: &str,
) -> io::Result<()> {
    let now = SystemTime::now();
    let expired = accounting
        .traces
        .iter()
        .filter(|(trace_id, info)| {
            trace_id.as_str() != active_trace_id
                && info.evictable
                && now
                    .duration_since(info.modified)
                    .map(|age| age > config.retention)
                    .unwrap_or(false)
        })
        .map(|(trace_id, _)| trace_id.clone())
        .collect::<Vec<_>>();

    for trace_id in expired {
        let Some(path) = accounting
            .traces
            .get(&trace_id)
            .map(|info| info.path.clone())
        else {
            continue;
        };
        if !path.join(TRACE_OWNER_MARKER).is_file() {
            if let Some(info) = accounting.traces.get_mut(&trace_id) {
                info.evictable = false;
            }
            accounting.invalidate();
            continue;
        }
        match fs::remove_dir_all(&path) {
            Ok(()) => remove_accounted_trace(accounting, &trace_id),
            Err(_) => {
                // Keep the old byte count rather than assuming any partial
                // cleanup succeeded. The next foreground capture reconciles it.
                accounting.invalidate();
            }
        }
    }
    Ok(())
}

pub(super) fn reconcile_trace_store(
    accounting: &mut TraceStoreAccounting,
    config: &TraceStoreConfig,
    active_trace_id: &str,
) -> io::Result<()> {
    fs::create_dir_all(&config.root)?;
    let (traces, total_bytes) = scan_trace_store(accounting, config, active_trace_id)?;
    accounting.config = Some(config.clone());
    accounting.initialized = true;
    accounting.total_bytes = total_bytes;
    accounting.traces = traces;
    accounting.last_reconcile = Some(Instant::now());
    prune_expired_traces(accounting, config, active_trace_id)?;
    Ok(())
}

pub(super) fn accounting_requires_reconcile(
    accounting: &TraceStoreAccounting,
    config: &TraceStoreConfig,
) -> bool {
    !accounting.initialized
        || accounting.config.as_ref() != Some(config)
        || accounting
            .last_reconcile
            .map(|last| last.elapsed() >= TRACE_STORE_RECONCILE_INTERVAL)
            .unwrap_or(true)
}

/// Enforce age and total-byte bounds before one new file/event is persisted.
/// The first operation, configuration changes, invalidation, and low-frequency
/// foreground maintenance rebuild accounting from the filesystem. Ordinary
/// captures use the cached total and trace index without recursively scanning
/// the store. The active trace is never deleted out from under its own write.
pub(super) fn reserve_trace_capacity(
    accounting: &mut TraceStoreAccounting,
    config: &TraceStoreConfig,
    trace_id: &str,
    incoming: u64,
) -> io::Result<bool> {
    fs::create_dir_all(&config.root)?;
    if accounting_requires_reconcile(accounting, config) {
        reconcile_trace_store(accounting, config, trace_id)?;
    }
    if accounting.total_bytes.saturating_add(incoming) <= config.budget {
        return Ok(true);
    }

    let mut candidates = accounting
        .traces
        .iter()
        .filter(|(candidate_id, info)| candidate_id.as_str() != trace_id && info.evictable)
        .map(|(candidate_id, info)| (candidate_id.clone(), info.modified))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(_, modified)| *modified);

    for (candidate_id, _) in candidates {
        if accounting.total_bytes.saturating_add(incoming) <= config.budget {
            break;
        }
        let Some(path) = accounting
            .traces
            .get(&candidate_id)
            .map(|info| info.path.clone())
        else {
            continue;
        };
        // Revalidate ownership immediately before destructive cleanup. This is
        // O(1) and closes the external/manual marker-removal race without
        // reintroducing recursive hot-path traversal.
        if !path.join(TRACE_OWNER_MARKER).is_file() {
            if let Some(info) = accounting.traces.get_mut(&candidate_id) {
                info.evictable = false;
            }
            accounting.invalidate();
            continue;
        }
        match fs::remove_dir_all(&path) {
            Ok(()) => remove_accounted_trace(accounting, &candidate_id),
            Err(error) => {
                // Do not deduct a failed eviction. The unchanged cached byte
                // count is conservative even if remove_dir_all made partial
                // progress; force a filesystem rebuild before the next capture.
                accounting.invalidate();
                return Err(error);
            }
        }
    }

    Ok(accounting.total_bytes.saturating_add(incoming) <= config.budget)
}

pub(super) fn commit_trace_write(
    accounting: &mut TraceStoreAccounting,
    config: &TraceStoreConfig,
    trace_id: &str,
    bytes: u64,
) {
    let now = SystemTime::now();
    let entry = accounting
        .traces
        .entry(trace_id.to_string())
        .or_insert_with(|| TraceDirInfo {
            path: config.root.join(trace_id),
            bytes: 0,
            modified: now,
            evictable: true,
        });
    entry.bytes = entry.bytes.saturating_add(bytes);
    entry.modified = now;
    entry.evictable = true;
    accounting.total_bytes = accounting.total_bytes.saturating_add(bytes);
}

pub(super) fn remove_file_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(super) fn append_event_locked(
    accounting: &mut TraceStoreAccounting,
    config: &TraceStoreConfig,
    trace_id: &str,
    event: &Value,
) -> io::Result<bool> {
    let mut line = serde_json::to_vec(event).map_err(io::Error::other)?;
    line.push(b'\n');
    if !reserve_trace_capacity(accounting, config, trace_id, line.len() as u64)? {
        return Ok(false);
    }
    let trace_dir = config.root.join(trace_id);
    create_private_trace_dir(&trace_dir)?;
    ensure_trace_owner_marker(&trace_dir)?;
    let mut file = open_private_append(&trace_dir.join("events.jsonl"))?;
    if let Err(error) = file.write_all(&line) {
        // write_all may have appended a prefix. Force the next foreground
        // operation to rescan rather than undercount an uncertain file length.
        accounting.invalidate();
        return Err(error);
    }
    commit_trace_write(accounting, config, trace_id, line.len() as u64);
    Ok(true)
}

pub(super) fn persist_metadata_event_with_config(
    trace_id: &str,
    event: Value,
    config: &TraceStoreConfig,
) -> io::Result<bool> {
    let mut accounting = trace_io_state()
        .lock()
        .map_err(|_| io::Error::other("tool trace I/O lock poisoned"))?;
    append_event_locked(&mut accounting, config, trace_id, &event)
}

#[cfg(test)]
pub(super) fn persist_metadata_event(trace_id: &str, event: Value) -> io::Result<bool> {
    let config = trace_store_config();
    persist_metadata_event_with_config(trace_id, event, &config)
}

pub(super) fn persist_payload_with_config(
    trace_id: &str,
    phase: &str,
    value: &Value,
    mut event: Value,
    config: &TraceStoreConfig,
) -> io::Result<Option<(usize, usize, String, String)>> {
    let raw = serde_json::to_vec(value).map_err(io::Error::other)?;
    let digest = sha256_hex(&raw);
    let compressed = zstd::stream::encode_all(&raw[..], 3)?;
    let file_name = format!("{}-{}.json.zst", Uuid::new_v4(), safe_phase(phase));
    let relative_path = format!("payloads/{file_name}");
    merge_event_fields(
        &mut event,
        json!({
            "phase": phase,
            "payload_bytes": raw.len(),
            "compressed_bytes": compressed.len(),
            "payload_sha256": digest,
            "payload_path": relative_path,
            "encoding": "json+zstd",
        }),
    );
    let mut event_line = serde_json::to_vec(&event).map_err(io::Error::other)?;
    event_line.push(b'\n');
    let incoming = (compressed.len() + event_line.len()) as u64;

    let mut accounting = trace_io_state()
        .lock()
        .map_err(|_| io::Error::other("tool trace I/O lock poisoned"))?;
    if !reserve_trace_capacity(&mut accounting, config, trace_id, incoming)? {
        return Ok(None);
    }
    let trace_dir = config.root.join(trace_id);
    let payload_dir = trace_dir.join("payloads");
    create_private_trace_dir(&trace_dir)?;
    ensure_trace_owner_marker(&trace_dir)?;
    create_private_trace_dir(&payload_dir)?;
    let final_path = payload_dir.join(&file_name);
    let temp_path = payload_dir.join(format!(".{file_name}.tmp"));
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut temp_file = options.open(&temp_path)?;
    #[cfg(unix)]
    if let Err(error) = temp_file.set_permissions(fs::Permissions::from_mode(0o600)) {
        drop(temp_file);
        if remove_file_if_present(&temp_path).is_err() {
            accounting.invalidate();
        }
        return Err(error);
    }
    if let Err(error) = temp_file.write_all(&compressed) {
        drop(temp_file);
        if remove_file_if_present(&temp_path).is_err() {
            accounting.invalidate();
        }
        return Err(error);
    }
    if let Err(error) = temp_file.flush() {
        drop(temp_file);
        if remove_file_if_present(&temp_path).is_err() {
            accounting.invalidate();
        }
        return Err(error);
    }
    drop(temp_file);
    if let Err(error) = fs::rename(&temp_path, &final_path) {
        if remove_file_if_present(&temp_path).is_err() {
            accounting.invalidate();
        }
        return Err(error);
    }

    let mut event_file = match open_private_append(&trace_dir.join("events.jsonl")) {
        Ok(file) => file,
        Err(error) => {
            if remove_file_if_present(&final_path).is_err() {
                accounting.invalidate();
            }
            return Err(error);
        }
    };
    if let Err(error) = event_file.write_all(&event_line) {
        // The append may have written a prefix. Even if payload cleanup succeeds,
        // the store total is uncertain until the next reconciliation.
        let _ = remove_file_if_present(&final_path);
        accounting.invalidate();
        return Err(error);
    }

    commit_trace_write(&mut accounting, config, trace_id, incoming);
    Ok(Some((raw.len(), compressed.len(), digest, relative_path)))
}

#[cfg(test)]
pub(super) fn persist_payload(
    trace_id: &str,
    phase: &str,
    value: &Value,
) -> io::Result<Option<(usize, usize, String, String)>> {
    let config = trace_store_config();
    persist_payload_with_config(
        trace_id,
        phase,
        value,
        base_event(trace_id, "tool_trace_payload_captured"),
        &config,
    )
}
