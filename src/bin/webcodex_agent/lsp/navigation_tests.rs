use super::navigation::{handle_lsp_request, is_lsp_request_kind};
use super::position::{lsp_to_public, public_to_lsp};
use super::supervisor::{LspCommand, LspSupervisor, LspSupervisorConfig, PositionEncoding};
use crate::lsp_bridge::{
    parse_agent_lsp_result_envelope, AgentLspPayload, AgentLspRequest, AGENT_LSP_REQUEST_KIND,
};
use crate::shell_protocol::{ShellAgentShellRequest, ShellClientCapabilities};
use crate::webcodex_agent::config::AgentPolicy;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

struct FakeServerBinary {
    path: PathBuf,
    _dir: tempfile::TempDir,
}

fn fake_server_binary() -> &'static FakeServerBinary {
    static BINARY: OnceLock<FakeServerBinary> = OnceLock::new();
    BINARY.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake-lsp-server");
        let src =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bin/webcodex_agent/lsp/fake_server.rs");
        let status = Command::new("rustc")
            .arg("--edition=2021")
            .arg("--crate-name=webcodex_lsp_fake")
            .arg("-O")
            .arg("-o")
            .arg(&path)
            .arg(&src)
            .status()
            .expect("rustc fake server");
        assert!(status.success());
        FakeServerBinary { path, _dir: dir }
    })
}

struct NavFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    projects_dir: PathBuf,
    supervisor: LspSupervisor,
    policy: AgentPolicy,
}

impl NavFixture {
    fn new(scenario: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"demo\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        // Enough lines for fake-server ranges (0..30) plus emoji coverage.
        let mut main_rs =
            String::from("fn main() {}\nlet x = 1;\n// 😀 emoji line\nfn other() {}\n");
        for i in 4..40 {
            main_rs.push_str(&format!("// pad line {i}\n"));
        }
        fs::write(root.join("src/main.rs"), main_rs).unwrap();
        let mut other = String::from("fn helper() {}\nfn a() {}\nfn b() {}\n");
        for i in 3..10 {
            other.push_str(&format!("// other {i}\n"));
        }
        fs::write(root.join("src/other.rs"), other).unwrap();

        let projects_dir = temp.path().join("projects.d");
        fs::create_dir_all(&projects_dir).unwrap();
        fs::write(
            projects_dir.join("demo.toml"),
            format!("id = \"demo\"\npath = {:?}\n", root.to_string_lossy()),
        )
        .unwrap();

        let fake = fake_server_binary();
        let marker = temp.path().join("marker");
        let exit_marker = temp.path().join("exit");
        let supervisor = LspSupervisor::new(LspSupervisorConfig {
            rust_analyzer: Some(
                LspCommand::new(fake.path.as_os_str())
                    .arg(scenario)
                    .arg(marker.as_os_str())
                    .arg(exit_marker.as_os_str()),
            ),
            request_timeout: Duration::from_secs(3),
            initialize_timeout: Duration::from_secs(3),
            shutdown_timeout: Duration::from_millis(500),
            ..LspSupervisorConfig::default()
        });
        let policy = AgentPolicy {
            allow_cwd_anywhere: true,
            allowed_roots: vec![temp.path().to_path_buf()],
            ..AgentPolicy::default()
        };
        Self {
            _temp: temp,
            root,
            projects_dir,
            supervisor,
            policy,
        }
    }

    fn request(&self, payload: AgentLspPayload) -> Value {
        let req = ShellAgentShellRequest {
            request_id: "lsp-1".to_string(),
            client_id: "agent".to_string(),
            kind: AGENT_LSP_REQUEST_KIND.to_string(),
            job_id: None,
            cwd: None,
            path: None,
            content: None,
            max_bytes: None,
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: String::new(),
            stdin: None,
            timeout_secs: 30,
            requested_by: "test".to_string(),
            created_at: 0,
            lsp: Some(payload),
        };
        let result = handle_lsp_request(&self.policy, &self.projects_dir, &self.supervisor, &req);
        assert!(result.error.is_none(), "{result:?}");
        let stdout = result.stdout.expect("stdout envelope");
        let envelope = parse_agent_lsp_result_envelope(&stdout).expect("valid envelope");
        serde_json::to_value(envelope).unwrap()
    }
}

#[test]
fn lsp_kind_never_matches_shell() {
    assert!(is_lsp_request_kind("lsp"));
    assert!(!is_lsp_request_kind("run_shell"));
    assert!(!is_lsp_request_kind("file_read"));
}

#[test]
fn capability_default_is_false_and_new_agent_sets_true() {
    let old: ShellClientCapabilities = serde_json::from_str(r#"{"shell":true}"#).unwrap();
    assert!(!old.lsp_read_only_navigation);
    let mut caps = ShellClientCapabilities::default();
    caps.lsp_read_only_navigation = true;
    let json = serde_json::to_string(&caps).unwrap();
    assert!(json.contains("lsp_read_only_navigation"));
}

#[test]
fn status_does_not_start_server_and_unavailable_succeeds() {
    let fixture = NavFixture::new("normal");
    // Point supervisor at a missing binary via separate fixture.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(&root).unwrap();
    let projects_dir = temp.path().join("projects.d");
    fs::create_dir_all(&projects_dir).unwrap();
    fs::write(
        projects_dir.join("demo.toml"),
        format!("id = \"demo\"\npath = {:?}\n", root.to_string_lossy()),
    )
    .unwrap();
    let supervisor = LspSupervisor::new(LspSupervisorConfig {
        rust_analyzer: Some(LspCommand::new("/nonexistent/rust-analyzer-webcodex-test")),
        ..LspSupervisorConfig::default()
    });
    let policy = AgentPolicy {
        allow_cwd_anywhere: true,
        ..AgentPolicy::default()
    };
    let req = ShellAgentShellRequest {
        request_id: "s".into(),
        client_id: "c".into(),
        kind: AGENT_LSP_REQUEST_KIND.into(),
        job_id: None,
        cwd: None,
        path: None,
        content: None,
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        command: String::new(),
        stdin: None,
        timeout_secs: 10,
        requested_by: "t".into(),
        created_at: 0,
        lsp: Some(AgentLspPayload {
            project_id: "demo".into(),
            request: AgentLspRequest::Status,
        }),
    };
    let result = handle_lsp_request(&policy, &projects_dir, &supervisor, &req);
    let envelope = parse_agent_lsp_result_envelope(result.stdout.as_deref().unwrap()).unwrap();
    assert!(envelope.success);
    let value = envelope.result.unwrap();
    assert_eq!(value["servers"][0]["available"], false);
    assert_eq!(value["servers"][0]["status"], "unavailable");
    assert_eq!(value["servers"][0]["running"], false);
    // No absolute executable path.
    let serialized = value.to_string();
    assert!(!serialized.contains("/nonexistent"));
    let _ = fixture;
}

#[test]
fn document_symbols_hierarchical_and_budget() {
    let fixture = NavFixture::new("normal");
    let envelope = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "src/main.rs".into(),
            limit: 1,
        },
    });
    assert_eq!(envelope["success"], true);
    let result = &envelope["result"];
    assert_eq!(result["path"], "src/main.rs");
    assert_eq!(result["language"], "rust");
    assert!(result["total_count"].as_u64().unwrap() >= 1);
    assert_eq!(result["returned_count"], 1);
    assert_eq!(result["truncated"], true);
    assert_eq!(result["symbols"][0]["name"], "outer");
    assert_eq!(result["symbols"][0]["kind"], "class");
    // Nested children not returned once budget is exhausted at root.
    let serialized = result.to_string();
    assert!(!serialized.contains(fixture.root.to_string_lossy().as_ref()));
    assert!(!serialized.contains("file://"));
}

#[test]
fn document_symbols_symbol_information_fallback() {
    let fixture = NavFixture::new("symbol_information");
    let envelope = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "src/main.rs".into(),
            limit: 100,
        },
    });
    assert_eq!(envelope["success"], true);
    assert_eq!(envelope["result"]["symbols"][0]["name"], "main");
    assert_eq!(envelope["result"]["symbols"][0]["kind"], "function");
}

#[test]
fn definition_variants_and_external_invalid() {
    let single = NavFixture::new("normal").request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::GotoDefinition {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            limit: 20,
        },
    });
    assert_eq!(single["success"], true);
    assert_eq!(single["result"]["returned_count"], 1);
    assert_eq!(single["result"]["locations"][0]["path"], "src/main.rs");

    let multi = NavFixture::new("definition_array").request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::GotoDefinition {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            limit: 20,
        },
    });
    assert!(multi["result"]["returned_count"].as_u64().unwrap() >= 1);

    let link = NavFixture::new("definition_link").request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::GotoDefinition {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            limit: 20,
        },
    });
    assert_eq!(link["result"]["locations"][0]["path"], "src/main.rs");
    assert!(
        link["result"]["locations"][0]["target_range"].is_object(),
        "link response: {link}"
    );

    let external = NavFixture::new("definition_external").request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::GotoDefinition {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            limit: 20,
        },
    });
    assert_eq!(external["result"]["returned_count"], 0);
    assert!(
        external["result"]["external_results_omitted"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(!external.to_string().contains("/usr/lib"));

    let malformed = NavFixture::new("definition_malformed").request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::GotoDefinition {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            limit: 20,
        },
    });
    assert!(
        malformed["result"]["invalid_results_omitted"]
            .as_u64()
            .unwrap()
            >= 1
    );
}

#[test]
fn references_dedup_truncation_and_external() {
    let dedup = NavFixture::new("references_duplicates").request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::FindReferences {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            include_declaration: true,
            limit: 50,
        },
    });
    assert_eq!(dedup["success"], true);
    assert_eq!(dedup["result"]["total_results"], 3);
    assert_eq!(dedup["result"]["returned_count"], 2);

    let overflow = NavFixture::new("references_overflow").request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::FindReferences {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            include_declaration: true,
            limit: 5,
        },
    });
    assert_eq!(
        overflow["result"]["returned_count"], 5,
        "overflow: {overflow}"
    );
    assert_eq!(overflow["result"]["truncated"], true);
    assert_eq!(overflow["result"]["total_results"], 30);

    let external = NavFixture::new("references_external").request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::FindReferences {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            include_declaration: true,
            limit: 50,
        },
    });
    assert_eq!(external["result"]["external_results_omitted"], 1);
    assert_eq!(external["result"]["returned_count"], 1);
}

#[test]
fn rejects_absolute_traversal_symlink_and_non_rs() {
    let fixture = NavFixture::new("normal");
    let absolute = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "/etc/passwd.rs".into(),
            limit: 10,
        },
    });
    assert_eq!(absolute["success"], false);
    assert_eq!(absolute["error"]["code"], "invalid_project_path");

    let traversal = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "../secret.rs".into(),
            limit: 10,
        },
    });
    assert_eq!(traversal["success"], false);
    assert_eq!(traversal["error"]["code"], "invalid_project_path");

    let non_rs = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "Cargo.toml".into(),
            limit: 10,
        },
    });
    assert_eq!(non_rs["success"], false);
    assert_eq!(non_rs["error"]["code"], "unsupported_language");

    // Symlink outside project.
    let outside = fixture._temp.path().join("outside.rs");
    fs::write(&outside, "fn x() {}\n").unwrap();
    let link = fixture.root.join("src/linked.rs");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let sym = fixture.request(AgentLspPayload {
            project_id: "demo".into(),
            request: AgentLspRequest::DocumentSymbols {
                path: "src/linked.rs".into(),
                limit: 10,
            },
        });
        assert_eq!(sym["success"], false);
        assert_eq!(sym["error"]["code"], "invalid_project_path");
    }
}

#[test]
fn project_relative_normalization_and_no_absolute_in_result() {
    let fixture = NavFixture::new("normal");
    let envelope = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::GotoDefinition {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            limit: 20,
        },
    });
    let serialized = envelope.to_string();
    assert!(!serialized.contains(fixture.root.to_string_lossy().as_ref()));
    assert!(!serialized.contains("file://"));
    assert_eq!(envelope["result"]["locations"][0]["path"], "src/main.rs");
}

#[test]
fn utf_encoding_public_conversions() {
    let text = "a😀b\n";
    for encoding in [
        PositionEncoding::Utf8,
        PositionEncoding::Utf16,
        PositionEncoding::Utf32,
    ] {
        let (line, character) = public_to_lsp(text, 1, 3, encoding).unwrap();
        assert_eq!(line, 0);
        let back = lsp_to_public(text, line, character, encoding).unwrap();
        assert_eq!(back, (1, 3));
    }
}

#[test]
fn missing_lsp_payload_returns_structured_error() {
    let fixture = NavFixture::new("normal");
    let req = ShellAgentShellRequest {
        request_id: "x".into(),
        client_id: "c".into(),
        kind: AGENT_LSP_REQUEST_KIND.into(),
        job_id: None,
        cwd: None,
        path: None,
        content: None,
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        command: "echo should-not-run".into(),
        stdin: None,
        timeout_secs: 5,
        requested_by: "t".into(),
        created_at: 0,
        lsp: None,
    };
    let result = handle_lsp_request(
        &fixture.policy,
        &fixture.projects_dir,
        &fixture.supervisor,
        &req,
    );
    let envelope = parse_agent_lsp_result_envelope(result.stdout.as_deref().unwrap()).unwrap();
    assert!(!envelope.success);
    assert_eq!(envelope.error.unwrap().code, "missing_lsp_payload");
}

#[test]
fn lsp_request_ignores_command_field() {
    // Typed LSP handling must not consult or execute `command`.
    let fixture = NavFixture::new("normal");
    let marker = fixture._temp.path().join("shell-ran");
    let req = ShellAgentShellRequest {
        request_id: "req".into(),
        client_id: "c".into(),
        kind: AGENT_LSP_REQUEST_KIND.into(),
        job_id: None,
        cwd: Some(fixture.root.to_string_lossy().into()),
        path: None,
        content: None,
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        command: format!("printf ran > {:?}", marker),
        stdin: None,
        timeout_secs: 5,
        requested_by: "t".into(),
        created_at: 0,
        lsp: Some(AgentLspPayload {
            project_id: "demo".into(),
            request: AgentLspRequest::Status,
        }),
    };
    let result = handle_lsp_request(
        &fixture.policy,
        &fixture.projects_dir,
        &fixture.supervisor,
        &req,
    );
    assert!(result.error.is_none(), "{result:?}");
    assert!(!marker.exists(), "LSP handler must not execute command");
    let envelope = parse_agent_lsp_result_envelope(result.stdout.as_deref().unwrap()).unwrap();
    assert!(envelope.success);
}
