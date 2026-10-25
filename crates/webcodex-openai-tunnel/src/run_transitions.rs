//! Append-only authority transitions rooted in the original protected header.
//! A torn or corrupt transition is never truncated or repaired automatically.
use super::*;

const MAGIC: &[u8; 8] = b"WCTTRN01";
const RECORD_BYTES: usize = 152;
const MAX_TRANSITIONS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Transition {
    expected_revision: u64,
    old_binding: [u8; 32],
    new_binding: [u8; 32],
    cutoff_seconds: u64,
}

pub(super) struct ProtectedHistory {
    pub(super) original_binding: [u8; 32],
    transitions: Vec<Transition>,
    tail_hash: [u8; 32],
    records: Vec<[u8; RECORD_BYTES]>,
    partial_tail: Vec<u8>,
}

impl ProtectedHistory {
    pub(super) fn current_binding(&self) -> [u8; 32] {
        self.transitions
            .last()
            .map_or(self.original_binding, |item| item.new_binding)
    }

    pub(super) fn cutoff_seconds(&self) -> Option<u64> {
        self.transitions.last().map(|item| item.cutoff_seconds)
    }

    fn revision(&self) -> u64 {
        self.transitions.len() as u64
    }

    pub(super) fn contains_binding_text(&self, text: &str) -> bool {
        std::iter::once(self.original_binding)
            .chain(self.transitions.iter().map(|item| item.new_binding))
            .any(|binding| format!("{:x}", Sha256::digest(binding)) == text)
    }
}

pub(super) fn read_history(lock: &mut File) -> Result<Option<ProtectedHistory>, &'static str> {
    read_history_inner(lock, false)
}

fn read_history_inner(
    lock: &mut File,
    allow_partial: bool,
) -> Result<Option<ProtectedHistory>, &'static str> {
    let length = lock.metadata().map_err(|_| "tunnel_fence_io")?.len();
    if length == 0 {
        return Ok(None);
    }
    let base = PROTECTED_HISTORY_BYTES as u64;
    if length < base
        || length > base + (MAX_TRANSITIONS * RECORD_BYTES) as u64
        || (!allow_partial && !(length - base).is_multiple_of(RECORD_BYTES as u64))
    {
        return Err("tunnel_protected_history_invalid");
    }
    lock.seek(SeekFrom::Start(0))
        .map_err(|_| "tunnel_fence_io")?;
    let mut header = [0; PROTECTED_HISTORY_BYTES];
    lock.read_exact(&mut header)
        .map_err(|_| "tunnel_fence_io")?;
    if &header[..8] != PROTECTED_HISTORY_MAGIC
        || Sha256::digest(&header[..40]).as_slice() != &header[40..]
    {
        return Err("tunnel_protected_history_invalid");
    }
    let mut history = ProtectedHistory {
        original_binding: header[8..40].try_into().unwrap(),
        transitions: Vec::new(),
        tail_hash: Sha256::digest(header).into(),
        records: Vec::new(),
        partial_tail: Vec::new(),
    };
    for _ in 0..(length - base) / RECORD_BYTES as u64 {
        let mut record = [0; RECORD_BYTES];
        lock.read_exact(&mut record)
            .map_err(|_| "tunnel_fence_io")?;
        let transition = Transition {
            expected_revision: u64::from_le_bytes(record[8..16].try_into().unwrap()),
            old_binding: record[16..48].try_into().unwrap(),
            new_binding: record[48..80].try_into().unwrap(),
            cutoff_seconds: u64::from_le_bytes(record[80..88].try_into().unwrap()),
        };
        if &record[..8] != MAGIC
            || Sha256::digest(&record[..120]).as_slice() != &record[120..]
            || record[88..120] != history.tail_hash
            || transition.expected_revision != history.revision()
            || transition.old_binding != history.current_binding()
            || transition.old_binding == transition.new_binding
            || transition.cutoff_seconds <= history.cutoff_seconds().unwrap_or(0)
        {
            return Err("tunnel_protected_history_invalid");
        }
        history.tail_hash = Sha256::digest(record).into();
        history.transitions.push(transition);
        history.records.push(record);
    }
    let remainder = (length - base) as usize % RECORD_BYTES;
    history.partial_tail.resize(remainder, 0);
    lock.read_exact(&mut history.partial_tail)
        .map_err(|_| "tunnel_fence_io")?;
    Ok(Some(history))
}

fn intent_directory(active: &Path) -> PathBuf {
    active.with_extension("transitions")
}

fn intent_name(revision: usize) -> String {
    format!("{revision:02}.intent")
}

fn private_file_metadata(path: &Path, max_bytes: usize) -> Result<fs::Metadata, &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "tunnel_transition_intent_missing")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > max_bytes as u64 {
        return Err("tunnel_transition_intent_invalid");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 || meta.mode() & 0o077 != 0 {
            return Err("tunnel_transition_intent_invalid");
        }
    }
    Ok(meta)
}

/// All committed transitions require their unchanged published intent. The
/// only additional published file may be the next exact revision. A staging
/// file is not authorization, but still blocks automatic startup.
pub(super) fn check_intents(
    active: &Path,
    history: &ProtectedHistory,
    allow_pending: bool,
) -> Result<(Option<[u8; RECORD_BYTES]>, bool), &'static str> {
    let directory = intent_directory(active);
    let meta = match fs::symlink_metadata(&directory) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if history.records.is_empty() && history.partial_tail.is_empty() {
                return Ok((None, false));
            }
            return Err("tunnel_transition_intent_missing");
        }
        Err(_) => return Err("tunnel_transition_intent_invalid"),
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("tunnel_transition_intent_invalid");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o077 != 0 {
            return Err("tunnel_transition_intent_invalid");
        }
    }
    let mut seen = [false; MAX_TRANSITIONS];
    let mut pending = None;
    let mut preparing = false;
    for (count, entry) in fs::read_dir(&directory)
        .map_err(|_| "tunnel_transition_intent_invalid")?
        .enumerate()
    {
        if count > MAX_TRANSITIONS {
            return Err("tunnel_transition_capacity");
        }
        let entry = entry.map_err(|_| "tunnel_transition_intent_invalid")?;
        if entry.file_name() == ".prepare" {
            private_file_metadata(&entry.path(), RECORD_BYTES)?;
            preparing = true;
            continue;
        }
        let name = entry.file_name();
        let revision: usize = name
            .to_str()
            .and_then(|name| name.strip_suffix(".intent"))
            .and_then(|value| value.parse().ok())
            .ok_or("tunnel_transition_intent_invalid")?;
        if revision >= MAX_TRANSITIONS
            || name.to_str() != Some(intent_name(revision).as_str())
            || revision > history.records.len()
        {
            return Err("tunnel_transition_intent_invalid");
        }
        if private_file_metadata(&entry.path(), RECORD_BYTES)?.len() != RECORD_BYTES as u64 {
            return Err("tunnel_transition_intent_invalid");
        }
        let mut bytes = [0; RECORD_BYTES];
        private_options()
            .open(entry.path())
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(|_| "tunnel_transition_intent_invalid")?;
        if &bytes[..8] != MAGIC || Sha256::digest(&bytes[..120]).as_slice() != &bytes[120..] {
            return Err("tunnel_transition_intent_invalid");
        }
        if revision < history.records.len() {
            if bytes != history.records[revision] {
                return Err("tunnel_transition_intent_mismatch");
            }
            seen[revision] = true;
        } else {
            pending = Some(bytes);
        }
    }
    if seen[..history.records.len()].iter().any(|value| !value) {
        return Err("tunnel_transition_intent_missing");
    }
    if !history.partial_tail.is_empty() && pending.is_none() {
        return Err("tunnel_transition_intent_missing");
    }
    if (!allow_pending && (pending.is_some() || preparing)) || (pending.is_some() && preparing) {
        return Err("tunnel_transition_pending");
    }
    Ok((pending, preparing))
}

/// `.prepare` is explicitly uncommitted staging. It may be replaced only under
/// the identity lock; published per-revision intent files are never replaced.
fn publish_intent(
    active: &Path,
    revision: usize,
    record: &[u8; RECORD_BYTES],
) -> Result<(), &'static str> {
    let directory = intent_directory(active);
    private_directory(&directory)?;
    let destination = directory.join(intent_name(revision));
    if !missing(&destination)? {
        return Err("tunnel_transition_intent_mismatch");
    }
    let staging = directory.join(".prepare");
    if !missing(&staging)? {
        private_file_metadata(&staging, RECORD_BYTES)?;
    }
    let mut file = private_options()
        .create(true)
        .truncate(false)
        .open(&staging)
        .map_err(|_| "tunnel_transition_uncertain")?;
    file.set_len(0)
        .and_then(|_| file.write_all(record))
        .and_then(|_| file.sync_all())
        .map_err(|_| "tunnel_transition_uncertain")?;
    drop(file);
    fs::rename(&staging, &destination).map_err(|_| "tunnel_transition_uncertain")?;
    sync_directory(&directory).map_err(|_| "tunnel_transition_uncertain")
}

fn encode(transition: &Transition, previous_hash: [u8; 32]) -> [u8; RECORD_BYTES] {
    let mut record = [0; RECORD_BYTES];
    record[..8].copy_from_slice(MAGIC);
    record[8..16].copy_from_slice(&transition.expected_revision.to_le_bytes());
    record[16..48].copy_from_slice(&transition.old_binding);
    record[48..80].copy_from_slice(&transition.new_binding);
    record[80..88].copy_from_slice(&transition.cutoff_seconds.to_le_bytes());
    record[88..120].copy_from_slice(&previous_hash);
    let checksum = Sha256::digest(&record[..120]);
    record[120..].copy_from_slice(&checksum);
    record
}

fn append_with(
    lock: &mut File,
    record: &[u8; RECORD_BYTES],
    offset: usize,
    write_and_sync: impl FnOnce(&mut File, &[u8]) -> std::io::Result<()>,
) -> Result<(), &'static str> {
    lock.seek(SeekFrom::End(0))
        .map_err(|_| "tunnel_transition_uncertain")?;
    write_and_sync(lock, &record[offset..]).map_err(|_| "tunnel_transition_uncertain")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CredentialTransition {
    pub revision: u64,
    pub cutoff_seconds: u64,
    pub replayed: bool,
}

impl RunFence {
    /// Persist operator authorization only; this does not update configuration,
    /// establish the UTC trust premise, or prove credential-only target equality.
    /// The higher-level TunnelClient API must establish those preconditions.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn authorize_credential_transition(
        root: &Path,
        origin: &str,
        tunnel: &str,
        old_binding: [u8; 32],
        new_binding: [u8; 32],
        cutoff_seconds: u64,
        expected_revision: u64,
    ) -> Result<CredentialTransition, &'static str> {
        // Never create a namespace as a side effect of authorizing a transition.
        if missing(&paths(root, origin, tunnel).1)? {
            return Err("tunnel_protected_history_missing");
        }
        let (path, mut lock) = lock(root, origin, tunnel)?;
        if !missing(&path)? {
            return Err("tunnel_transition_active_owner");
        }
        let history =
            read_history_inner(&mut lock, true)?.ok_or("tunnel_protected_history_missing")?;
        check_archives(&archive_directory(&path)?, None, false, Some(&history))?;
        let (pending, preparing) = check_intents(&path, &history, true)?;
        let _journal = Receipts::open_for_transition(
            &path.with_extension("receipts"),
            history.original_binding,
        )?;
        let requested = Transition {
            expected_revision,
            old_binding,
            new_binding,
            cutoff_seconds,
        };
        if history.transitions.last() == Some(&requested) {
            if pending.is_some() || preparing || !history.partial_tail.is_empty() {
                return Err("tunnel_transition_pending");
            }
            // A complete append whose sync result was uncertain is confirmed by
            // an exact retry. No bytes are rewritten or receipts repaired.
            lock.sync_all().map_err(|_| "tunnel_transition_uncertain")?;
            sync_directory(&intent_directory(&path)).map_err(|_| "tunnel_transition_uncertain")?;
            return Ok(CredentialTransition {
                revision: history.revision(),
                cutoff_seconds,
                replayed: true,
            });
        }
        if expected_revision != history.revision() {
            return Err("tunnel_transition_revision_changed");
        }
        if old_binding != history.current_binding() || new_binding == old_binding {
            return Err("tunnel_transition_binding_changed");
        }
        if cutoff_seconds <= history.cutoff_seconds().unwrap_or(0) {
            return Err("tunnel_transition_cutoff_not_increasing");
        }
        if history.transitions.len() >= MAX_TRANSITIONS {
            return Err("tunnel_transition_capacity");
        }
        let record = encode(&requested, history.tail_hash);
        let replayed = pending.is_some();
        match pending {
            Some(saved) if saved != record => return Err("tunnel_transition_intent_mismatch"),
            Some(_) => {
                if !record.starts_with(&history.partial_tail) {
                    return Err("tunnel_transition_partial_mismatch");
                }
                // Reconfirm publication durability before completing an append.
                sync_directory(&intent_directory(&path))
                    .map_err(|_| "tunnel_transition_uncertain")?;
            }
            None => publish_intent(&path, history.transitions.len(), &record)?,
        }
        append_with(
            &mut lock,
            &record,
            history.partial_tail.len(),
            |file, bytes| {
                file.write_all(bytes)?;
                file.sync_all()
            },
        )?;
        Ok(CredentialTransition {
            revision: expected_revision + 1,
            cutoff_seconds,
            replayed,
        })
    }
}

#[cfg(test)]
#[path = "run_transition_tests.rs"]
mod tests;
