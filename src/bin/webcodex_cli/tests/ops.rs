use super::support::*;
use crate::webcodex_cli::ops::{
    ops_agents_report, ops_projects_report, ops_smoke_preflight_report, ops_status_report,
    render_ops_status,
};

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
            },
            "server_static": {
                "status": "ok",
                "severity": "info"
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
fn ops_status_server_static_not_configured_info_does_not_fail() {
    let mut runtime = runtime_status_fixture();
    runtime["projects"]["server_static"] = json!({
        "status": "not_configured",
        "severity": "info"
    });
    let report = ops_status_report("https://ops.example.test", &Some(runtime));
    assert_ne!(report.verdict.status, "fail");
    assert!(report
        .verdict
        .warning_reasons
        .contains(&"server_static_not_configured_info".to_string()));
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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 16384];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            tx.send(request.clone()).unwrap();
            let first_line = request.lines().next().unwrap_or_default().to_string();
            let body = if first_line.starts_with("POST /api/runtime/status ") {
                json!({"success": true, "output": runtime_status_fixture()})
            } else if first_line.starts_with("POST /api/projects/list ") {
                json!({"success": true, "output": projects_fixture(true)})
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
    });

    let opts = OpsSmokePreflightOptions {
        common: OpsCommonOptions {
            server_url: format!("http://{}", addr),
            env_file: None,
            token_file: None,
            token: Some("secret-smoke-token".to_string()),
            json: false,
        },
        project: "agent:ops:smoke".to_string(),
    };
    let output = run_ops_command(OpsCommand::SmokePreflight(opts))
        .await
        .unwrap();
    handle.join().unwrap();
    let requests = rx.try_iter().collect::<Vec<_>>();
    assert_eq!(requests.len(), 4);
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
