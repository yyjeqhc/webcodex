use super::*;
use serde_json::json;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("desktop-projectless-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// A real CLI transport with closed stdin and captured output; the fixture only
// supports observation. Any login, activation, or process start fails the test.
#[cfg(unix)]
#[tokio::test]
async fn restart_projectless_full_runtime_observes_without_initial_setup_or_registration() {
    use crate::webcodex::cli::{ResolvedBinaries, ResolvedBinarySource};
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    for local in [true, false] {
        let fixture = Fixture::new();
        let data = &fixture.0;
        let runner = data.join("runner.toml");
        let token = data.join("token");
        std::fs::write(&runner, "fixture").unwrap();
        std::fs::write(&token, "fixture").unwrap();
        let mut core = DesktopCore::new(data.clone(), data.join("resources")).unwrap();
        core.config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: if local {
                ServerTopology::Local
            } else {
                ServerTopology::Remote {
                    url: "http://127.0.0.1:1".into(),
                }
            },
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        core.config.runtime = Some(StoredRuntime {
            server_url: "http://127.0.0.1:1".into(),
            server_env_file: None,
            runner_config: Some(runner.clone()),
            user_token_file: Some(token),
            runner_client_id: Some("mini".into()),
            project_id: None,
            runtime_project_id: None,
        });
        core.save_config().await.unwrap();
        let mut core = DesktopCore::new(data.clone(), data.join("resources")).unwrap();
        let cli = data.join("webcodex");
        let server = json!({"http_reachable":true,"probe_url":"http://127.0.0.1:1","desktop_runtime_contract":webcodex_core::desktop_runtime_contract::DESKTOP_RUNTIME_CONTRACT});
        let status = json!({"config":{"path":runner,"client_id":"mini","server_url":"http://127.0.0.1:1"},"runtime":{"checked":true,"reachable":true,"client_online":true}});
        std::fs::write(&cli, format!("#!/bin/sh\ncase \"$1 $2\" in\n'server status') printf '%s' '{}' ;;\n'runner status') printf '%s' '{}' ;;\n*) exit 91 ;;\nesac\n", server, status)).unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut hash = Sha256::new();
        hash.update(data.to_string_lossy().as_bytes());
        for name in ["webcodex", "webcodex-server", "webcodex-runner"] {
            if name != "webcodex" {
                std::fs::copy(&cli, data.join(name)).unwrap();
            }
            hash.update(
                crate::runtime_selection::file_digest(&data.join(name))
                    .unwrap()
                    .as_bytes(),
            );
        }
        let fingerprint = format!("{:x}", hash.finalize());
        core.adapter.activate_binaries(
            Default::default(),
            ResolvedBinaries {
                directory: data.clone(),
                webcodex: cli.clone(),
                server: cli.clone(),
                runner: cli,
                version: "fixture".into(),
                git_commit: "fixture".into(),
                source: ResolvedBinarySource::Bundled,
                builds: vec![],
                fingerprint,
            },
        );
        let snapshot = core
            .resume_saved_runtime(&CancellationContext::never())
            .await
            .unwrap();
        assert!(snapshot.readiness.runtime_ready);
        assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(snapshot.readiness.project, ProjectReadiness::None);
        assert!(snapshot.project.is_none());
        assert!(snapshot.chatgpt_activity.is_none());
        assert!(snapshot.workspace_runner.is_some());
        let reloaded = DesktopCore::new(data.clone(), data.join("resources")).unwrap();
        assert!(reloaded.config.project.is_none());
        assert!(runner_identity_from_config(&reloaded.config).is_some());
        assert!(identity_from_config(&reloaded.config).is_none());
        assert!(core
            .process_snapshot(ProcessKey::LocalRunner)
            .await
            .is_none());
        assert!(core
            .process_snapshot(ProcessKey::LocalServer)
            .await
            .is_none());
    }
}

#[tokio::test]
async fn complete_inventory_persists_only_changed_history_and_retries_failed_persistence() {
    let fixture = Fixture::new();
    let data = &fixture.0;
    let mut core = DesktopCore::new(data.clone(), data.join("resources")).unwrap();
    let selected = ProjectSelection {
        path: "/repos/a".into(),
        allowed_root: "/repos/a".into(),
        is_git_repository: true,
        runtime_project_id: Some("agent:mini:a".into()),
    };
    core.config.project = Some(selected.clone());
    core.config.runtime = Some(StoredRuntime {
        server_url: "http://127.0.0.1:1".into(),
        server_env_file: None,
        runner_config: Some(data.join("runner.toml")),
        user_token_file: None,
        runner_client_id: Some("mini".into()),
        project_id: Some("a".into()),
        runtime_project_id: Some("agent:mini:a".into()),
    });
    core.save_config().await.unwrap();
    let complete = json!({"client_id":"mini","connected":true,"projects_available":true,"projects_truncated":false,"visible_project_count":0,"projects":[]});
    let before = std::fs::read(&core.config_path).unwrap();
    let mut partial = complete.clone();
    partial["projects_truncated"] = json!(true);
    core.reconcile_inventory(&partial).await;
    assert_eq!(std::fs::read(&core.config_path).unwrap(), before);
    // Force a local persistence error without changing the authoritative reply.
    let real_path = core.config_path.clone();
    core.config_path = data.clone();
    core.reconcile_inventory(&complete).await;
    assert!(core.config.project.is_none());
    assert!(core.config.saved_projects.is_empty());
    assert!(core.inventory_persistence_pending);
    assert_eq!(std::fs::read(&real_path).unwrap(), before);
    core.config_path = real_path;
    core.reconcile_inventory(&complete).await;
    let reloaded = DesktopCore::new(data.clone(), data.join("resources")).unwrap();
    assert!(reloaded.config.project.is_none());
    assert!(reloaded.config.saved_projects.is_empty());
    let modified = std::fs::metadata(&core.config_path)
        .unwrap()
        .modified()
        .unwrap();
    core.reconcile_inventory(&complete).await;
    assert_eq!(
        std::fs::metadata(&core.config_path)
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    // Canonical unregister may already have cleared memory when its write fails.
    core.inventory_persistence_pending = true;
    core.reconcile_inventory(&complete).await;
    assert!(!core.inventory_persistence_pending);
}

#[tokio::test]
async fn workspace_observation_fences_completed_concurrent_operations_and_persists_inventory() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for concurrent in [false, true] {
        let fixture = Fixture::new();
        let data = &fixture.0;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let token = data.join("token");
        std::fs::write(&token, "fixture-only").unwrap();
        let app = Arc::new(AppState::new(data.clone(), data.join("resources")).unwrap());
        {
            let mut slot = app.core.lock().await;
            let core = slot.as_mut().unwrap();
            core.config.runtime = Some(StoredRuntime {
                server_url: url,
                server_env_file: None,
                runner_config: Some(data.join("runner.toml")),
                user_token_file: Some(token),
                runner_client_id: Some("mini".into()),
                project_id: Some("a".into()),
                runtime_project_id: Some("agent:mini:a".into()),
            });
            core.config.project = Some(ProjectSelection {
                path: "/repos/a".into(),
                allowed_root: "/repos/a".into(),
                is_git_repository: true,
                runtime_project_id: Some("agent:mini:a".into()),
            });
            core.save_config().await.unwrap();
        }
        let (received_tx, received_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            let (mut stream, _) = tokio::time::timeout_at(deadline, listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 2048];
                let n = tokio::time::timeout_at(deadline, stream.read(&mut chunk))
                    .await
                    .unwrap()
                    .unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 8192);
                if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    assert!(header.starts_with("post /api/runtime-console/runner "));
                    let length: usize = header
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            received_tx.send(()).unwrap();
            tokio::time::timeout_at(deadline, release_rx)
                .await
                .unwrap()
                .unwrap();
            let body = json!({"client_id":"mini","connected":true,"projects_available":true,"projects_truncated":false,
                "visible_project_count":1,"projects":[{"id":"agent:mini:b","path":"/repos/b"}]}).to_string();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
        });
        let query_app = app.clone();
        let query = tokio::spawn(async move {
            query_app
                .workspace_query(crate::workspace::WorkspaceRequest::Overview {})
                .await
        });
        received_rx.await.unwrap();
        if concurrent {
            // Same config before/after: the generation fence also rejects ABA.
            let operation = app
                .operations
                .admit(DesktopOperationKind::RuntimeRefresh, false)
                .unwrap();
            app.operations
                .finish(&operation.id, &Ok::<_, DesktopError>(()));
        }
        release_tx.send(()).unwrap();
        let result = query.await.unwrap();
        server.await.unwrap();
        assert_eq!(result.is_err(), concurrent);
        let reloaded = DesktopCore::new(data.clone(), data.join("resources")).unwrap();
        assert_eq!(reloaded.config.project.is_some(), concurrent);
        assert_eq!(
            reloaded.config.saved_projects.len(),
            usize::from(concurrent)
        );
        if !concurrent {
            assert!(app.get_state().project.is_none());
            assert_eq!(result.unwrap()["projects"][0]["id"], "agent:mini:b");
        }
    }
}

#[tokio::test]
async fn unregister_lifecycle_preserves_runtime_and_converges_after_local_write_failure() {
    for (names, removed, disk_failure) in [
        (vec!["a", "b", "c"], "b", false),
        (vec!["a", "b"], "a", false),
        (vec!["a"], "a", false),
        (vec!["a", "b"], "a", true),
    ] {
        let fixture = Fixture::new();
        let data = &fixture.0;
        let id = format!("agent:mini:{removed}");
        let revision = format!("sha256:{}", "a".repeat(64));
        let (url, server) = crate::project_inventory::tests::server(vec![
            json!({"success":true,"output":{"projects":[{"id":id,"path":format!("/repos/{removed}"),"revision":revision}]}}),
            json!({"success":true,"output":{"project":id,"outcome":"unregistered"}}),
        ]).await;
        let app = AppState::new(data.clone(), data.join("resources")).unwrap();
        let token = data.join("token");
        std::fs::write(&token, "fixture-only").unwrap();
        let keep = data.join("user-files.txt");
        std::fs::write(&keep, "preserved").unwrap();
        let runtime = StoredRuntime {
            server_url: url,
            server_env_file: None,
            runner_config: Some(data.join("runner.toml")),
            user_token_file: Some(token),
            runner_client_id: Some("mini".into()),
            project_id: Some("a".into()),
            runtime_project_id: Some("agent:mini:a".into()),
        };
        let project = |name: &str| ProjectSelection {
            path: format!("/repos/{name}"),
            allowed_root: format!("/repos/{name}"),
            is_git_repository: true,
            runtime_project_id: Some(format!("agent:mini:{name}")),
        };
        {
            let mut slot = app.core.lock().await;
            let core = slot.as_mut().unwrap();
            core.config.runtime = Some(runtime.clone());
            core.config.project = Some(project("a"));
            core.config.saved_projects = names
                .iter()
                .map(|name| crate::models::SavedProject {
                    runner_config: runtime.runner_config.clone().unwrap(),
                    project: project(name),
                })
                .collect();
            core.snapshot.project = core.config.project.clone();
            core.snapshot.chatgpt_activity = Some(ChatGptActivitySnapshot {
                observed: true,
                last_meaningful_activity_at_ms: Some(123),
            });
            core.snapshot.readiness = aggregate_readiness(
                ServerReadiness::Ready,
                RunnerReadiness::Ready,
                ExposureReadiness::LocalReady,
                ProjectReadiness::Ready,
            );
            core.save_config().await.unwrap();
            core.publish_snapshot();
            if disk_failure {
                core.config_path = data.clone();
            }
        }
        let result = app
            .unregister_project(crate::project_inventory::UnregisterRequest {
                target: crate::webcodex::settings::target(&runtime).unwrap(),
                project: id,
                expected_revision: revision,
                confirmed: true,
            })
            .await
            .unwrap();
        assert!(result.readiness.runtime_ready);
        assert_eq!(result.readiness.server, ServerReadiness::Ready);
        assert_eq!(result.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(result.project, (removed != "a").then(|| project("a")));
        assert_eq!(result.chatgpt_activity.is_some(), removed != "a");
        assert_eq!(result.saved_projects.len(), names.len() - 1);
        assert!(result
            .saved_projects
            .iter()
            .all(|saved| saved != &project(removed)));
        assert_eq!(std::fs::read_to_string(&keep).unwrap(), "preserved");
        assert_eq!(
            server.await.unwrap().len(),
            2,
            "no activation, login or unregister retry"
        );
        if disk_failure {
            let mut slot = app.core.lock().await;
            let core = slot.as_mut().unwrap();
            assert!(core.inventory_persistence_pending);
            assert!(DesktopCore::new(data.clone(), data.join("resources"))
                .unwrap()
                .config
                .project
                .is_some());
            core.config_path = data.join("desktop-state.json");
            core.reconcile_inventory(&json!({"client_id":"mini","connected":true,"projects_available":true,"projects_truncated":false,
                "visible_project_count":1,"projects":[{"id":"agent:mini:b","path":"/repos/b"}]})).await;
            assert!(!core.inventory_persistence_pending);
        }
        let reloaded = DesktopCore::new(data.clone(), data.join("resources")).unwrap();
        assert_eq!(
            reloaded.config.project,
            (removed != "a").then(|| project("a"))
        );
        assert_eq!(reloaded.config.saved_projects.len(), names.len() - 1);
        assert!(app
            .supervisor
            .lock()
            .await
            .snapshot(ProcessKey::LocalRunner)
            .is_none());
        assert!(app
            .supervisor
            .lock()
            .await
            .snapshot(ProcessKey::LocalServer)
            .is_none());
    }
}
