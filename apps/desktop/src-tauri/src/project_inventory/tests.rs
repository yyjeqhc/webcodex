use super::*;
use crate::models::SavedProject;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    root: std::path::PathBuf,
    runtime: StoredRuntime,
}
impl Fixture {
    fn new(server_url: String) -> Self {
        let root =
            std::env::temp_dir().join(format!("webcodex-unregister-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let token = root.join("fixture-token");
        std::fs::write(&token, "test-only-token").unwrap();
        std::fs::write(root.join("keep.txt"), "user files stay").unwrap();
        let runtime = StoredRuntime {
            server_url,
            server_env_file: None,
            runner_config: Some(root.join("runner.toml")),
            user_token_file: Some(token),
            runner_client_id: Some("mini".into()),
            project_id: Some("repo".into()),
            runtime_project_id: Some("agent:mini:repo".into()),
        };
        Self { root, runtime }
    }
    fn request(&self) -> UnregisterRequest {
        UnregisterRequest {
            target: settings::target(&self.runtime).unwrap(),
            project: "agent:mini:repo".into(),
            expected_revision: format!("sha256:{}", "a".repeat(64)),
            confirmed: true,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn server(responses: Vec<Value>) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut calls = vec![];
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        for response in responses {
            let (mut stream, _) = tokio::time::timeout_at(deadline, listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut bytes = vec![];
            let body = loop {
                let mut chunk = [0; 4096];
                let count = tokio::time::timeout_at(deadline, stream.read(&mut chunk))
                    .await
                    .unwrap()
                    .unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                assert!(bytes.len() < 65536);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    assert!(header.starts_with("post /api/tools/call "));
                    let length: usize = header
                        .lines()
                        .find_map(|s| s.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                    }
                }
            };
            calls.push(body);
            let body = response.to_string();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
        }
        calls
    });
    (url, task)
}
fn inventory() -> Value {
    json!({"success":true,"output":{"projects":[{"id":"agent:mini:repo","path":"D:\\repo","revision":format!("sha256:{}", "a".repeat(64))}]}})
}

#[tokio::test]
async fn unregister_uses_exact_revision_and_keeps_files() {
    let (url, task) = server(vec![
        inventory(),
        json!({"success":true,"output":{"project":"agent:mini:repo","outcome":"unregistered"}}),
    ])
    .await;
    let fixture = Fixture::new(url);
    assert_eq!(
        unregister(&fixture.runtime, &fixture.request())
            .await
            .unwrap(),
        "D:\\repo"
    );
    let calls = task.await.unwrap();
    assert_eq!(
        calls[1],
        json!({"tool":"unregister_project","params":{"project":"agent:mini:repo","expected_revision":fixture.request().expected_revision}})
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("keep.txt")).unwrap(),
        "user files stay"
    );
}

#[tokio::test]
async fn changed_revision_never_dispatches_removal() {
    let (url, task) = server(vec![inventory()]).await;
    let fixture = Fixture::new(url);
    let mut request = fixture.request();
    request.expected_revision = format!("sha256:{}", "b".repeat(64));
    assert!(unregister(&fixture.runtime, &request).await.is_err());
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn rejects_unconfirmed_or_retargeted_requests_before_network() {
    let fixture = Fixture::new("http://127.0.0.1:1".into());
    let mut request = fixture.request();
    request.confirmed = false;
    assert!(unregister(&fixture.runtime, &request).await.is_err());
    request.confirmed = true;
    request.target.client_id = "other".into();
    assert!(unregister(&fixture.runtime, &request).await.is_err());
    assert!(observe(&fixture.runtime, "agent:other:repo").await.is_err());
}

#[tokio::test]
async fn rejection_and_wrong_project_outcomes_are_not_retried() {
    for result in [
        json!({"success":false,"error":"active_jobs_conflict"}),
        json!({"success":true,"output":{"project":"agent:other:repo","outcome":"unregistered"}}),
    ] {
        let (url, task) = server(vec![inventory(), result]).await;
        let fixture = Fixture::new(url);
        assert!(unregister(&fixture.runtime, &fixture.request())
            .await
            .is_err());
        assert_eq!(task.await.unwrap().len(), 2);
    }
}

#[test]
fn forget_only_removes_exact_runner_registration_and_clears_default() {
    let fixture = Fixture::new("http://127.0.0.1:1".into());
    let selected = ProjectSelection {
        path: "D:\\repo".into(),
        allowed_root: "D:\\repo".into(),
        is_git_repository: true,
        runtime_project_id: Some("agent:mini:repo".into()),
    };
    let saved = SavedProject {
        project: selected.clone(),
        runner_config: fixture.runtime.runner_config.clone().unwrap(),
    };
    let mut other = saved.clone();
    other.project.runtime_project_id = Some("agent:mini:other".into());
    let mut elsewhere = saved.clone();
    elsewhere.runner_config = fixture.root.join("other.toml");
    let mut config = StoredDesktopConfig {
        runtime: Some(fixture.runtime.clone()),
        project: Some(selected),
        saved_projects: vec![saved, other.clone(), elsewhere.clone()],
        ..Default::default()
    };
    forget(&mut config, "agent:mini:repo", r"\\?\D:\repo");
    assert_eq!(config.saved_projects, vec![other, elsewhere]);
    assert!(config.project.is_none());
    assert!(config.runtime.unwrap().runtime_project_id.is_none());
    assert!(fixture.root.join("keep.txt").is_file());
}

#[cfg(windows)]
#[test]
fn forget_matches_legacy_display_paths_without_affecting_other_roots() {
    let fixture = Fixture::new("http://127.0.0.1:1".into());
    let selected = ProjectSelection {
        path: "D:\\".into(),
        allowed_root: "D:\\".into(),
        is_git_repository: false,
        runtime_project_id: None,
    };
    let saved = SavedProject {
        project: selected.clone(),
        runner_config: fixture.runtime.runner_config.clone().unwrap(),
    };
    let mut other = saved.clone();
    other.project.path = "C:\\".into();
    let mut config = StoredDesktopConfig {
        runtime: Some(fixture.runtime.clone()),
        project: Some(selected),
        saved_projects: vec![saved, other.clone()],
        ..Default::default()
    };
    forget(&mut config, "agent:mini:repo", r"\\?\D:\");
    assert_eq!(config.saved_projects, vec![other]);
    assert!(config.project.is_none());
}
