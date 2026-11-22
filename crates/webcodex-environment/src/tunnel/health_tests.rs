use super::*;

#[test]
fn terminal_binding_diagnostics_survive_staleness_only_for_the_exact_owner_and_revision() {
    for code in [
        "tunnel_protected_history_binding_changed",
        "tunnel_receipts_binding_changed",
        "tunnel_archive_binding_changed",
    ] {
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let path = store.root().join("readiness.json");
        atomic_private_write(&path, b"{}").unwrap();
        write_embedded_tunnel_observation(&path, 3, TunnelState::Failed, false, false, Some(code))
            .unwrap();
        let health = read_tunnel_health(&path).unwrap();
        assert_eq!(health.diagnostic.as_deref(), Some(code));

        let mut saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        saved["observed_at_ms"] = serde_json::json!(0);
        saved["tunnel_ready"] = serde_json::json!(true);
        saved["local_mcp_ready"] = serde_json::json!(true);
        atomic_private_write(&path, saved.to_string().as_bytes()).unwrap();
        let health = read_tunnel_health(&path).unwrap();
        assert!(!health.fresh && !health.tunnel_ready && !health.local_mcp_ready);
        let pid = Some(std::process::id());
        let matched = matching_tunnel_health(Some(&health), true, pid, Some(3)).unwrap();
        assert_eq!(matched.diagnostic.as_deref(), Some(code));
        assert_eq!(matched.state, Some(TunnelState::Failed));
        assert!(matching_tunnel_health(Some(&health), false, pid, Some(3)).is_none());
        assert!(matching_tunnel_health(Some(&health), true, None, Some(3)).is_none());
        assert!(matching_tunnel_health(Some(&health), true, Some(0), Some(3)).is_none());
        assert!(matching_tunnel_health(Some(&health), true, pid, Some(4)).is_none());

        saved["diagnostic"] = serde_json::json!("private credential must not pass through");
        atomic_private_write(&path, saved.to_string().as_bytes()).unwrap();
        assert_eq!(
            read_tunnel_health(&path).unwrap().diagnostic.as_deref(),
            Some("tunnel_protocol_failed")
        );
    }
}

#[test]
#[cfg(unix)]
fn health_locks_release_without_waiting_for_inherited_handles() {
    for exclusive in [false, true] {
        for fail_after_lock in [false, true] {
            let temp = crate::test_tempdir().unwrap();
            let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
            let path = store.root().join("readiness.json");
            atomic_private_write(&path, b"{}").unwrap();
            let file = crate::storage::open_existing_private(&path, true).unwrap();
            if exclusive {
                fs2::FileExt::try_lock_exclusive(&file).unwrap();
            } else {
                fs2::FileExt::try_lock_shared(&file).unwrap();
            }
            let mut inherited = None;
            let result = (|| -> SetupResultValue<()> {
                let locked = TunnelHealthFileLock { file };
                // dup and fork share an open-file description. An unrelated concurrent
                // spawn can retain this descriptor until exec despite O_CLOEXEC.
                inherited = Some(locked.file.try_clone().unwrap());

                assert!(write_tunnel_health(&path, true, true).is_err());
                assert_eq!(std::fs::read(&path).unwrap(), b"{}");
                let contender = crate::storage::open_existing_private(&path, true).unwrap();
                assert!(fs2::FileExt::try_lock_exclusive(&contender).is_err());
                if fail_after_lock {
                    return Err(SetupDiagnostic::io());
                }
                Ok(())
            })();
            assert_eq!(result.is_err(), fail_after_lock);

            write_embedded_tunnel_observation(
                &path,
                1,
                TunnelState::Failed,
                false,
                false,
                Some("tunnel_restart_uncertain"),
            )
            .unwrap_or_else(|error| {
                panic!("lock survived operation, exclusive={exclusive}: {error:?}")
            });
            let health = read_tunnel_health(&path).unwrap();
            assert_eq!(health.state, Some(TunnelState::Failed));
            assert_eq!(
                health.diagnostic.as_deref(),
                Some("tunnel_restart_uncertain")
            );
            drop(inherited);
        }
    }
}

#[test]
fn missed_heartbeats_expire_during_contention_and_recover_after_unlock() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let path = store.root().join("readiness.json");
    let stale = serde_json::json!({
        "schema_version": 1,
        "observed_at_ms": 0,
        "service_pid": std::process::id(),
        "profile_revision": 7,
        "state": "running",
        "tunnel_ready": true,
        "local_mcp_ready": true,
    });
    atomic_private_write(&path, stale.to_string().as_bytes()).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    fs2::FileExt::try_lock_shared(&file).unwrap();
    let reader = TunnelHealthFileLock { file };
    for _ in 0..4 {
        assert!(write_embedded_tunnel_observation(
            &path,
            7,
            TunnelState::Running,
            true,
            true,
            None,
        )
        .is_err());
        let health = read_tunnel_health(&path).unwrap();
        assert!(!health.fresh);
        assert!(!health.tunnel_ready && !health.local_mcp_ready);
        assert_eq!(health.state, Some(TunnelState::Running));
        assert_eq!(health.profile_revision, Some(7));
    }
    drop(reader);
    write_embedded_tunnel_observation(&path, 7, TunnelState::Running, true, true, None).unwrap();
    let health = read_tunnel_health(&path).unwrap();
    assert!(health.fresh && health.tunnel_ready && health.local_mcp_ready);
    assert_eq!(health.service_pid, Some(std::process::id()));
    assert_eq!(health.profile_revision, Some(7));
}
