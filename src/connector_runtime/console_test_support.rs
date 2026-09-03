use super::{ConnectorContext, ConnectorRuntime};
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    ShellClientCapabilities, ShellClientRegisterRequest, AGENT_PROTOCOL_GENERATION_V2,
};
use crate::Database;
use std::sync::Arc;

pub(crate) struct ConsoleFixture {
    pub(crate) _temp: tempfile::TempDir,
    pub(crate) runtime: Arc<ConnectorRuntime>,
    pub(crate) own_client_id: String,
    pub(crate) shared_client_id: String,
}

pub(crate) async fn console_fixture() -> ConsoleFixture {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let state = temp.path().join("state");
    super::tests::init_repo(&project);
    let registry = Arc::new(ShellClientRegistry::default());
    register_client(&registry, "hosted", "instance-a", &super::tests::auth("u1")).await;
    register_client(&registry, "laptop", "instance-b", &super::tests::auth("u2")).await;
    let tools =
        Arc::new(crate::tool_runtime::ToolRuntime::new_for_tests_with_shell_clients(registry));
    let runtime = Arc::new(
        ConnectorRuntime::new(
            tools,
            Arc::new(Database::open(&temp.path().join("connector.db")).unwrap()),
            ConnectorContext {
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
                project_grant_id: super::tests::PROJECT_GRANT_ID.into(),
            },
            super::tests::credential(),
        )
        .unwrap(),
    );
    ConsoleFixture {
        _temp: temp,
        runtime,
        own_client_id: "hosted".to_string(),
        shared_client_id: "laptop".to_string(),
    }
}

async fn register_client(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance_id: &str,
    auth: &crate::auth::AuthContext,
) {
    registry
        .register_with_auth(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: client_id.to_string(),
                agent_instance_id: instance_id.to_string(),
                agent_protocol_generation: AGENT_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: Some("owner".into()),
                hostname: None,
                host_context: None,
                capabilities: crate::test_support::current_runner_capabilities(
                    ShellClientCapabilities::default(),
                ),
                policy: None,
            },
            Some(&crate::test_support::runner_access(auth)),
        )
        .await
        .unwrap();
}
