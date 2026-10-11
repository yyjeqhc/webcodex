use super::super::protocol::{wait_response, AcpFailure, ReaderEvent};
use super::*;

const PRIVATE_DIAGNOSTIC: &str = "synthetic-secret-/private/operator/auth-session";

#[test]
fn response_faults_keep_safe_distinct_causes() {
    let cases = [
        (ReaderEvent::Eof, AcpFailure::OutputClosed),
        (ReaderEvent::Malformed, AcpFailure::Malformed),
        (ReaderEvent::TooLarge, AcpFailure::TooLarge),
        (
            ReaderEvent::Io(std::io::ErrorKind::PermissionDenied),
            AcpFailure::ReadIo(std::io::ErrorKind::PermissionDenied),
        ),
        (
            ReaderEvent::Message(json!({"id": 1})),
            AcpFailure::MissingResult,
        ),
        (
            ReaderEvent::Message(
                json!({"id": 1, "error": {"code": -32001, "message": PRIVATE_DIAGNOSTIC, "data": PRIVATE_DIAGNOSTIC}}),
            ),
            AcpFailure::RpcRejected(Some(-32001)),
        ),
    ];
    let mut messages = std::collections::BTreeSet::new();
    for (event, expected) in cases {
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(event).unwrap();
        let error = wait_response(&rx, 1, Duration::from_secs(1), None).unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.safe_message().contains(PRIVATE_DIAGNOSTIC));
        assert!(
            messages.insert(error.safe_message()),
            "faults must remain distinguishable"
        );
    }
    let (tx, rx) = mpsc::sync_channel(1);
    assert_eq!(
        wait_response(&rx, 1, Duration::ZERO, None),
        Err(AcpFailure::TimedOut)
    );
    let cancelled = AtomicBool::new(true);
    assert_eq!(
        wait_response(&rx, 1, Duration::from_secs(1), Some(&cancelled)),
        Err(AcpFailure::Interrupted)
    );
    drop(tx);
    assert_eq!(
        wait_response(&rx, 1, Duration::from_secs(1), None),
        Err(AcpFailure::Disconnected)
    );
}

#[test]
fn writer_discards_raw_io_diagnostics() {
    struct SecretErrorWriter;
    impl Write for SecretErrorWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                PRIVATE_DIAGNOSTIC,
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let threads = BackgroundThreads::default();
    let writer = AcpOutboundWriter::spawn(SecretErrorWriter, &threads).unwrap();
    let pending = writer.start_frame(b"test\n".to_vec()).unwrap();
    let outcome = wait_outbound_write(pending, Instant::now() + Duration::from_secs(1), None, None);
    let OutboundWriteOutcome::Failed(error) = outcome else {
        panic!("writer failure was lost")
    };
    assert_eq!(
        error,
        AcpFailure::WriteIo(std::io::ErrorKind::PermissionDenied)
    );
    assert_eq!(error.safe_message(), "ACP stdin write failed");
    assert!(writer.wait_finished_until(Instant::now() + Duration::from_secs(1)));
}

#[test]
#[cfg(unix)]
fn process_exit_requires_pre_cleanup_exit_evidence() {
    use std::os::unix::process::ExitStatusExt;
    let status = std::process::ExitStatus::from_raw(23 << 8);
    assert_eq!(
        AcpFailure::OutputClosed.diagnose("run", "initialize", None),
        AcpFailure::OutputClosed
    );
    assert_eq!(
        AcpFailure::OutputClosed.diagnose("run", "initialize", Some(status)),
        AcpFailure::ProcessExited
    );
    assert_eq!(
        AcpFailure::Malformed.diagnose("run", "initialize", Some(status)),
        AcpFailure::Malformed
    );
    assert!(!AcpFailure::ProcessExited.safe_message().contains("23"));
}

#[cfg(unix)]
fn diagnostic_agent(temp: &TempDir, stage: &str, fault: &str) -> (String, Vec<String>) {
    let path = temp.path().join("diagnostic-acp.py");
    let script = r#"#!/usr/bin/env python3
import json, os, sys
stage, fault = sys.argv[1:3]
secret = 'synthetic-secret-/private/operator/auth-session'
options = [{'id':'mode','name':'Mode','type':'select','currentValue':'read-only','options':[{'value':'read-only','name':'Read only'}]}]
def send(value):
 print(json.dumps(value), flush=True)
for line in sys.stdin:
 request = json.loads(line)
 method, rid = request.get('method'), request.get('id')
 if method == 'session/prompt':
  open(os.path.join(os.path.dirname(__file__), 'prompt-dispatched'), 'w').close()
 if method == stage:
  if fault == 'block':
   open(os.path.join(os.path.dirname(__file__), 'setup-blocked'), 'w').close()
  elif fault == 'rpc':
   # More than pipe capacity: a missing stderr drain would deadlock this response.
   sys.stderr.write((secret + '\n') * 4096); sys.stderr.flush()
   send({'jsonrpc':'2.0','id':rid,'error':{'code':-32001,'message':secret,'data':{'sessionId':secret,'path':secret}}})
  elif fault == 'malformed':
   print(secret, flush=True)
  elif fault == 'oversized':
   print('x' * (1024 * 1024 + 1), flush=True)
  elif fault == 'read_io':
   os.write(1, b'\xff\n')
  elif fault == 'eof':
   os.close(1)
  sys.stdin.read()
  break
 elif method == 'initialize': send({'jsonrpc':'2.0','id':rid,'result':{'protocolVersion':1}})
 elif method == 'session/new': send({'jsonrpc':'2.0','id':rid,'result':{'sessionId':'private-session','configOptions':options}})
 elif method == 'session/set_config_option': send({'jsonrpc':'2.0','id':rid,'result':{'configOptions':options}})
 elif method == 'session/prompt': send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'end_turn'}})
"#;
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    (
        path.to_string_lossy().into_owned(),
        vec![stage.into(), fault.into()],
    )
}

#[test]
#[cfg(unix)]
fn setup_stages_preserve_fault_class_and_never_dispatch_prompt() {
    for (stage, code) in [
        ("initialize", "coding_agent_initialize_failed"),
        ("session/new", "coding_agent_session_new_failed"),
        ("session/set_config_option", "coding_agent_config_failed"),
    ] {
        for (fault, expected) in [
            ("malformed", "ACP returned malformed JSON"),
            ("oversized", "ACP message exceeded the size limit"),
            ("read_io", "ACP stdout read failed"),
            ("eof", "ACP stdout closed before responding; check the executable and explicit Runner environment mappings"),
        ] {
            let temp = crate::tests::executable_tempdir();
            let (exe,args) = diagnostic_agent(&temp, stage, fault);
            let manager = CodingAgentManager::with_store(&fake_config(exe,args), temp.path().join("store")).unwrap();
            let projects = project_fixture(&temp);
            let run = "wc_agent_run_diagnosticstage01";
            let config = BTreeMap::from([("mode".into(), CodingAgentConfigValue::String("read-only".into()))]);
            assert!(manager.handle(start_request(&manager, &temp.path().join("repo"), run, config), &projects).error.is_none());
            let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
            assert_eq!(snapshot.state, CodingAgentRunState::Failed, "{stage}/{fault}");
            assert_eq!(snapshot.execution_state, CodingAgentExecutionState::NotStarted);
            let terminal = snapshot.terminal.unwrap();
            assert_eq!(terminal.error_code.as_deref(), Some(code));
            assert_eq!(terminal.message.as_deref(), Some(expected));
            assert!(!temp.path().join("prompt-dispatched").exists());
            assert_eq!(manager.drain_workers_until(Instant::now()+Duration::from_secs(3)).timed_out, 0);
        }
    }
}

#[test]
#[cfg(unix)]
fn rpc_and_stderr_secrets_never_reach_observations_inventory_or_durable_state() {
    for (stage, code) in [
        ("initialize", "coding_agent_initialize_failed"),
        ("session/new", "coding_agent_session_new_failed"),
        ("session/set_config_option", "coding_agent_config_failed"),
        ("session/prompt", "prompt_error"),
    ] {
        let temp = crate::tests::executable_tempdir();
        let (exe, args) = diagnostic_agent(&temp, stage, "rpc");
        let manager =
            CodingAgentManager::with_store(&fake_config(exe, args), temp.path().join("store"))
                .unwrap();
        let projects = project_fixture(&temp);
        let run = "wc_agent_run_diagnosticprivacy01";
        let config = BTreeMap::from([(
            "mode".into(),
            CodingAgentConfigValue::String("read-only".into()),
        )]);
        assert!(manager
            .handle(
                start_request(&manager, &temp.path().join("repo"), run, config),
                &projects
            )
            .error
            .is_none());
        let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        assert_eq!(
            snapshot.terminal.as_ref().unwrap().error_code.as_deref(),
            Some(code)
        );
        assert_eq!(
            snapshot.terminal.as_ref().unwrap().message.as_deref(),
            Some("ACP provider rejected the request")
        );
        let entry = manager.runs.lock().unwrap().get(run).unwrap().clone();
        let observation = entry.observe(None, 64, 0).unwrap();
        let response =
            CodingAgentResponse::success(CodingAgentResponsePayload::Observe { observation });
        let inventory = manager.inventory();
        let durable = fs::read_to_string(manager.store.state_path(run)).unwrap();
        for output in [
            serde_json::to_string(&response).unwrap(),
            serde_json::to_string(&inventory).unwrap(),
            durable,
        ] {
            assert!(
                !output.contains(PRIVATE_DIAGNOSTIC),
                "{stage} leaked private diagnostic"
            );
            assert!(
                !output.contains("-32001"),
                "numeric RPC detail belongs only in Runner diagnostics"
            );
        }
        assert_eq!(
            manager
                .drain_workers_until(Instant::now() + Duration::from_secs(3))
                .timed_out,
            0
        );
        assert_eq!(
            temp.path().join("prompt-dispatched").exists(),
            stage == "session/prompt"
        );
    }
}

#[test]
#[cfg(unix)]
fn explicit_path_mapping_restores_an_interpreter_outside_default_search_paths() {
    let outer = crate::tests::executable_tempdir();
    let runtime = outer.path().join("runtime");
    fs::create_dir(&runtime).unwrap();
    crate::tests::IsolatedEnv::new()
        .set("ACP_TEST_RUNTIME_PATH", runtime.to_str().unwrap())
        .run("inherited", || {
            let temp = crate::tests::executable_tempdir();
            let runtime = PathBuf::from(std::env::var_os("ACP_TEST_RUNTIME_PATH").unwrap());
            let (exe, args) = fake_agent(&temp, "end");
            // A unique interpreter name prevents system Node from masking this
            // /usr/bin/env lookup regression on CI hosts.
            let interpreter = format!("wc-acp-node-{}", Uuid::new_v4().simple());
            let python = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|directory| directory.join("python3"))
                .find(|path| path.is_file())
                .expect("existing ACP fixtures require Python");
            std::os::unix::fs::symlink(python, runtime.join(&interpreter)).unwrap();
            let script = fs::read_to_string(&exe).unwrap().replacen(
                "#!/usr/bin/env python3",
                &format!("#!/usr/bin/env {interpreter}"),
                1,
            );
            fs::write(&exe, script).unwrap();
            let projects = project_fixture(&temp);
            let root = temp.path().join("repo");
            let mut cfg = fake_config(exe, args);
            let manager =
                CodingAgentManager::with_store(&cfg, temp.path().join("unmapped-store")).unwrap();
            let run = "wc_agent_run_unmappedruntime01";
            assert!(manager
                .handle(
                    start_request(&manager, &root, run, BTreeMap::new()),
                    &projects
                )
                .error
                .is_none());
            let failed = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
            assert_eq!(failed.state, CodingAgentRunState::Failed);
            assert_eq!(
                failed.execution_state,
                CodingAgentExecutionState::NotStarted
            );
            assert!(wire_log(&temp).is_empty());
            assert_eq!(
                manager
                    .drain_workers_until(Instant::now() + Duration::from_secs(3))
                    .timed_out,
                0
            );
            cfg.agents[0].env_from_env =
                BTreeMap::from([("PATH".into(), "ACP_TEST_RUNTIME_PATH".into())]);
            let manager =
                CodingAgentManager::with_store(&cfg, temp.path().join("mapped-store")).unwrap();
            let run = "wc_agent_run_mappedruntime01";
            assert!(manager
                .handle(
                    start_request(&manager, &root, run, BTreeMap::new()),
                    &projects
                )
                .error
                .is_none());
            let completed = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
            assert_eq!(completed.state, CodingAgentRunState::Completed);
            assert_eq!(prompt_count(&temp), 1);
            assert_eq!(
                manager
                    .drain_workers_until(Instant::now() + Duration::from_secs(3))
                    .timed_out,
                0
            );
        });
}

#[test]
#[cfg(unix)]
fn each_setup_stage_cancels_and_reports_its_deadline_without_prompt_dispatch() {
    for stage in ["initialize", "session/new", "session/set_config_option"] {
        for cancel in [true, false] {
            let temp = crate::tests::executable_tempdir();
            let (exe, args) = diagnostic_agent(&temp, stage, "block");
            let manager =
                CodingAgentManager::with_store(&fake_config(exe, args), temp.path().join("store"))
                    .unwrap();
            let projects = project_fixture(&temp);
            let run = "wc_agent_run_setupinterrupt01";
            let config = BTreeMap::from([(
                "mode".into(),
                CodingAgentConfigValue::String("read-only".into()),
            )]);
            let budget = if cancel { 10 } else { 1 };
            assert!(manager
                .handle(
                    start_request_with_timeout(
                        &manager,
                        &temp.path().join("repo"),
                        run,
                        config,
                        budget
                    ),
                    &projects
                )
                .error
                .is_none());
            let deadline = Instant::now() + Duration::from_secs(3);
            while !temp.path().join("setup-blocked").exists() {
                assert!(
                    Instant::now() < deadline,
                    "provider did not reach setup stage"
                );
                thread::sleep(Duration::from_millis(10));
            }
            if cancel {
                assert!(manager
                    .handle(
                        CodingAgentRequest::Cancel(CodingAgentCancelRequest { run_id: run.into() }),
                        &projects
                    )
                    .error
                    .is_none());
            }
            let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
            assert_eq!(
                terminal.execution_state,
                CodingAgentExecutionState::NotStarted
            );
            if cancel {
                assert_eq!(terminal.state, CodingAgentRunState::Cancelled);
                assert_eq!(terminal.terminal.unwrap().error_code, None);
            } else {
                assert_eq!(terminal.state, CodingAgentRunState::Failed);
                let error = terminal.terminal.unwrap();
                assert_eq!(
                    error.error_code.as_deref(),
                    Some("coding_agent_setup_timeout")
                );
                assert!(error
                    .message
                    .unwrap()
                    .contains(&format!("during ACP {stage} before prompt dispatch")));
            }
            assert!(!temp.path().join("prompt-dispatched").exists());
            assert_eq!(
                manager
                    .drain_workers_until(Instant::now() + Duration::from_secs(3))
                    .timed_out,
                0
            );
        }
    }
}

#[test]
#[cfg(unix)]
fn natural_process_exit_is_observed_before_cleanup() {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "exit 23"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = ManagedChild::spawn(&mut command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(23));
    let failure = AcpFailure::OutputClosed.diagnose("run", "initialize", Some(status));
    assert_eq!(failure, AcpFailure::ProcessExited);
    assert!(!failure.safe_message().contains("23"));
}
