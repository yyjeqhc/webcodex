use super::navigation::{handle_lsp_request, is_lsp_request_kind};
use super::position::{lsp_to_public, public_to_lsp, MAX_LSP_DOCUMENT_BYTES};
use super::supervisor::{
    LspCommand, LspServerKind, LspSupervisor, LspSupervisorConfig, PositionEncoding,
};
use crate::lsp_bridge::{
    parse_agent_lsp_result_envelope, AgentLspPayload, AgentLspRequest, AGENT_LSP_REQUEST_KIND,
};
use crate::shell_protocol::{ShellAgentShellRequest, ShellClientCapabilities};
use crate::webcodex_agent::config::AgentPolicy;
use serde_json::Value;
use std::collections::HashMap;
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

/// Minimal agent shell request carrying a typed LSP payload.
fn shell_lsp_request(payload: AgentLspPayload) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
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
        timeout_secs: 60,
        requested_by: "test".to_string(),
        created_at: 0,
        validation: None,
        lsp: Some(payload),
    }
}

struct NavFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    projects_dir: PathBuf,
    marker: PathBuf,
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
        Self::finish(temp, root, scenario, LspServerKind::RustAnalyzer)
    }

    /// Fixture for any language: writes the given project-relative files
    /// (creating parent directories) and wires the fake server under `kind`.
    fn with_language(scenario: &str, kind: LspServerKind, files: &[(&str, &str)]) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir_all(&root).unwrap();
        for (relative, body) in files {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, body).unwrap();
        }
        Self::finish(temp, root, scenario, kind)
    }

    /// Shared wiring: register the project, start a fake server under `kind`,
    /// and build the fixture. The fake server is language-agnostic, so the
    /// language behavior under test comes from the profile registry.
    fn finish(temp: tempfile::TempDir, root: PathBuf, scenario: &str, kind: LspServerKind) -> Self {
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
            commands: HashMap::from([(
                kind,
                LspCommand::new(fake.path.as_os_str())
                    .arg(scenario)
                    .arg(marker.as_os_str())
                    .arg(exit_marker.as_os_str()),
            )]),
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
            marker,
            supervisor,
            policy,
        }
    }

    fn request(&self, payload: AgentLspPayload) -> Value {
        let req = shell_lsp_request(payload);
        let result = handle_lsp_request(&self.policy, &self.projects_dir, &self.supervisor, &req);
        assert!(result.error.is_none(), "{result:?}");
        let stdout = result.stdout.expect("stdout envelope");
        let envelope = parse_agent_lsp_result_envelope(&stdout).expect("valid envelope");
        serde_json::to_value(envelope).unwrap()
    }

    fn diagnostics(&self, limit: usize) -> Value {
        self.request(AgentLspPayload {
            project_id: "demo".into(),
            request: AgentLspRequest::DocumentDiagnostics {
                path: "src/main.rs".into(),
                limit,
            },
        })
    }

    fn hover(&self, line: usize, column: usize) -> Value {
        self.request(AgentLspPayload {
            project_id: "demo".into(),
            request: AgentLspRequest::Hover {
                path: "src/main.rs".into(),
                line,
                column,
            },
        })
    }

    fn workspace_symbols(&self, query: &str, limit: usize) -> Value {
        self.request(AgentLspPayload {
            project_id: "demo".into(),
            request: AgentLspRequest::WorkspaceSymbols {
                query: query.into(),
                limit,
            },
        })
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
    let available = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::Status,
    });
    assert_eq!(available["success"], true);
    assert_eq!(available["result"]["servers"][0]["status"], "available");
    assert!(
        !fixture.marker.exists(),
        "status-only startup probes must not start the fake rust-analyzer"
    );
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
        commands: HashMap::from([(
            LspServerKind::RustAnalyzer,
            LspCommand::new("/nonexistent/rust-analyzer-webcodex-test"),
        )]),
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
        validation: None,
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
fn navigation_reuses_one_did_open_for_the_same_document() {
    let fixture = NavFixture::new("normal");
    let requests = [
        AgentLspRequest::DocumentSymbols {
            path: "src/main.rs".into(),
            limit: 10,
        },
        AgentLspRequest::GotoDefinition {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            limit: 10,
        },
        AgentLspRequest::FindReferences {
            path: "src/main.rs".into(),
            line: 1,
            column: 1,
            include_declaration: true,
            limit: 10,
        },
    ];
    for _ in 0..2 {
        for request in &requests {
            let envelope = fixture.request(AgentLspPayload {
                project_id: "demo".into(),
                request: request.clone(),
            });
            assert_eq!(envelope["success"], true, "{envelope}");
        }
    }
    let marker = fs::read_to_string(&fixture.marker).unwrap();
    assert_eq!(
        marker
            .lines()
            .filter(|line| line.starts_with("didOpen:"))
            .count(),
        1,
        "{marker}"
    );
}

#[test]
fn navigation_sends_full_text_changes_once_per_disk_content_version() {
    let fixture = NavFixture::new("normal");
    let request = || {
        fixture.request(AgentLspPayload {
            project_id: "demo".into(),
            request: AgentLspRequest::DocumentSymbols {
                path: "src/main.rs".into(),
                limit: 10,
            },
        })
    };

    assert_eq!(request()["success"], true);
    fs::write(fixture.root.join("src/main.rs"), "fn changed_once() {}\n").unwrap();
    assert_eq!(request()["success"], true);
    assert_eq!(request()["success"], true);
    fs::write(fixture.root.join("src/main.rs"), "fn changed_twice() {}\n").unwrap();
    assert_eq!(request()["success"], true);
    assert_eq!(request()["success"], true);

    let marker = fs::read_to_string(&fixture.marker).unwrap();
    let opens = marker
        .lines()
        .filter(|line| line.starts_with("didOpen:"))
        .collect::<Vec<_>>();
    let changes = marker
        .lines()
        .filter(|line| line.starts_with("didChange:"))
        .collect::<Vec<_>>();
    assert_eq!(opens.len(), 1, "{marker}");
    assert_eq!(changes.len(), 2, "{marker}");
    assert!(opens[0].contains("\"version\":1"), "{marker}");
    assert!(changes[0].contains("\"version\":2"), "{marker}");
    assert!(changes[0].contains("fn changed_once()"), "{marker}");
    assert!(changes[1].contains("\"version\":3"), "{marker}");
    assert!(changes[1].contains("fn changed_twice()"), "{marker}");
    assert!(changes
        .iter()
        .all(|line| line.contains("\"contentChanges\":[{\"text\":")));
}

#[test]
fn document_diagnostics_empty_and_one_error_are_fresh_successes() {
    let empty = NavFixture::new("diagnostics_empty").diagnostics(100);
    assert_eq!(empty["success"], true, "{empty}");
    assert_eq!(empty["result"]["diagnostics"], serde_json::json!([]));
    assert_eq!(empty["result"]["total_count"], 0);
    assert_eq!(empty["result"]["fresh"], true);
    assert_eq!(empty["result"]["timed_out"], false);
    assert_eq!(empty["result"]["published_version"], 1);

    let one = NavFixture::new("diagnostics_one").diagnostics(100);
    assert_eq!(one["success"], true, "{one}");
    let result = &one["result"];
    assert_eq!(result["returned_count"], 1);
    assert_eq!(result["diagnostics"][0]["severity"], "error");
    assert_eq!(result["diagnostics"][0]["severity_code"], 1);
    assert_eq!(result["diagnostics"][0]["code"], "E0308");
    assert_eq!(result["diagnostics"][0]["source"], "rust-analyzer");
    assert_eq!(result["diagnostics"][0]["message"], "type mismatch");
    assert_eq!(result["diagnostics"][0]["range"]["start"]["line"], 1);
    assert_eq!(result["diagnostics"][0]["range"]["start"]["column"], 1);
}

#[test]
fn document_diagnostics_normalizes_sorts_tags_and_omits_private_payloads() {
    let envelope = NavFixture::new("diagnostics_mixed").diagnostics(100);
    assert_eq!(envelope["success"], true, "{envelope}");
    let result = &envelope["result"];
    let severities = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["severity"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        severities,
        ["error", "warning", "information", "hint", "unknown"]
    );
    assert_eq!(result["diagnostics"][1]["code"], "7");
    assert_eq!(
        result["diagnostics"][1]["tags"],
        serde_json::json!(["unnecessary"])
    );
    assert_eq!(
        result["diagnostics"][3]["tags"],
        serde_json::json!(["deprecated", "unknown"])
    );
    assert_eq!(result["related_information_omitted"], 1);
    let serialized = result.to_string();
    assert!(!serialized.contains("file://"), "{serialized}");
    assert!(!serialized.contains("/secret/"), "{serialized}");
    assert!(!serialized.contains("private"), "{serialized}");
    assert!(!serialized.contains("relatedInformation"), "{serialized}");
}

#[test]
fn document_diagnostics_deduplicates_and_truncates_on_the_agent() {
    let duplicate = NavFixture::new("diagnostics_duplicates").diagnostics(100);
    assert_eq!(duplicate["result"]["total_count"], 2);
    assert_eq!(duplicate["result"]["returned_count"], 1);

    let overflow = NavFixture::new("diagnostics_overflow").diagnostics(10);
    assert_eq!(overflow["success"], true, "{overflow}");
    assert_eq!(overflow["result"]["total_count"], 520);
    assert_eq!(overflow["result"]["returned_count"], 10);
    assert_eq!(overflow["result"]["truncated"], true);

    let text_budget = NavFixture::new("diagnostics_text_budget").diagnostics(200);
    assert_eq!(text_budget["success"], true, "{text_budget}");
    assert!(text_budget["result"]["returned_count"].as_u64().unwrap() < 30);
    assert_eq!(text_budget["result"]["truncated"], true);
}

#[test]
fn document_diagnostics_omits_bad_ranges_and_converts_utf16_emoji() {
    let malformed = NavFixture::new("diagnostics_malformed_range").diagnostics(100);
    assert_eq!(malformed["result"]["returned_count"], 1, "{malformed}");
    assert_eq!(malformed["result"]["invalid_results_omitted"], 3);

    let emoji = NavFixture::new("diagnostics_utf16").diagnostics(100);
    assert_eq!(emoji["success"], true, "{emoji}");
    let range = &emoji["result"]["diagnostics"][0]["range"];
    assert_eq!(range["start"], serde_json::json!({"line": 3, "column": 4}));
    assert_eq!(range["end"], serde_json::json!({"line": 3, "column": 5}));
}

#[test]
fn document_diagnostics_bounds_and_sanitizes_text_fields() {
    let envelope = NavFixture::new("diagnostics_oversized_message").diagnostics(100);
    let diagnostic = &envelope["result"]["diagnostics"][0];
    assert!(diagnostic["message"].as_str().unwrap().chars().count() <= 4096);
    assert!(diagnostic["source"].as_str().unwrap().chars().count() <= 128);
    assert!(diagnostic["code"].as_str().unwrap().chars().count() <= 128);
    assert!(!diagnostic.to_string().contains("file://"));
}

#[test]
fn document_diagnostics_handles_publication_timing_and_timeouts() {
    let delayed = NavFixture::new("diagnostics_delayed").diagnostics(100);
    assert_eq!(delayed["result"]["fresh"], true, "{delayed}");
    assert_eq!(delayed["result"]["timed_out"], false);

    let timeout = NavFixture::new("diagnostics_timeout").diagnostics(100);
    assert_eq!(timeout["success"], true, "{timeout}");
    assert_eq!(timeout["result"]["diagnostics"], serde_json::json!([]));
    assert_eq!(timeout["result"]["fresh"], false);
    assert_eq!(timeout["result"]["timed_out"], true);

    let stale_fixture = NavFixture::new("diagnostics_stale_then_timeout");
    let first = stale_fixture.diagnostics(100);
    assert_eq!(first["result"]["fresh"], true, "{first}");
    assert_eq!(first["result"]["published_version"], 0);
    let stale = stale_fixture.diagnostics(100);
    assert_eq!(stale["result"]["fresh"], false, "{stale}");
    assert_eq!(stale["result"]["timed_out"], true);
    assert_eq!(stale["result"]["published_version"], 0);
}

#[test]
fn document_diagnostics_ignores_wrong_external_and_malformed_notifications() {
    for scenario in ["diagnostics_wrong_uri", "diagnostics_external_uri"] {
        let result = NavFixture::new(scenario).diagnostics(100);
        assert_eq!(result["success"], true, "scenario={scenario}: {result}");
        assert_eq!(result["result"]["returned_count"], 0);
        assert_eq!(result["result"]["fresh"], false);
        assert_eq!(result["result"]["timed_out"], true);
        assert!(!result.to_string().contains("file://"));
        assert!(!result.to_string().contains("/usr/lib"));
    }

    let malformed = NavFixture::new("diagnostics_malformed_notification").diagnostics(100);
    assert_eq!(malformed["success"], true, "{malformed}");
    assert_eq!(malformed["result"]["fresh"], true);
    assert_eq!(malformed["result"]["timed_out"], false);
}

#[test]
fn hover_normalizes_markup_content_string_and_marked_string_forms() {
    let markdown = NavFixture::new("hover_markup_markdown").hover(1, 1);
    assert_eq!(markdown["success"], true, "{markdown}");
    assert_eq!(markdown["result"]["hover"]["kind"], "markdown");
    assert_eq!(markdown["result"]["hover"]["value"], "**main** docs");
    assert_eq!(
        markdown["result"]["hover"]["range"]["start"],
        serde_json::json!({"line": 1, "column": 1})
    );

    let plaintext = NavFixture::new("hover_markup_plaintext").hover(1, 1);
    assert_eq!(plaintext["result"]["hover"]["kind"], "plaintext");
    assert_eq!(plaintext["result"]["hover"]["value"], "plain docs");

    let string = NavFixture::new("hover_string").hover(1, 1);
    assert_eq!(string["result"]["hover"]["kind"], "markdown");
    assert_eq!(string["result"]["hover"]["value"], "string docs");

    let marked = NavFixture::new("hover_marked_string").hover(1, 1);
    let marked_value = marked["result"]["hover"]["value"].as_str().unwrap();
    assert!(marked_value.starts_with("```rust\n"), "{marked_value}");
    assert!(marked_value.ends_with("\n```"), "{marked_value}");
}

#[test]
fn hover_normalizes_arrays_null_bounds_ranges_and_utf16() {
    let array = NavFixture::new("hover_array").hover(1, 1);
    assert_eq!(array["success"], true, "{array}");
    let value = array["result"]["hover"]["value"].as_str().unwrap();
    assert!(value.starts_with("first\n\n```rust"), "{value}");
    assert!(value.ends_with("\n\nlast"), "{value}");

    let null = NavFixture::new("hover_null").hover(1, 1);
    assert_eq!(null["success"], true, "{null}");
    assert_eq!(null["result"]["hover"], serde_json::Value::Null);
    assert_eq!(null["result"]["truncated"], false);

    let oversized = NavFixture::new("hover_oversized").hover(1, 1);
    assert_eq!(oversized["result"]["truncated"], true);
    assert!(
        oversized["result"]["hover"]["value"]
            .as_str()
            .unwrap()
            .chars()
            .count()
            <= 16 * 1024
    );

    let invalid = NavFixture::new("hover_invalid_range").hover(1, 1);
    assert_eq!(invalid["result"]["hover"]["range"], serde_json::Value::Null);
    assert_eq!(invalid["result"]["range_omitted"], true);

    let emoji = NavFixture::new("hover_utf16").hover(3, 4);
    assert_eq!(emoji["success"], true, "{emoji}");
    assert_eq!(
        emoji["result"]["hover"]["range"],
        serde_json::json!({
            "start": {"line": 3, "column": 4},
            "end": {"line": 3, "column": 5}
        })
    );
}

#[test]
fn hover_sanitizes_private_material_and_rejects_malformed_contents() {
    let sanitized = NavFixture::new("hover_sanitizer").hover(1, 1);
    assert_eq!(sanitized["success"], true, "{sanitized}");
    let serialized = sanitized.to_string();
    assert!(!serialized.contains("file://"), "{serialized}");
    assert!(!serialized.contains("/secret/"), "{serialized}");
    assert!(!serialized.contains("\\u0001"), "{serialized}");

    let malformed = NavFixture::new("hover_malformed").hover(1, 1);
    assert_eq!(malformed["success"], false, "{malformed}");
    assert_eq!(malformed["error"]["code"], "lsp_protocol_error");
}

#[test]
fn workspace_symbols_supports_information_workspace_and_uri_only_shapes() {
    let information = NavFixture::new("workspace_symbol_information").workspace_symbols("Tool", 50);
    assert_eq!(information["success"], true, "{information}");
    let symbol = &information["result"]["symbols"][0];
    assert_eq!(symbol["name"], "ToolRuntime");
    assert_eq!(symbol["kind"], "struct");
    assert_eq!(symbol["kind_code"], 23);
    assert_eq!(symbol["container_name"], "runtime");
    assert_eq!(symbol["path"], "src/main.rs");
    assert!(symbol["range"].is_object());

    let workspace = NavFixture::new("workspace_symbol").workspace_symbols("Agent", 50);
    assert_eq!(workspace["result"]["symbols"][0]["name"], "AgentBridge");
    assert_eq!(workspace["result"]["symbols"][0]["path"], "src/other.rs");
    assert!(!workspace.to_string().contains("hidden"));

    let uri_only = NavFixture::new("workspace_uri_only").workspace_symbols("Deferred", 50);
    assert_eq!(uri_only["result"]["symbols"][0]["path"], "src/main.rs");
    assert_eq!(
        uri_only["result"]["symbols"][0]["range"],
        serde_json::Value::Null
    );

    let empty = NavFixture::new("workspace_empty").workspace_symbols("Nothing", 50);
    assert_eq!(empty["result"]["symbols"], serde_json::json!([]));
    assert_eq!(empty["result"]["total_results"], 0);
}

#[test]
fn workspace_symbols_sorts_deduplicates_filters_and_truncates() {
    let duplicates = NavFixture::new("workspace_duplicates").workspace_symbols("Any", 50);
    assert_eq!(duplicates["result"]["total_results"], 3);
    assert_eq!(duplicates["result"]["returned_count"], 2);
    assert_eq!(duplicates["result"]["symbols"][0]["name"], "Alpha");
    assert_eq!(duplicates["result"]["symbols"][1]["name"], "Zulu");

    let external = NavFixture::new("workspace_external").workspace_symbols("Any", 50);
    assert_eq!(external["result"]["returned_count"], 1);
    assert_eq!(external["result"]["external_results_omitted"], 1);
    assert!(!external.to_string().contains("/usr/lib"));
    assert!(!external.to_string().contains("file://"));

    let malformed = NavFixture::new("workspace_malformed").workspace_symbols("Any", 50);
    assert_eq!(malformed["result"]["returned_count"], 1, "{malformed}");
    assert_eq!(malformed["result"]["invalid_results_omitted"], 4);

    let overflow = NavFixture::new("workspace_overflow").workspace_symbols("Symbol", 20);
    assert_eq!(overflow["result"]["total_results"], 230);
    assert_eq!(overflow["result"]["returned_count"], 20);
    assert_eq!(overflow["result"]["truncated"], true);
}

#[test]
fn workspace_symbols_validates_query_and_sanitizes_names() {
    let trimmed = NavFixture::new("workspace_empty").workspace_symbols("  ToolRuntime  ", 50);
    assert_eq!(trimmed["success"], true, "{trimmed}");
    assert_eq!(trimmed["result"]["query"], "ToolRuntime");

    for query in ["", "   "] {
        let fixture = NavFixture::new("workspace_empty");
        let result = fixture.workspace_symbols(query, 50);
        assert_eq!(result["success"], false, "query={query:?}: {result}");
        assert_eq!(result["error"]["code"], "invalid_arguments");
        assert!(!fixture.marker.exists());
    }
    let long = "x".repeat(201);
    let fixture = NavFixture::new("workspace_empty");
    let result = fixture.workspace_symbols(&long, 50);
    assert_eq!(result["success"], false);
    assert!(!fixture.marker.exists());

    let fixture = NavFixture::new("workspace_empty");
    let absolute = fixture.workspace_symbols("/tmp/private", 50);
    assert_eq!(absolute["success"], false);
    assert!(!fixture.marker.exists());

    let sanitized = NavFixture::new("workspace_sanitizer").workspace_symbols("Safe", 50);
    assert_eq!(sanitized["success"], true, "{sanitized}");
    assert_eq!(sanitized["result"]["symbols"][0]["name"], "<path>");
    assert_eq!(
        sanitized["result"]["symbols"][0]["container_name"],
        "<path>"
    );
    assert!(!sanitized.to_string().contains("file://"));
    assert!(!sanitized.to_string().contains("/secret/"));
}

#[test]
fn navigation_restart_opens_the_document_on_the_new_server_instance() {
    let fixture = NavFixture::new("restart_then_success");
    let envelope = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "src/main.rs".into(),
            limit: 10,
        },
    });
    assert_eq!(envelope["success"], true, "{envelope}");

    let marker = fs::read_to_string(&fixture.marker).unwrap();
    let start_pids = marker
        .lines()
        .filter_map(|line| line.strip_prefix("start:"))
        .filter_map(|line| line.split(':').next())
        .collect::<Vec<_>>();
    assert_eq!(start_pids.len(), 2, "{marker}");
    for pid in start_pids {
        assert_eq!(
            marker
                .lines()
                .filter(|line| line.starts_with(&format!("didOpen:{pid}:")))
                .count(),
            1,
            "{marker}"
        );
    }
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
fn oversized_document_is_rejected_before_read_and_server_start() {
    let fixture = NavFixture::new("normal");
    // Sparse file: metadata length is over the cap without writing the bytes.
    let oversized = fixture.root.join("src/generated.rs");
    let file = fs::File::create(&oversized).unwrap();
    file.set_len(MAX_LSP_DOCUMENT_BYTES + 1).unwrap();
    drop(file);

    let symbols = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "src/generated.rs".into(),
            limit: 10,
        },
    });
    assert_eq!(symbols["success"], false);
    assert_eq!(symbols["error"]["code"], "document_too_large");

    let goto = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::GotoDefinition {
            path: "src/generated.rs".into(),
            line: 1,
            column: 1,
            limit: 10,
        },
    });
    assert_eq!(goto["success"], false);
    assert_eq!(goto["error"]["code"], "document_too_large");

    // The guard fires before any didOpen, so no server may have started.
    assert!(
        !fixture.marker.exists(),
        "oversized documents must be rejected before starting rust-analyzer"
    );
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
        validation: None,
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
        validation: None,
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

fn recorded_did_open_language_id(marker: &Path) -> String {
    let text = fs::read_to_string(marker).unwrap();
    let line = text
        .lines()
        .find(|line| line.starts_with("didOpen:"))
        .expect("fake server should record a didOpen");
    let body = line.splitn(3, ':').nth(2).expect("didOpen body");
    let value: Value = serde_json::from_str(body).expect("didOpen body is JSON");
    value["params"]["textDocument"]["languageId"]
        .as_str()
        .expect("languageId")
        .to_string()
}

#[test]
fn navigation_routes_python_file_to_pyright_with_python_language_id() {
    let fixture = NavFixture::with_language(
        "normal",
        LspServerKind::Pyright,
        &[
            ("pyproject.toml", "[project]\nname = \"demo\"\n"),
            ("src/app.py", "def main():\n    return 1\n"),
        ],
    );
    let envelope = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "src/app.py".into(),
            limit: 10,
        },
    });
    assert_eq!(envelope["success"], true, "{envelope}");
    assert_eq!(envelope["result"]["language"], "python");
    assert_eq!(recorded_did_open_language_id(&fixture.marker), "python");
}

#[test]
fn navigation_routes_tsx_file_with_react_dialect_language_id() {
    let fixture = NavFixture::with_language(
        "normal",
        LspServerKind::TypeScriptLanguageServer,
        &[
            ("tsconfig.json", "{}\n"),
            ("src/App.tsx", "export const App = () => null;\n"),
        ],
    );
    let envelope = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "src/App.tsx".into(),
            limit: 10,
        },
    });
    assert_eq!(envelope["success"], true, "{envelope}");
    // Public label is the profile's primary language...
    assert_eq!(envelope["result"]["language"], "typescript");
    // ...but the LSP wire announces the React dialect for `.tsx`.
    assert_eq!(
        recorded_did_open_language_id(&fixture.marker),
        "typescriptreact"
    );
}

#[test]
fn unsupported_extension_is_rejected_with_supported_list() {
    let fixture = NavFixture::with_language(
        "normal",
        LspServerKind::Pyright,
        &[
            ("pyproject.toml", "[project]\n"),
            ("notes.md", "# not a source file\n"),
        ],
    );
    let envelope = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "notes.md".into(),
            limit: 10,
        },
    });
    assert_eq!(envelope["success"], false, "{envelope}");
    assert_eq!(envelope["error"]["code"], "unsupported_language");
    let message = envelope["error"]["message"].as_str().unwrap();
    assert!(message.contains(".py"), "{message}");
    assert!(message.contains(".ts"), "{message}");
}

#[test]
fn lsp_status_reports_every_registered_language_server() {
    let fixture = NavFixture::with_language(
        "normal",
        LspServerKind::Pyright,
        &[("pyproject.toml", "[project]\n"), ("src/app.py", "x = 1\n")],
    );
    let envelope = fixture.request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::Status,
    });
    assert_eq!(envelope["success"], true, "{envelope}");
    assert_eq!(
        envelope["result"]["detected_languages"],
        serde_json::json!(["python"])
    );
    let servers = envelope["result"]["servers"].as_array().unwrap();
    let names: Vec<&str> = servers
        .iter()
        .map(|server| server["server"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"rust-analyzer"), "{names:?}");
    assert!(names.contains(&"pyright"), "{names:?}");
    assert!(names.contains(&"typescript-language-server"), "{names:?}");
    // The pyright server is configured (fake) here, so it resolves available.
    let pyright = servers
        .iter()
        .find(|server| server["server"] == "pyright")
        .unwrap();
    assert_eq!(pyright["language"], "python");
    assert_eq!(pyright["available"], true, "{pyright}");
}

/// Resolve a real language server for an ignored end-to-end smoke test: the
/// given env override first, then `executable` on `PATH`.
fn real_language_server(env_var: &str, executable: &str) -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os(env_var) {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(executable))
        .find(|candidate| candidate.is_file())
}

/// Real end-to-end validation that the language abstraction drives an actual
/// second language server, not just the fake. Ignored by default because it
/// needs `pyright-langserver` (`npm i -g pyright`) and Node on the host.
///
/// Run with:
/// `cargo test --bin webcodex-agent real_pyright -- --ignored --nocapture`
#[test]
#[ignore = "requires a real pyright-langserver (npm i -g pyright)"]
fn real_pyright_document_symbols_end_to_end() {
    let Some(pyright) = real_language_server("WEBCODEX_PYRIGHT", "pyright-langserver") else {
        panic!("pyright-langserver not found; set WEBCODEX_PYRIGHT or install pyright");
    };

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("pyproject.toml"), "[project]\nname = \"demo\"\n").unwrap();
    fs::write(
        root.join("src/app.py"),
        "def greet(name):\n    return f\"hi {name}\"\n\n\nclass Widget:\n    def render(self):\n        return greet(\"world\")\n",
    )
    .unwrap();

    let projects_dir = temp.path().join("projects.d");
    fs::create_dir_all(&projects_dir).unwrap();
    fs::write(
        projects_dir.join("demo.toml"),
        format!("id = \"demo\"\npath = {:?}\n", root.to_string_lossy()),
    )
    .unwrap();

    // Exercise the real supervisor path with generous timeouts; pyright does a
    // cold analysis on start. `--stdio` mirrors the profile's default_args.
    let supervisor = LspSupervisor::new(LspSupervisorConfig {
        commands: HashMap::from([(
            LspServerKind::Pyright,
            LspCommand::new(pyright).arg("--stdio"),
        )]),
        request_timeout: Duration::from_secs(30),
        initialize_timeout: Duration::from_secs(30),
        shutdown_timeout: Duration::from_secs(3),
        ..LspSupervisorConfig::default()
    });
    let policy = AgentPolicy {
        allow_cwd_anywhere: true,
        allowed_roots: vec![temp.path().to_path_buf()],
        ..AgentPolicy::default()
    };

    let req = shell_lsp_request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "src/app.py".into(),
            limit: 50,
        },
    });
    let result = handle_lsp_request(&policy, &projects_dir, &supervisor, &req);
    assert!(result.error.is_none(), "{result:?}");
    let envelope =
        parse_agent_lsp_result_envelope(result.stdout.as_deref().unwrap()).expect("envelope");
    let value = serde_json::to_value(&envelope).unwrap();
    assert_eq!(value["success"], true, "{value}");
    assert_eq!(value["result"]["language"], "python");

    // Flatten symbol names across the (possibly nested) document symbol tree.
    fn collect_names(node: &Value, out: &mut Vec<String>) {
        if let Some(name) = node["name"].as_str() {
            out.push(name.to_string());
        }
        if let Some(children) = node["children"].as_array() {
            for child in children {
                collect_names(child, out);
            }
        }
    }
    let mut names = Vec::new();
    for symbol in value["result"]["symbols"]
        .as_array()
        .expect("symbols array")
    {
        collect_names(symbol, &mut names);
    }
    assert!(
        names.iter().any(|name| name == "greet"),
        "expected `greet` in {names:?}"
    );
    assert!(
        names.iter().any(|name| name == "Widget"),
        "expected `Widget` in {names:?}"
    );

    // Goto-definition on the `greet(...)` call in `render` resolves back to the
    // function definition on line 1 — real cross-symbol navigation.
    let goto = shell_lsp_request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::GotoDefinition {
            path: "src/app.py".into(),
            line: 7,
            column: 16,
            limit: 10,
        },
    });
    let goto_result = handle_lsp_request(&policy, &projects_dir, &supervisor, &goto);
    let goto_value = serde_json::to_value(
        parse_agent_lsp_result_envelope(goto_result.stdout.as_deref().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(goto_value["success"], true, "{goto_value}");
    let locations = goto_value["result"]["locations"].as_array().unwrap();
    assert!(
        locations
            .iter()
            .any(|loc| loc["path"] == "src/app.py" && loc["range"]["start"]["line"] == 1),
        "greet definition should resolve to line 1: {goto_value}"
    );
}

/// Real end-to-end validation for the third language: a `.tsx` file driven
/// through the same supervisor path against a real
/// `typescript-language-server`. Ignored by default (needs the server and
/// Node). Run with:
/// `cargo test --bin webcodex-agent real_typescript -- --ignored --nocapture`
#[test]
// Needs typescript@5 (classic tsserver.js); typescript@7 native preview lacks it.
#[ignore = "requires typescript-language-server + typescript@5 (npm i -g typescript-language-server typescript@5)"]
fn real_typescript_document_symbols_end_to_end() {
    let Some(server) = real_language_server(
        "WEBCODEX_TYPESCRIPT_LANGUAGE_SERVER",
        "typescript-language-server",
    ) else {
        panic!("typescript-language-server not found; set WEBCODEX_TYPESCRIPT_LANGUAGE_SERVER");
    };

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("tsconfig.json"),
        "{\n  \"compilerOptions\": { \"jsx\": \"react-jsx\", \"strict\": true }\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/App.tsx"),
        "export function greet(name: string): string {\n  return `hi ${name}`;\n}\n\nexport const App = () => greet(\"world\");\n",
    )
    .unwrap();

    // tsserver needs a TypeScript install; a real TS project has
    // `node_modules/typescript`. Mirror that by linking the global typescript
    // (derived from the server's npm prefix) into the fixture.
    let ts_lib = server
        .parent()
        .and_then(Path::parent)
        .map(|prefix| prefix.join("lib/node_modules/typescript"))
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| {
            panic!("global typescript not found next to {server:?}; npm i -g typescript")
        });
    fs::create_dir_all(root.join("node_modules")).unwrap();
    std::os::unix::fs::symlink(&ts_lib, root.join("node_modules/typescript")).unwrap();

    let projects_dir = temp.path().join("projects.d");
    fs::create_dir_all(&projects_dir).unwrap();
    fs::write(
        projects_dir.join("demo.toml"),
        format!("id = \"demo\"\npath = {:?}\n", root.to_string_lossy()),
    )
    .unwrap();

    let supervisor = LspSupervisor::new(LspSupervisorConfig {
        commands: HashMap::from([(
            LspServerKind::TypeScriptLanguageServer,
            LspCommand::new(server).arg("--stdio"),
        )]),
        request_timeout: Duration::from_secs(30),
        initialize_timeout: Duration::from_secs(30),
        shutdown_timeout: Duration::from_secs(3),
        ..LspSupervisorConfig::default()
    });
    let policy = AgentPolicy {
        allow_cwd_anywhere: true,
        allowed_roots: vec![temp.path().to_path_buf()],
        ..AgentPolicy::default()
    };

    let req = shell_lsp_request(AgentLspPayload {
        project_id: "demo".into(),
        request: AgentLspRequest::DocumentSymbols {
            path: "src/App.tsx".into(),
            limit: 50,
        },
    });
    let result = handle_lsp_request(&policy, &projects_dir, &supervisor, &req);
    assert!(result.error.is_none(), "{result:?}");
    let value = serde_json::to_value(
        parse_agent_lsp_result_envelope(result.stdout.as_deref().unwrap()).expect("envelope"),
    )
    .unwrap();
    assert_eq!(value["success"], true, "{value}");
    // Public label stays the primary language even for the `.tsx` dialect.
    assert_eq!(value["result"]["language"], "typescript");

    fn collect_names(node: &Value, out: &mut Vec<String>) {
        if let Some(name) = node["name"].as_str() {
            out.push(name.to_string());
        }
        if let Some(children) = node["children"].as_array() {
            for child in children {
                collect_names(child, out);
            }
        }
    }
    let mut names = Vec::new();
    for symbol in value["result"]["symbols"]
        .as_array()
        .expect("symbols array")
    {
        collect_names(symbol, &mut names);
    }
    assert!(
        names.iter().any(|name| name == "greet"),
        "expected `greet` in {names:?}"
    );
    assert!(
        names.iter().any(|name| name == "App"),
        "expected `App` in {names:?}"
    );
}
