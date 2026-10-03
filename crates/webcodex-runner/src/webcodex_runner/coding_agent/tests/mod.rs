use super::protocol::{
    bounded_json_summary, bounded_text, wait_outbound_write, AcpOutboundWriter,
    OutboundInterruption, OutboundWriteOutcome,
};
#[cfg(unix)]
use super::protocol::{notification_frame, request_frame};
#[cfg(unix)]
use super::store::TerminalWriteGate;
use super::store::{DurableDispatchPhase, DurableRunRecord, DurableRunStore, STORE_SCHEMA_VERSION};
use super::*;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

#[derive(Default)]
struct BlockingWriteState {
    entered: bool,
    released: bool,
}

#[derive(Clone)]
struct BlockingWrite {
    state: Arc<(Mutex<BlockingWriteState>, Condvar)>,
}

impl Write for BlockingWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let (state, changed) = &*self.state;
        let mut state = state.lock().unwrap();
        state.entered = true;
        changed.notify_all();
        while !state.released {
            state = changed.wait(state).unwrap();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn wait_for_blocking_write(state: &Arc<(Mutex<BlockingWriteState>, Condvar)>) {
    let (state, changed) = &**state;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut state = state.lock().unwrap();
    while !state.entered {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "writer never entered blocking sink");
        let (next, _) = changed.wait_timeout(state, remaining).unwrap();
        state = next;
    }
}

fn release_blocking_write(state: &Arc<(Mutex<BlockingWriteState>, Condvar)>) {
    let (state, changed) = &**state;
    let mut state = state.lock().unwrap();
    state.released = true;
    changed.notify_all();
}

fn fake_config(executable: String, args: Vec<String>) -> AcpConfig {
    AcpConfig {
        max_concurrent_runs: 1,
        permission_timeout_secs: 1,
        forced_config: BTreeMap::new(),
        agents: vec![AcpAgentConfig {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            executable,
            args,
            env_from_env: BTreeMap::new(),
            allowed_config_options: vec!["mode".to_string()],
        }],
    }
}

#[cfg(unix)]
fn fake_agent(temp: &TempDir, scenario: &str) -> (String, Vec<String>) {
    let path = temp.path().join("fake-acp.py");
    let script = r#"#!/usr/bin/env python3
import json,os,sys,time,subprocess
scenario=sys.argv[1]
config_values={'one':'a','two':'a','three':'a','four':'a','mode':'agent','model':'default-model','reasoning_effort':'medium','feature_flag':False}
log_path=os.path.join(os.path.dirname(__file__),'fake-acp.log')
def log(x):
 with open(log_path,'a',encoding='utf-8') as f: f.write(json.dumps(x,separators=(',',':'))+'\n')
def send(x):
 log({'send':x}); print(json.dumps(x),flush=True)
log({'startup_pid':os.getpid(),'env_keys':sorted(k for k in os.environ if k.startswith('WEBCODEX_TEST_ACP_') or k=='ACP_VISIBLE')})
for line in sys.stdin:
 m=json.loads(line); log({'recv':m}); method=m.get('method'); rid=m.get('id')
 if method=='initialize':
  if scenario=='crash_before_prompt': sys.exit(9)
  if scenario in ('block_initialize','block_initialize_tree'):
   if scenario=='block_initialize_tree':
    child=subprocess.Popen(['/bin/sh','-c','sleep 60']); log({'descendant_pid':child.pid})
   ready=os.path.join(os.path.dirname(__file__),'initialize.ready')
   release=os.path.join(os.path.dirname(__file__),'initialize.release')
   open(ready,'w').close()
   while not os.path.exists(release): time.sleep(0.01)
  send({'jsonrpc':'2.0','id':rid,'result':{'protocolVersion':1,'agentCapabilities':{}}})
 elif method=='session/new':
  if scenario=='slow_configs':
   opts=[{'id':k,'name':k.title(),'type':'select','currentValue':config_values[k],'options':[{'value':'a','name':'A'},{'value':'b','name':'B'}]} for k in ('one','two','three','four')]
  elif scenario in ('forced_configs','forced_not_applied','forced_reset_by_caller','forced_duplicate'):
   opts=[
    {'id':'mode','name':'Mode','type':'select','currentValue':config_values['mode'],'options':[{'value':'agent','name':'Agent'},{'value':'read-only','name':'Read Only'}]},
    {'id':'model','name':'Model','type':'select','currentValue':config_values['model'],'options':[{'value':'default-model','name':'Default'},{'value':'policy-model','name':'Policy'}]},
    {'id':'reasoning_effort','name':'Reasoning Effort','type':'select','currentValue':config_values['reasoning_effort'],'options':[{'value':'medium','name':'Medium'},{'value':'high','name':'High'}]},
    {'id':'feature_flag','name':'Feature Flag','type':'boolean','currentValue':config_values['feature_flag']}
   ]
   if scenario=='forced_duplicate':
    opts.append({'id':'feature_flag','name':'Duplicate Feature Flag','type':'boolean','currentValue':False})
  else:
   opts=[{'id':'mode','name':'Mode','type':'select','currentValue':'agent','options':[{'value':'agent','name':'Agent'},{'value':'read-only','name':'Read Only'}]}]
  session_id='s'*70000 if scenario=='block_cancel_write' else 's1'
  send({'jsonrpc':'2.0','id':rid,'result':{'sessionId':session_id,'configOptions':opts}})
  if scenario in ('block_after_session_new','block_after_session_new_tree'):
   if scenario=='block_after_session_new_tree':
    child=subprocess.Popen(['/bin/sh','-c','sleep 60']); log({'descendant_pid':child.pid})
   open(os.path.join(os.path.dirname(__file__),'stdin_stopped.ready'),'w').close()
   while True: time.sleep(1)
 elif method=='session/set_config_option':
  if scenario=='slow_configs':
   # Three replies cross the 2s setup budget. At 0.6s a fourth RPC
   # could be sent at 1.8s and finish during I/O cleanup: remote
   # completion after a caller timeout does not imply extra admission.
   time.sleep(0.75)
   k=m['params']['configId']; v=m['params']['value']; config_values[k]=v
   opts=[{'id':key,'name':key.title(),'type':'select','currentValue':config_values[key],'options':[{'value':'a','name':'A'},{'value':'b','name':'B'}]} for key in ('one','two','three','four')]
  elif scenario in ('forced_configs','forced_not_applied','forced_reset_by_caller'):
   k=m['params']['configId']; v=m['params']['value']
   if not (scenario=='forced_not_applied' and k=='model'):
    config_values[k]=v
   if scenario=='forced_reset_by_caller' and k=='mode':
    config_values['model']='default-model'; config_values['reasoning_effort']='medium'
   opts=[
    {'id':'mode','name':'Mode','type':'select','currentValue':config_values['mode'],'options':[{'value':'agent','name':'Agent'},{'value':'read-only','name':'Read Only'}]},
    {'id':'model','name':'Model','type':'select','currentValue':config_values['model'],'options':[{'value':'default-model','name':'Default'},{'value':'policy-model','name':'Policy'}]},
    {'id':'reasoning_effort','name':'Reasoning Effort','type':'select','currentValue':config_values['reasoning_effort'],'options':[{'value':'medium','name':'Medium'},{'value':'high','name':'High'}]},
    {'id':'feature_flag','name':'Feature Flag','type':'boolean','currentValue':config_values['feature_flag']}
   ]
  else:
   v=m['params']['value']; opts=[{'id':'mode','name':'Mode','type':'select','currentValue':v,'options':[{'value':'agent','name':'Agent'},{'value':'read-only','name':'Read Only'}]}]
  send({'jsonrpc':'2.0','id':rid,'result':{'configOptions':opts}})
  if scenario=='slow_configs': log({'config_applied':k})
 elif method=='session/prompt':
  if scenario=='crash_after_prompt': sys.exit(7)
  if scenario=='spawn_descendant':
   child=subprocess.Popen(['/bin/sh','-c','sleep 60']); log({'descendant_pid':child.pid})
  if scenario=='block_cancel_write':
   child=subprocess.Popen(['/bin/sh','-c','sleep 60']); log({'descendant_pid':child.pid})
   open(os.path.join(os.path.dirname(__file__),'prompt_read.ready'),'w').close()
   while True: time.sleep(1)
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'hello'}}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'agent_thought_chunk','content':{'type':'text','text':'thinking'}}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'plan','entries':[]}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'tool_call','toolCallId':'t1','title':'inspect','kind':'execute','status':'in_progress'}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'tool_call','toolCallId':'t2','title':'edit','kind':'edit','status':'in_progress'}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'usage_update','used':53,'size':200,'cost':{'amount':0.045,'currency':'USD'}}}})
  if scenario=='many_events':
   for i in range(300): send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'m'+str(i)}}}})
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'end_turn'}})
  elif scenario=='permission':
   send({'jsonrpc':'2.0','id':99,'method':'session/request_permission','params':{'sessionId':'s1','toolCall':{'toolCallId':'t1','title':'permission','status':'pending'},'options':[{'optionId':'allow','name':'Allow','kind':'allow_once'}]}})
   response=json.loads(sys.stdin.readline()); log({'recv':response})
   assert response['result']['outcome']['outcome']=='cancelled'
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'cancelled'}})
  elif scenario=='permission_hold':
   send({'jsonrpc':'2.0','id':99,'method':'session/request_permission','params':{'sessionId':'s1','toolCall':{'toolCallId':'t1','title':'permission','status':'pending'},'options':[{'optionId':'allow','name':'Allow','kind':'allow_once'}]}})
   response=json.loads(sys.stdin.readline()); log({'recv':response})
   assert response['result']['outcome']['outcome']=='cancelled'
   cancel=json.loads(sys.stdin.readline()); log({'recv':cancel}); assert cancel.get('method')=='session/cancel'
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'cancelled'}})
  elif scenario=='unsupported_callback':
   send({'jsonrpc':'2.0','id':98,'method':'fs/read_text_file','params':{'path':'private'}})
   response=json.loads(sys.stdin.readline()); log({'recv':response}); assert response['error']['code']==-32601
   cancel=json.loads(sys.stdin.readline()); log({'recv':cancel}); assert cancel.get('method')=='session/cancel'
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'cancelled'}})
  elif scenario=='wait_cancel':
   while True:
    x=json.loads(sys.stdin.readline()); log({'recv':x})
    if x.get('method')=='session/cancel': send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'cancelled'}}); break
  else:
   stop={'end':'end_turn','spawn_descendant':'end_turn','cancelled':'cancelled','max_tokens':'max_tokens','max_turn_requests':'max_turn_requests','refusal':'refusal','unknown':'future_reason'}.get(scenario,'end_turn')
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':stop}})
"#;
    fs::write(&path, script).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    (
        path.to_string_lossy().to_string(),
        vec![scenario.to_string()],
    )
}

#[cfg(unix)]
fn project_fixture(temp: &TempDir) -> PathBuf {
    let root = temp.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    let projects = temp.path().join("projects");
    fs::create_dir_all(&projects).unwrap();
    fs::write(
        projects.join("p.toml"),
        format!("id = \"demo\"\npath = {:?}\n", root.to_string_lossy()),
    )
    .unwrap();
    projects
}

#[cfg(unix)]
fn start_request(
    manager: &CodingAgentManager,
    root: &Path,
    run: &str,
    config: BTreeMap<String, CodingAgentConfigValue>,
) -> CodingAgentRequest {
    let provider = manager.providers().remove(0);
    CodingAgentRequest::Start(webcodex_core::coding_agent::CodingAgentStartRequest {
        run_id: run.to_string(),
        intent_fingerprint: "fingerprint".to_string(),
        authority_fingerprint: "auth_test".to_string(),
        runtime_project_id: "agent:test:demo".to_string(),
        project_root: root.to_string_lossy().to_string(),
        provider_id: "codex".to_string(),
        provider_instance_id: provider.provider_instance_id,
        instruction: "inspect".to_string(),
        config,
        timeout_secs: 10,
    })
}

#[cfg(unix)]
fn start_request_with_timeout(
    manager: &CodingAgentManager,
    root: &Path,
    run: &str,
    config: BTreeMap<String, CodingAgentConfigValue>,
    timeout_secs: u64,
) -> CodingAgentRequest {
    let mut request = start_request(manager, root, run, config);
    let CodingAgentRequest::Start(start) = &mut request else {
        unreachable!();
    };
    start.timeout_secs = timeout_secs;
    request
}

#[cfg(unix)]
fn max_instruction_request(
    manager: &CodingAgentManager,
    root: &Path,
    run: &str,
    timeout_secs: u64,
) -> CodingAgentRequest {
    let mut request = start_request_with_timeout(manager, root, run, BTreeMap::new(), timeout_secs);
    let CodingAgentRequest::Start(start) = &mut request else {
        unreachable!();
    };
    start.instruction = "x".repeat(webcodex_core::coding_agent::CODING_AGENT_MAX_INSTRUCTION_BYTES);
    let frame = request_frame(
        3,
        "session/prompt",
        json!({
            "sessionId":"s1",
            "prompt":[{"type":"text","text":start.instruction.clone()}]
        }),
    )
    .unwrap();
    assert!(
        frame.len() > 64 * 1024,
        "max legal prompt frame must exceed the measured special Linux pipe capacity"
    );
    request
}

#[cfg(unix)]
fn wait_for_prompt_handoff(manager: &CodingAgentManager, run: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
        if *entry.prompt_dispatch.lock().unwrap()
            == PromptDispatchGateState::PromptDispatchMayHaveOccurred
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "prompt was never handed to writer"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
fn wait_for_terminal_observation(
    manager: &CodingAgentManager,
    run: &str,
) -> CodingAgentObserveResult {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
        let observation = entry
            .observe(None, 64, 0)
            .expect("retained fake Run cursor must be valid");
        if observation.run.state.terminal()
            && observation
                .events
                .iter()
                .any(|event| event.kind == CodingAgentEventKind::Terminal)
        {
            return observation;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for terminal ACP observation: {observation:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn received_config_ids(log: &[Value]) -> Vec<String> {
    log.iter()
        .filter_map(|entry| {
            let recv = entry.get("recv")?;
            if recv.get("method").and_then(Value::as_str) != Some("session/set_config_option") {
                return None;
            }
            recv.pointer("/params/configId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

#[cfg(unix)]
fn force_policy(cfg: &mut AcpConfig) {
    cfg.forced_config = BTreeMap::from([
        (
            "model".to_string(),
            CodingAgentConfigValue::String("policy-model".to_string()),
        ),
        (
            "reasoning_effort".to_string(),
            CodingAgentConfigValue::String("high".to_string()),
        ),
    ]);
}

#[cfg(unix)]
fn start_request_for_provider(
    manager: &CodingAgentManager,
    root: &Path,
    run: &str,
    provider_id: &str,
    config: BTreeMap<String, CodingAgentConfigValue>,
) -> CodingAgentRequest {
    let provider = manager
        .providers()
        .into_iter()
        .find(|provider| provider.provider_id == provider_id)
        .unwrap();
    CodingAgentRequest::Start(webcodex_core::coding_agent::CodingAgentStartRequest {
        run_id: run.to_string(),
        intent_fingerprint: format!("fingerprint-{provider_id}"),
        authority_fingerprint: "auth_test".to_string(),
        runtime_project_id: "agent:test:demo".to_string(),
        project_root: root.to_string_lossy().to_string(),
        provider_id: provider_id.to_string(),
        provider_instance_id: provider.provider_instance_id,
        instruction: "inspect".to_string(),
        config,
        timeout_secs: 10,
    })
}

#[cfg(unix)]
fn run_scenario(
    scenario: &str,
    config: BTreeMap<String, CodingAgentConfigValue>,
) -> (Arc<CodingAgentManager>, String, CodingAgentObserveResult) {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, scenario);
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let store = temp.path().join("store");
    let manager = CodingAgentManager::with_store(&cfg, store).unwrap();
    let run = "wc_agent_run_0123456789abcdef".to_string();
    let response = manager.handle(start_request(&manager, &root, &run, config), &projects);
    assert!(response.error.is_none(), "{:?}", response.error);
    let observation = wait_for_terminal_observation(&manager, &run);
    std::mem::forget(temp);
    (manager, run, observation)
}

#[cfg(unix)]
fn wait_for_snapshot_until(
    manager: &CodingAgentManager,
    run: &str,
    deadline: Instant,
    predicate: impl Fn(&CodingAgentRunSnapshot) -> bool,
) -> CodingAgentRunSnapshot {
    loop {
        let snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for CodingAgentRun state: {snapshot:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn wait_for_snapshot(
    manager: &CodingAgentManager,
    run: &str,
    predicate: impl Fn(&CodingAgentRunSnapshot) -> bool,
) -> CodingAgentRunSnapshot {
    wait_for_snapshot_until(
        manager,
        run,
        Instant::now() + Duration::from_secs(5),
        predicate,
    )
}

#[cfg(unix)]
fn wire_log(temp: &TempDir) -> Vec<Value> {
    let path = temp.path().join("fake-acp.log");
    if !path.exists() {
        return Vec::new();
    }
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[cfg(unix)]
fn received_methods(log: &[Value]) -> Vec<String> {
    log.iter()
        .filter_map(|entry| entry.pointer("/recv/method").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

#[cfg(unix)]
fn wait_for_received_method_count(temp: &TempDir, method: &str, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let count = received_methods(&wire_log(temp))
            .iter()
            .filter(|candidate| candidate.as_str() == method)
            .count();
        if count >= expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected} {method} calls; observed {count}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn wait_for_path(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
fn wait_for_proc_exit(pid: u64) {
    let proc_path = PathBuf::from(format!("/proc/{pid}"));
    let deadline = Instant::now() + Duration::from_secs(3);
    while proc_path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !proc_path.exists(),
        "process {pid} survived CodingAgent worker drain"
    );
}

fn successful_start_run_id(response: &CodingAgentResponse) -> Option<String> {
    match response.payload.as_ref() {
        Some(CodingAgentResponsePayload::Start { run }) => Some(run.run_id.clone()),
        _ => None,
    }
}

fn seed_terminal_test_run(
    manager: &CodingAgentManager,
    run: &str,
    phase: DurableDispatchPhase,
    state: CodingAgentRunState,
    execution_state: CodingAgentExecutionState,
) -> Arc<RunEntry> {
    let provider = manager.providers().remove(0);
    let timestamp = now();
    let record = DurableRunRecord {
        schema_version: STORE_SCHEMA_VERSION,
        run_id: run.to_string(),
        intent_fingerprint: "fingerprint".to_string(),
        authority_fingerprint: "auth_test".to_string(),
        runtime_project_id: "agent:test:demo".to_string(),
        provider_id: "codex".to_string(),
        provider_instance_id: provider.provider_instance_id,
        state,
        execution_state,
        dispatch_phase: phase,
        created_at: timestamp,
        updated_at: timestamp,
        terminal: None,
    };
    manager.store.write(&record).unwrap();
    let entry = Arc::new(RunEntry::new(record.snapshot(0)));
    manager
        .runs
        .lock()
        .unwrap()
        .insert(run.to_string(), Arc::clone(&entry));
    entry
}

fn assert_terminal_persistence_uncertain(
    entry: &RunEntry,
    expected_state: CodingAgentRunState,
    expected_execution_state: CodingAgentExecutionState,
) {
    let snapshot = entry.snapshot();
    assert_eq!(snapshot.state, expected_state);
    assert_eq!(snapshot.execution_state, expected_execution_state);
    let terminal = snapshot.terminal.as_ref().expect("uncertain terminal");
    assert_eq!(terminal.stop_reason, None);
    assert_eq!(
        terminal.error_code.as_deref(),
        Some(CODING_AGENT_TERMINAL_PERSISTENCE_UNCERTAIN)
    );
    assert_eq!(
        terminal.message.as_deref(),
        Some(TERMINAL_PERSISTENCE_UNCERTAIN_MESSAGE)
    );
    let observation = entry.observe(None, 64, 0).unwrap();
    let terminal_events = observation
        .events
        .iter()
        .filter(|event| event.kind == CodingAgentEventKind::Terminal)
        .collect::<Vec<_>>();
    assert_eq!(terminal_events.len(), 1);
    assert_eq!(
        terminal_events[0].label.as_deref(),
        Some(CODING_AGENT_TERMINAL_PERSISTENCE_UNCERTAIN)
    );
    let expected_status = if expected_state == CodingAgentRunState::Failed {
        "failed"
    } else {
        "lost"
    };
    assert_eq!(terminal_events[0].status.as_deref(), Some(expected_status));
}

#[cfg(unix)]
fn prompt_count(temp: &TempDir) -> usize {
    received_methods(&wire_log(temp))
        .iter()
        .filter(|method| method.as_str() == "session/prompt")
        .count()
}
mod admission;
#[cfg(unix)]
mod dogfood;
#[cfg(unix)]
mod lifecycle;
#[cfg(unix)]
mod model_gateway;
mod protocol;
mod recovery;
mod terminal;
