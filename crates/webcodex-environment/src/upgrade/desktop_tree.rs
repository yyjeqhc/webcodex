//! The Desktop payload digest matches scripts/package_unified_installer.py.
//! Directory links are recorded but never traversed when backing up a bundle.
use super::{error, SetupDiagnostic, SetupResultValue, SNAPSHOT_LIMIT};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

const ENTRY_LIMIT: usize = 100_000;

fn relative(root: &Path, path: &Path) -> SetupResultValue<String> {
    let path = if root == path && root.is_file() {
        path.file_name()
            .map(PathBuf::from)
            .ok_or_else(SetupDiagnostic::io)?
    } else {
        path.strip_prefix(root)
            .map_err(|_| SetupDiagnostic::io())?
            .to_path_buf()
    };
    let mut names = Vec::new();
    for part in path.components() {
        let std::path::Component::Normal(name) = part else {
            return Err(SetupDiagnostic::io());
        };
        names.push(name.to_str().ok_or_else(SetupDiagnostic::io)?);
    }
    Ok(names.join("/"))
}

fn entries(root: &Path) -> SetupResultValue<Vec<(String, PathBuf)>> {
    let metadata = std::fs::symlink_metadata(root).map_err(|_| SetupDiagnostic::io())?;
    if metadata.is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
        return Err(error(
            "desktop_payload",
            "The Desktop payload is not a regular file or directory",
        ));
    }
    let mut paths = Vec::new();
    if metadata.is_file() {
        paths.push(root.to_path_buf());
    } else {
        let mut stack = vec![root.to_path_buf()];
        while let Some(directory) = stack.pop() {
            for entry in std::fs::read_dir(directory).map_err(|_| SetupDiagnostic::io())? {
                if paths.len() >= ENTRY_LIMIT {
                    return Err(error(
                        "desktop_payload",
                        "The Desktop payload has too many entries",
                    ));
                }
                let path = entry.map_err(|_| SetupDiagnostic::io())?.path();
                let entry_metadata =
                    std::fs::symlink_metadata(&path).map_err(|_| SetupDiagnostic::io())?;
                if entry_metadata.is_dir() && !entry_metadata.is_symlink() {
                    stack.push(path.clone());
                }
                paths.push(path);
            }
        }
    }
    let mut named: Vec<_> = paths
        .into_iter()
        .map(|path| Ok((relative(root, &path)?, path)))
        .collect::<SetupResultValue<_>>()?;
    named.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(named)
}

#[cfg(unix)]
pub(super) fn digest(root: &Path) -> SetupResultValue<String> {
    digest_named(root, None)
}

#[cfg(unix)]
pub(super) fn digest_named(
    root: &Path,
    logical_file_name: Option<&str>,
) -> SetupResultValue<String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let mut hash = Sha256::new();
    let mut remaining = SNAPSHOT_LIMIT;
    let canonical_root = root.canonicalize().map_err(|_| SetupDiagnostic::io())?;
    let root_metadata = std::fs::symlink_metadata(root).map_err(|_| SetupDiagnostic::io())?;
    if root_metadata.mode() & 0o6022 != 0 {
        return Err(error(
            "desktop_payload",
            "The Desktop payload has unsafe permissions",
        ));
    }
    for (name, path) in entries(root)? {
        let name = if root_metadata.is_file() {
            logical_file_name.unwrap_or(&name)
        } else {
            &name
        };
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| SetupDiagnostic::io())?;
        if metadata.is_symlink() {
            let target = std::fs::read_link(&path).map_err(|_| SetupDiagnostic::io())?;
            if target.is_absolute() {
                return Err(error(
                    "desktop_payload",
                    "The Desktop bundle has an absolute link",
                ));
            }
            let resolved = path
                .canonicalize()
                .map_err(|_| error("desktop_payload", "The Desktop bundle has a broken link"))?;
            if !resolved.starts_with(&canonical_root) {
                return Err(error(
                    "desktop_payload",
                    "The Desktop bundle has an escaping link",
                ));
            }
            let target = target.to_str().ok_or_else(SetupDiagnostic::io)?;
            hash.update(b"L\0");
            hash.update(name.as_bytes());
            hash.update(b"\0");
            hash.update(target.as_bytes());
            hash.update(b"\0");
        } else if metadata.is_dir() {
            if metadata.mode() & 0o6022 != 0 {
                return Err(error(
                    "desktop_payload",
                    "The Desktop bundle has unsafe directory permissions",
                ));
            }
            hash.update(b"D\0");
            hash.update(name.as_bytes());
            hash.update(b"\0");
        } else if metadata.is_file() {
            if metadata.len() > remaining || metadata.mode() & 0o6000 != 0 {
                return Err(error(
                    "desktop_payload",
                    "The Desktop bundle is too large or contains privileged file modes",
                ));
            }
            remaining -= metadata.len();
            hash.update(b"F\0");
            hash.update(name.as_bytes());
            hash.update(b"\0");
            hash.update(format!("{:04o}", metadata.permissions().mode() & 0o7777).as_bytes());
            hash.update(b"\0");
            use std::os::unix::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)
                .map_err(|_| SetupDiagnostic::io())?;
            let opened = file.metadata().map_err(|_| SetupDiagnostic::io())?;
            if !opened.is_file()
                || opened.dev() != metadata.dev()
                || opened.ino() != metadata.ino()
                || opened.len() != metadata.len()
            {
                return Err(error(
                    "desktop_payload",
                    "A Desktop file changed while opening it",
                ));
            }
            let mut file = file.take(opened.len() + 1);
            let mut bytes = [0u8; 64 * 1024];
            let mut read_total = 0u64;
            loop {
                let count = file.read(&mut bytes).map_err(|_| SetupDiagnostic::io())?;
                if count == 0 {
                    break;
                }
                read_total += count as u64;
                if read_total > opened.len() {
                    return Err(error(
                        "desktop_payload",
                        "A Desktop file grew while hashing it",
                    ));
                }
                hash.update(&bytes[..count]);
            }
        } else {
            return Err(error(
                "desktop_payload",
                "The Desktop bundle contains a special file",
            ));
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(unix)]
pub(super) fn directory_modes_digest(root: &Path) -> SetupResultValue<String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(root).map_err(|_| SetupDiagnostic::io())?;
    let mut hash = Sha256::new();
    if metadata.is_dir() {
        hash.update(b"ROOT\0");
        hash.update(format!("{:04o}", metadata.mode() & 0o7777).as_bytes());
        hash.update(b"\0");
        for (name, path) in entries(root)? {
            let metadata = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
            if metadata.is_dir() {
                hash.update(name.as_bytes());
                hash.update(b"\0");
                hash.update(format!("{:04o}", metadata.mode() & 0o7777).as_bytes());
                hash.update(b"\0");
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(unix)]
pub(super) fn copy(source: &Path, target: &Path) -> SetupResultValue<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let private_dir = |path: &Path| {
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(path).map_err(|_| SetupDiagnostic::io())
    };
    // Validate all links and bounds before creating any destination entry.
    let _ = digest(source)?;
    let metadata = std::fs::symlink_metadata(source).map_err(|_| SetupDiagnostic::io())?;
    if metadata.mode() & 0o6022 != 0 {
        return Err(error(
            "desktop_payload",
            "The Desktop backup permissions changed",
        ));
    }
    if metadata.is_file() {
        let mut remaining = SNAPSHOT_LIMIT;
        super::copy_bounded(source, target, &mut remaining)?;
        std::fs::set_permissions(
            target,
            std::fs::Permissions::from_mode(metadata.mode() & 0o7777),
        )
        .map_err(|_| SetupDiagnostic::io())?;
    } else {
        private_dir(target)?;
        let mut remaining = SNAPSHOT_LIMIT;
        for (name, path) in entries(source)? {
            let dest = target.join(name);
            let metadata = std::fs::symlink_metadata(&path).map_err(|_| SetupDiagnostic::io())?;
            if metadata.mode() & 0o6022 != 0 && !metadata.is_symlink() {
                return Err(error(
                    "desktop_payload",
                    "The Desktop backup permissions changed",
                ));
            }
            if metadata.is_dir() {
                private_dir(&dest)?;
            } else if metadata.is_symlink() {
                std::os::unix::fs::symlink(
                    std::fs::read_link(&path).map_err(|_| SetupDiagnostic::io())?,
                    &dest,
                )
                .map_err(|_| SetupDiagnostic::io())?;
            } else if metadata.is_file() {
                super::copy_bounded(&path, &dest, &mut remaining)?;
                std::fs::set_permissions(
                    &dest,
                    std::fs::Permissions::from_mode(metadata.mode() & 0o7777),
                )
                .map_err(|_| SetupDiagnostic::io())?;
            }
        }
        // Directory permissions are applied last, after all children exist.
        for (name, path) in entries(source)?.into_iter().rev() {
            let metadata = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
            if metadata.mode() & 0o6022 != 0 && metadata.is_dir() {
                return Err(error(
                    "desktop_payload",
                    "The Desktop backup permissions changed",
                ));
            }
            if metadata.is_dir() {
                std::fs::set_permissions(
                    target.join(name),
                    std::fs::Permissions::from_mode(metadata.mode() & 0o7777),
                )
                .map_err(|_| SetupDiagnostic::io())?;
            }
        }
        std::fs::set_permissions(
            target,
            std::fs::Permissions::from_mode(metadata.mode() & 0o7777),
        )
        .map_err(|_| SetupDiagnostic::io())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn bundle_digest_and_copy_keep_relative_links_but_reject_escape() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("WebCodex.app");
        std::fs::create_dir(&bundle).unwrap();
        std::fs::set_permissions(&bundle, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(bundle.join("binary"), b"old desktop").unwrap();
        std::fs::set_permissions(
            bundle.join("binary"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        symlink("binary", bundle.join("alias")).unwrap();
        let expected = digest(&bundle).unwrap();
        let modes = directory_modes_digest(&bundle).unwrap();
        let backup = temp.path().join("backup.app");
        copy(&bundle, &backup).unwrap();
        assert_eq!(digest(&backup).unwrap(), expected);
        assert_eq!(directory_modes_digest(&backup).unwrap(), modes);
        std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o750)).unwrap();
        assert_eq!(digest(&backup).unwrap(), expected);
        assert_ne!(directory_modes_digest(&backup).unwrap(), modes);
        std::fs::write(bundle.join("binary"), b"changed").unwrap();
        assert_ne!(digest(&bundle).unwrap(), expected);
        symlink("../outside", bundle.join("escape")).unwrap();
        assert!(digest(&bundle).is_err());
    }
}
