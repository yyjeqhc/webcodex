//! Shared test helpers for tool_runtime tests.

use super::super::git::*;
use super::super::helpers::*;
use super::super::types::*;
use super::super::*;
use crate::projects::{Executor, ProjectConfig, ProjectsConfig, ProjectsState};
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentShellRequest, ShellClientCapabilities, ShellClientRegisterRequest,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) const SAMPLE_PROJECT: &str = "agent:oe:private-drop";
pub(super) const UNIT_TOOL_FIXTURES: &[&str] = &[
    "list_tools",
    "list_projects",
    "list_agents",
    "runtime_status",
];

pub(super) fn test_runtime() -> ToolRuntime {
    let projects = Arc::new(ProjectsState::failed(
        "projects not configured for test".to_string(),
        "test".to_string(),
    ));
    let shell_clients = Arc::new(ShellClientRegistry::default());
    ToolRuntime::new(
        projects,
        shell_clients,
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(super) fn sample_tool_args(name: &str) -> Value {
    let runtime = test_runtime();
    let spec = runtime
        .tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .unwrap_or_else(|| panic!("missing tool spec for {name}"));
    sample_tool_args_for_spec(&spec)
}

pub(super) fn sample_tool_args_for_spec(spec: &ToolSpec) -> Value {
    let required = spec.input_schema["required"]
        .as_array()
        .unwrap_or_else(|| panic!("{} schema should list required fields", spec.name));
    if required.is_empty() && UNIT_TOOL_FIXTURES.contains(&spec.name.as_str()) {
        return Value::Null;
    }

    let args = required
        .iter()
        .map(|field| {
            let field = field
                .as_str()
                .unwrap_or_else(|| panic!("{} required field should be a string", spec.name));
            (field.to_string(), sample_field_value(field))
        })
        .collect();
    Value::Object(args)
}

pub(super) fn sample_field_value(field: &str) -> Value {
    match field {
        "project" => json!(SAMPLE_PROJECT),
        "command" => json!("true"),
        "patch" => json!("diff --git a/a b/a\n"),
        "paths" => json!(["old.txt"]),
        "path" => json!("src/lib.rs"),
        "old" | "old_text" => json!("a"),
        "new" | "new_text" => json!("b"),
        "pattern" => json!("fn main"),
        "text" => json!("// hi\n"),
        "content" => json!("fn main() {}\n"),
        "content_base64" => json!("AA=="),
        "start_line" | "end_line" | "line" => json!(1),
        "edits" => json!([{"kind": "replace_exact", "old_text": "a", "new_text": "b"}]),
        "prompt" => json!("summarize"),
        "job_id" => json!("job_123"),
        "session_id" => json!("wc_sess_existing"),
        "checkpoint_id" => json!("wc_ckpt_1234"),
        "confirm" => json!(true),
        "client_id" => json!("oe"),
        "id" => json!("private-drop"),
        "name" => json!("Private Drop"),
        "kind" => json!("note"),
        "message" => json!("hello"),
        "message_id" => json!("wc_msg_0001"),
        other => panic!("missing sample value for required field {other}"),
    }
}

pub(super) fn sample_tool_args_with_session(name: &str) -> Value {
    let mut args = sample_tool_args(name);
    let obj = args
        .as_object_mut()
        .unwrap_or_else(|| panic!("{name} does not accept object arguments"));
    obj.insert(
        "session_id".to_string(),
        Value::String("wc_sess_accessor".to_string()),
    );
    args
}

/// Build a placeholder value for a required field from its JSON Schema
/// property definition.  When the property carries an `enum` constraint the
/// first allowed value is used so that serde deserialization succeeds.
pub(super) fn placeholder_from_prop(prop: &Value) -> Value {
    if let Some(vals) = prop["enum"].as_array() {
        if let Some(first) = vals.first() {
            return first.clone();
        }
    }
    let kind = prop["type"].as_str().unwrap_or("string");
    match kind {
        "integer" => json!(1),
        "array" => json!([]),
        "boolean" => json!(true),
        _ => json!("value"),
    }
}

// =========================================================================
// Phase 2: schema coverage for the generic callRuntimeTool tool set
// =========================================================================

/// Helper: fetch a ToolSpec by name from the runtime.
pub(super) fn spec_named<'a>(specs: &'a [ToolSpec], name: &str) -> &'a ToolSpec {
    specs
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("tool '{}' missing from specs", name))
}

/// Helper: the `required` field of a tool's input schema, as Strings.
pub(super) fn required_fields(spec: &ToolSpec) -> Vec<String> {
    spec.input_schema["required"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// =========================================================================
// Phase 2: local job recovery, path safety, status normalization, bounded logs
// =========================================================================

pub(super) fn local_project_config(path: &str) -> ProjectConfig {
    ProjectConfig {
        path: path.to_string(),
        executor: Executor::Local,
        client_id: None,
        allow_patch: true,
        allow_command_requests: false,
        allow_raw_command_requests: false,
        default_apply_patch_backend: None,
        allowed_checks: vec![],
        checks: None,
        commands: HashMap::new(),
        hooks: HashMap::new(),
    }
}

pub(super) fn runtime_with_project(root: &Path, project_id: &str) -> ToolRuntime {
    let mut projects = HashMap::new();
    projects.insert(
        project_id.to_string(),
        local_project_config(&root.to_string_lossy()),
    );
    let config = ProjectsConfig { projects };
    let state = ProjectsState::loaded(config, "test".to_string());
    ToolRuntime::new(
        Arc::new(state),
        Arc::new(ShellClientRegistry::default()),
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(super) fn init_git_repo(root: &Path) {
    for cmd in [
        "git init",
        "git config user.email webcodex-test@example.com",
        "git config user.name 'WebCodex Test'",
    ] {
        let (exit_code, stdout, stderr, _) = run_command_sync(cmd, root, 30);
        assert_eq!(
            exit_code, 0,
            "git setup command failed: {cmd}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
}

pub(super) fn commit_file(root: &Path, path: &str, content: &str, subject: &str) {
    fs::write(root.join(path), content).unwrap();
    for cmd in [
        format!("git add -- {}", shell_escape_simple(path)),
        format!("git commit -m {}", shell_escape_simple(subject)),
    ] {
        let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, root, 30);
        assert_eq!(
            exit_code, 0,
            "git commit helper command failed: {cmd}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
}

pub(super) fn git_log_stdout(root: &Path, limit: usize, skip: usize) -> String {
    let command = git_log_command(limit, skip);
    let (exit_code, stdout, stderr, _) = run_command_sync(&command, root, 30);
    assert_eq!(
        exit_code, 0,
        "git log helper command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    stdout
}

pub(super) async fn register_agent_project_at_path(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    root: &Path,
) -> String {
    let project_path = root.to_string_lossy().to_string();
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                git: true,
                ..Default::default()
            }),
            projects: Some(vec![registered_project(project_id, &project_path)]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    ToolRuntime::agent_project_runtime_id(client_id, project_id)
}

pub(super) fn run_agent_shell_request_locally(
    req: &ShellAgentShellRequest,
) -> (i32, String, String) {
    let mut command = std::process::Command::new("sh");
    command.arg("-c").arg(&req.command);
    if let Some(cwd) = req.cwd.as_deref() {
        command.current_dir(cwd);
    }
    if req.stdin.is_some() {
        command.stdin(std::process::Stdio::piped());
    }
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn agent shell request");
    if let Some(stdin) = req.stdin.as_deref() {
        use std::io::Write;
        child
            .stdin
            .take()
            .expect("agent shell request stdin")
            .write_all(stdin.as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

pub(super) async fn complete_agent_request_by_running_locally(
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

pub(super) async fn dispatch_checkpoint_with_local_agent(
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
    let mut req = None;
    for _ in 0..200 {
        req = next_patch_agent_request(runtime, client_id).await;
        if req.is_some() || task.is_finished() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let req = match req {
        Some(req) => req,
        None => {
            let result = task.await.unwrap();
            panic!("checkpoint helper did not enqueue an agent shell request: {result:?}");
        }
    };
    assert!(
        req.command.starts_with("python3 -c "),
        "unexpected checkpoint helper command: {}",
        req.command
    );
    complete_agent_request_by_running_locally(runtime, client_id, req).await;
    task.await.unwrap()
}

pub(super) fn output_has_file(output: &Value, path: &str) -> bool {
    output["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"].as_str() == Some(path))
}

pub(super) fn preview_for_path<'a>(output: &'a Value, path: &str) -> &'a Value {
    output["untracked_previews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|preview| preview["path"].as_str() == Some(path))
        .unwrap_or_else(|| {
            panic!(
                "missing preview for {path}: {}",
                output["untracked_previews"]
            )
        })
}

pub(super) fn show_changes_output_from_command(root: &Path, include_diff: bool) -> Value {
    let command = show_changes_command(include_diff);
    let (exit_code, stdout, stderr, _) = run_command_sync(&command, root, 30);
    assert_eq!(
        exit_code, 0,
        "show_changes command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let (status_stdout, head_stdout, diff_stat, diff_stdout, untracked_preview_stdout) =
        split_show_changes_stdout(&stdout, include_diff);
    let mut output = parse_show_changes_output(
        "demo",
        &status_stdout,
        &head_stdout,
        &diff_stat,
        include_diff.then_some(diff_stdout.as_str()),
        20,
        80,
        Some(exit_code),
        &stderr,
    );
    if include_diff {
        apply_show_changes_untracked_previews(&mut output, &untracked_preview_stdout);
    }
    output
}

pub(super) fn finished_event<'a>(
    summary: &'a crate::tool_runtime::sessions::SessionSummary,
    tool_name: &str,
) -> &'a crate::tool_runtime::sessions::SessionEvent {
    summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == tool_name)
        .unwrap_or_else(|| {
            panic!(
                "missing finished event for {tool_name}: {:?}",
                summary.events
            )
        })
}

/// Write a fake on-disk local job simulating a job that survived a restart.
pub(super) fn write_fake_job(
    root: &Path,
    job_id: &str,
    project: &str,
    path: &str,
    status: &str,
    stdout: &str,
    stderr: &str,
    meta_extra: Value,
) -> PathBuf {
    let dir = root.join(format!(".codex/jobs/{}", job_id));
    fs::create_dir_all(&dir).unwrap();
    let mut meta = json!({
        "job_id": job_id,
        "project": project,
        "path": path,
        "command": "echo test",
        "status": "running",
        "created_at": 1000,
        "started_at": 1000,
        "max_runtime_secs": 3600,
        "executor": "local",
        "kind": "shell",
    });
    if let (Value::Object(ref mut m), Value::Object(extra)) = (&mut meta, meta_extra) {
        for (k, v) in extra {
            m.insert(k, v);
        }
    }
    fs::write(
        dir.join("metadata.json"),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .unwrap();
    fs::write(dir.join("status"), status).unwrap();
    fs::write(dir.join("stdout.log"), stdout).unwrap();
    fs::write(dir.join("stderr.log"), stderr).unwrap();
    dir
}

/// A deterministic fake process-killer for testing timeout/stop invariants.
/// Records which (pid, pgid) pairs it was asked to terminate and reports
/// AlreadyGone so the runtime persists a terminal status without touching
/// any real process.
#[derive(Default, Clone)]
pub(super) struct FakeJobKiller {
    calls: Arc<std::sync::Mutex<Vec<(i64, i64)>>>,
}

impl FakeJobKiller {
    pub(super) fn calls(&self) -> Vec<(i64, i64)> {
        self.calls.lock().unwrap().clone()
    }
}

impl LocalJobKiller for FakeJobKiller {
    fn terminate_group(&self, pid: i64, pgid: i64) -> TerminateOutcome {
        self.calls.lock().unwrap().push((pid, pgid));
        TerminateOutcome::AlreadyGone
    }
}

pub(super) fn runtime_with_fake_killer(
    root: &Path,
    project_id: &str,
) -> (ToolRuntime, FakeJobKiller) {
    let mut runtime = runtime_with_project(root, project_id);
    let killer = FakeJobKiller::default();
    let killer_dyn: Arc<dyn LocalJobKiller> = Arc::new(killer.clone());
    runtime.job_killer = killer_dyn;
    (runtime, killer)
}

/// Write a fake on-disk local job plus a `pid` file and `process_group_id`
/// metadata field, simulating a job spawned by the current code.
pub(super) fn write_fake_job_with_pgid(
    root: &Path,
    job_id: &str,
    project: &str,
    path: &str,
    status: &str,
    pid: i64,
    meta_extra: Value,
) -> PathBuf {
    let dir = write_fake_job(root, job_id, project, path, status, "", "", meta_extra);
    fs::write(dir.join("pid"), pid.to_string()).unwrap();
    dir
}

// =========================================================================
// Phase 3: harden run_codex — command construction, validation, output
// =========================================================================

pub(super) fn codex_config_with_allowlist(allowlist: &[&str]) -> CodexConfig {
    CodexConfig {
        bin: "codex".to_string(),
        approval_mode: String::new(),
        default_timeout_secs: 3600,
        max_prompt_bytes: 100_000,
        allowed_extra_args: allowlist.iter().map(|s| s.to_string()).collect(),
    }
}

pub(super) fn runtime_with_codex(root: &Path, codex: CodexConfig) -> ToolRuntime {
    let mut projects = HashMap::new();
    projects.insert(
        "demo".to_string(),
        local_project_config(&root.to_string_lossy()),
    );
    let config = ProjectsConfig { projects };
    let state = ProjectsState::loaded(config, "test".to_string());
    ToolRuntime::new(
        Arc::new(state),
        Arc::new(ShellClientRegistry::default()),
        Arc::new(codex),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(super) fn auth_context(username: Option<&str>, is_bootstrap: bool) -> crate::auth::AuthContext {
    let (role, scopes) = if is_bootstrap {
        ("admin".to_string(), vec!["admin".to_string()])
    } else {
        ("user".to_string(), Vec::new())
    };
    crate::auth::AuthContext {
        kind: if is_bootstrap {
            crate::auth::AuthKind::Bootstrap
        } else {
            crate::auth::AuthKind::ApiToken
        },
        user_id: username.map(|u| format!("user-{}", u)),
        username: username.map(str::to_string),
        api_key_id: username.map(|u| format!("key-{}", u)),
        api_key_name: username.map(|u| format!("{} key", u)),
        role: Some(role),
        scopes,
        is_bootstrap,
        token_kind: if is_bootstrap {
            None
        } else {
            Some("user".to_string())
        },
        allowed_client_id: None,
        shared_key_hash: None,
    }
}

pub(super) fn shared_key_auth_context(hash: &str) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::SharedKey,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("shared-key".to_string()),
        scopes: vec![
            crate::auth::SCOPE_RUNTIME_READ.to_string(),
            crate::auth::SCOPE_PROJECT_READ.to_string(),
            crate::auth::SCOPE_PROJECT_WRITE.to_string(),
            crate::auth::SCOPE_JOB_RUN.to_string(),
            crate::auth::SCOPE_AGENT_REGISTER.to_string(),
        ],
        is_bootstrap: false,
        token_kind: Some("shared-key".to_string()),
        allowed_client_id: None,
        shared_key_hash: Some(hash.to_string()),
    }
}

pub(super) fn open_auth_context() -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OpenAnonymous,
        user_id: Some("open-anonymous".to_string()),
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("open".to_string()),
        scopes: vec![
            crate::auth::SCOPE_RUNTIME_READ.to_string(),
            crate::auth::SCOPE_PROJECT_READ.to_string(),
            crate::auth::SCOPE_PROJECT_WRITE.to_string(),
            crate::auth::SCOPE_JOB_RUN.to_string(),
            crate::auth::SCOPE_AGENT_REGISTER.to_string(),
        ],
        is_bootstrap: false,
        token_kind: Some("open".to_string()),
        allowed_client_id: None,
        shared_key_hash: None,
    }
}

pub(super) fn bootstrap_auth_context() -> crate::auth::AuthContext {
    auth_context(None, true)
}

pub(super) fn agent_project_config(path: &str, client_id: &str) -> ProjectConfig {
    ProjectConfig {
        path: path.to_string(),
        executor: Executor::Agent,
        client_id: Some(client_id.to_string()),
        allow_patch: true,
        allow_command_requests: false,
        allow_raw_command_requests: false,
        default_apply_patch_backend: None,
        allowed_checks: vec![],
        checks: None,
        commands: HashMap::new(),
        hooks: HashMap::new(),
    }
}

pub(super) fn runtime_with_agent_project(client_id: &str) -> ToolRuntime {
    let mut projects = HashMap::new();
    projects.insert(
        "agent-proj".to_string(),
        agent_project_config("/tmp/agent-proj", client_id),
    );
    let config = ProjectsConfig { projects };
    let state = ProjectsState::loaded(config, "test".to_string());
    ToolRuntime::new(
        Arc::new(state),
        Arc::new(ShellClientRegistry::default()),
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(super) async fn register_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    owner: Option<&str>,
    caps: ShellClientCapabilities,
) {
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: None,
            capabilities: Some(caps),
            projects: Some(vec![registered_project("agent-proj", "/tmp/agent-proj")]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
}

pub(super) fn agent_test_project_id(client_id: &str) -> String {
    ToolRuntime::agent_project_runtime_id(client_id, "agent-proj")
}

/// Build a ToolRuntime backed by a single server-configured (local) project
/// rooted at `root`. Used to assert the runtime surface rejects
/// server-configured projects in favor of agent-registered ones.
pub(super) fn runtime_with_local_project(root: &Path, project_id: &str) -> ToolRuntime {
    let mut projects = HashMap::new();
    projects.insert(
        project_id.to_string(),
        ProjectConfig {
            path: root.to_string_lossy().to_string(),
            executor: Executor::Local,
            client_id: None,
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: Vec::new(),
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        },
    );
    let config = ProjectsConfig { projects };
    let state = ProjectsState::loaded(config, "test".to_string());
    ToolRuntime::new(
        Arc::new(state),
        Arc::new(ShellClientRegistry::default()),
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(super) fn registered_project(id: &str, path: &str) -> ShellAgentProjectSummary {
    ShellAgentProjectSummary {
        id: id.to_string(),
        name: Some(id.to_string()),
        path: path.to_string(),
        allow_patch: true,
        kind: Some("repo".to_string()),
        description: None,
        hooks: Vec::new(),
        disabled: false,
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at: 123,
        shell_profile: None,
    }
}

pub(super) fn named_registered_project(
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
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at,
        shell_profile: None,
    }
}

pub(super) async fn register_agent_projects(
    runtime: &ToolRuntime,
    client_id: &str,
    owner: Option<&str>,
    caps: ShellClientCapabilities,
    projects: Vec<ShellAgentProjectSummary>,
) {
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: client_id.to_string(),
            agent_instance_id: format!("inst-{}", client_id),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: None,
            capabilities: Some(caps),
            projects: Some(projects),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
}

pub(super) async fn next_agent_request_for_client(
    runtime: &ToolRuntime,
    client_id: &str,
) -> Option<ShellAgentShellRequest> {
    next_agent_request_for_instance(runtime, client_id, &format!("inst-{}", client_id)).await
}

pub(super) async fn next_agent_request_for_instance(
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

pub(super) async fn runtime_with_resolver_projects() -> ToolRuntime {
    let runtime = test_runtime();
    let mut file_caps = ShellClientCapabilities::default();
    file_caps.file_read = true;
    file_caps.git = true;
    file_caps.shell = true;
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

pub(super) async fn next_patch_agent_request(
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

pub(super) async fn complete_patch_agent_request(
    runtime: &ToolRuntime,
    client_id: &str,
    request_id: &str,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) {
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
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

/// A small patch carrying a distinctive marker line so tests can prove the
/// patch body never leaks into the shell `command` string.
pub(super) fn marker_patch(filename: &str, marker: &str) -> String {
    format!(
        "diff --git a/{f} b/{f}\nnew file mode 100644\n--- /dev/null\n+++ b/{f}\n\
             @@ -0,0 +1 @@\n+{m}\n",
        f = filename,
        m = marker,
    )
}

/// A patch deliberately larger than the agent shell command limit
/// (`MAX_COMMAND_LEN` = 8000 bytes) so tests can prove the patch still
/// validates/applies via `stdin` rather than the command string.
pub(super) fn large_marker_patch(filename: &str, marker: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "diff --git a/{f} b/{f}\nnew file mode 100644\n--- /dev/null\n+++ b/{f}\n\
             @@ -0,0 +1,200 @@\n",
        f = filename,
    ));
    s.push_str(&format!("+{m}\n", m = marker));
    for i in 0..199 {
        s.push_str(&format!("+line-{:04}-{}\n", i, "x".repeat(48)));
    }
    s
}

/// Assert a patch-related agent command is one of the fixed, known-safe
/// invocations and never carries patch content, a `cd` prefix, a heredoc,
/// or an `echo`/`cat` splice of the patch body.
pub(super) fn assert_safe_patch_command(command: &str, marker: &str) {
    let allowed = [
        "git apply --check -",
        "git apply --check - && echo OK",
        "git apply --stat -",
        "git apply -",
    ];
    assert!(
        allowed.contains(&command),
        "unexpected patch command (must be a fixed git apply invocation): {}",
        command
    );
    assert!(
        !command.contains(marker),
        "patch content leaked into command: {}",
        command
    );
    assert!(
        !command.contains("cd "),
        "command must not use a cd prefix (cwd is supplied via the shell request): {}",
        command
    );
    assert!(
        !command.contains("<<"),
        "command must not use a heredoc: {}",
        command
    );
    // The only permitted `echo` is the fixed `echo OK` success marker; it
    // never carries patch content. `cat` must never appear (no splicing).
    if command.contains("echo ") {
        assert_eq!(command, "git apply --check - && echo OK");
    }
    assert!(
        !command.contains("cat "),
        "command must not splice the patch via cat: {}",
        command
    );
}

pub(super) async fn register_agent_with_projects(
    runtime: &ToolRuntime,
    client_id: &str,
    owner: Option<&str>,
    caps: ShellClientCapabilities,
    projects: Vec<ShellAgentProjectSummary>,
) {
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: owner.map(str::to_string),
            hostname: None,
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
pub(super) async fn register_agent_with_shell_profiles(
    runtime: &ToolRuntime,
    client_id: &str,
    policy: Option<crate::shell_protocol::AgentPolicySummary>,
    projects: Vec<ShellAgentProjectSummary>,
) {
    use crate::shell_protocol::ShellClientRegisterRequest;
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            capabilities: Some(ShellClientCapabilities::default()),
            projects: Some(projects),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy,
        })
        .await
        .unwrap();
}

pub(super) fn profile_summary_entry(
    name: &str,
    has_init_script: bool,
    env_keys_count: usize,
) -> crate::shell_protocol::ShellProfileSummaryEntry {
    crate::shell_protocol::ShellProfileSummaryEntry {
        name: name.to_string(),
        has_init_script,
        env_keys_count,
        program: "sh".to_string(),
        args_count: 1,
    }
}

// =========================================================================
// Phase 7: runtime_status observability tool
// =========================================================================

pub(super) fn runtime_with_info(info: RuntimeInfo) -> ToolRuntime {
    let projects = Arc::new(ProjectsState::failed(
        "projects not configured for test".to_string(),
        "test".to_string(),
    ));
    ToolRuntime::new(
        projects,
        Arc::new(ShellClientRegistry::default()),
        Arc::new(CodexConfig::default()),
        Arc::new(info),
    )
}

// -------------------------------------------------------------------------
// Python helper integration: run the actual fixed helper scripts locally
// against temp files (python3 is required by the e2e suite and the agent
// host; these tests skip gracefully when python3 is not on PATH so cargo
// test stays green on minimal CI).
// -------------------------------------------------------------------------

pub(super) fn python3_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a fixed helper script locally with a JSON payload on stdin, in the
/// given cwd, and return the parsed JSON object the helper prints.
pub(super) fn run_helper_locally(helper: &str, payload: &Value, cwd: &Path) -> Value {
    let stdin = serde_json::to_string(payload).unwrap();
    let mut child = std::process::Command::new("python3")
        .arg("-c")
        .arg(helper)
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn python3");
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "helper exited {:?}: stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "helper returned invalid JSON: {} (got: {})",
            e,
            stdout.trim()
        )
    })
}

/// Compute a lowercase hex sha256 of a string (test helper).
pub(super) fn sha256_hex(s: &str) -> String {
    // Use the same approach as the python helper: sha256 of utf-8 bytes.
    // We shell out to python3 to avoid pulling a sha256 crate into tests.
    let child = std::process::Command::new("python3")
        .arg("-c")
        .arg(
            "import sys,hashlib;sys.stdout.write(hashlib.sha256(sys.argv[1].encode()).hexdigest())",
        )
        .arg(s)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn python3");
    let output = child.wait_with_output().unwrap();
    String::from_utf8(output.stdout).unwrap()
}

// =========================================================================
// apply_text_edits — pure function + runtime dispatch tests
// =========================================================================

pub(super) fn text_edit(
    kind: ApplyTextEditKind,
    old_text: Option<&str>,
    new_text: Option<&str>,
    anchor_text: Option<&str>,
) -> ApplyTextEditInput {
    ApplyTextEditInput {
        kind,
        old_text: old_text.map(str::to_string),
        new_text: new_text.map(str::to_string),
        anchor_text: anchor_text.map(str::to_string),
    }
}
