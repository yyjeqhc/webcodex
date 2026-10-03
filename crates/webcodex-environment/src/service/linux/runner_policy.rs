//! The only accepted legacy difference is omission of the new Runner restart
//! guard. Do not normalize arbitrary unit text or infer ownership from a marker.
use super::*;

pub(super) const GUARD: &str = "RestartPreventExitStatus=2\n";

pub(super) fn matches(current: &str, expected: &str) -> bool {
    current == expected || legacy(expected).is_some_and(|old| current == old)
}

fn legacy(expected: &str) -> Option<String> {
    (expected.matches(GUARD).count() == 1).then(|| expected.replacen(GUARD, "", 1))
}

/// Explicit installation may upgrade this one historical policy difference.
/// Existing active processes are not restarted; daemon-reload applies the guard
/// to future failures. Ownership/ExecStart and every other byte stay unchanged.
pub(super) fn upgrade(path: &Path, expected: &str, owner: u32) -> Result<bool, ServiceError> {
    let Some(old) = legacy(expected) else {
        return Ok(false);
    };
    let failed = || {
        ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "Runner unit changed during restart-policy reconciliation",
        )
    };
    let before = fs::symlink_metadata(path).map_err(|_| failed())?;
    if !before.is_file()
        || before.file_type().is_symlink()
        || before.uid() != owner
        || before.permissions().mode() & 0o022 != 0
    {
        return Err(failed());
    }
    let bytes = fs::read_to_string(path).map_err(|_| failed())?;
    if bytes == expected {
        return Ok(false);
    }
    if bytes != old {
        return Err(failed());
    }
    let parent = path.parent().ok_or_else(failed)?;
    let temporary = parent.join(format!(".webcodex-policy-{}", uuid::Uuid::new_v4()));
    write_new(&temporary, expected)?;
    let result = (|| {
        let current = fs::symlink_metadata(path).map_err(|_| failed())?;
        if current.dev() != before.dev()
            || current.ino() != before.ino()
            || current.uid() != owner
            || current.permissions().mode() != before.permissions().mode()
            || fs::read_to_string(path).map_err(|_| failed())? != old
        {
            return Err(failed());
        }
        fs::rename(&temporary, path).map_err(|_| failed())?;
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OutcomeUnknown,
                    "Runner policy replaced but directory sync failed; inspect before retrying",
                )
            })?;
        Ok(true)
    })();
    if temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_old_runner_policy_can_upgrade_without_accepting_foreign_units() {
        let spec = crate::service::tests::sample_spec();
        let expected = render_unit(&spec).unwrap();
        let old = expected.replace(GUARD, "");
        assert_ne!(old, expected);
        assert!(matches(&old, &expected));
        assert!(matches(&expected, &expected));
        assert!(!matches(
            &old.replace("Restart=on-failure", "Restart=always"),
            &expected
        ));
        assert!(!matches(
            &(old.clone() + "ExecStart=/tmp/other\n"),
            &expected
        ));
        let temp = crate::test_tempdir().unwrap();
        let path = temp.path().join("runner.service");
        write_new(&path, &old).unwrap();
        assert!(upgrade(&path, &expected, unsafe { libc::geteuid() }).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert!(!upgrade(&path, &expected, unsafe { libc::geteuid() }).unwrap());
        fs::write(&path, old.replace("Restart=on-failure", "Restart=no")).unwrap();
        assert!(upgrade(&path, &expected, unsafe { libc::geteuid() }).is_err());
    }

    #[test]
    fn symlink_or_writable_unit_is_never_policy_upgraded() {
        let expected = render_unit(&crate::service::tests::sample_spec()).unwrap();
        let temp = crate::test_tempdir().unwrap();
        let path = temp.path().join("runner.service");
        let target = temp.path().join("other");
        write_new(&target, &expected.replace(GUARD, "")).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(upgrade(&path, &expected, unsafe { libc::geteuid() }).is_err());
        fs::remove_file(&path).unwrap();
        fs::rename(&target, &path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(upgrade(&path, &expected, unsafe { libc::geteuid() }).is_err());
    }
}
