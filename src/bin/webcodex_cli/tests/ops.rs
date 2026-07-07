use super::support::*;
use crate::webcodex_cli::ops::{
    ops_agents_report, ops_exit_code, ops_projects_report, ops_smoke_preflight_report,
    ops_status_report, render_ops_status,
};
use crate::webcodex_cli::run_ops_command;
use std::time::Duration;

#[test]
fn ops_help_entrypoints_print_usage() {
    let cases: &[(&[&str], &[&str])] = &[
        (
            &["ops", "--help"],
            &[
                "Usage: webcodex-cli ops <COMMAND>",
                "status",
                "agents",
                "projects",
                "smoke-preflight",
                "--server-url URL",
                "--token TOKEN",
            ],
        ),
        (
            &["ops", "status", "--help"],
            &[
                "Usage: webcodex-cli ops status",
                "--server-url URL",
                "--url URL",
                "--env-file PATH",
                "--token-file PATH",
                "--token TOKEN",
                "--json",
                "--strict",
            ],
        ),
        (
            &["ops", "agents", "--help"],
            &[
                "Usage: webcodex-cli ops agents",
                "--server-url URL",
                "--url URL",
                "--env-file PATH",
                "--token-file PATH",
                "--token TOKEN",
                "--json",
                "--strict",
            ],
        ),
        (
            &["ops", "projects", "--help"],
            &[
                "Usage: webcodex-cli ops projects",
                "--server-url URL",
                "--url URL",
                "--env-file PATH",
                "--token-file PATH",
                "--token TOKEN",
                "--json",
                "--strict",
            ],
        ),
        (
            &["ops", "smoke-preflight", "--help"],
            &[
                "Usage: webcodex-cli ops smoke-preflight",
                "--project PROJECT_ID",
                "--server-url URL",
                "--url URL",
                "--env-file PATH",
                "--token-file PATH",
                "--token TOKEN",
                "--json",
                "--strict",
            ],
        ),
    ];

    for (args, expected) in cases {
        let out = cli_exit(args.iter().copied())
            .unwrap_or_else(|err| panic!("expected {args:?} help to exit successfully: {err}"));
        for needle in *expected {
            assert!(
                out.contains(needle),
                "help for {args:?} did not contain {needle:?}\n{out}"
            );
        }
    }
}

#[test]
fn top_level_help_mentions_ops() {
    let out = cli_exit(["--help"]).unwrap();
    assert!(out.contains("ops status|agents|projects|smoke-preflight"));
}

#[test]
fn ops_unknown_subcommand_is_clear() {
    match cli_action(["ops", "unknown"]) {
        CliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 2);
            assert!(stdout.is_empty());
            assert!(stderr.contains("unknown ops subcommand: unknown"));
        }
        other => panic!("expected unknown ops subcommand exit, got {other:?}"),
    }
}

#[test]
fn ops_common_flags_parse_without_printing_token() {
    match cli_action([
        "ops",
        "status",
        "--url",
        "http://runtime.example",
        "--env-file",
        "/tmp/webcodex.env",
        "--token-file",
        "/tmp/token",
        "--token",
        "secret-token-value",
        "--json",
        "--strict",
    ]) {
        CliAction::Ops(OpsCommand::Status(opts)) => {
            assert_eq!(opts.server_url, "http://runtime.example");
            assert_eq!(
                opts.env_file.as_deref(),
                Some(Path::new("/tmp/webcodex.env"))
            );
            assert_eq!(opts.token_file.as_deref(), Some(Path::new("/tmp/token")));
            assert_eq!(opts.token.as_deref(), Some("secret-token-value"));
            assert!(opts.json);
            assert!(opts.strict);
        }
        other => panic!("expected ops status action, got {other:?}"),
    }
}

#[test]
fn ops_smoke_preflight_requires_project() {
    match cli_action(["ops", "smoke-preflight", "--json"]) {
        CliAction::Exit { code, stderr, .. } => {
            assert_eq!(code, 2);
            assert!(stderr.contains("--project is required"));
        }
        other => panic!("expected missing project exit, got {other:?}"),
    }
}

#[test]
fn ops_parser_errors_do_not_leak_token_value() {
    let secret = "secret-token-value";
    match cli_action(["ops", "status", "--token", secret, "--bad-flag"]) {
        CliAction::Exit { code, stderr, .. } => {
            assert_eq!(code, 2);
            assert!(stderr.contains("unknown ops status flag: --bad-flag"));
            assert!(!stderr.contains(secret));
        }
        other => panic!("expected parser error, got {other:?}"),
    }
}

#[tokio::test]
async fn ops_status_http_401_reports_auth_required_not_runtime_unreachable() {
    let output = run_ops_with_routes(
        OpsCommand::Status(ops_common_opts(String::new())),
        vec![(
            "/api/runtime/status",
            json_http_response(401, json!({"error": "missing token"})),
        )],
    )
    .await;
    assert!(output.contains("Overall: FAIL"), "{output}");
    assert!(output.contains("auth_required"), "{output}");
    assert!(output.contains("status: 401"), "{output}");
    assert!(!output.contains("runtime_unreachable"), "{output}");
}

#[tokio::test]
async fn ops_projects_http_401_uses_ops_report() {
    let output = run_ops_with_routes(
        OpsCommand::Projects(ops_common_opts(String::new())),
        vec![(
            "/api/projects/list",
            json_http_response(401, json!({"error": "missing token"})),
        )],
    )
    .await;
    assert!(output.contains("Overall: FAIL"), "{output}");
    assert!(output.contains("auth_required"), "{output}");
    assert!(output.contains("status: 401"), "{output}");
    assert!(!output.contains("projects list failed"), "{output}");
}

#[tokio::test]
async fn ops_smoke_preflight_projects_401_uses_ops_report() {
    let output = run_ops_with_routes(
        OpsCommand::SmokePreflight(OpsSmokePreflightOptions {
            common: ops_common_opts(String::new()),
            project: "agent:ops:smoke".to_string(),
        }),
        vec![
            (
                "/api/runtime/status",
                json_http_response(
                    200,
                    json!({"success": true, "output": runtime_status_fixture()}),
                ),
            ),
            (
                "/api/projects/list",
                json_http_response(401, json!({"error": "missing token"})),
            ),
        ],
    )
    .await;
    assert!(output.contains("Overall: FAIL"), "{output}");
    assert!(output.contains("auth_required"), "{output}");
    assert!(output.contains("endpoint: list_projects"), "{output}");
    assert!(!output.contains("projects list failed"), "{output}");
}

#[tokio::test]
async fn ops_http_403_reports_forbidden() {
    let mut opts = ops_common_opts(String::new());
    opts.token = Some("test-token".to_string());
    let output = run_ops_with_routes(
        OpsCommand::Status(opts),
        vec![(
            "/api/runtime/status",
            json_http_response(403, json!({"error": "forbidden"})),
        )],
    )
    .await;
    assert!(output.contains("Overall: FAIL"), "{output}");
    assert!(output.contains("forbidden"), "{output}");
    assert!(output.contains("status: 403"), "{output}");
    assert!(!output.contains("runtime_unreachable"), "{output}");
}

#[tokio::test]
async fn ops_connection_failure_reports_runtime_unreachable() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let output = run_ops_command(OpsCommand::Status(ops_common_opts(format!(
        "http://{addr}"
    ))))
    .await
    .unwrap()
    .stdout;
    assert!(output.contains("Overall: FAIL"), "{output}");
    assert!(output.contains("runtime_unreachable"), "{output}");
}

#[test]
fn ops_strict_exit_code_follows_report_status() {
    let pass = ops_status_report("https://ops.example.test", &Some(runtime_status_fixture()));
    assert_eq!(pass.verdict.status, "pass");
    assert_eq!(ops_exit_code(true, pass.verdict.status), 0);

    let mut warn_runtime = runtime_status_fixture();
    warn_runtime["jobs"]["active_count"] = json!(1);
    let warn = ops_status_report("https://ops.example.test", &Some(warn_runtime));
    assert_eq!(warn.verdict.status, "warn");
    assert_eq!(ops_exit_code(true, warn.verdict.status), 0);

    let mut fail_runtime = runtime_status_fixture();
    fail_runtime["agents"]["online_count"] = json!(0);
    fail_runtime["agents"]["summary"]["online"] = json!(0);
    let fail = ops_status_report("https://ops.example.test", &Some(fail_runtime));
    assert_eq!(fail.verdict.status, "fail");
    assert_eq!(ops_exit_code(true, fail.verdict.status), 2);
    assert_eq!(ops_exit_code(false, fail.verdict.status), 0);
}

#[tokio::test]
async fn ops_http_error_output_does_not_leak_token_value() {
    let secret = "secret-token-value";
    let mut opts = ops_common_opts(String::new());
    opts.token = Some(secret.to_string());
    let output = run_ops_with_routes(
        OpsCommand::Status(opts),
        vec![(
            "/api/runtime/status",
            json_http_response(401, json!({"error": format!("bad token {secret}")})),
        )],
    )
    .await;
    assert!(output.contains("Overall: FAIL"), "{output}");
    assert!(output.contains("unauthorized"), "{output}");
    assert!(!output.contains(secret), "{output}");
}

fn runtime_status_fixture() -> Value {
    json!({
        "service": "webcodex",
        "version": "0.2.0",
        "build": {
            "git_commit": "15138884e3a8ddcf294cae98183ecaac37af7230",
            "git_dirty": false
        },
        "tools": {
            "count": 66
        },
        "jobs": {
            "active_count": 0
        },
        "agents": {
            "online_count": 1,
            "offline_count": 0,
            "stale_count": 0,
            "summary": {
                "online": 1,
                "offline": 0,
                "stale": 0,
                "clients": [
                    {
                        "client_id": "ops-agent",
                        "status": "online",
                        "transport": "websocket",
                        "projects_count": 1,
                        "active_jobs": 0,
                        "pending_requests": 0,
                        "last_seen_age_secs": 2
                    }
                ]
            }
        },
        "projects": {
            "effective": {
                "status": "ok",
                "count": 1
            }
        }
    })
}

fn projects_fixture(recommended: bool) -> Value {
    json!({
        "count": 1,
        "recommended_for_smoke": if recommended { json!(["agent:ops:smoke"]) } else { json!([]) },
        "projects": [
            {
                "id": "agent:ops:smoke",
                "client_id": "ops",
                "agent_status": "online",
                "connected": true,
                "allow_patch": true,
                "path": "/srv/webcodex-smoke",
                "capabilities": {
                    "git_available": true,
                    "safe_smoke_project": true,
                    "recommended_for_smoke": recommended
                }
            }
        ]
    })
}

fn clean_show_changes_fixture() -> Value {
    json!({
        "clean": true,
        "git_available": true,
        "verdict": {
            "status": "pass",
            "blocking": false,
            "blocking_reasons": [],
            "warning_reasons": [],
            "suggested_next_actions": ["no action needed"]
        }
    })
}

fn clean_hygiene_fixture() -> Value {
    json!({
        "clean": true,
        "git_available": true,
        "counts": {
            "findings": 0,
            "low": 0
        },
        "verdict": {
            "status": "pass",
            "blocking": false,
            "blocking_reasons": [],
            "warning_reasons": [],
            "suggested_next_actions": ["no action needed"]
        }
    })
}

fn ops_common_opts(server_url: String) -> OpsCommonOptions {
    OpsCommonOptions {
        server_url,
        env_file: None,
        token_file: None,
        token: None,
        json: false,
        strict: false,
    }
}

#[derive(Clone)]
struct OpsHttpResponse {
    status: u16,
    content_type: String,
    body: String,
}

fn json_http_response(status: u16, body: Value) -> OpsHttpResponse {
    OpsHttpResponse {
        status,
        content_type: "application/json".to_string(),
        body: serde_json::to_string(&body).unwrap(),
    }
}

fn spawn_ops_route_server(
    routes: Vec<(&'static str, OpsHttpResponse)>,
) -> (
    String,
    std::sync::mpsc::Sender<()>,
    thread::JoinHandle<Vec<String>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 16384];
                    let n = stream.read(&mut buf).unwrap();
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    requests.push(request.clone());
                    let first_line = request.lines().next().unwrap_or_default();
                    let response = routes
                        .iter()
                        .find(|(path, _)| first_line.starts_with(&format!("POST {path} ")))
                        .map(|(_, response)| response.clone())
                        .unwrap_or_else(|| {
                            json_http_response(404, json!({"error": "unexpected request"}))
                        });
                    write!(
                        stream,
                        "HTTP/1.1 {} OK\r\ncontent-type: {}\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
                        response.status,
                        response.content_type,
                        response.body.len(),
                        response.body
                    )
                    .unwrap();
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    if stop_rx.try_recv().is_ok() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("fake ops route server accept failed: {err}"),
            }
        }
        requests
    });
    (format!("http://{}", addr), stop_tx, handle)
}

async fn run_ops_with_routes(
    command: OpsCommand,
    routes: Vec<(&'static str, OpsHttpResponse)>,
) -> String {
    let (server_url, stop_tx, handle) = spawn_ops_route_server(routes);
    let command = match command {
        OpsCommand::Status(mut opts) => {
            opts.server_url = server_url;
            OpsCommand::Status(opts)
        }
        OpsCommand::Agents(mut opts) => {
            opts.server_url = server_url;
            OpsCommand::Agents(opts)
        }
        OpsCommand::Projects(mut opts) => {
            opts.server_url = server_url;
            OpsCommand::Projects(opts)
        }
        OpsCommand::SmokePreflight(mut opts) => {
            opts.common.server_url = server_url;
            OpsCommand::SmokePreflight(opts)
        }
    };
    let output = run_ops_command(command).await.unwrap().stdout;
    stop_tx.send(()).unwrap();
    handle.join().unwrap();
    output
}

fn smoke_preflight_opts(server_url: String, project: &str) -> OpsSmokePreflightOptions {
    OpsSmokePreflightOptions {
        common: OpsCommonOptions {
            server_url,
            env_file: None,
            token_file: None,
            token: Some("secret-smoke-token".to_string()),
            json: false,
            strict: false,
        },
        project: project.to_string(),
    }
}

fn spawn_smoke_preflight_server(
    projects: Value,
) -> (
    String,
    std::sync::mpsc::Sender<()>,
    thread::JoinHandle<Vec<String>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 16384];
                    let n = stream.read(&mut buf).unwrap();
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    requests.push(request.clone());
                    let first_line = request.lines().next().unwrap_or_default().to_string();
                    let body = if first_line.starts_with("POST /api/runtime/status ") {
                        json!({"success": true, "output": runtime_status_fixture()})
                    } else if first_line.starts_with("POST /api/projects/list ") {
                        json!({"success": true, "output": projects.clone()})
                    } else if request.contains(r#""tool":"show_changes""#) {
                        json!({"success": true, "output": clean_show_changes_fixture()})
                    } else if request.contains(r#""tool":"workspace_hygiene_check""#) {
                        json!({"success": true, "output": clean_hygiene_fixture()})
                    } else {
                        json!({"success": false, "error": "unexpected request"})
                    };
                    let body = serde_json::to_string(&body).unwrap();
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    if stop_rx.try_recv().is_ok() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("fake ops server accept failed: {err}"),
            }
        }
        requests
    });
    (format!("http://{}", addr), stop_tx, handle)
}

async fn run_smoke_preflight_with_projects(
    projects: Value,
    project: &str,
) -> (String, Vec<String>) {
    let (server_url, stop_tx, handle) = spawn_smoke_preflight_server(projects);
    let output = run_ops_command(OpsCommand::SmokePreflight(smoke_preflight_opts(
        server_url, project,
    )))
    .await
    .unwrap()
    .stdout;
    stop_tx.send(()).unwrap();
    let requests = handle.join().unwrap();
    (output, requests)
}

fn smoke_request_kinds(requests: &[String]) -> Vec<&'static str> {
    requests
        .iter()
        .map(|request| {
            let first_line = request.lines().next().unwrap_or_default();
            if first_line.starts_with("POST /api/runtime/status ") {
                "runtime_status"
            } else if first_line.starts_with("POST /api/projects/list ") {
                "projects_list"
            } else if request.contains(r#""tool":"show_changes""#) {
                "show_changes"
            } else if request.contains(r#""tool":"workspace_hygiene_check""#) {
                "workspace_hygiene_check"
            } else {
                "unexpected"
            }
        })
        .collect()
}

fn assert_no_workspace_preflight_tools(requests: &[String]) {
    let joined = requests.join("\n---\n");
    assert!(!joined.contains(r#""tool":"show_changes""#));
    assert!(!joined.contains(r#""tool":"workspace_hygiene_check""#));
}

#[test]
fn ops_status_runtime_ok_passes() {
    let runtime = Some(runtime_status_fixture());
    let report = ops_status_report("https://ops.example.test", &runtime);
    assert_eq!(report.verdict.status, "pass");
    assert_eq!(report.summary["tools"]["count"], 66);
    assert_eq!(
        report.source["runtime_commit"],
        "15138884e3a8ddcf294cae98183ecaac37af7230"
    );
}

#[test]
fn ops_status_no_online_agents_fails() {
    let mut runtime = runtime_status_fixture();
    runtime["agents"]["online_count"] = json!(0);
    runtime["agents"]["offline_count"] = json!(1);
    runtime["agents"]["summary"]["online"] = json!(0);
    runtime["agents"]["summary"]["offline"] = json!(1);
    runtime["agents"]["summary"]["clients"][0]["status"] = json!("offline");
    let report = ops_status_report("https://ops.example.test", &Some(runtime));
    assert_eq!(report.verdict.status, "fail");
    assert!(report
        .verdict
        .blocking_reasons
        .contains(&"no_online_agents".to_string()));
}

#[test]
fn ops_status_active_jobs_warns() {
    let mut runtime = runtime_status_fixture();
    runtime["jobs"]["active_count"] = json!(2);
    let report = ops_status_report("https://ops.example.test", &Some(runtime));
    assert_eq!(report.verdict.status, "warn");
    assert!(report
        .verdict
        .warning_reasons
        .contains(&"active_jobs:2".to_string()));
}

#[test]
fn ops_agents_maps_online_offline_stale_and_jobs() {
    let mut runtime = runtime_status_fixture();
    runtime["agents"]["online_count"] = json!(1);
    runtime["agents"]["offline_count"] = json!(1);
    runtime["agents"]["stale_count"] = json!(1);
    runtime["agents"]["summary"]["online"] = json!(1);
    runtime["agents"]["summary"]["offline"] = json!(1);
    runtime["agents"]["summary"]["stale"] = json!(1);
    runtime["agents"]["summary"]["clients"] = json!([
        {
            "client_id": "online",
            "status": "online",
            "transport": "quic",
            "projects_count": 2,
            "active_jobs": 1,
            "pending_requests": 0,
            "last_seen_age_secs": 1
        },
        {
            "client_id": "stale",
            "status": "stale",
            "transport": "polling",
            "projects_count": 1,
            "active_jobs": 0,
            "pending_requests": 1,
            "last_seen_age_secs": 120
        }
    ]);
    let report = ops_agents_report("https://ops.example.test", &Some(runtime));
    assert_eq!(report.verdict.status, "warn");
    assert_eq!(report.summary["online_count"], 1);
    assert_eq!(report.summary["offline_count"], 1);
    assert_eq!(report.summary["stale_count"], 1);
    assert_eq!(report.summary["active_jobs"], 1);
}

#[test]
fn ops_projects_no_recommended_smoke_warns() {
    let projects = projects_fixture(false);
    let report = ops_projects_report("https://ops.example.test", Some(&projects));
    assert_eq!(report.verdict.status, "warn");
    assert!(report
        .verdict
        .warning_reasons
        .contains(&"no_recommended_smoke_project".to_string()));
}

#[test]
fn ops_projects_disconnected_and_stale_warns() {
    let projects = json!({
        "count": 3,
        "recommended_for_smoke": ["agent:ops:smoke"],
        "projects": [
            {
                "id": "agent:ops:smoke",
                "client_id": "ops",
                "agent_status": "online",
                "connected": true,
                "allow_patch": true,
                "path": "/srv/webcodex-smoke",
                "capabilities": {
                    "git_available": true,
                    "safe_smoke_project": true,
                    "recommended_for_smoke": true
                }
            },
            {
                "id": "agent:ops:disconnected",
                "client_id": "ops",
                "agent_status": "online",
                "connected": false,
                "allow_patch": true,
                "path": "/srv/webcodex-disconnected",
                "capabilities": {
                    "git_available": true,
                    "safe_smoke_project": false,
                    "recommended_for_smoke": false
                }
            },
            {
                "id": "agent:ops:stale",
                "client_id": "stale",
                "agent_status": "stale",
                "connected": true,
                "allow_patch": true,
                "path": "/srv/webcodex-stale",
                "capabilities": {
                    "git_available": true,
                    "safe_smoke_project": false,
                    "recommended_for_smoke": false
                }
            }
        ]
    });
    let report = ops_projects_report("https://ops.example.test", Some(&projects));
    assert_eq!(report.verdict.status, "warn");
    assert!(report
        .verdict
        .warning_reasons
        .contains(&"disconnected_projects:1".to_string()));
    assert!(report
        .verdict
        .warning_reasons
        .contains(&"stale_projects:1".to_string()));
}

#[test]
fn ops_projects_recommended_smoke_offline_warns() {
    let projects = json!({
        "count": 2,
        "recommended_for_smoke": ["agent:ops:offline-smoke"],
        "projects": [
            {
                "id": "agent:ops:online",
                "client_id": "ops",
                "agent_status": "online",
                "connected": true,
                "allow_patch": true,
                "path": "/srv/webcodex-online",
                "capabilities": {
                    "git_available": true,
                    "safe_smoke_project": false,
                    "recommended_for_smoke": false
                }
            },
            {
                "id": "agent:ops:offline-smoke",
                "client_id": "special",
                "agent_status": "stale",
                "connected": false,
                "allow_patch": true,
                "path": "/srv/webcodex-offline-smoke",
                "capabilities": {
                    "git_available": true,
                    "safe_smoke_project": true,
                    "recommended_for_smoke": true
                }
            }
        ]
    });
    let report = ops_projects_report("https://ops.example.test", Some(&projects));
    assert_eq!(report.verdict.status, "warn");
    assert!(report
        .verdict
        .warning_reasons
        .contains(&"recommended_smoke_offline:1".to_string()));
}

#[test]
fn ops_smoke_preflight_clean_project_passes() {
    let runtime = runtime_status_fixture();
    let projects = projects_fixture(true);
    let show_changes = clean_show_changes_fixture();
    let hygiene = clean_hygiene_fixture();
    let report = ops_smoke_preflight_report(
        "https://ops.example.test",
        "agent:ops:smoke",
        Some(&runtime),
        Some(&projects),
        Some(&show_changes),
        Some(&hygiene),
    );
    assert_eq!(report.verdict.status, "pass");
}

#[test]
fn ops_smoke_preflight_dirty_workspace_fails() {
    let runtime = runtime_status_fixture();
    let projects = projects_fixture(true);
    let mut show_changes = clean_show_changes_fixture();
    show_changes["clean"] = json!(false);
    show_changes["verdict"]["status"] = json!("fail");
    let hygiene = clean_hygiene_fixture();
    let report = ops_smoke_preflight_report(
        "https://ops.example.test",
        "agent:ops:smoke",
        Some(&runtime),
        Some(&projects),
        Some(&show_changes),
        Some(&hygiene),
    );
    assert_eq!(report.verdict.status, "fail");
    assert!(report
        .verdict
        .blocking_reasons
        .contains(&"workspace_dirty".to_string()));
}

#[test]
fn ops_smoke_preflight_online_non_recommended_project_warns() {
    let runtime = runtime_status_fixture();
    let projects = projects_fixture(false);
    let show_changes = clean_show_changes_fixture();
    let hygiene = clean_hygiene_fixture();
    let report = ops_smoke_preflight_report(
        "https://ops.example.test",
        "agent:ops:smoke",
        Some(&runtime),
        Some(&projects),
        Some(&show_changes),
        Some(&hygiene),
    );
    assert_eq!(report.verdict.status, "warn");
    assert!(report
        .verdict
        .warning_reasons
        .contains(&"project_not_recommended_for_smoke".to_string()));
}

#[test]
fn ops_json_and_human_outputs_do_not_contain_secret_values() {
    let secret = "secret-token-value";
    let mut runtime = runtime_status_fixture();
    runtime["agents"]["summary"]["clients"][0]["client_id"] = json!("safe-agent");
    let report = ops_status_report("https://ops.example.test", &Some(runtime));
    let json_output = render_ops_status(&report, true).unwrap();
    let human_output = render_ops_status(&report, false).unwrap();
    assert!(!json_output.contains(secret));
    assert!(!json_output.contains("WEBCODEX_TOKEN="));
    assert!(!human_output.contains(secret));
    assert!(!human_output.contains("WEBCODEX_TOKEN="));
}

#[tokio::test]
async fn ops_smoke_preflight_calls_only_read_only_endpoints() {
    let (output, requests) =
        run_smoke_preflight_with_projects(projects_fixture(true), "agent:ops:smoke").await;
    assert_eq!(requests.len(), 4);
    assert_eq!(
        smoke_request_kinds(&requests),
        vec![
            "runtime_status",
            "projects_list",
            "show_changes",
            "workspace_hygiene_check"
        ]
    );
    let joined = requests.join("\n---\n");
    assert!(joined.contains("POST /api/runtime/status "));
    assert!(joined.contains("POST /api/projects/list "));
    assert!(joined.contains(r#""tool":"show_changes""#));
    assert!(joined.contains(r#""tool":"workspace_hygiene_check""#));
    assert!(!joined.contains(r#""tool":"run_shell""#));
    assert!(!joined.contains(r#""tool":"run_job""#));
    assert!(!output.contains("secret-smoke-token"));
    assert!(output.contains("Overall: PASS"));
}

#[tokio::test]
async fn ops_smoke_preflight_project_missing_short_circuits() {
    let (output, requests) =
        run_smoke_preflight_with_projects(projects_fixture(true), "agent:ops:missing").await;
    assert_eq!(
        smoke_request_kinds(&requests),
        vec!["runtime_status", "projects_list"]
    );
    assert_no_workspace_preflight_tools(&requests);
    assert!(output.contains("Overall: FAIL"));
    assert!(output.contains("project_missing"));
}

#[tokio::test]
async fn ops_smoke_preflight_disconnected_project_short_circuits() {
    let mut projects = projects_fixture(true);
    projects["projects"][0]["connected"] = json!(false);
    projects["projects"][0]["agent_status"] = json!("stale");
    projects["projects"][0]["capabilities"]["recommended_for_smoke"] = json!(false);
    projects["projects"][0]["capabilities"]["safe_smoke_project"] = json!(false);
    let (output, requests) = run_smoke_preflight_with_projects(projects, "agent:ops:smoke").await;
    assert_eq!(
        smoke_request_kinds(&requests),
        vec!["runtime_status", "projects_list"]
    );
    assert_no_workspace_preflight_tools(&requests);
    assert!(output.contains("Overall: FAIL"));
    assert!(
        output.contains("project_disconnected") || output.contains("project_offline"),
        "{output}"
    );
    assert!(
        !output.contains("project_not_recommended_for_smoke"),
        "{output}"
    );
    assert!(
        !output.contains("project_not_safe_smoke_project"),
        "{output}"
    );
}

#[tokio::test]
async fn ops_smoke_preflight_non_git_project_short_circuits() {
    let mut projects = projects_fixture(true);
    projects["projects"][0]["capabilities"]["git_available"] = json!(false);
    let (output, requests) = run_smoke_preflight_with_projects(projects, "agent:ops:smoke").await;
    assert_eq!(
        smoke_request_kinds(&requests),
        vec!["runtime_status", "projects_list"]
    );
    assert_no_workspace_preflight_tools(&requests);
    assert!(output.contains("Overall: FAIL"));
    assert!(output.contains("project_git_unavailable"));
}
