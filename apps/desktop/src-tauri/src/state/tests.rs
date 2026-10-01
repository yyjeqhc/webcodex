#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_server_defaults_append_missing_values_and_preserve_explicit_config() {
        let dir = unique_state_dir("server-defaults-explicit");
        std::fs::create_dir_all(&dir).unwrap();
        let env_file = dir.join("webcodex.env");
        std::fs::write(
            &env_file,
            "WEBCODEX_TOKEN=secret\nWEBCODEX_MCP_HOST_PROFILE=direct\nWEBCODEX_MCP_HOST_BUDGET_SECS=37\nWEBCODEX_MCP_COMPACT_SCHEMAS=false\nWEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT=false\n",
        )
        .unwrap();

        ensure_desktop_server_defaults(&env_file).unwrap();
        let once = std::fs::read_to_string(&env_file).unwrap();
        assert!(once.contains("WEBCODEX_TOKEN=secret\n"));
        assert!(once.contains("WEBCODEX_MCP_HOST_PROFILE=direct\n"));
        assert_eq!(once.matches("WEBCODEX_MCP_HOST_PROFILE=").count(), 1);
        assert!(once.contains("WEBCODEX_MCP_HOST_BUDGET_SECS=37\n"));
        assert_eq!(once.matches("WEBCODEX_MCP_HOST_BUDGET_SECS=").count(), 1);
        assert!(once.contains("WEBCODEX_MCP_COMPACT_SCHEMAS=false\n"));
        assert_eq!(once.matches("WEBCODEX_MCP_COMPACT_SCHEMAS=").count(), 1);
        assert!(once.contains("WEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT=false\n"));
        assert_eq!(
            once.matches("WEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT=")
                .count(),
            1
        );

        ensure_desktop_server_defaults(&env_file).unwrap();
        assert_eq!(std::fs::read_to_string(&env_file).unwrap(), once);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn desktop_server_defaults_do_not_change_an_explicit_direct_profile_budget() {
        let dir = unique_state_dir("server-defaults-direct");
        std::fs::create_dir_all(&dir).unwrap();
        let env_file = dir.join("webcodex.env");
        std::fs::write(
            &env_file,
            "WEBCODEX_ADDR=127.0.0.1:12345\nWEBCODEX_MCP_HOST_PROFILE=direct\n",
        )
        .unwrap();

        ensure_desktop_server_defaults(&env_file).unwrap();
        let content = std::fs::read_to_string(&env_file).unwrap();
        assert!(content.contains("WEBCODEX_MCP_HOST_PROFILE=direct\n"));
        assert!(!content.contains("WEBCODEX_MCP_HOST_BUDGET_SECS="));
        assert!(content.contains("WEBCODEX_MCP_COMPACT_SCHEMAS=true\n"));
        assert!(content.contains("WEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT=true\n"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn desktop_server_defaults_add_both_values_to_fresh_server_env() {
        let dir = unique_state_dir("server-defaults-fresh");
        std::fs::create_dir_all(&dir).unwrap();
        let env_file = dir.join("webcodex.env");
        std::fs::write(&env_file, "WEBCODEX_ADDR=127.0.0.1:12345").unwrap();

        ensure_desktop_server_defaults(&env_file).unwrap();
        let content = std::fs::read_to_string(&env_file).unwrap();
        assert!(content.starts_with("WEBCODEX_ADDR=127.0.0.1:12345\n"));
        assert!(content.contains("WEBCODEX_MCP_HOST_PROFILE=host_code_mode\n"));
        assert!(content.contains("WEBCODEX_MCP_HOST_BUDGET_SECS=55\n"));
        assert!(content.contains("WEBCODEX_MCP_COMPACT_SCHEMAS=true\n"));
        assert!(content.contains("WEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT=true\n"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stale_desktop_loopback_port_recovery_preserves_other_env_values() {
        let dir = unique_state_dir("stale-loopback-recovery");
        std::fs::create_dir_all(&dir).unwrap();
        let env_file = dir.join("webcodex.env");

        let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let occupied_addr = occupied.local_addr().unwrap();
        let original = format!(
            "WEBCODEX_ADDR={occupied_addr}\r\nWEBCODEX_TOKEN=secret-value\r\nWEBCODEX_MCP_COMPACT_SCHEMAS=false\r\nCUSTOM_SETTING=preserved\r\n"
        );
        std::fs::write(&env_file, original).unwrap();

        let recovered =
            recover_stale_desktop_loopback_address(&env_file, &format!("http://{occupied_addr}"))
                .unwrap()
                .expect("occupied persisted loopback address should recover");
        assert_ne!(recovered, format!("http://{occupied_addr}"));

        let recovered_addr =
            loopback_socket_from_server_url(&recovered).expect("recovered loopback socket");
        assert_ne!(recovered_addr, occupied_addr);
        let rebound = TcpListener::bind(recovered_addr)
            .expect("newly reserved replacement address should be bindable after reservation");
        drop(rebound);

        let content = std::fs::read_to_string(&env_file).unwrap();
        assert_eq!(content.matches("WEBCODEX_ADDR=").count(), 1);
        assert!(content.contains(&format!("WEBCODEX_ADDR={recovered_addr}\r\n")));
        assert!(content.contains("WEBCODEX_TOKEN=secret-value\r\n"));
        assert!(content.contains("WEBCODEX_MCP_COMPACT_SCHEMAS=false\r\n"));
        assert!(content.contains("CUSTOM_SETTING=preserved\r\n"));
        assert!(!content.contains(&format!("WEBCODEX_ADDR={occupied_addr}\r\n")));

        drop(occupied);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bindable_or_non_loopback_saved_address_is_not_rewritten() {
        let dir = unique_state_dir("loopback-recovery-noop");
        std::fs::create_dir_all(&dir).unwrap();
        let env_file = dir.join("webcodex.env");

        let reservation = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let bindable_addr = reservation.local_addr().unwrap();
        drop(reservation);
        let original = format!(
            "WEBCODEX_ADDR={bindable_addr}\nWEBCODEX_TOKEN=secret\nCUSTOM_SETTING=preserved\n"
        );
        std::fs::write(&env_file, &original).unwrap();

        assert_eq!(
            recover_stale_desktop_loopback_address(&env_file, &format!("http://{bindable_addr}"))
                .unwrap(),
            None
        );
        assert_eq!(std::fs::read_to_string(&env_file).unwrap(), original);

        assert_eq!(
            recover_stale_desktop_loopback_address(&env_file, "https://example.com:8443").unwrap(),
            None
        );
        assert_eq!(std::fs::read_to_string(&env_file).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn desktop_server_address_rewrite_rejects_duplicate_address_entries() {
        let dir = unique_state_dir("duplicate-server-address");
        std::fs::create_dir_all(&dir).unwrap();
        let env_file = dir.join("webcodex.env");
        let original =
            "WEBCODEX_ADDR=127.0.0.1:1111\nWEBCODEX_TOKEN=secret\nWEBCODEX_ADDR=127.0.0.1:2222\n";
        std::fs::write(&env_file, original).unwrap();

        let error = rewrite_desktop_server_address(&env_file, "127.0.0.1:3333").unwrap_err();
        assert_eq!(error.code, "desktop_state_invalid");
        assert_eq!(std::fs::read_to_string(&env_file).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn unique_state_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "webcodex-desktop-state-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn test_stored_config(label: &str) -> StoredDesktopConfig {
        StoredDesktopConfig {
            saved_projects: Vec::new(),
            topology: None,
            project: Some(ProjectSelection {
                path: format!("/{label}"),
                allowed_root: "/".to_string(),
                is_git_repository: false,
                runtime_project_id: None,
            }),
            runtime_autostart: None,
            preferred_connection: None,
            tunnel_proxy: TunnelProxyConfig::default(),
            runtime: None,
            ..StoredDesktopConfig::default()
        }
    }

    #[test]
    fn chatgpt_activity_observation_is_fenced_to_the_captured_project_identity() {
        let data_dir = unique_state_dir("chatgpt-activity-fence");
        let resource_dir = data_dir.join("resources");
        std::fs::create_dir_all(&resource_dir).unwrap();
        let runner_config = data_dir.join("runner.toml");
        let user_token_file = data_dir.join("user-token");
        std::fs::write(&runner_config, "fixture").unwrap();
        std::fs::write(&user_token_file, "fixture").unwrap();

        let mut core = DesktopCore::new(data_dir.clone(), resource_dir).unwrap();
        core.snapshot.readiness.runtime_ready = true;
        core.config.project = Some(ProjectSelection {
            path: data_dir.join("project-b").to_string_lossy().to_string(),
            allowed_root: data_dir.to_string_lossy().to_string(),
            is_git_repository: false,
            runtime_project_id: Some("agent:desktop:project-b".to_string()),
        });
        core.config.runtime = Some(StoredRuntime {
            server_url: "http://127.0.0.1:8080".to_string(),
            server_env_file: None,
            runner_config: Some(runner_config),
            user_token_file: Some(user_token_file),
            runner_client_id: Some("desktop".to_string()),
            project_id: Some("project-b".to_string()),
            runtime_project_id: Some("agent:desktop:project-b".to_string()),
        });

        let current_identity =
            identity_from_config(&core.config).expect("current project identity");
        let mut stale_identity = current_identity.clone();
        stale_identity.project_id = "project-a".to_string();
        stale_identity.runtime_project_id = "agent:desktop:project-a".to_string();

        let stale = core
            .apply_chatgpt_activity_observation(&stale_identity, Some(1234))
            .unwrap();
        assert!(
            stale.chatgpt_activity.is_none(),
            "an observation captured for the old Project must not cross a Project switch"
        );

        let current = core
            .apply_chatgpt_activity_observation(&current_identity, Some(5678))
            .unwrap();
        assert_eq!(
            current
                .chatgpt_activity
                .as_ref()
                .and_then(|activity| activity.last_meaningful_activity_at_ms),
            Some(5678)
        );
        assert!(current.chatgpt_activity.unwrap().observed);
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn staging_a_project_scope_clears_prior_chatgpt_evidence() {
        let data_dir = unique_state_dir("chatgpt-project-scope-reset");
        let resource_dir = data_dir.join("resources");
        std::fs::create_dir_all(&resource_dir).unwrap();
        let mut core = DesktopCore::new(data_dir.clone(), resource_dir).unwrap();
        core.snapshot.chatgpt_activity = Some(ChatGptActivitySnapshot {
            observed: true,
            last_meaningful_activity_at_ms: Some(1234),
        });

        let project = ProjectSelection {
            path: data_dir.join("project-b").to_string_lossy().to_string(),
            allowed_root: data_dir.to_string_lossy().to_string(),
            is_git_repository: false,
            runtime_project_id: None,
        };
        core.stage_project_scope(project.clone());

        assert_eq!(core.snapshot.project, Some(project));
        assert!(core.snapshot.chatgpt_activity.is_none());
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn legacy_project_refresh_never_claims_external_or_inactive_runner_ownership() {
        assert!(!can_refresh_legacy_runner(None));
        assert!(!can_refresh_legacy_runner(Some(
            crate::process::ProcessSnapshot {
                kind: ProcessKey::LocalRunner,
                phase: ProcessPhase::Running,
                pid: Some(42),
                generation: 1,
                exit_code: None,
                owned_by_desktop: false,
            }
        )));
        assert!(!can_refresh_legacy_runner(Some(
            crate::process::ProcessSnapshot {
                kind: ProcessKey::LocalRunner,
                phase: ProcessPhase::Exited,
                pid: Some(43),
                generation: 2,
                exit_code: Some(0),
                owned_by_desktop: true,
            }
        )));
        assert!(can_refresh_legacy_runner(Some(
            crate::process::ProcessSnapshot {
                kind: ProcessKey::LocalRunner,
                phase: ProcessPhase::Running,
                pid: Some(44),
                generation: 3,
                exit_code: None,
                owned_by_desktop: true,
            }
        )));
    }

    #[test]
    fn legacy_runtime_project_identity_recovers_runner_client_id_without_project_coupling() {
        let mut config = test_stored_config("legacy");
        config.runtime = Some(StoredRuntime {
            server_url: "https://example.test".to_string(),
            server_env_file: None,
            runner_config: None,
            user_token_file: None,
            runner_client_id: None,
            project_id: Some("repo".to_string()),
            runtime_project_id: Some("agent:desktop:runner:repo".to_string()),
        });
        assert_eq!(
            stored_runner_client_id(&config).as_deref(),
            Some("desktop:runner")
        );
        config.runtime.as_mut().unwrap().runner_client_id = Some("explicit-runner".to_string());
        assert_eq!(
            stored_runner_client_id(&config).as_deref(),
            Some("explicit-runner")
        );
    }

    #[test]
    fn local_enrollment_preserves_saved_files_and_reuses_only_the_inactive_slot() {
        let dir = unique_state_dir("enrollment-slots");
        std::fs::create_dir_all(&dir).unwrap();
        let mut config = test_stored_config("previous");
        let first = local_enrollment_directory(&dir, &config);
        let saved_runner = first.join("server").join("runner.toml");
        std::fs::create_dir_all(saved_runner.parent().unwrap()).unwrap();
        std::fs::write(&saved_runner, "previous project fixture").unwrap();
        config.runtime = Some(StoredRuntime {
            server_url: "http://127.0.0.1:7890".into(),
            server_env_file: None,
            runner_config: Some(saved_runner.clone()),
            user_token_file: None,
            runner_client_id: None,
            project_id: None,
            runtime_project_id: None,
        });

        let candidate = local_enrollment_directory(&dir, &config);
        assert_ne!(candidate, first);
        let candidate_runner = candidate.join("server").join("runner.toml");
        std::fs::create_dir_all(candidate_runner.parent().unwrap()).unwrap();
        std::fs::write(&candidate_runner, "replacement project fixture").unwrap();
        assert_eq!(
            std::fs::read_to_string(saved_runner).unwrap(),
            "previous project fixture"
        );
        // A failed activation retries the candidate, never the saved connection.
        assert_eq!(local_enrollment_directory(&dir, &config), candidate);
        config.runtime.as_mut().unwrap().runner_config = Some(candidate_runner);
        assert_eq!(local_enrollment_directory(&dir, &config), first);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn atomic_save_interruption_keeps_prior_valid_state() {
        let dir = unique_state_dir("interrupted-save");
        std::fs::create_dir_all(&dir).expect("create state fixture dir");
        let path = dir.join("desktop-state.json");
        let previous = test_stored_config("previous");
        let replacement = test_stored_config("replacement");
        let previous_bytes = serde_json::to_vec_pretty(&previous).unwrap();
        let replacement_bytes = serde_json::to_vec_pretty(&replacement).unwrap();
        write_atomic_file(&path, &previous_bytes).expect("write previous state");

        let interrupted = write_atomic_file_with_hook(&path, &replacement_bytes, |_| {
            Err(io::Error::other("injected interruption before replace"))
        });
        assert!(interrupted.is_err());
        match read_stored_config(&path).expect("read state after interruption") {
            StoredConfigFile::Valid { config, .. } => assert_eq!(config, previous),
            other => panic!("previous state was not preserved: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn atomic_save_preserves_previous_known_good_backup() {
        let dir = unique_state_dir("known-good-backup");
        std::fs::create_dir_all(&dir).expect("create state fixture dir");
        let path = dir.join("desktop-state.json");
        let previous = test_stored_config("previous");
        let replacement = test_stored_config("replacement");
        save_config_atomically(&path, &serde_json::to_vec_pretty(&previous).unwrap())
            .expect("initial atomic save");
        save_config_atomically(&path, &serde_json::to_vec_pretty(&replacement).unwrap())
            .expect("replacement atomic save");

        match read_stored_config(&path).expect("read primary") {
            StoredConfigFile::Valid { config, .. } => assert_eq!(config, replacement),
            other => panic!("replacement state was not valid: {other:?}"),
        }
        match read_stored_config(&desktop_state_backup_path(&path)).expect("read backup") {
            StoredConfigFile::Valid { config, .. } => assert_eq!(config, previous),
            other => panic!("previous snapshot was not valid: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_primary_with_valid_backup_recovers_explicitly() {
        let dir = unique_state_dir("recover-backup");
        std::fs::create_dir_all(&dir).expect("create state fixture dir");
        let path = dir.join("desktop-state.json");
        let expected = test_stored_config("recovered");
        write_atomic_file(
            &desktop_state_backup_path(&path),
            &serde_json::to_vec_pretty(&expected).unwrap(),
        )
        .expect("write valid backup");
        std::fs::write(&path, b"{corrupt-primary").expect("write corrupt primary");
        let activity = ActivityLog::default();

        let recovered = load_config(&path, &activity).expect("recover from backup");
        assert_eq!(recovered, expected);
        assert!(matches!(
            read_stored_config(&path).expect("read restored primary"),
            StoredConfigFile::Valid { .. }
        ));
        assert!(activity.snapshot().iter().any(|entry| {
            entry.event_kind == ActivityEventKind::StateRecovered && entry.source == "desktop_state"
        }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_primary_and_backup_returns_explicit_error() {
        let dir = unique_state_dir("both-corrupt");
        std::fs::create_dir_all(&dir).expect("create state fixture dir");
        let path = dir.join("desktop-state.json");
        std::fs::write(&path, b"{corrupt-primary").expect("write corrupt primary");
        std::fs::write(desktop_state_backup_path(&path), b"{corrupt-backup")
            .expect("write corrupt backup");

        let error = load_config(&path, &ActivityLog::default())
            .expect_err("both corrupt copies must fail closed");
        assert_eq!(error.code, "desktop_state_corrupt");
        assert_eq!(
            error
                .details
                .as_ref()
                .and_then(|details| details.get("category"))
                .and_then(Value::as_str),
            Some("state_corrupt")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stored_runtime_contains_paths_not_credentials() {
        let runtime = StoredRuntime {
            server_url: "https://example.com".to_string(),
            server_env_file: Some(PathBuf::from("webcodex.env")),
            runner_config: Some(PathBuf::from("runner.toml")),
            user_token_file: Some(PathBuf::from("user-token")),
            runner_client_id: Some("desktop-runner".to_string()),
            project_id: Some("project".to_string()),
            runtime_project_id: Some("agent:desktop:project".to_string()),
        };
        let json = serde_json::to_string(&runtime).unwrap();
        assert!(!json.contains("wc_pat_"));
        assert!(!json.contains("wc_agent_"));
        assert!(!json.contains("CONTROL_PLANE_API_KEY"));
    }

    #[tokio::test]
    async fn explicit_stop_survives_refresh_and_desktop_restart() {
        let data_dir = unique_state_dir("stopped-refresh");
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("resources")).unwrap();
        core.config = test_stored_config("stopped");
        core.config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        let cancellation = CancellationContext::never();
        core.stop_local_runtime(&cancellation).await.unwrap();
        let refreshed = core.refresh_runtime_status(&cancellation).await.unwrap();
        assert_eq!(
            refreshed.readiness.summary_kind,
            ReadinessSummaryKind::RuntimeStopped
        );
        assert!(!refreshed.runtime_autostart);
        let restarted = DesktopCore::new(data_dir.clone(), data_dir.join("resources")).unwrap();
        assert_eq!(
            restarted.snapshot.readiness.summary_kind,
            ReadinessSummaryKind::RuntimeStopped
        );
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn legacy_full_runtime_defaults_to_autostart_but_explicit_stop_is_preserved() {
        let mut config = test_stored_config("resume");
        config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        config.runtime = Some(StoredRuntime {
            server_url: "http://127.0.0.1:58208".to_string(),
            server_env_file: None,
            runner_config: None,
            user_token_file: None,
            runner_client_id: None,
            project_id: None,
            runtime_project_id: None,
        });
        assert!(runtime_autostart(&config));

        config.runtime_autostart = Some(false);
        assert!(!runtime_autostart(&config));
    }

    #[test]
    fn explicit_tunnel_proxy_is_bounded_and_direct_mode_clears_routing() {
        let custom = TunnelProxyConfig {
            mode: TunnelProxyMode::Custom,
            custom_url: Some("http://127.0.0.1:7890".to_string()),
        };
        let effective = effective_tunnel_proxy(&custom).expect("valid custom proxy");
        assert_eq!(effective.url.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(effective.source, "custom");

        let direct = effective_tunnel_proxy(&TunnelProxyConfig {
            mode: TunnelProxyMode::Direct,
            custom_url: Some("http://ignored.example.test:8080".to_string()),
        })
        .expect("direct mode");
        assert_eq!(direct.url, None);
        assert_eq!(direct.source, "direct");

        assert!(validate_tunnel_proxy_url("http://user:secret@127.0.0.1:7890").is_err());
    }

    #[test]
    fn tunnel_proxy_resolution_has_explicit_precedence() {
        let auto = TunnelProxyConfig::default();
        let system = crate::platform::SystemProxyCandidate {
            url: "http://127.0.0.1:7890".to_string(),
        };
        let environment = "http://environment.example.test:8080".to_string();

        let resolved =
            resolve_tunnel_proxy(&auto, Some(environment.clone()), Some(system.clone())).unwrap();
        assert_eq!(resolved.source, "environment");
        assert_eq!(resolved.url.as_deref(), Some(environment.as_str()));
        assert!(resolved.system_proxy_detected);

        let resolved = resolve_tunnel_proxy(&auto, None, Some(system.clone())).unwrap();
        assert_eq!(resolved.source, "system");
        assert_eq!(resolved.url.as_deref(), Some(system.url.as_str()));
        assert!(resolved.system_proxy_detected);

        let resolved = resolve_tunnel_proxy(&auto, None, None).unwrap();
        assert_eq!(resolved.source, "direct");
        assert_eq!(resolved.url, None);
        assert!(!resolved.system_proxy_detected);

        let custom = TunnelProxyConfig {
            mode: TunnelProxyMode::Custom,
            custom_url: Some("http://custom.example.test:9000".to_string()),
        };
        let resolved =
            resolve_tunnel_proxy(&custom, Some(environment.clone()), Some(system.clone())).unwrap();
        assert_eq!(resolved.source, "custom");
        assert_eq!(
            resolved.url.as_deref(),
            Some("http://custom.example.test:9000")
        );

        let direct = TunnelProxyConfig {
            mode: TunnelProxyMode::Direct,
            custom_url: Some("http://ignored.example.test:9000".to_string()),
        };
        let resolved = resolve_tunnel_proxy(&direct, Some(environment), Some(system)).unwrap();
        assert_eq!(resolved.source, "direct");
        assert_eq!(resolved.url, None);
        assert!(resolved.system_proxy_detected);
    }

    #[test]
    fn failed_runtime_resume_restores_the_last_published_state() {
        let data_dir = unique_state_dir("resume-failure-reconcile");
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("resources"))
            .expect("create Desktop core");
        let mut baseline_snapshot = DesktopStateSnapshot::default();
        baseline_snapshot.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        baseline_snapshot.readiness = aggregate_readiness(
            ServerReadiness::Stopped,
            RunnerReadiness::Stopped,
            ExposureReadiness::LocalReady,
            ProjectReadiness::Configured,
        );
        let baseline = ProcessBaseline {
            generations: Default::default(),
            snapshot: baseline_snapshot.clone(),
        };
        core.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Connecting,
            ExposureReadiness::LocalReady,
            ProjectReadiness::Configured,
        );

        core.reconcile_after_operation_failure(
            DesktopOperationKind::RuntimeResume,
            &baseline,
            ProcessCleanup {
                local_server: true,
                local_runner: true,
                ..ProcessCleanup::default()
            },
            false,
        );

        assert_eq!(core.snapshot.readiness, baseline_snapshot.readiness);
        assert_eq!(core.snapshot.topology, baseline_snapshot.topology);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn invalid_stored_identity_is_not_advertised_for_reuse() {
        let config = StoredDesktopConfig {
            saved_projects: Vec::new(),
            topology: Some(RuntimeTopology {
                experience: Experience::Full,
                server: ServerTopology::Remote {
                    url: "https://example.test".to_string(),
                },
                runner: RunnerTopology::Local,
                exposure: Exposure::ExistingHttps {
                    url: "https://example.test".to_string(),
                },
                enrollment: Enrollment::ManagedPairing,
            }),
            project: Some(ProjectSelection {
                path: r"C:\repo".to_string(),
                allowed_root: r"C:\".to_string(),
                is_git_repository: true,
                runtime_project_id: Some("agent:desktop:repo".to_string()),
            }),
            runtime_autostart: None,
            preferred_connection: None,
            tunnel_proxy: TunnelProxyConfig::default(),
            runtime: Some(StoredRuntime {
                server_url: "https://example.test".to_string(),
                server_env_file: None,
                runner_config: Some(PathBuf::from("missing-runner.toml")),
                user_token_file: Some(PathBuf::from("missing-user-token")),
                runner_client_id: Some("desktop".to_string()),
                project_id: Some("repo".to_string()),
                runtime_project_id: Some("agent:desktop:repo".to_string()),
            }),
            ..StoredDesktopConfig::default()
        };
        assert_eq!(
            project_snapshot(&config).and_then(|project| project.runtime_project_id),
            None
        );
    }

    #[tokio::test]
    async fn control_plane_stays_observable_and_cancel_is_exact_while_mutation_is_stuck() {
        let data_dir = std::env::temp_dir().join(format!(
            "webcodex-desktop-control-plane-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let state = Arc::new(
            AppState::new(data_dir.clone(), data_dir.join("test-resources"))
                .expect("create Desktop test state"),
        );
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let operation_state = Arc::clone(&state);
        let operation = tokio::spawn(async move {
            operation_state
                .hold_test_operation(started_tx, release_rx)
                .await
        });
        let first_id = tokio::time::timeout(Duration::from_secs(1), started_rx)
            .await
            .expect("stuck operation must start")
            .expect("operation id");

        let observed = tokio::time::timeout(Duration::from_millis(250), async {
            (state.get_state(), state.activity())
        })
        .await
        .expect("control-plane reads must not wait for the mutation core");
        assert_eq!(
            observed
                .0
                .current_operation
                .as_ref()
                .map(|operation| operation.id.as_str()),
            Some(first_id.as_str())
        );
        assert_eq!(
            observed
                .0
                .current_operation
                .as_ref()
                .map(|operation| operation.phase),
            Some(crate::models::DesktopOperationPhase::Running)
        );
        assert!(
            !observed.1.is_empty(),
            "Activity must stay independently readable"
        );

        let busy = tokio::time::timeout(Duration::from_millis(250), state.refresh_runtime_status())
            .await
            .expect("second mutation must fail fast")
            .expect_err("second mutation must not queue behind the stuck operation");
        assert_eq!(busy.code, "desktop_operation_busy");

        let cancelling = state
            .cancel_operation(&first_id)
            .expect("exact observed operation can be stopped");
        assert_eq!(
            cancelling
                .current_operation
                .as_ref()
                .map(|operation| operation.phase),
            Some(crate::models::DesktopOperationPhase::Cancelling)
        );
        let still_busy =
            tokio::time::timeout(Duration::from_millis(250), state.refresh_runtime_status())
                .await
                .expect("cancelling operation must retain the mutation slot")
                .expect_err("cleanup has not completed yet");
        assert_eq!(still_busy.code, "desktop_operation_busy");

        release_tx.send(()).expect("release first cleanup");
        let first_error = operation
            .await
            .expect("first operation task")
            .expect_err("first operation was cancelled");
        assert_eq!(first_error.code, "desktop_operation_cancelled");
        assert!(state.get_state().current_operation.is_none());

        let (second_started_tx, second_started_rx) = tokio::sync::oneshot::channel();
        let (second_release_tx, second_release_rx) = tokio::sync::oneshot::channel();
        let second_state = Arc::clone(&state);
        let second_operation = tokio::spawn(async move {
            second_state
                .hold_test_operation(second_started_tx, second_release_rx)
                .await
        });
        let second_id = tokio::time::timeout(Duration::from_secs(1), second_started_rx)
            .await
            .expect("second operation must start")
            .expect("second operation id");
        assert_ne!(first_id, second_id);

        let stale = state
            .cancel_operation(&first_id)
            .expect_err("late cancel for A must not target B");
        assert_eq!(stale.code, "desktop_operation_not_current");
        let second_snapshot = state.get_state();
        assert_eq!(
            second_snapshot
                .current_operation
                .as_ref()
                .map(|operation| operation.id.as_str()),
            Some(second_id.as_str())
        );
        assert_eq!(
            second_snapshot
                .current_operation
                .as_ref()
                .map(|operation| operation.phase),
            Some(crate::models::DesktopOperationPhase::Running)
        );

        state
            .cancel_operation(&second_id)
            .expect("exact second cancel");
        second_release_tx.send(()).expect("release second cleanup");
        let second_error = second_operation
            .await
            .expect("second operation task")
            .expect_err("second operation was cancelled");
        assert_eq!(second_error.code, "desktop_operation_cancelled");
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[cfg(target_os = "macos")]
    fn mac_process_exists(pid: u32) -> bool {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        let result = unsafe { libc::kill(pid, 0) };
        if result == 0 {
            return true;
        }
        !matches!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        )
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn shutdown_cancels_stuck_one_shot_and_reclaims_all_desktop_owned_trees() {
        let data_dir = std::env::temp_dir().join(format!(
            "webcodex-desktop-shutdown-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let long_marker = data_dir.join("long-lived-pids.txt");
        let one_shot_marker = data_dir.join("one-shot-pids.txt");
        std::fs::create_dir_all(&data_dir).expect("create shutdown fixture dir");
        let state = Arc::new(
            AppState::new(data_dir.clone(), data_dir.join("test-resources"))
                .expect("create Desktop shutdown test state"),
        );

        let mut long_command = std::process::Command::new("/bin/sh");
        long_command.args([
            "-c",
            "sleep 8 & descendant=$!; printf '%s %s\\n' \"$$\" \"$descendant\" > \"$1\"; wait \"$descendant\"",
            "webcodex-long-lived-shutdown",
            &long_marker.to_string_lossy(),
        ]);
        state
            .supervisor
            .lock()
            .await
            .spawn_owned(ProcessKey::LocalServer, long_command, false)
            .await
            .expect("start long-lived Desktop-owned fixture");

        let marker_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while !long_marker.is_file() {
            assert!(
                tokio::time::Instant::now() < marker_deadline,
                "long-lived fixture did not start"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let long_pids = std::fs::read_to_string(&long_marker)
            .expect("long-lived fixture pids")
            .split_whitespace()
            .map(|value| value.parse::<u32>().expect("long-lived fixture pid"))
            .collect::<Vec<_>>();

        let args = vec![
            "-c".to_string(),
            "sleep 8 & descendant=$!; printf '%s %s\\n' \"$$\" \"$descendant\" > \"$1\"; wait \"$descendant\"".to_string(),
            "webcodex-one-shot-shutdown".to_string(),
            one_shot_marker.to_string_lossy().to_string(),
        ];
        let operation_state = Arc::clone(&state);
        let operation = tokio::spawn(async move {
            operation_state
                .run_test_one_shot_operation(
                    PathBuf::from("/bin/sh"),
                    args,
                    vec![b'x'; 64 * 1024],
                    Duration::from_secs(8),
                )
                .await
        });
        let marker_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while !one_shot_marker.is_file() {
            assert!(
                tokio::time::Instant::now() < marker_deadline,
                "one-shot fixture did not start"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let one_shot_pids = std::fs::read_to_string(&one_shot_marker)
            .expect("one-shot fixture pids")
            .split_whitespace()
            .map(|value| value.parse::<u32>().expect("one-shot fixture pid"))
            .collect::<Vec<_>>();

        tokio::time::timeout(Duration::from_secs(7), state.shutdown())
            .await
            .expect("shutdown must remain bounded during a stuck one-shot operation");
        let operation_error = operation
            .await
            .expect("one-shot operation task")
            .expect_err("shutdown must cancel the one-shot operation");
        assert_eq!(operation_error.code, "desktop_operation_cancelled");
        for pid in long_pids.into_iter().chain(one_shot_pids) {
            assert!(
                !mac_process_exists(pid),
                "Desktop-owned PID {pid} survived application shutdown"
            );
        }
        assert!(state
            .supervisor
            .lock()
            .await
            .snapshot(ProcessKey::LocalServer)
            .is_none());
        tokio::time::timeout(Duration::from_millis(250), state.shutdown())
            .await
            .expect("repeated shutdown must be idempotent and fast");
        let _ = std::fs::remove_dir_all(data_dir);
    }

    /// Serializes tests that mutate process environment variables.
    ///
    /// Env mutation is process-global, so env-mutating tests in this test
    /// binary hold one shared lock for their whole body. Desktop is its own
    /// Cargo workspace, so this per-crate `TEST_ENV_LOCK` follows the
    /// convention documented in docs/TESTING.md.
    static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Restores one process environment variable when the guard is dropped,
    /// including through panic unwinding. Holding `TEST_ENV_LOCK` for the
    /// guard's lifetime also serializes env mutation across sibling tests;
    /// poisoning is tolerated because the RAII restore already fixed the
    /// environment before the lock is released.
    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
        _test_env_lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let test_env_lock = TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self {
                key,
                previous,
                _test_env_lock: test_env_lock,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn native_local_full_dogfood_reuses_enrollment_and_stops_owned_runtime() {
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        // The Server only accepts lowercase local pairing usernames. Pin a
        // mixed-case OS username so the compatibility path is exercised on
        // every machine, not only where the login name already fails. The
        // guard restores the previous value on every exit path, including
        // panics, so this override never leaks to sibling tests.
        let _username_guard = EnvVarGuard::set("USERNAME", "Alice Dogfood");
        // macOS exposes its temporary root through /var -> /private/var.
        // Resolve the fixture root, not the credential-store security checks.
        let temporary_root = std::env::temp_dir()
            .canonicalize()
            .expect("resolve the native temporary fixture root");
        let data_dir = temporary_root.join(format!(
            "webcodex-desktop-local-dogfood-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("test-resources"))
            .expect("create local dogfood state");
        let cancellation = CancellationContext::never();
        let setup = core
            .configure_local_setup(Some(&project), &cancellation)
            .await;
        let snapshot = match setup {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let stages: Vec<_> = core
                    .activity
                    .snapshot()
                    .into_iter()
                    .map(|entry| entry.event_kind)
                    .collect();
                let readiness = core.snapshot.readiness.clone();
                let initialized = data_dir.join("runtime/local/webcodex.env").is_file();
                core.supervisor.lock().await.stop_all().await;
                let _ = std::fs::remove_dir_all(&data_dir);
                panic!(
                    "local full setup failed: {error:?}; initialized={initialized}; readiness={readiness:?}; stages={stages:?}"
                );
            }
        };
        assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
        assert_eq!(snapshot.readiness.exposure, ExposureReadiness::LocalReady);
        assert!(snapshot.readiness.runtime_ready);
        assert!(!snapshot.readiness.ready_for_chatgpt);

        let first_runtime = core
            .config
            .runtime
            .as_ref()
            .expect("local setup stores runtime identity");
        let first_user_token_file = first_runtime
            .user_token_file
            .clone()
            .expect("local setup stores managed user token path");
        let first_user_token =
            std::fs::read(&first_user_token_file).expect("read managed user token before restart");
        let first_runner_config = first_runtime.runner_config.clone().unwrap();
        let first_runner_bytes = std::fs::read(&first_runner_config).unwrap();

        let stopped = core
            .stop_local_runtime(&cancellation)
            .await
            .expect("stop local runtime");
        assert_eq!(stopped.readiness.server, ServerReadiness::Stopped);
        assert_eq!(stopped.readiness.runner, RunnerReadiness::Stopped);
        assert!(core
            .process_snapshot(ProcessKey::LocalServer)
            .await
            .is_none());
        assert!(core
            .process_snapshot(ProcessKey::LocalRunner)
            .await
            .is_none());

        let restarted = core
            .configure_local_setup(Some(&project), &cancellation)
            .await
            .expect("restart local full setup without re-enrollment");
        assert_eq!(restarted.readiness.server, ServerReadiness::Ready);
        assert_eq!(restarted.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(restarted.readiness.project, ProjectReadiness::Ready);
        let second_user_token_file = core
            .config
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.user_token_file.clone())
            .expect("restarted local setup keeps managed user token path");
        assert_eq!(second_user_token_file, first_user_token_file);
        let second_user_token =
            std::fs::read(&second_user_token_file).expect("read managed user token after restart");
        assert!(
            first_user_token == second_user_token,
            "local restart must reuse enrollment instead of rotating the managed user token"
        );
        let project_a_identity =
            identity_from_config(&core.config).expect("project A runtime identity after restart");
        let first_runner_client_id =
            stored_runner_client_id(&core.config).expect("stored Runner client identity");
        let server_before_switch = core
            .process_snapshot(ProcessKey::LocalServer)
            .await
            .expect("Desktop owns local Server before project switch");
        let runner_before_switch = core
            .process_snapshot(ProcessKey::LocalRunner)
            .await
            .expect("Desktop owns local Runner before project switch");
        assert!(server_before_switch.owned_by_desktop);
        assert!(runner_before_switch.owned_by_desktop);
        // Switch projects while the Desktop-owned Server and Runner are still
        // running. A compatible connection hot-extends exact-root authority and
        // activates the new Project without replacing either owned process.
        let second_project = data_dir.join("second-project");
        std::fs::create_dir_all(&second_project).expect("create second project fixture");
        let expected_project = core
            .adapter
            .inspect_project(&second_project.to_string_lossy())
            .await
            .expect("inspect second project fixture");
        let switched = match core
            .activate_local_project(&second_project.to_string_lossy(), &cancellation)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let readiness = core.snapshot.readiness.clone();
                core.supervisor.lock().await.stop_all().await;
                let _ = std::fs::remove_dir_all(&data_dir);
                panic!("switch project setup failed: {error:?}; readiness={readiness:?}");
            }
        };
        assert_eq!(switched.readiness.server, ServerReadiness::Ready);
        assert_eq!(switched.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(switched.readiness.project, ProjectReadiness::Ready);
        assert!(same_project(
            &core.config.project.as_ref().expect("selected project").path,
            &expected_project.path,
        ));
        let server_after_switch = core
            .process_snapshot(ProcessKey::LocalServer)
            .await
            .expect("Desktop still owns local Server after project switch");
        let runner_after_switch = core
            .process_snapshot(ProcessKey::LocalRunner)
            .await
            .expect("Desktop still owns the same Runner after project switch");
        assert_eq!(
            server_after_switch.pid, server_before_switch.pid,
            "project switch must keep the existing Desktop-owned Server"
        );
        assert_eq!(
            runner_after_switch.pid, runner_before_switch.pid,
            "compatible project switch must preserve the Desktop-owned Runner PID"
        );
        assert!(server_after_switch.owned_by_desktop);
        assert!(runner_after_switch.owned_by_desktop);
        let switched_identity =
            identity_from_config(&core.config).expect("project B runtime identity after switch");
        assert_eq!(
            stored_runner_client_id(&core.config).as_deref(),
            Some(first_runner_client_id.as_str()),
            "project switch must preserve the Runner client identity"
        );
        assert_eq!(
            switched_identity.runner_config, first_runner_config,
            "project switch must hot-update the same Runner config"
        );
        assert_eq!(
            switched_identity.user_token_file, first_user_token_file,
            "project switch must preserve the managed user-token path"
        );
        assert_ne!(
            switched_identity.runtime_project_id, project_a_identity.runtime_project_id,
            "different canonical projects must keep distinct runtime Project identities"
        );
        let config_after_switch = std::fs::read(&first_runner_config).unwrap();
        assert_ne!(
            config_after_switch, first_runner_bytes,
            "switching to a new exact root must persist the policy candidate in the existing Runner config"
        );
        assert_eq!(
            std::fs::read(&first_user_token_file).unwrap(),
            first_user_token,
            "switching projects must not rotate the managed user token"
        );
        assert!(
            core.adapter
                .project_ready(&project_a_identity, &cancellation)
                .await
                .expect("project A inventory remains observable"),
            "activating Project B must not delete Project A registration"
        );

        let project_b_runtime_id = switched_identity.runtime_project_id.clone();
        let repeated = core
            .activate_local_project(&second_project.to_string_lossy(), &cancellation)
            .await
            .expect("reselecting Project B is idempotent");
        assert_eq!(repeated.readiness.project, ProjectReadiness::Ready);
        let runner_after_repeat = core
            .process_snapshot(ProcessKey::LocalRunner)
            .await
            .expect("Desktop still owns Runner after idempotent activation");
        assert_eq!(
            runner_after_repeat.pid, runner_before_switch.pid,
            "idempotent project activation must not restart the Runner"
        );
        let repeated_identity = identity_from_config(&core.config)
            .expect("Project B identity after repeated activation");
        assert_eq!(repeated_identity.runtime_project_id, project_b_runtime_id);
        assert_eq!(
            stored_runner_client_id(&core.config),
            Some(first_runner_client_id)
        );
        assert_eq!(
            std::fs::read(&first_runner_config).unwrap(),
            config_after_switch,
            "idempotent activation must not duplicate or rewrite allowed_roots"
        );
        assert_eq!(
            std::fs::read(&first_user_token_file).unwrap(),
            first_user_token,
            "idempotent activation must keep the same managed user token"
        );
        core.stop_local_runtime(&cancellation)
            .await
            .expect("stop restarted local runtime");
        drop(core);
        std::fs::remove_dir_all(&data_dir).expect("remove local dogfood app data");
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn native_local_stale_loopback_port_dogfood_recovers_to_ready() {
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        let _username_guard = EnvVarGuard::set("USERNAME", "Alice Port Recovery");
        let temporary_root = std::env::temp_dir()
            .canonicalize()
            .expect("resolve the native temporary fixture root");
        let data_dir = temporary_root.join(format!(
            "webcodex-desktop-stale-port-dogfood-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("test-resources"))
            .expect("create stale-port dogfood state");
        let cancellation = CancellationContext::never();

        let first = core
            .configure_local_setup(Some(&project), &cancellation)
            .await
            .expect("initial local setup");
        assert_eq!(first.readiness.server, ServerReadiness::Ready);
        assert_eq!(first.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(first.readiness.project, ProjectReadiness::Ready);
        let first_runtime = core
            .config
            .runtime
            .as_ref()
            .expect("initial setup stores runtime")
            .clone();
        let first_url = first_runtime.server_url.clone();
        let first_addr =
            loopback_socket_from_server_url(&first_url).expect("initial local loopback address");
        let env_file = first_runtime
            .server_env_file
            .clone()
            .expect("initial local setup stores Server env file");

        core.stop_local_runtime(&cancellation)
            .await
            .expect("stop initial runtime");
        assert!(core
            .process_snapshot(ProcessKey::LocalServer)
            .await
            .is_none());
        assert!(core
            .process_snapshot(ProcessKey::LocalRunner)
            .await
            .is_none());

        let occupied = TcpListener::bind(first_addr)
            .expect("occupy the saved local Server port before restart");
        let restarted = core
            .configure_local_setup(Some(&project), &cancellation)
            .await
            .expect("restart must recover stale persisted loopback port");
        assert_eq!(restarted.readiness.server, ServerReadiness::Ready);
        assert_eq!(restarted.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(restarted.readiness.project, ProjectReadiness::Ready);

        let second_runtime = core
            .config
            .runtime
            .as_ref()
            .expect("recovered setup stores runtime");
        assert_ne!(
            second_runtime.server_url, first_url,
            "port conflict must migrate the local Server URL"
        );
        let second_addr = loopback_socket_from_server_url(&second_runtime.server_url)
            .expect("recovered local loopback address");
        assert_ne!(second_addr, first_addr);
        assert_eq!(
            occupied
                .local_addr()
                .expect("occupied address remains live"),
            first_addr,
            "Desktop recovery must not disturb the external process holding the old port"
        );

        let env = std::fs::read_to_string(&env_file).expect("read recovered Server env");
        assert!(env.contains(&format!("WEBCODEX_ADDR={second_addr}")));
        assert!(!env.contains(&format!("WEBCODEX_ADDR={first_addr}")));
        assert!(
            core.activity
                .snapshot()
                .iter()
                .any(|entry| entry.event_kind == ActivityEventKind::StateRecovered),
            "stale port migration must leave a safe recovery activity event"
        );

        core.stop_local_runtime(&cancellation)
            .await
            .expect("stop recovered runtime");
        drop(occupied);
        drop(core);
        std::fs::remove_dir_all(&data_dir).expect("remove stale-port dogfood app data");
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn windows_quick_share_dogfood_reaches_ready_and_stops_foreground_owner() {
        if !cfg!(windows) {
            return;
        }
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        let data_dir = std::env::temp_dir().join(format!(
            "webcodex-desktop-share-dogfood-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("test-resources"))
            .expect("create Quick Share dogfood state");
        let cancellation = CancellationContext::never();
        let started = core
            .start_quick_share(&project, "none", &cancellation)
            .await;
        let snapshot = match started {
            Ok(snapshot) => snapshot,
            Err(error) => {
                core.supervisor.lock().await.stop_all().await;
                let _ = std::fs::remove_dir_all(&data_dir);
                panic!("Quick Share setup failed: {error:?}");
            }
        };
        assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
        assert_eq!(snapshot.readiness.exposure, ExposureReadiness::LocalReady);
        assert!(snapshot.readiness.runtime_ready);
        assert!(!snapshot.readiness.ready_for_chatgpt);
        assert!(core
            .process_snapshot(ProcessKey::QuickShare)
            .await
            .is_some());

        core.stop_quick_share(&cancellation)
            .await
            .expect("stop Quick Share");
        assert!(core
            .process_snapshot(ProcessKey::QuickShare)
            .await
            .is_none());
        drop(core);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn windows_remote_full_dogfood_reuses_enrollment_without_local_server() {
        if !cfg!(windows) {
            return;
        }
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        let suffix = std::process::id();
        let host_data =
            std::env::temp_dir().join(format!("webcodex-desktop-remote-host-dogfood-{suffix}"));
        let client_data =
            std::env::temp_dir().join(format!("webcodex-desktop-remote-client-dogfood-{suffix}"));
        let _ = std::fs::remove_dir_all(&host_data);
        let _ = std::fs::remove_dir_all(&client_data);

        let mut host = DesktopCore::new(host_data.clone(), host_data.join("test-resources"))
            .expect("create remote dogfood host state");
        let cancellation = CancellationContext::never();
        let host_runtime = host_data.join("runtime");
        let env_file = host_runtime.join("webcodex.env");
        let data_dir = host_runtime.join("data");
        tokio::fs::create_dir_all(&host_runtime)
            .await
            .expect("create remote dogfood host state");
        let listen = reserve_loopback_address().expect("reserve remote dogfood host port");
        let server_url = host
            .adapter
            .init_local_server(&listen, &data_dir, &env_file, &cancellation)
            .await
            .expect("initialize remote dogfood Server")
            .probe_url;
        let command = host
            .adapter
            .local_server_command(&env_file)
            .expect("build remote dogfood Server command");
        host.spawn_owned(ProcessKey::LocalServer, command, false, &cancellation)
            .await
            .expect("start remote dogfood Server");

        let mut client = DesktopCore::new(client_data.clone(), client_data.join("test-resources"))
            .expect("create remote dogfood client state");
        let result: DesktopResult<(DesktopStateSnapshot, DesktopStateSnapshot, bool, bool)> =
            async {
                host.wait_for_server(
                    &server_url,
                    Some(&env_file),
                    None,
                    &cancellation,
                    Deadline::after(SERVER_READY_TIMEOUT),
                    true,
                )
                .await?;
                let pairing_code = host
                    .adapter
                    .create_local_pairing(&server_url, &env_file, &cancellation)
                    .await?;
                let first = client
                    .configure_remote_setup(&server_url, &pairing_code, &project, &cancellation)
                    .await?;
                let first_started_server = client
                    .process_snapshot(ProcessKey::LocalServer)
                    .await
                    .is_some();
                client.stop_local_runtime(&cancellation).await?;

                let second = client
                    .configure_remote_setup(&server_url, "", &project, &cancellation)
                    .await?;
                let second_started_server = client
                    .process_snapshot(ProcessKey::LocalServer)
                    .await
                    .is_some();
                client.stop_local_runtime(&cancellation).await?;
                Ok((first, second, first_started_server, second_started_server))
            }
            .await;

        client.supervisor.lock().await.stop_all().await;
        host.supervisor.lock().await.stop_all().await;
        let _ = std::fs::remove_dir_all(&client_data);
        let _ = std::fs::remove_dir_all(&host_data);

        let (first, second, first_started_server, second_started_server) =
            result.expect("remote full dogfood should complete");
        for snapshot in [&first, &second] {
            assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
            assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
            assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
            assert!(snapshot.readiness.runtime_ready);
            assert!(matches!(
                snapshot.topology.as_ref().map(|topology| &topology.server),
                Some(ServerTopology::Remote { .. })
            ));
        }
        assert!(!first_started_server);
        assert!(!second_started_server);
    }

    #[test]
    fn configured_https_exposure_stays_unverified_without_mcp_handoff_evidence() {
        let local = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        };
        let remote = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Remote {
                url: "https://example.com".to_string(),
            },
            runner: RunnerTopology::Local,
            exposure: Exposure::ExistingHttps {
                url: "https://example.com".to_string(),
            },
            enrollment: Enrollment::ManagedPairing,
        };
        assert_eq!(
            exposure_readiness(Some(&local)),
            ExposureReadiness::LocalReady
        );
        assert_eq!(
            exposure_readiness(Some(&remote)),
            ExposureReadiness::Unknown
        );
    }
}
