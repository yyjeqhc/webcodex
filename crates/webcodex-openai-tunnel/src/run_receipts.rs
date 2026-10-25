//! Durable admission tombstones, not task results or a distributed transaction.
use super::{private_options, sync_directory};
use crate::Error;
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Arc, Mutex},
};

const HEADER: &[u8; 8] = b"WCTRCP01";
const HEADER_BYTES: u64 = 40;
const RECORD_BYTES: u64 = 64;
// No automatic eviction: deleting a receipt would permit an old effect to replay.
const MAX_RECEIPTS: usize = 1_000_000;

#[derive(Clone)]
pub(crate) struct Receipts(Arc<Mutex<Journal>>);
struct Journal {
    file: File,
    admitted: HashSet<[u8; 32]>,
    failed: bool,
}

impl Receipts {
    pub(super) fn open(
        path: &Path,
        binding: [u8; 32],
        required: bool,
    ) -> Result<Self, &'static str> {
        Self::open_inner(path, binding, required, true)
    }

    /// Authorization must validate existing history without repairing even an
    /// incomplete tail. Rebinding never changes or reseeds admission evidence.
    pub(super) fn open_for_transition(
        path: &Path,
        binding: [u8; 32],
    ) -> Result<Self, &'static str> {
        Self::open_inner(path, binding, true, false)
    }

    fn open_inner(
        path: &Path,
        binding: [u8; 32],
        required: bool,
        repair_tail: bool,
    ) -> Result<Self, &'static str> {
        let existing = match fs::symlink_metadata(path) {
            Ok(meta) => {
                if !meta.is_file()
                    || meta.file_type().is_symlink()
                    || meta.len() > HEADER_BYTES + RECORD_BYTES * MAX_RECEIPTS as u64
                {
                    return Err("tunnel_receipts_invalid");
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if meta.nlink() != 1 || meta.mode() & 0o077 != 0 {
                        return Err("tunnel_receipts_invalid");
                    }
                }
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => false,
            Err(_) => return Err("tunnel_receipts_missing"),
        };
        let mut options = private_options();
        if !existing {
            options.create_new(true);
        }
        let mut file = options.open(path).map_err(|_| "tunnel_receipts_io")?;
        file.try_lock_exclusive()
            .map_err(|_| "tunnel_owner_active")?;
        if !existing {
            file.write_all(HEADER)
                .and_then(|_| file.write_all(&binding))
                .and_then(|_| file.sync_all())
                .map_err(|_| "tunnel_receipts_io")?;
            sync_directory(path.parent().ok_or("tunnel_fence_path")?)?;
        }
        let length = file.metadata().map_err(|_| "tunnel_receipts_io")?.len();
        if length < HEADER_BYTES || length > HEADER_BYTES + RECORD_BYTES * MAX_RECEIPTS as u64 {
            return Err("tunnel_receipts_invalid");
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|_| "tunnel_receipts_io")?;
        let mut header = [0; HEADER_BYTES as usize];
        file.read_exact(&mut header)
            .map_err(|_| "tunnel_receipts_io")?;
        if &header[..8] != HEADER || header[8..] != binding {
            return Err("tunnel_receipts_binding_changed");
        }
        let count = (length - HEADER_BYTES) / RECORD_BYTES;
        let mut admitted = HashSet::with_capacity(count as usize);
        for _ in 0..count {
            let mut record = [0; RECORD_BYTES as usize];
            file.read_exact(&mut record)
                .map_err(|_| "tunnel_receipts_io")?;
            if Sha256::digest(&record[..32]).as_slice() != &record[32..] {
                return Err("tunnel_receipts_invalid");
            }
            let key: [u8; 32] = record[..32]
                .try_into()
                .map_err(|_| "tunnel_receipts_invalid")?;
            if !admitted.insert(key) {
                return Err("tunnel_receipts_invalid");
            }
        }
        // An incomplete final admission was never fsynced and could not dispatch.
        // Complete records are checksummed; corruption is never silently repaired.
        let committed_length = HEADER_BYTES + count * RECORD_BYTES;
        if length != committed_length {
            if !repair_tail {
                return Err("tunnel_receipts_incomplete");
            }
            file.set_len(committed_length)
                .and_then(|_| file.sync_all())
                .map_err(|_| "tunnel_receipts_io")?;
        }
        file.seek(SeekFrom::End(0))
            .map_err(|_| "tunnel_receipts_io")?;
        Ok(Self(Arc::new(Mutex::new(Journal {
            file,
            admitted,
            failed: false,
        }))))
    }

    pub(crate) fn claim(&self, request_id: &str) -> Result<bool, Error> {
        let key: [u8; 32] = Sha256::digest(request_id.as_bytes()).into();
        let mut journal = self.0.lock().map_err(|_| Error::Uncertain)?;
        if journal.failed {
            return Err(Error::Uncertain);
        }
        if journal.admitted.contains(&key) {
            return Ok(false);
        }
        if journal.admitted.len() >= MAX_RECEIPTS {
            return Err(Error::Capacity);
        }
        let checksum = Sha256::digest(key);
        // Persist admission before polling any local HTTP future. If this write
        // fails, poison this journal for the rest of the run and never dispatch.
        let result = journal
            .file
            .write_all(&key)
            .and_then(|_| journal.file.write_all(&checksum))
            .and_then(|_| journal.file.sync_all());
        if result.is_err() {
            journal.failed = true;
            return Err(Error::Uncertain);
        }
        journal.admitted.insert(key);
        Ok(true)
    }
}
