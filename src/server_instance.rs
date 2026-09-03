use crate::Database;
use anyhow::{anyhow, Context};
use fs2::FileExt;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// Process-lifetime ownership proof for one standalone Control Server database.
///
/// The lock file intentionally contains no PID, credential, path metadata, or
/// other authority claim. Its OS-level exclusive lock is the proof; an empty
/// sidecar merely provides a stable cross-platform lock target. Process exit
/// releases the lock even when normal Rust destructors do not run.
pub(crate) struct ServerInstanceGuard {
    file: File,
    database_path: PathBuf,
}

impl ServerInstanceGuard {
    pub(crate) fn acquire(db: &Database) -> anyhow::Result<Self> {
        let database_path = db.state_path().to_path_buf();
        let lock_path = lock_path_for_database(&database_path);
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&lock_path)
            .context("open standalone Server ownership lock")?;
        file.try_lock_exclusive()
            .map_err(|_| anyhow!("another WebCodex Server already owns this standalone state"))?;
        Ok(Self {
            file,
            database_path,
        })
    }

    pub(crate) fn owns_database(&self, db: &Database) -> bool {
        self.database_path == db.state_path()
    }
}

impl Drop for ServerInstanceGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn lock_path_for_database(database_path: &Path) -> PathBuf {
    let mut lock_path = OsString::from(database_path.as_os_str());
    lock_path.push(".server-instance.lock");
    PathBuf::from(lock_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_is_exclusive_per_database_and_released_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let first_db = Database::open(&temp.path().join("first.db")).unwrap();
        let unrelated_db = Database::open(&temp.path().join("unrelated.db")).unwrap();

        let first_owner = ServerInstanceGuard::acquire(&first_db).unwrap();
        assert!(ServerInstanceGuard::acquire(&first_db).is_err());
        let unrelated_owner = ServerInstanceGuard::acquire(&unrelated_db).unwrap();
        assert!(unrelated_owner.owns_database(&unrelated_db));
        assert!(!unrelated_owner.owns_database(&first_db));

        drop(first_owner);
        let successor = ServerInstanceGuard::acquire(&first_db).unwrap();
        assert!(successor.owns_database(&first_db));
    }

    #[test]
    fn ownership_sidecar_contains_no_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("metadata.db")).unwrap();
        let guard = ServerInstanceGuard::acquire(&db).unwrap();
        assert_eq!(guard.file.metadata().unwrap().len(), 0);
        drop(guard);
    }
}
