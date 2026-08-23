use super::auth::auth_context;
use super::runtime::test_runtime;
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    AgentPolicySummary, ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentShellRequest, ShellClientCapabilities, ShellClientRegisterRequest,
    ShellProfileSummaryEntry, EXTERNAL_SEARCH_REQUEST_PREFIX,
};
use crate::tool_runtime::{RuntimeInfo, ToolCall, ToolResult, ToolRuntime};
use crate::workspace_checkpoint::{create_workspace_checkpoint, restore_workspace_checkpoint};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(in crate::tool_runtime::tests) async fn register_agent_project_at_path(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    root: &Path,
) -> String {
    let project_path = root.to_string_lossy().to_string();
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                git: true,
                file_read: true,
                file_write: true,
                internal_posix_script: true,
                ..Default::default()
            }),
            projects: Some(vec![registered_project(project_id, &project_path)]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    crate::tool_runtime::agent_project_runtime_id(client_id, project_id)
}

pub(in crate::tool_runtime::tests) async fn register_agent_project_at_path_with_auth(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    root: &Path,
    auth: &crate::auth::AuthContext,
) -> String {
    let project_path = root.to_string_lossy().to_string();
    runtime
        .shell_clients
        .register_with_auth(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: Some(ShellClientCapabilities {
                    shell: true,
                    git: true,
                    file_read: true,
                    file_write: true,
                    internal_posix_script: true,
                    ..Default::default()
                }),
                projects: Some(vec![registered_project(project_id, &project_path)]),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            },
            Some(auth),
        )
        .await
        .unwrap();
    crate::tool_runtime::agent_project_runtime_id(client_id, project_id)
}

pub(in crate::tool_runtime::tests) fn run_agent_shell_request_locally(
    req: &ShellAgentShellRequest,
) -> (i32, String, String) {
    if req.kind == "file_read" {
        return run_agent_file_read_request_locally(req);
    }
    if req.kind == "file_list" {
        return run_agent_file_list_request_locally(req);
    }
    if req.kind == "file_project_overview" {
        return run_agent_project_overview_request_locally(req);
    }
    let structured_process = if req.kind == "run_process" {
        assert!(req.command.is_empty());
        assert!(req.script.is_none());
        Some(
            req.process
                .as_ref()
                .expect("run_process request must carry a typed process payload"),
        )
    } else {
        None
    };
    let internal_posix = if req.kind == "run_internal_posix_script" {
        let payload = req
            .script
            .as_ref()
            .expect("internal POSIX request must carry a script payload");
        assert_eq!(
            payload.language,
            crate::shell_protocol::ShellScriptLanguage::Sh
        );
        assert!(payload.args.is_empty());
        assert!(req.command.is_empty());
        assert!(req.stdin.is_none());
        Some(payload.script.as_str())
    } else {
        None
    };
    let internal_search = req
        .command
        .strip_prefix(EXTERNAL_SEARCH_REQUEST_PREFIX)
        .and_then(|rest| rest.strip_prefix('\n'));
    let (mut command, stdin_payload) = if let Some(process) = structured_process {
        let mut command = std::process::Command::new(&process.executable);
        command.args(&process.args);
        (command, req.stdin.clone())
    } else if let Some(script) = internal_posix {
        #[cfg(windows)]
        let mut command = std::process::Command::new("bash.exe");
        #[cfg(not(windows))]
        let mut command = std::process::Command::new("sh");
        command.arg("-s");
        (command, Some(script.to_string()))
    } else if let Some(script) = internal_search {
        #[cfg(windows)]
        let mut command = std::process::Command::new("bash.exe");
        #[cfg(not(windows))]
        let mut command = std::process::Command::new("sh");
        command.arg("-s");
        let script = if cfg!(windows) {
            format!(
                "if ! command -v rg >/dev/null 2>&1 && command -v rg.exe >/dev/null 2>&1; then rg() {{ command rg.exe --path-separator / \"$@\"; }}; fi\n{script}"
            )
        } else {
            script.to_string()
        };
        (command, Some(script))
    } else {
        let mut command = std::process::Command::new("sh");
        command.arg("-c").arg(&req.command);
        (command, req.stdin.clone())
    };
    if let Some(cwd) = req.cwd.as_deref() {
        command.current_dir(cwd);
    }
    if stdin_payload.is_some() {
        command.stdin(std::process::Stdio::piped());
    }
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn agent shell request");
    if let Some(stdin) = stdin_payload.as_deref() {
        use std::io::Write;
        let write_result = child
            .stdin
            .take()
            .expect("agent shell request stdin")
            .write_all(stdin.as_bytes());
        if let Err(error) = write_result {
            assert_eq!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe,
                "agent shell request stdin write failed: {error}"
            );
        }
    }
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn run_agent_file_list_request_locally(req: &ShellAgentShellRequest) -> (i32, String, String) {
    let Some(cwd) = req.cwd.as_deref() else {
        return (-1, String::new(), "file_list missing cwd".to_string());
    };
    let path = req.path.as_deref().unwrap_or(".");
    let target = Path::new(cwd).join(path);
    let entries = match std::fs::read_dir(&target) {
        Ok(entries) => entries,
        Err(error) => return (-1, String::new(), error.to_string()),
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let mut name = entry.file_name().to_string_lossy().to_string();
        if entry.file_type().ok().is_some_and(|kind| kind.is_dir()) {
            name.push('/');
        }
        names.push(name);
    }
    names.sort();
    (0, format!("{}\n", names.join("\n")), String::new())
}

fn run_agent_file_read_request_locally(req: &ShellAgentShellRequest) -> (i32, String, String) {
    use webcodex_workspace::file_read_range::{self, EffectiveRange};

    let Some(cwd) = req.cwd.as_deref() else {
        return (-1, String::new(), "file_read missing cwd".to_string());
    };
    let Some(path) = req.path.as_deref() else {
        return (-1, String::new(), "file_read missing path".to_string());
    };
    let target = Path::new(cwd).join(path);
    // Reconstruct the shared effective range from the runner's inclusive window.
    let start_line = req.start_line.unwrap_or(1).max(1);
    let end_line = req.end_line.unwrap_or(start_line);
    let limit = end_line.saturating_sub(start_line).saturating_add(1);
    let range = EffectiveRange::new(Some(start_line), Some(limit));
    match file_read_range::read_range(&target, range) {
        Ok(result) => {
            let output = json!({
                "format": "webcodex.file_read_range.v1",
                "content": result.content,
                "sha256": result.sha256,
                "total_lines": result.total_lines,
                "start_line": result.start_line,
                "limit": result.limit,
            });
            (0, output.to_string(), String::new())
        }
        Err(error) => (-1, String::new(), error.to_string()),
    }
}

fn run_agent_project_overview_request_locally(
    req: &ShellAgentShellRequest,
) -> (i32, String, String) {
    let Some(cwd) = req.cwd.as_deref() else {
        return (
            -1,
            String::new(),
            "project_overview missing cwd".to_string(),
        );
    };
    let requested_path = req.path.as_deref().unwrap_or(".");
    let options = req
        .content
        .as_deref()
        .and_then(|content| serde_json::from_str::<Value>(content).ok())
        .unwrap_or_else(|| json!({}));
    match crate::project_overview::build_project_overview(
        Path::new(cwd),
        requested_path,
        options["max_depth"].as_u64().map(|value| value as usize),
        options["limit"].as_u64().map(|value| value as usize),
    ) {
        Ok(output) => (0, output.to_string(), String::new()),
        Err(error) => (-1, String::new(), error),
    }
}

pub(in crate::tool_runtime::tests) async fn complete_agent_request_by_running_locally(
    runtime: &ToolRuntime,
    client_id: &str,
    req: ShellAgentShellRequest,
) {
    let (exit_code, stdout, stderr) = run_agent_shell_request_locally(&req);
    complete_patch_agent_request(
        runtime,
        client_id,
        &req.request_id,
        exit_code,
        &stdout,
        &stderr,
    )
    .await;
}

pub(in crate::tool_runtime::tests) async fn complete_project_overview_agent_request_locally(
    runtime: &ToolRuntime,
    client_id: &str,
    req: &ShellAgentShellRequest,
) {
    assert_eq!(req.kind, "file_project_overview");
    let options = req
        .content
        .as_deref()
        .and_then(|content| serde_json::from_str::<Value>(content).ok())
        .expect("project_overview agent options");
    let output = crate::project_overview::build_project_overview(
        Path::new(req.cwd.as_deref().expect("project_overview cwd")),
        req.path.as_deref().expect("project_overview path"),
        options["max_depth"].as_u64().map(|value| value as usize),
        options["limit"].as_u64().map(|value| value as usize),
    )
    .expect("agent-side project_overview scan");
    complete_patch_agent_request(
        runtime,
        client_id,
        &req.request_id,
        0,
        &output.to_string(),
        "",
    )
    .await;
}

pub(in crate::tool_runtime::tests) fn run_agent_checkpoint_request_locally(
    req: &ShellAgentShellRequest,
) -> (i32, String, String) {
    let root = request_root(req);
    let payload = req
        .content
        .as_deref()
        .and_then(|content| serde_json::from_str::<Value>(content).ok())
        .unwrap_or_else(|| json!({}));
    let output = match req.kind.as_str() {
        "file_checkpoint_create" => {
            let include_untracked = payload
                .get("include_untracked")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            create_workspace_checkpoint(&root, include_untracked)
        }
        "file_checkpoint_restore" => {
            let checkpoint = payload.get("checkpoint").unwrap_or(&Value::Null);
            restore_workspace_checkpoint(&root, checkpoint)
        }
        other => panic!("unexpected checkpoint request kind: {other}"),
    };
    (0, serde_json::to_string(&output).unwrap(), String::new())
}

fn request_root(req: &ShellAgentShellRequest) -> PathBuf {
    let path = req.path.as_deref().unwrap_or(".");
    let raw = PathBuf::from(path);
    if raw.is_absolute() {
        raw
    } else {
        req.cwd
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(raw)
    }
}

pub(in crate::tool_runtime::tests) async fn dispatch_checkpoint_with_local_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
) -> ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime.dispatch_with_auth(call, Some(&bootstrap)).await
        }
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let req = loop {
        let request = probe_patch_agent_request(runtime, client_id).await;
        if request.is_some() || task.is_finished() {
            break request;
        }
        if Instant::now() >= deadline {
            panic!("checkpoint Agent request readiness failed for client {client_id} within 10 seconds");
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let req = match req {
        Some(req) => req,
        None => {
            let result = task.await.unwrap();
            panic!("checkpoint did not enqueue an agent file request: {result:?}");
        }
    };
    assert!(
        matches!(
            req.kind.as_str(),
            "file_checkpoint_create" | "file_checkpoint_restore"
        ),
        "unexpected checkpoint request kind: {}",
        req.kind
    );
    assert!(
        req.command.is_empty(),
        "checkpoint native request must not use a shell command: {}",
        req.command
    );
    let (exit_code, stdout, stderr) = run_agent_checkpoint_request_locally(&req);
    complete_patch_agent_request(
        runtime,
        client_id,
        &req.request_id,
        exit_code,
        &stdout,
        &stderr,
    )
    .await;
    task.await.unwrap()
}

pub(in crate::tool_runtime::tests) fn runtime_with_agent_project(client_id: &str) -> ToolRuntime {
    let _ = client_id;
    ToolRuntime::new(
        Arc::new(ShellClientRegistry::default()),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(in crate::tool_runtime::tests) async fn register_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    owner: Option<&str>,
    caps: ShellClientCapabilities,
) {
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: None,
            host_context: None,
            capabilities: Some(caps),
            projects: Some(vec![registered_project("agent-proj", "/tmp/agent-proj")]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
}

pub(in crate::tool_runtime::tests) fn agent_test_project_id(client_id: &str) -> String {
    crate::tool_runtime::agent_project_runtime_id(client_id, "agent-proj")
}

/// Register an agent under an explicit `agent_instance_id`. `register_agent`
/// always uses `"inst"`; replacement scenarios need a different instance id to
/// model a new Runner process taking over the lease.
pub(in crate::tool_runtime::tests) async fn register_agent_with_instance(
    runtime: &ToolRuntime,
    client_id: &str,
    agent_instance_id: &str,
    owner: Option<&str>,
    caps: ShellClientCapabilities,
) {
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: None,
            host_context: None,
            capabilities: Some(caps),
            projects: Some(vec![registered_project("agent-proj", "/tmp/agent-proj")]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
}

/// Build a ToolRuntime backed by a single server-configured (local) project
/// rooted at `root`. Used to assert the runtime surface rejects
/// server-configured projects in favor of agent-registered ones.
pub(in crate::tool_runtime::tests) fn runtime_with_local_project(
    root: &Path,
    project_id: &str,
) -> ToolRuntime {
    let _ = (root, project_id);
    ToolRuntime::new(
        Arc::new(ShellClientRegistry::default()),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(in crate::tool_runtime::tests) fn registered_project(
    id: &str,
    path: &str,
) -> ShellAgentProjectSummary {
    ShellAgentProjectSummary {
        id: id.to_string(),
        name: Some(id.to_string()),
        path: path.to_string(),
        allow_patch: true,
        kind: Some("repo".to_string()),
        description: None,
        hooks: Vec::new(),
        disabled: false,
        revision: None,
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at: 123,
        shell_profile: None,
    }
}

pub(in crate::tool_runtime::tests) fn named_registered_project(
    client_id: &str,
    id: &str,
    name: &str,
    path: &str,
    updated_at: i64,
) -> ShellAgentProjectSummary {
    let _ = client_id;
    ShellAgentProjectSummary {
        id: id.to_string(),
        name: Some(name.to_string()),
        path: path.to_string(),
        allow_patch: true,
        kind: Some("repo".to_string()),
        description: None,
        hooks: Vec::new(),
        disabled: false,
        revision: None,
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at,
        shell_profile: None,
    }
}

pub(in crate::tool_runtime::tests) async fn register_agent_projects(
    runtime: &ToolRuntime,
    client_id: &str,
    owner: Option<&str>,
    caps: ShellClientCapabilities,
    projects: Vec<ShellAgentProjectSummary>,
) {
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: format!("inst-{}", client_id),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: None,
            host_context: None,
            capabilities: Some(caps),
            projects: Some(projects),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
}

pub(in crate::tool_runtime::tests) async fn register_agent_projects_for_auth(
    runtime: &ToolRuntime,
    client_id: &str,
    auth: &crate::auth::AuthContext,
    caps: ShellClientCapabilities,
    projects: Vec<ShellAgentProjectSummary>,
) {
    runtime
        .shell_clients
        .register_with_auth(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                client_id: client_id.to_string(),
                agent_instance_id: format!("inst-{}", client_id),
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: Some(caps),
                projects: Some(projects),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            },
            Some(auth),
        )
        .await
        .unwrap();
}

pub(in crate::tool_runtime::tests) async fn probe_agent_request_for_client(
    runtime: &ToolRuntime,
    client_id: &str,
) -> Option<ShellAgentShellRequest> {
    probe_agent_request_for_instance(runtime, client_id, &format!("inst-{}", client_id)).await
}

pub(in crate::tool_runtime::tests) async fn probe_agent_request_for_instance(
    runtime: &ToolRuntime,
    client_id: &str,
    agent_instance_id: &str,
) -> Option<ShellAgentShellRequest> {
    for _ in 0..20 {
        let req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: client_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if req.is_some() {
            return req;
        }
        tokio::task::yield_now().await;
    }
    None
}

pub(in crate::tool_runtime::tests) async fn wait_for_agent_request_for_instance(
    runtime: &ToolRuntime,
    client_id: &str,
    agent_instance_id: &str,
) -> ShellAgentShellRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(request) = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: client_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                projects: None,
            })
            .await
            .unwrap()
        {
            return request;
        }
        if Instant::now() >= deadline {
            panic!(
                "Agent request readiness failed for client {client_id} instance {agent_instance_id} within 10 seconds"
            );
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

pub(in crate::tool_runtime::tests) async fn wait_for_agent_request_for_client(
    runtime: &ToolRuntime,
    client_id: &str,
) -> ShellAgentShellRequest {
    wait_for_agent_request_for_instance(runtime, client_id, &format!("inst-{client_id}")).await
}

pub(in crate::tool_runtime::tests) async fn runtime_with_resolver_projects() -> ToolRuntime {
    let runtime = test_runtime();
    let file_caps = ShellClientCapabilities {
        file_read: true,
        git: true,
        shell: true,
        internal_posix_script: true,
        ..Default::default()
    };
    register_agent_projects(
        &runtime,
        "workstation",
        None,
        file_caps.clone(),
        vec![
            named_registered_project(
                "workstation",
                "my-repo",
                "My Repo",
                "/root/git/workstation-my-repo",
                200,
            ),
            named_registered_project(
                "workstation",
                "other-repo",
                "Other Repo",
                "/root/git/workstation-other-repo",
                210,
            ),
        ],
    )
    .await;
    register_agent_projects(
        &runtime,
        "laptop",
        None,
        file_caps,
        vec![named_registered_project(
            "laptop",
            "my-repo",
            "My Repo",
            "/root/git/laptop-my-repo",
            190,
        )],
    )
    .await;
    runtime
}

//   * the working directory is supplied via the shell request `cwd` field,
//     never via a `cd <path> && ...` prefix in the command;
//   * `apply_patch_checked` checks before applying and skips the apply step
//     when the preflight fails (no partial application);
//   * `validate_patch` only ever enqueues read-only `git apply --check` /
//     `--stat` commands, never a bare mutating `git apply -`;
//   * server-configured (non-agent) projects are rejected by every patch
//     tool, so the server never touches the filesystem directly.

pub(in crate::tool_runtime::tests) async fn probe_patch_agent_request(
    runtime: &ToolRuntime,
    client_id: &str,
) -> Option<ShellAgentShellRequest> {
    for _ in 0..20 {
        let req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if req.is_some() {
            return req;
        }
        tokio::task::yield_now().await;
    }
    None
}

/// Wait for a request that the test requires to be dispatched. Unlike
/// `probe_patch_agent_request`, which is intentionally a short probe for
/// negative/no-dispatch assertions, this positive readiness wait uses one
/// absolute wall-clock deadline so scheduler contention cannot turn a fixed
/// yield count into a flaky failure.
pub(in crate::tool_runtime::tests) async fn wait_for_patch_agent_request(
    runtime: &ToolRuntime,
    client_id: &str,
) -> ShellAgentShellRequest {
    wait_for_agent_request_for_instance(runtime, client_id, "inst").await
}

pub(in crate::tool_runtime::tests) fn assert_internal_posix_script_contains(
    request: &ShellAgentShellRequest,
    needle: &str,
) {
    assert_eq!(request.kind, "run_internal_posix_script");
    assert!(request.command.is_empty());
    assert!(request.stdin.is_none());
    let payload = request
        .script
        .as_ref()
        .expect("internal POSIX request must carry a typed script payload");
    assert_eq!(
        payload.language,
        crate::shell_protocol::ShellScriptLanguage::Sh
    );
    assert!(payload.args.is_empty());
    assert!(
        payload.script.contains(needle),
        "internal POSIX script did not contain {needle:?}: {}",
        payload.script
    );
}

pub(in crate::tool_runtime::tests) async fn complete_patch_agent_request(
    runtime: &ToolRuntime,
    client_id: &str,
    request_id: &str,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) {
    complete_patch_agent_request_for_instance(
        runtime, client_id, "inst", request_id, exit_code, stdout, stderr,
    )
    .await;
}

pub(in crate::tool_runtime::tests) async fn complete_patch_agent_request_for_instance(
    runtime: &ToolRuntime,
    client_id: &str,
    agent_instance_id: &str,
    request_id: &str,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) {
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: client_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            request_id: request_id.to_string(),
            exit_code: Some(exit_code),
            stdout: Some(stdout.to_string()),
            stderr: Some(stderr.to_string()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
}

pub(in crate::tool_runtime::tests) async fn complete_agent_ranged_file_read_request(
    runtime: &ToolRuntime,
    client_id: &str,
    request: &ShellAgentShellRequest,
    content: &str,
) {
    let start = request
        .start_line
        .expect("ToolRuntime file_read test request must include start_line");
    let end = request
        .end_line
        .expect("ToolRuntime file_read test request must include end_line");
    let limit = end.saturating_sub(start).saturating_add(1);
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        0,
        &canonical_agent_file_read_range(content, start, limit),
        "",
    )
    .await;
}

pub(in crate::tool_runtime::tests) async fn register_agent_with_projects(
    runtime: &ToolRuntime,
    client_id: &str,
    owner: Option<&str>,
    caps: ShellClientCapabilities,
    projects: Vec<ShellAgentProjectSummary>,
) {
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: None,
            host_context: None,
            capabilities: Some(caps),
            projects: Some(projects),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
}

/// Helper: register an agent carrying a sanitized shell-profiles summary
/// (inside its policy) plus a set of projects with optional per-project
/// `shell_profile`. Used by the shell-profile observability tests.
pub(in crate::tool_runtime::tests) async fn register_agent_with_shell_profiles(
    runtime: &ToolRuntime,
    client_id: &str,
    policy: Option<AgentPolicySummary>,
    projects: Vec<ShellAgentProjectSummary>,
) {
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities::default()),
            projects: Some(projects),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy,
        })
        .await
        .unwrap();
}

/// Build a canonical `webcodex.file_read_range.v1` envelope for the full file
/// `content` under the ToolRuntime default effective range (start_line=1,
/// limit=2000). The selected window, SHA-256, total line count, and range
/// fields are all derived through the shared range reader so the envelope is
/// exactly what a real agent produces and passes the ToolRuntime strict
/// validation. `total_lines` is ignored in favor of the shared reader's count;
/// it is kept only for call-site compatibility.
pub(in crate::tool_runtime::tests) fn canonical_agent_file_read_output(
    content: &str,
    _total_lines: usize,
) -> String {
    canonical_agent_file_read_range(content, 1, 2000)
}

/// Build a canonical `webcodex.file_read_range.v1` envelope for the full file
/// `content` under an explicit effective request range. The selected window is
/// computed by the shared range reader so content, total_lines, start_line,
/// and limit are internally consistent and match the ToolRuntime validation.
pub(in crate::tool_runtime::tests) fn canonical_agent_file_read_range(
    content: &str,
    start_line: usize,
    limit: usize,
) -> String {
    use webcodex_workspace::file_read_range::{self, EffectiveRange};
    let range = EffectiveRange::new(Some(start_line), Some(limit));
    let result = file_read_range::read_range_from(content.as_bytes(), range)
        .expect("canonical agent file read fixture range fits budget");
    serde_json::json!({
        "format": "webcodex.file_read_range.v1",
        "content": result.content,
        "sha256": result.sha256,
        "total_lines": result.total_lines,
        "start_line": result.start_line,
        "limit": result.limit,
    })
    .to_string()
}

pub(in crate::tool_runtime::tests) fn profile_summary_entry(
    name: &str,
    has_init_script: bool,
    env_keys_count: usize,
) -> ShellProfileSummaryEntry {
    ShellProfileSummaryEntry {
        dialect: None,
        name: name.to_string(),
        has_init_script,
        env_keys_count,
        program: "sh".to_string(),
        args_count: 1,
    }
}
