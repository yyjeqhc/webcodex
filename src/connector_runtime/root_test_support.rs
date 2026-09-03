use crate::auth::{
    AuthContext, AuthKind, ProjectCredentialVerifier, SCOPE_JOB_RUN, SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ,
};
use crate::Database;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

pub(crate) const PROJECT_GRANT_ID: &str = "wc_pgrant_1111111111111111";
const PROJECT_CREDENTIAL: &str =
    "webcodex_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub(crate) fn credential() -> ProjectCredentialVerifier {
    ProjectCredentialVerifier::new(PROJECT_GRANT_ID.to_string(), PROJECT_CREDENTIAL).unwrap()
}

pub(crate) fn auth(user_id: &str) -> AuthContext {
    let project_grant_id = if user_id == "u1" {
        PROJECT_GRANT_ID.to_string()
    } else {
        "wc_pgrant_2222222222222222".to_string()
    };
    AuthContext {
        role: Some("project".to_string()),
        scopes: vec![
            SCOPE_RUNTIME_READ.to_string(),
            SCOPE_PROJECT_READ.to_string(),
            SCOPE_PROJECT_WRITE.to_string(),
            SCOPE_JOB_RUN.to_string(),
        ],
        token_kind: Some("project".to_string()),
        project_grant_id: Some(project_grant_id),
        ..AuthContext::new(AuthKind::ProjectCredential)
    }
}

pub(crate) fn init_repo(project: &Path) {
    std::fs::create_dir(project).unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(project)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init", "-q"]);
    run(&["config", "core.autocrlf", "false"]);
    run(&["config", "core.longpaths", "true"]);
    std::fs::write(project.join("README.md"), "fixture\n").unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"connector-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    run(&["add", "README.md", "Cargo.toml"]);
    run(&[
        "-c",
        "user.name=WebCodex Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "initial",
    ]);
}

#[tokio::test]
async fn root_wrapper_preserves_unknown_capability_before_authentication() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    init_repo(&project);
    let state = temp.path().join("state");
    let tools = Arc::new(
        crate::tool_runtime::ToolRuntime::new_for_tests_with_runner_registry(Arc::new(
            crate::runner_http::RunnerRegistry::default(),
        )),
    );
    let runtime = super::ConnectorRuntime::new(
        tools,
        Arc::new(Database::open(&temp.path().join("connector.db")).unwrap()),
        super::ConnectorContext {
            project_id: "wc_proj_1234567890".into(),
            project_name: "project".into(),
            workspace_id: "wc_ws_1234567890".into(),
            executor_project: "agent:hosted:project".into(),
            executor_root: project.to_string_lossy().into_owned(),
            runs_root: state.join("runs").to_string_lossy().into_owned(),
            results_root: state.join("results").to_string_lossy().into_owned(),
            project_registry_dir: state
                .join("agent/project-registry")
                .to_string_lossy()
                .into_owned(),
            profile: "personal".into(),
            project_grant_id: PROJECT_GRANT_ID.into(),
        },
        credential(),
    )
    .unwrap();

    let outcome = runtime
        .call_for_window(
            "missing_capability",
            json!({}),
            None,
            super::ConnectorTransport::Mcp,
            None,
        )
        .await;

    assert_eq!(outcome.http_status, 400);
    assert!(outcome.protocol_error);
    assert_eq!(outcome.body["error"]["code"], "unknown_capability");
}
