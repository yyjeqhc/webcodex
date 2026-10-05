use super::config::Config;
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// A host-local restart latch, not an execution ledger or cross-host mutex.
/// Drop never removes it: only an observed shutdown without unsettled work does.
pub struct Guard(PathBuf);
impl Guard {
    pub fn acquire(config: &Config) -> Result<Self, &'static str> {
        let root = config
            .state_dir
            .clone()
            .map(Ok)
            .unwrap_or_else(default_root)?;
        if !root.is_absolute() {
            return Err("state directory must be absolute");
        }
        if std::fs::symlink_metadata(&root).is_ok_and(|m| !m.file_type().is_dir()) {
            return Err("state directory must be a real directory");
        }
        std::fs::create_dir_all(&root).map_err(|_| "state directory unavailable")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "state directory permissions unavailable")?;
        }
        let origin =
            reqwest::Url::parse(&config.base).map_err(|_| "invalid control-plane origin")?;
        // Same key as the embedded WebCodex host; one identity must not acquire
        // concurrent consumers simply because they use different binaries.
        let key = format!(
            "{:x}",
            Sha256::digest(format!(
                "{}/{}",
                origin.as_str().trim_end_matches('/'),
                config.tunnel
            ))
        );
        let path = root.join(format!("{key}.active"));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|_| "Tunnel is already running or its previous shutdown is unconfirmed; inspect prior work before clearing its run marker")?;
        file.write_all(b"native-tunnel-run-v1\n")
            .and_then(|_| file.sync_all())
            .map_err(|_| "run marker could not be synced")?;
        sync_directory(&root)?;
        Ok(Self(path))
    }
    pub fn finish(self, unsettled: bool) -> Result<(), &'static str> {
        if unsettled {
            return Err("Tunnel stopped with unconfirmed work; run marker retained, automatic restart is blocked");
        }
        std::fs::remove_file(&self.0)
            .map_err(|_| "clean shutdown observed, but run marker removal failed")?;
        sync_directory(self.0.parent().expect("marker parent"))
    }
}
fn sync_directory(path: &Path) -> Result<(), &'static str> {
    #[cfg(unix)]
    {
        // Persist newly created ancestors before dispatch too.
        for directory in path.ancestors() {
            std::fs::File::open(directory)
                .and_then(|f| f.sync_all())
                .map_err(|_| "run marker directory sync failed")?;
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
#[cfg(any(windows, test))]
fn windows_default_root(local_app_data: &std::ffi::OsStr) -> PathBuf {
    PathBuf::from(local_app_data).join("WebCodex/state/tunnel-runs")
}

fn default_root() -> Result<PathBuf, &'static str> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(|s| windows_default_root(&s))
            .ok_or("LOCALAPPDATA is unavailable; specify --state-dir")
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|s| PathBuf::from(s).join(".local/state")))
            .map(|p| p.join("webcodex/tunnel-runs"))
            .ok_or("user state directory unavailable; specify --state-dir")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_default_root_matches_embedded_webcodex_state_layout() {
        let base = std::ffi::OsStr::new(r"C:\Users\fixture\AppData\Local");
        assert_eq!(
            windows_default_root(base),
            PathBuf::from(base).join("WebCodex/state/tunnel-runs")
        );
    }
}
