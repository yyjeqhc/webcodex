use super::*;

// Manual, networked evidence only; ordinary CI never runs these tests:
// cargo test --locked -p webcodex-runner real_codex_acp_opt_in -- --ignored --test-threads=1
// Uses existing local authentication without reading its files. Optional test-owned
// WEBCODEX_TEST_CODEX_ACP_SCRIPT selects an existing 2.1.1 dist/index.js;
// WEBCODEX_TEST_CODEX_ACP_NODE selects its Node executable (default: node).
// Without SCRIPT, npx resolves the exact pinned package, never latest.
const CODEX_ACP_PACKAGE: &str = "@agentclientprotocol/codex-acp@2.1.1";
const FIXTURE_TESTS: &str = r#"import unittest
from clamp import clamp

class ClampTests(unittest.TestCase):
    def test_inside(self):
        self.assertEqual(clamp(4, 1, 7), 4)
    def test_lower(self):
        self.assertEqual(clamp(-5, 1, 7), 1)
    def test_upper(self):
        self.assertEqual(clamp(12, 1, 7), 7)
    def test_boundaries(self):
        self.assertEqual(clamp(1, 1, 7), 1)
        self.assertEqual(clamp(7, 1, 7), 7)
    def test_equal_limits(self):
        self.assertEqual(clamp(100, 3, 3), 3)
    def test_negative(self):
        self.assertEqual(clamp(-8, -6, -2), -6)
    def test_reversed(self):
        with self.assertRaises(ValueError):
            clamp(4, 7, 1)
"#;

fn real_codex_config() -> AcpConfig {
    let (executable, args) = match std::env::var_os("WEBCODEX_TEST_CODEX_ACP_SCRIPT") {
        Some(script) => {
            let script = PathBuf::from(script);
            assert!(
                script.is_absolute() && script.is_file(),
                "ACP SCRIPT must be an existing absolute file"
            );
            // Read package metadata only, never Codex auth/config files. This
            // prevents a local override from silently testing a different adapter.
            let metadata = script
                .parent()
                .and_then(Path::parent)
                .map(|path| path.join("package.json"))
                .expect("ACP SCRIPT must be a package dist/index.js");
            let metadata: Value =
                serde_json::from_slice(&fs::read(metadata).expect("read adapter package metadata"))
                    .expect("parse adapter package metadata");
            assert!(
                metadata.get("name").and_then(Value::as_str)
                    == Some("@agentclientprotocol/codex-acp")
                    && metadata.get("version").and_then(Value::as_str) == Some("2.1.1"),
                "local adapter must be codex-acp 2.1.1"
            );
            let node = std::env::var("WEBCODEX_TEST_CODEX_ACP_NODE")
                .unwrap_or_else(|_| "node".to_string());
            (
                node,
                vec![script
                    .to_str()
                    .expect("ACP SCRIPT must be UTF-8")
                    .to_string()],
            )
        }
        None => (
            "npx".to_string(),
            vec!["-y".to_string(), CODEX_ACP_PACKAGE.to_string()],
        ),
    };
    let mut env_from_env = BTreeMap::new();
    // Test/operator-owned allowlist. Production still inherits nothing unless
    // explicitly mapped; do not forward CODEX_CONFIG, debug flags, or auth values.
    for name in [
        "HOME",
        "PATH",
        "CODEX_HOME",
        "USER",
        "SHELL",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ] {
        if std::env::var_os(name).is_some() {
            env_from_env.insert(name.to_string(), name.to_string());
        }
    }
    for name in ["HOME", "PATH"] {
        assert!(
            env_from_env.contains_key(name),
            "real ACP dogfood requires {name}"
        );
    }
    AcpConfig {
        max_concurrent_runs: 1,
        permission_timeout_secs: 3,
        forced_config: BTreeMap::new(),
        agents: vec![AcpAgentConfig {
            id: "codex".to_string(),
            name: "Codex ACP dogfood".to_string(),
            executable,
            args,
            env_from_env,
            allowed_config_options: Vec::new(),
        }],
    }
}

struct RealCodexFixture {
    manager: Arc<CodingAgentManager>,
    root: PathBuf,
    projects: PathBuf,
    _temp: TempDir,
}

impl RealCodexFixture {
    fn new(config: AcpConfig) -> Self {
        let temp = TempDir::new().expect("create dogfood directory");
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let mut git = Command::new("git");
        git.args([
            "-c",
            "init.templateDir=",
            "-c",
            "core.hooksPath=/dev/null",
            "init",
            "-q",
        ]);
        assert!(
            local_fixture_command(&mut git, &root, temp.path()),
            "initialize disposable Git project"
        );
        fs::write(
            root.join("clamp.py"),
            "def clamp(value, lower, upper):\n    raise NotImplementedError\n",
        )
        .expect("write function scaffold");
        fs::write(root.join("test_clamp.py"), FIXTURE_TESTS).expect("write fixed acceptance tests");
        let manager = CodingAgentManager::with_store(&config, temp.path().join("store"))
            .unwrap_or_else(|_| panic!("initialize real ACP dogfood manager"));
        Self {
            manager,
            root,
            projects,
            _temp: temp,
        }
    }

    fn start(&self, run: &str, instruction: &str) -> Arc<RunEntry> {
        let mut request =
            start_request_with_timeout(&self.manager, &self.root, run, BTreeMap::new(), 180);
        let CodingAgentRequest::Start(start) = &mut request else {
            unreachable!()
        };
        start.instruction = instruction.to_string();
        let admitted = self.manager.handle(request, &self.projects);
        assert!(
            admitted.error.is_none(),
            "real ACP dogfood admission failed"
        );
        self.manager
            .runs
            .lock()
            .unwrap()
            .get(run)
            .cloned()
            .expect("admitted dogfood Run")
    }
}

impl Drop for RealCodexFixture {
    fn drop(&mut self) {
        // Panic/timeout also cancels and joins owned provider workers before the
        // disposable project is removed. Never leave a paid prompt running.
        self.manager.stop_accepting();
        let drain = self
            .manager
            .drain_workers_until(Instant::now() + Duration::from_secs(10));
        if !thread::panicking() {
            assert_eq!(
                drain.timed_out, 0,
                "real ACP provider worker cleanup timed out"
            );
        }
    }
}

fn local_fixture_command(command: &mut Command, root: &Path, home: &Path) -> bool {
    command
        .current_dir(root)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").expect("fixture PATH"))
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child =
        ManagedChild::spawn(command).unwrap_or_else(|_| panic!("spawn disposable fixture command"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => panic!("disposable fixture command failed or exceeded deadline"),
        }
    }
}

#[derive(Default)]
struct RealProgress {
    message: bool,
    activity: bool,
    permission: bool,
}

fn wait_real_progress(
    entry: &RunEntry,
    progress: &mut RealProgress,
    active: bool,
    deadline: Instant,
) -> CodingAgentRunSnapshot {
    let mut cursor = Some(0);
    loop {
        assert!(
            Instant::now() < deadline,
            "real ACP dogfood observation deadline expired"
        );
        let observation = entry
            .observe(cursor, 64, 1)
            .unwrap_or_else(|_| panic!("observe real ACP Run"));
        assert!(
            !observation.history_lost,
            "real ACP acceptance event history was lost"
        );
        for event in &observation.events {
            cursor = Some(event.sequence);
            progress.message |= event.kind == CodingAgentEventKind::AgentMessage;
            progress.activity |= matches!(
                event.kind,
                CodingAgentEventKind::ToolActivity
                    | CodingAgentEventKind::FileChange
                    | CodingAgentEventKind::TerminalActivity
            );
            progress.permission |= event.kind == CodingAgentEventKind::PermissionRequest;
        }
        if (observation.run.state.terminal() && !observation.has_more)
            || (active && progress.message)
        {
            return observation.run;
        }
    }
}

fn assert_real_terminal(
    snapshot: &CodingAgentRunSnapshot,
    state: CodingAgentRunState,
    reason: &str,
) {
    // Debug output is deliberately limited to typed states. Provider events,
    // messages, private paths, auth state, and raw stderr never enter test failures.
    assert!(
        snapshot.state == state,
        "unexpected real ACP terminal state: {:?}/{:?}",
        snapshot.state,
        snapshot.execution_state
    );
    assert!(
        snapshot
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.stop_reason.as_deref())
            == Some(reason),
        "real ACP correlated stop reason did not match"
    );
}

#[test]
#[ignore = "opt-in paid real Codex ACP coding acceptance; requires local auth and network"]
fn real_codex_acp_opt_in_dogfood() {
    let fixture = RealCodexFixture::new(real_codex_config());
    let mut baseline = Command::new("python3");
    baseline.args([
        "-I",
        "-m",
        "unittest",
        "discover",
        "-s",
        ".",
        "-p",
        "test_clamp.py",
    ]);
    assert!(
        !local_fixture_command(&mut baseline, &fixture.root, fixture._temp.path()),
        "scaffold must initially fail acceptance tests"
    );
    let entry = fixture.start("wc_agent_run_realcodexdogfood01", "Implement clamp(value, lower, upper) in clamp.py: return value bounded inclusively to lower/upper, and raise ValueError if lower > upper. Read test_clamp.py and leave that file unchanged. Add a separate small unittest test file for one additional case, run the tests, and briefly report the result. Work only in this disposable Git project. Use Python's standard library; do not install dependencies, access network or credentials, or request elevated permissions.");
    let mut progress = RealProgress::default();
    let terminal = wait_real_progress(
        &entry,
        &mut progress,
        false,
        Instant::now() + Duration::from_secs(150),
    );
    assert!(
        !progress.permission,
        "real coding fixture required permission; approvals remain fail-closed"
    );
    assert_real_terminal(&terminal, CodingAgentRunState::Completed, "end_turn");
    assert!(
        progress.message && progress.activity,
        "real coding turn lacked normalized message/activity progress"
    );
    assert!(
        fs::read(fixture.root.join("test_clamp.py")).expect("read fixed tests")
            == FIXTURE_TESTS.as_bytes(),
        "agent changed fixed acceptance tests"
    );
    assert!(
        fs::read(fixture.root.join("clamp.py")).expect("read implementation")
            != b"def clamp(value, lower, upper):\n    raise NotImplementedError\n",
        "agent did not implement the function"
    );
    assert!(
        fs::read_dir(&fixture.root)
            .expect("list fixture files")
            .filter_map(Result::ok)
            .any(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with("test_") && name.ends_with(".py") && name != "test_clamp.py"
            }),
        "agent did not add the requested tests"
    );
    let mut validate = Command::new("python3");
    validate.args([
        "-I", "-m", "unittest", "discover", "-s", ".", "-p", "test*.py",
    ]);
    assert!(
        local_fixture_command(&mut validate, &fixture.root, fixture._temp.path()),
        "independent fixture unit tests failed"
    );
}

#[test]
#[ignore = "opt-in paid real Codex ACP cancellation; requires local auth and network"]
fn real_codex_acp_opt_in_cancel() {
    let fixture = RealCodexFixture::new(real_codex_config());
    let run = "wc_agent_run_realcodexcancel01";
    let entry = fixture.start(run, "First send a short progress message saying you are starting. Then enumerate the integers from 1 to 100000 in your answer, one per line. Do not use tools, modify files, access credentials, or request permissions. The client will cancel this turn after seeing the first message.");
    let mut progress = RealProgress::default();
    let deadline = Instant::now() + Duration::from_secs(150);
    let active = wait_real_progress(&entry, &mut progress, true, deadline);
    assert!(
        !active.state.terminal() && progress.message,
        "real cancellation prompt ended before observable active progress"
    );
    assert!(
        fixture
            .manager
            .handle(
                CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                    run_id: run.to_string()
                }),
                &fixture.projects
            )
            .error
            .is_none(),
        "real ACP cancel admission failed"
    );
    let terminal = wait_real_progress(
        &entry,
        &mut progress,
        false,
        deadline.min(Instant::now() + Duration::from_secs(10)),
    );
    assert_real_terminal(&terminal, CodingAgentRunState::Cancelled, "cancelled");
}

#[test]
#[ignore = "opt-in real codex-acp startup failure; requires pinned adapter, no model prompt"]
fn real_codex_acp_opt_in_failure_exit() {
    let mut config = real_codex_config();
    // The real adapter's CLI delegation rejects this unknown Codex option and
    // exits nonzero before ACP initialize. No login or paid prompt is dispatched.
    config.agents[0].args.extend([
        "cli".to_string(),
        "--webcodex-dogfood-invalid-option".to_string(),
    ]);
    let fixture = RealCodexFixture::new(config);
    let entry = fixture.start(
        "wc_agent_run_realcodexfailure01",
        "This prompt must never be dispatched.",
    );
    let mut progress = RealProgress::default();
    let terminal = wait_real_progress(
        &entry,
        &mut progress,
        false,
        Instant::now() + Duration::from_secs(15),
    );
    assert_eq!(terminal.state, CodingAgentRunState::Failed);
    assert_eq!(
        terminal.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    assert!(
        !progress.message && !progress.activity,
        "failed adapter unexpectedly dispatched a prompt"
    );
    let failure = terminal
        .terminal
        .as_ref()
        .expect("real adapter setup terminal");
    assert!(
        failure.stop_reason.is_none()
            && matches!(
                failure.error_code.as_deref(),
                Some("coding_agent_initialize_failed" | "coding_agent_initialize_write_failed")
            ),
        "real adapter exit lacked initialize failure evidence"
    );
    let message = failure.message.as_deref().unwrap_or_default();
    assert!(
        ["process exited", "stdout closed", "stdin write failed"]
            .iter()
            .any(|cause| message.contains(cause)),
        "real adapter failure did not identify process exit or closed stdio"
    );
}
