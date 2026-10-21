// Portions of the V8 integration pattern are adapted from OpenAI Codex's
// Apache-2.0 licensed codex-rs/code-mode implementation. WebCodex intentionally
// keeps only the one-shot runtime/thread, Promise callback, value conversion,
// microtask checkpoint, and isolate-termination ideas needed for E1.

use crate::{
    normalized_max_concurrent_executions, normalized_timeout_ms, CodeModeChildFailure,
    CodeModeError, CodeModeErrorKind, CodeModeExecuteRequest, CodeModeExecution, CodeModeHost,
    CodeModeLimit, CodeModeStats, CodeModeTerminationMode, CodeModeToolRequest,
    CodeModeToolResponse, MAX_CONCURRENT_EXECUTIONS_ENV, MAX_CONCURRENT_TOOL_CALLS,
    MAX_OUTPUT_BYTES, MAX_OUTPUT_ITEMS, MAX_SOURCE_BYTES, MAX_TOOL_CALLS,
};
use serde_json::{json, Value as JsonValue};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};

struct V8Initialization {
    _platform: v8::SharedRef<v8::Platform>,
}

static V8_INITIALIZATION: OnceLock<Result<V8Initialization, String>> = OnceLock::new();
static EXECUTION_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn execution_slots() -> Arc<Semaphore> {
    // Process configuration is sampled once, before the first V8 cell acquires a
    // slot. Later environment mutation cannot silently resize a live semaphore.
    Arc::clone(EXECUTION_SLOTS.get_or_init(|| {
        let configured = std::env::var(MAX_CONCURRENT_EXECUTIONS_ENV).ok();
        Arc::new(Semaphore::new(normalized_max_concurrent_executions(
            configured.as_deref(),
        )))
    }))
}

fn ensure_v8_initialized() -> Result<(), String> {
    match V8_INITIALIZATION.get_or_init(|| {
        v8::icu::set_common_data_77(deno_core_icudata::ICU_DATA)
            .map_err(|error_code| format!("failed to initialize ICU data: {error_code}"))?;
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform.clone());
        v8::V8::initialize();
        Ok(V8Initialization {
            _platform: platform,
        })
    }) {
        Ok(_) => Ok(()),
        Err(error) => Err(error.clone()),
    }
}

#[derive(Debug)]
enum RuntimeCommand {
    ToolResponse {
        id: String,
        result: JsonValue,
    },
    ToolError {
        id: String,
        failure: CodeModeChildFailure,
    },
    Terminate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeFailureKind {
    Runtime,
    ChildCallFailed,
    ToolCallBudgetExceeded,
    OutputLimitExceeded,
}

#[derive(Debug, Clone)]
struct RuntimeFailure {
    kind: RuntimeFailureKind,
    message: String,
    child_failure: Option<CodeModeChildFailure>,
    limit: Option<CodeModeLimit>,
}

#[derive(Debug)]
enum RuntimeEvent {
    ToolCall {
        id: String,
        ordinal: usize,
        tool_name: String,
        arguments: JsonValue,
    },
    Text(String),
    Finished(Option<RuntimeFailure>),
}

struct SpawnedRuntime {
    command_tx: std_mpsc::Sender<RuntimeCommand>,
    event_rx: mpsc::UnboundedReceiver<RuntimeEvent>,
    isolate_handle: v8::IsolateHandle,
    join: Option<RuntimeJoin>,
}

impl SpawnedRuntime {
    fn release_execution_slot(&mut self) {
        if let Some(join) = self.join.as_mut() {
            drop(join.execution_slot.take());
        }
    }

    fn take_join(&mut self) -> RuntimeJoin {
        self.join.take().expect("runtime owns join handle")
    }
}

impl Drop for SpawnedRuntime {
    fn drop(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        let _ = self.isolate_handle.terminate_execution();
        let _ = self.command_tx.send(RuntimeCommand::Terminate);
        reap_runtime(join);
    }
}

struct RuntimeJoin {
    join: thread::JoinHandle<()>,
    execution_slot: Option<OwnedSemaphorePermit>,
    #[cfg(test)]
    reaped: Option<Arc<tokio::sync::Notify>>,
}

impl RuntimeJoin {
    fn join(mut self) {
        let _ = self.join.join();
        // Cancellation cleanup keeps active-cell capacity owned until the OS
        // runtime thread has actually stopped. Notify tests only after release.
        drop(self.execution_slot.take());
        #[cfg(test)]
        if let Some(reaped) = self.reaped {
            reaped.notify_one();
        }
    }
}

#[derive(Debug)]
enum RuntimeStartupDecision {
    Run,
}

struct RuntimeStartupSignal {
    observed_at: tokio::time::Instant,
    result: Result<v8::IsolateHandle, String>,
}

#[derive(Debug, PartialEq, Eq)]
enum RuntimeStartupError {
    Runtime(String),
    Timeout,
}

struct RuntimeStartup {
    command_tx: Option<std_mpsc::Sender<RuntimeCommand>>,
    event_rx: Option<mpsc::UnboundedReceiver<RuntimeEvent>>,
    readiness_rx: oneshot::Receiver<RuntimeStartupSignal>,
    activation_tx: Option<std_mpsc::Sender<RuntimeStartupDecision>>,
    join: Option<RuntimeJoin>,
}

impl RuntimeStartup {
    async fn wait_until(
        mut self,
        deadline: tokio::time::Instant,
    ) -> Result<SpawnedRuntime, RuntimeStartupError> {
        use tokio::sync::oneshot::error::TryRecvError;

        let mut deadline_sleep = Box::pin(tokio::time::sleep_until(deadline));
        let signal = tokio::select! {
            biased;
            signal = &mut self.readiness_rx => {
                signal.map_err(|_| RuntimeStartupError::Runtime(
                    "code mode runtime failed before isolate readiness".to_string()
                ))?
            }
            _ = &mut deadline_sleep => {
                match self.readiness_rx.try_recv() {
                    Ok(signal) if signal.observed_at <= deadline => signal,
                    Ok(_) | Err(TryRecvError::Empty) => return Err(RuntimeStartupError::Timeout),
                    Err(TryRecvError::Closed) => {
                        return Err(RuntimeStartupError::Runtime(
                            "code mode runtime failed before isolate readiness".to_string()
                        ));
                    }
                }
            }
        };

        if signal.observed_at > deadline {
            return Err(RuntimeStartupError::Timeout);
        }
        let isolate_handle = signal.result.map_err(RuntimeStartupError::Runtime)?;
        if tokio::time::Instant::now() >= deadline {
            return Err(RuntimeStartupError::Timeout);
        }
        let activation_tx = self.activation_tx.take().ok_or_else(|| {
            RuntimeStartupError::Runtime("code mode runtime startup owner was lost".to_string())
        })?;
        activation_tx
            .send(RuntimeStartupDecision::Run)
            .map_err(|_| {
                RuntimeStartupError::Runtime(
                    "code mode runtime ended before startup activation".to_string(),
                )
            })?;

        Ok(SpawnedRuntime {
            command_tx: self.command_tx.take().expect("startup owns command sender"),
            event_rx: self.event_rx.take().expect("startup owns event receiver"),
            isolate_handle,
            join: Some(self.join.take().expect("startup owns runtime join handle")),
        })
    }
}

impl Drop for RuntimeStartup {
    fn drop(&mut self) {
        // Dropping the activation sender is the startup cancellation fence. The
        // runtime thread never evaluates user JavaScript until it receives Run.
        self.activation_tx.take();
        if let Some(join) = self.join.take() {
            reap_runtime(join);
        }
    }
}

#[derive(Default)]
struct RuntimeStartupConfig {
    #[cfg(test)]
    test_hook: Option<RuntimeStartupTestHook>,
}

#[cfg(test)]
struct RuntimeStartupTestHook {
    before_readiness: Option<Box<dyn FnOnce() + Send>>,
    fail_before_readiness: bool,
    ready_published: Option<Arc<tokio::sync::Notify>>,
    run_started: Arc<std::sync::atomic::AtomicBool>,
    run_started_notify: Arc<tokio::sync::Notify>,
    reaped: Arc<tokio::sync::Notify>,
}

struct RuntimeState {
    event_tx: mpsc::UnboundedSender<RuntimeEvent>,
    pending_tool_calls: HashMap<String, v8::Global<v8::PromiseResolver>>,
    allowed_tools: Vec<String>,
    next_tool_call_id: u64,
    tool_calls: usize,
    emitted_bytes: usize,
    emitted_items: usize,
    fatal_error: Option<RuntimeFailure>,
}

pub async fn execute(
    host: Arc<dyn CodeModeHost>,
    request: CodeModeExecuteRequest,
) -> Result<CodeModeExecution, CodeModeError> {
    execute_with_termination_mode(
        host,
        request,
        CodeModeTerminationMode::ReturnAtFrontendDeadline,
    )
    .await
}

pub async fn execute_with_termination_mode(
    host: Arc<dyn CodeModeHost>,
    request: CodeModeExecuteRequest,
    termination_mode: CodeModeTerminationMode,
) -> Result<CodeModeExecution, CodeModeError> {
    execute_with_termination_mode_inner(
        host,
        request,
        termination_mode,
        execution_slots(),
        RuntimeStartupConfig::default(),
    )
    .await
}

async fn execute_with_termination_mode_inner(
    host: Arc<dyn CodeModeHost>,
    request: CodeModeExecuteRequest,
    termination_mode: CodeModeTerminationMode,
    execution_slots: Arc<Semaphore>,
    startup_config: RuntimeStartupConfig,
) -> Result<CodeModeExecution, CodeModeError> {
    let started_at = Instant::now();
    validate_request(&request).map_err(|message| CodeModeError {
        kind: CodeModeErrorKind::InvalidRequest,
        message,
        stats: CodeModeStats::default(),
        child_failure: None,
        limit: None,
    })?;
    let timeout_ms = normalized_timeout_ms(request.timeout_ms);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    let slot_wait_started_at = Instant::now();
    let execution_slot = tokio::select! {
        permit = Arc::clone(&execution_slots).acquire_owned() => permit.map_err(|_| CodeModeError {
            kind: CodeModeErrorKind::Runtime,
            message: "code mode execution slots are unavailable".to_string(),
            stats: CodeModeStats {
                slot_wait_ms: elapsed_ms(slot_wait_started_at),
                ..CodeModeStats::default()
            },
            child_failure: None,
            limit: None,
        })?,
        _ = tokio::time::sleep_until(deadline) => {
            if termination_mode.drains_started_children() {
                host.stop_accepting_calls();
            }
            return Err(CodeModeError {
                kind: CodeModeErrorKind::Timeout,
                message: format!("code mode execution exceeded {timeout_ms} ms while waiting for a runtime slot"),
                stats: CodeModeStats {
                    slot_wait_ms: elapsed_ms(slot_wait_started_at),
                    ..CodeModeStats::default()
                },
                child_failure: None,
                limit: None,
            });
        }
    };
    let slot_wait_ms = elapsed_ms(slot_wait_started_at);
    let startup = spawn_runtime(
        request.source,
        request.allowed_tools,
        execution_slot,
        startup_config,
    )
    .map_err(|message| CodeModeError {
        kind: CodeModeErrorKind::Runtime,
        message,
        stats: CodeModeStats {
            slot_wait_ms,
            ..CodeModeStats::default()
        },
        child_failure: None,
        limit: None,
    })?;
    let mut runtime = match startup.wait_until(deadline).await {
        Ok(runtime) => runtime,
        Err(RuntimeStartupError::Runtime(message)) => {
            return Err(CodeModeError {
                kind: CodeModeErrorKind::Runtime,
                message,
                stats: finish_stats(started_at, 0, 0, 0, slot_wait_ms),
                child_failure: None,
                limit: None,
            });
        }
        Err(RuntimeStartupError::Timeout) => {
            if termination_mode.drains_started_children() {
                host.stop_accepting_calls();
            }
            return Err(CodeModeError {
                kind: CodeModeErrorKind::Timeout,
                message: format!(
                    "code mode execution exceeded {timeout_ms} ms while starting the runtime"
                ),
                stats: finish_stats(started_at, 0, 0, 0, slot_wait_ms),
                child_failure: None,
                limit: None,
            });
        }
    };

    let mut deadline_sleep = Box::pin(tokio::time::sleep_until(deadline));
    let mut content = Vec::new();
    let mut returned_bytes = 0usize;
    let mut tool_calls = 0usize;
    let mut in_flight_count = 0usize;
    let mut max_in_flight = 0usize;
    let mut pending_calls: VecDeque<(String, CodeModeToolRequest)> = VecDeque::new();
    let mut in_flight = tokio::task::JoinSet::new();
    let mut in_flight_identity = HashMap::new();
    let mut runtime_finished: Option<Option<RuntimeFailure>> = None;

    loop {
        while in_flight_count < MAX_CONCURRENT_TOOL_CALLS {
            let Some((id, request)) = pending_calls.pop_front() else {
                break;
            };
            let host = Arc::clone(&host);
            let ordinal = request.ordinal;
            let tool = request.tool_name.clone();
            let failure_tool = tool.clone();
            let promise_id = id.clone();
            let abort_handle = in_flight.spawn(async move {
                let result =
                    host.invoke_tool(request)
                        .await
                        .map_err(|error| CodeModeChildFailure {
                            ordinal,
                            tool: failure_tool,
                            failure_kind: error.failure_kind().to_string(),
                            message: bounded_child_failure_message(error.message()),
                        });
                (id, result)
            });
            in_flight_identity.insert(abort_handle.id(), (promise_id, ordinal, tool));
            in_flight_count += 1;
            max_in_flight = max_in_flight.max(in_flight_count);
        }

        if runtime_finished.is_some() && in_flight_count == 0 && pending_calls.is_empty() {
            break;
        }

        tokio::select! {
            _ = &mut deadline_sleep => {
                if termination_mode.drains_started_children() {
                    // Frontend failure closes admission before V8 termination. Runtime-queued
                    // requests are discarded, while host tasks already started are drained
                    // without attempting to resolve their Promises back into the terminated isolate.
                    host.stop_accepting_calls();
                    pending_calls.clear();
                }
                let _ = runtime.isolate_handle.terminate_execution();
                let _ = runtime.command_tx.send(RuntimeCommand::Terminate);
                let join = runtime.take_join();
                join_runtime(join).await;
                // RuntimeJoin retains the execution permit through termination and
                // releases it before this post-frontend host reconciliation.
                if let Some(max_drain_ms) = termination_mode.drain_timeout_ms() {
                    let _ = drain_in_flight(
                        &mut in_flight,
                        &mut in_flight_identity,
                        &mut in_flight_count,
                        Duration::from_millis(max_drain_ms),
                    )
                    .await;
                }
                let stats = finish_stats(started_at, tool_calls, max_in_flight, returned_bytes, slot_wait_ms);
                return Err(CodeModeError {
                    kind: CodeModeErrorKind::Timeout,
                    message: format!("code mode execution exceeded {timeout_ms} ms"),
                    stats,
                    child_failure: None,
                    limit: None,
                });
            }
            event = runtime.event_rx.recv(), if runtime_finished.is_none() => {
                match event {
                    Some(RuntimeEvent::ToolCall { id, ordinal, tool_name, arguments }) => {
                        tool_calls += 1;
                        pending_calls.push_back((id, CodeModeToolRequest { ordinal, tool_name, arguments }));
                    }
                    Some(RuntimeEvent::Text(text)) => {
                        returned_bytes = returned_bytes.saturating_add(text.len());
                        content.push(text);
                    }
                    Some(RuntimeEvent::Finished(failure)) => {
                        if termination_mode.drains_started_children() {
                            host.stop_accepting_calls();
                            pending_calls.clear();
                        }
                        runtime_finished = Some(failure);
                        // RuntimeEvent::Finished means the V8 decision phase has
                        // ended. Host reconciliation must not consume V8 capacity.
                        runtime.release_execution_slot();
                        if let Some(max_drain_ms) = termination_mode.drain_timeout_ms() {
                            let drained = drain_in_flight(
                                &mut in_flight,
                                &mut in_flight_identity,
                                &mut in_flight_count,
                                Duration::from_millis(max_drain_ms),
                            )
                            .await;
                            if !drained
                                && runtime_finished
                                    .as_ref()
                                    .is_some_and(|failure| failure.is_none())
                            {
                                runtime_finished = Some(Some(runtime_failure(format!(
                                    "code mode started-child drain exceeded {max_drain_ms} ms"
                                ))));
                            }
                        }
                    }
                    None => {
                        if termination_mode.drains_started_children() {
                            host.stop_accepting_calls();
                            pending_calls.clear();
                        }
                        runtime_finished = Some(Some(runtime_failure(
                            "code mode runtime thread ended without a terminal result",
                        )));
                        runtime.release_execution_slot();
                        if let Some(max_drain_ms) = termination_mode.drain_timeout_ms() {
                            let _ = drain_in_flight(
                                &mut in_flight,
                                &mut in_flight_identity,
                                &mut in_flight_count,
                                Duration::from_millis(max_drain_ms),
                            )
                            .await;
                        }
                    }
                }
            }
            completed = next_in_flight(&mut in_flight, &mut in_flight_identity), if in_flight_count > 0 => {
                let Some((id, result)) = completed else {
                    in_flight_count = 0;
                    continue;
                };
                in_flight_count -= 1;
                if runtime_finished.is_none() {
                    match result {
                        Ok(response) => {
                            let _ = runtime.command_tx.send(RuntimeCommand::ToolResponse {
                                id,
                                result: tool_response_json(response),
                            });
                        }
                        Err(failure) => {
                            let _ = runtime.command_tx.send(RuntimeCommand::ToolError { id, failure });
                        }
                    }
                }
            }
        }
    }

    let failure = runtime_finished.flatten();
    runtime.release_execution_slot();
    let join = runtime.take_join();
    join_runtime(join).await;
    let stats = finish_stats(
        started_at,
        tool_calls,
        max_in_flight,
        returned_bytes,
        slot_wait_ms,
    );
    if let Some(failure) = failure {
        let kind = match failure.kind {
            RuntimeFailureKind::Runtime => CodeModeErrorKind::Runtime,
            RuntimeFailureKind::ChildCallFailed => CodeModeErrorKind::ChildCallFailed,
            RuntimeFailureKind::ToolCallBudgetExceeded => CodeModeErrorKind::ToolCallBudgetExceeded,
            RuntimeFailureKind::OutputLimitExceeded => CodeModeErrorKind::OutputLimitExceeded,
        };
        return Err(CodeModeError {
            kind,
            message: failure.message,
            stats,
            child_failure: failure.child_failure,
            limit: failure.limit,
        });
    }

    Ok(CodeModeExecution { content, stats })
}

fn tool_response_json(response: CodeModeToolResponse) -> JsonValue {
    let mut result = serde_json::Map::new();
    result.insert("success".to_string(), JsonValue::Bool(response.success));
    result.insert("output".to_string(), response.output);
    if let Some(error) = response.error {
        result.insert("error".to_string(), JsonValue::String(error));
    }
    JsonValue::Object(result)
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn finish_stats(
    started_at: Instant,
    tool_calls: usize,
    max_in_flight: usize,
    returned_bytes: usize,
    slot_wait_ms: u64,
) -> CodeModeStats {
    CodeModeStats {
        tool_calls,
        max_in_flight,
        duration_ms: elapsed_ms(started_at),
        returned_bytes,
        slot_wait_ms,
    }
}

fn validate_request(request: &CodeModeExecuteRequest) -> Result<(), String> {
    if request.source.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "code mode source exceeds the {MAX_SOURCE_BYTES}-byte limit"
        ));
    }
    let mut seen = HashSet::new();
    for tool in &request.allowed_tools {
        let mut chars = tool.chars();
        let valid_first = chars
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic());
        let valid_rest = chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric());
        if tool.is_empty() || tool.len() > 128 || !valid_first || !valid_rest {
            return Err(format!("invalid code mode tool name `{tool}`"));
        }
        if !seen.insert(tool.as_str()) {
            return Err(format!("duplicate code mode tool name `{tool}`"));
        }
    }
    Ok(())
}

type InFlightToolResult = (String, Result<CodeModeToolResponse, CodeModeChildFailure>);

async fn next_in_flight(
    in_flight: &mut tokio::task::JoinSet<InFlightToolResult>,
    identities: &mut HashMap<tokio::task::Id, (String, usize, String)>,
) -> Option<InFlightToolResult> {
    match in_flight.join_next_with_id().await {
        Some(Ok((task_id, completed))) => {
            identities.remove(&task_id);
            Some(completed)
        }
        Some(Err(error)) => {
            let task_id = error.id();
            let (id, ordinal, tool) = identities
                .remove(&task_id)
                .expect("spawned Code Mode host task must retain child identity");
            Some((
                id,
                Err(CodeModeChildFailure {
                    ordinal,
                    tool,
                    failure_kind: "host_task_failure".to_string(),
                    message: bounded_child_failure_message(&format!(
                        "nested tool host task failed: {error}"
                    )),
                }),
            ))
        }
        None => None,
    }
}

async fn drain_in_flight(
    in_flight: &mut tokio::task::JoinSet<InFlightToolResult>,
    identities: &mut HashMap<tokio::task::Id, (String, usize, String)>,
    in_flight_count: &mut usize,
    max_drain: Duration,
) -> bool {
    let drain = async {
        while *in_flight_count > 0 {
            if next_in_flight(in_flight, identities).await.is_none() {
                *in_flight_count = 0;
                break;
            }
            *in_flight_count -= 1;
        }
    };
    if tokio::time::timeout(max_drain, drain).await.is_ok() {
        return true;
    }

    // Frontend termination must remain bounded even if an admitted read or
    // consequential host future stalls. Cancelling a consequential host future
    // leaves the host's pre-dispatch receipt at outcome_unknown; domain cleanup
    // guards retain their own canonical cancellation/reconciliation semantics.
    in_flight.abort_all();
    while in_flight.join_next().await.is_some() {}
    identities.clear();
    *in_flight_count = 0;
    false
}

async fn join_runtime(join: RuntimeJoin) {
    let _ = tokio::task::spawn_blocking(move || join.join()).await;
}

fn reap_runtime(join: RuntimeJoin) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let _ = handle.spawn_blocking(move || join.join());
        return;
    }
    join.join();
}

fn spawn_runtime(
    source: String,
    allowed_tools: Vec<String>,
    execution_slot: OwnedSemaphorePermit,
    startup_config: RuntimeStartupConfig,
) -> Result<RuntimeStartup, String> {
    let (command_tx, command_rx) = std_mpsc::channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (readiness_tx, readiness_rx) = oneshot::channel();
    let (activation_tx, activation_rx) = std_mpsc::channel();
    #[cfg(test)]
    let reaped = startup_config
        .test_hook
        .as_ref()
        .map(|hook| Arc::clone(&hook.reaped));
    let runtime_handle = tokio::runtime::Handle::current();
    let join = thread::Builder::new()
        .name("webcodex-code-mode-v8".to_string())
        .spawn(move || {
            // Readiness timestamps must use the same clock as the caller's
            // deadline, including when its Tokio runtime pauses virtual time.
            let _runtime_context = runtime_handle.enter();
            run_runtime(
                source,
                allowed_tools,
                command_rx,
                event_tx,
                readiness_tx,
                activation_rx,
                startup_config,
            )
        })
        .map_err(|error| format!("failed to spawn code mode runtime thread: {error}"))?;
    Ok(RuntimeStartup {
        command_tx: Some(command_tx),
        event_rx: Some(event_rx),
        readiness_rx,
        activation_tx: Some(activation_tx),
        join: Some(RuntimeJoin {
            join,
            execution_slot: Some(execution_slot),
            #[cfg(test)]
            reaped,
        }),
    })
}

fn run_runtime(
    source: String,
    allowed_tools: Vec<String>,
    command_rx: std_mpsc::Receiver<RuntimeCommand>,
    event_tx: mpsc::UnboundedSender<RuntimeEvent>,
    readiness_tx: oneshot::Sender<RuntimeStartupSignal>,
    activation_rx: std_mpsc::Receiver<RuntimeStartupDecision>,
    startup_config: RuntimeStartupConfig,
) {
    #[cfg(test)]
    let mut startup_config = startup_config;
    #[cfg(not(test))]
    let _startup_config = startup_config;

    if let Err(message) = ensure_v8_initialized() {
        let _ = readiness_tx.send(RuntimeStartupSignal {
            observed_at: tokio::time::Instant::now(),
            result: Err(message),
        });
        return;
    }

    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    let isolate_handle = isolate.thread_safe_handle();
    #[cfg(test)]
    if let Some(before_readiness) = startup_config
        .test_hook
        .as_mut()
        .and_then(|hook| hook.before_readiness.take())
    {
        before_readiness();
    }
    #[cfg(test)]
    if startup_config
        .test_hook
        .as_ref()
        .is_some_and(|hook| hook.fail_before_readiness)
    {
        return;
    }

    let readiness = RuntimeStartupSignal {
        observed_at: tokio::time::Instant::now(),
        result: Ok(isolate_handle.clone()),
    };
    if let Err(readiness) = readiness_tx.send(readiness) {
        if let Ok(late_handle) = readiness.result {
            let _ = late_handle.terminate_execution();
        }
        return;
    }
    #[cfg(test)]
    if let Some(ready_published) = startup_config
        .test_hook
        .as_ref()
        .and_then(|hook| hook.ready_published.as_ref())
    {
        ready_published.notify_one();
    }

    match activation_rx.recv() {
        Ok(RuntimeStartupDecision::Run) =>
        {
            #[cfg(test)]
            if let Some(hook) = startup_config.test_hook.as_ref() {
                hook.run_started
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                hook.run_started_notify.notify_one();
            }
        }
        Err(_) => {
            let _ = isolate_handle.terminate_execution();
            return;
        }
    }

    isolate.set_host_import_module_dynamically_callback(dynamic_import_callback);

    v8::scope!(let scope, isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);
    scope.set_slot(RuntimeState {
        event_tx: event_tx.clone(),
        pending_tool_calls: HashMap::new(),
        allowed_tools,
        next_tool_call_id: 1,
        tool_calls: 0,
        emitted_bytes: 0,
        emitted_items: 0,
        fatal_error: None,
    });

    if let Err(message) = install_globals(scope) {
        let _ = event_tx.send(RuntimeEvent::Finished(Some(runtime_failure(message))));
        return;
    }

    let promise = match evaluate_source(scope, &source) {
        Ok(promise) => promise,
        Err(failure) => {
            let _ = event_tx.send(RuntimeEvent::Finished(Some(failure)));
            return;
        }
    };
    if let Some(finished) = completion_state(scope, &promise) {
        let _ = event_tx.send(RuntimeEvent::Finished(finished));
        return;
    }

    loop {
        match command_rx.recv() {
            Ok(RuntimeCommand::ToolResponse { id, result }) => {
                if let Err(message) = resolve_tool_response(scope, &id, Ok(result)) {
                    let _ = event_tx.send(RuntimeEvent::Finished(Some(runtime_failure(message))));
                    return;
                }
            }
            Ok(RuntimeCommand::ToolError { id, failure }) => {
                if let Err(message) = resolve_tool_response(scope, &id, Err(failure)) {
                    let _ = event_tx.send(RuntimeEvent::Finished(Some(runtime_failure(message))));
                    return;
                }
            }
            Ok(RuntimeCommand::Terminate) | Err(_) => return,
        }
        scope.perform_microtask_checkpoint();
        if let Some(finished) = completion_state(scope, &promise) {
            let _ = event_tx.send(RuntimeEvent::Finished(finished));
            return;
        }
    }
}

fn install_globals(scope: &mut v8::PinScope<'_, '_>) -> Result<(), String> {
    let global = scope.get_current_context().global(scope);
    for name in ["console", "Atomics", "SharedArrayBuffer", "WebAssembly"] {
        delete_global(scope, global, name)?;
    }
    let tools = v8::Object::new(scope);
    let allowed_tools = scope
        .get_slot::<RuntimeState>()
        .map(|state| state.allowed_tools.clone())
        .unwrap_or_default();
    for (index, tool) in allowed_tools.iter().enumerate() {
        let key = v8::String::new(scope, tool)
            .ok_or_else(|| format!("failed to allocate tool name `{tool}`"))?;
        let data = v8::Integer::new(scope, index as i32);
        let function = v8::FunctionTemplate::builder(tool_callback)
            .data(data.into())
            .build(scope)
            .get_function(scope)
            .ok_or_else(|| format!("failed to create tool function `{tool}`"))?;
        if tools.set(scope, key.into(), function.into()) != Some(true) {
            return Err(format!("failed to install tool function `{tool}`"));
        }
    }
    set_global(scope, global, "tools", tools.into())?;
    let text = v8::FunctionTemplate::new(scope, text_callback)
        .get_function(scope)
        .ok_or_else(|| "failed to create text helper".to_string())?;
    set_global(scope, global, "text", text.into())?;
    Ok(())
}

fn evaluate_source(
    scope: &mut v8::PinScope<'_, '_>,
    source: &str,
) -> Result<v8::Global<v8::Promise>, RuntimeFailure> {
    let wrapped = format!("(async () => {{\n{source}\n}})()");
    let tc = std::pin::pin!(v8::TryCatch::new(scope));
    let mut tc = tc.init();
    let source = v8::String::new(&tc, &wrapped)
        .ok_or_else(|| runtime_failure("failed to allocate code mode source"))?;
    let script = v8::Script::compile(&tc, source, None).ok_or_else(|| {
        let message = tc
            .exception()
            .map(|exception| value_to_error_text(&mut tc, exception))
            .unwrap_or_else(|| "failed to compile code mode source".to_string());
        runtime_failure(message)
    })?;
    let value = script.run(&tc).ok_or_else(|| {
        let message = tc
            .exception()
            .map(|exception| value_to_error_text(&mut tc, exception))
            .unwrap_or_else(|| "failed to evaluate code mode source".to_string());
        runtime_failure(message)
    })?;
    tc.perform_microtask_checkpoint();
    let promise = v8::Local::<v8::Promise>::try_from(value)
        .map_err(|_| runtime_failure("code mode wrapper did not return a Promise"))?;
    Ok(v8::Global::new(&tc, promise))
}

fn completion_state(
    scope: &mut v8::PinScope<'_, '_>,
    promise: &v8::Global<v8::Promise>,
) -> Option<Option<RuntimeFailure>> {
    let promise = v8::Local::new(scope, promise);
    match promise.state() {
        v8::PromiseState::Pending => None,
        v8::PromiseState::Fulfilled => Some(
            scope
                .get_slot::<RuntimeState>()
                .and_then(|state| state.fatal_error.clone()),
        ),
        v8::PromiseState::Rejected => {
            if let Some(failure) = scope
                .get_slot::<RuntimeState>()
                .and_then(|state| state.fatal_error.clone())
            {
                return Some(Some(failure));
            }
            let result = promise.result(scope);
            if let Ok(Some(value)) = v8_value_to_json(scope, result) {
                if value.get("failure_kind").and_then(JsonValue::as_str)
                    == Some("child_call_failed")
                {
                    if let Some(child) = value.get("child_failure").cloned().and_then(|value| {
                        serde_json::from_value::<CodeModeChildFailure>(value).ok()
                    }) {
                        return Some(Some(RuntimeFailure {
                            kind: RuntimeFailureKind::ChildCallFailed,
                            message: child.message.clone(),
                            child_failure: Some(child),
                            limit: None,
                        }));
                    }
                }
            }
            Some(Some(runtime_failure(value_to_error_text(scope, result))))
        }
    }
}

fn tool_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue<v8::Value>,
) {
    let tool_index = args.data().integer_value(scope).unwrap_or(-1);
    let Ok(tool_index) = usize::try_from(tool_index) else {
        throw_error(scope, "invalid tool callback identity");
        return;
    };
    let arguments = if args.length() == 0 || args.get(0).is_undefined() {
        json!({})
    } else {
        match v8_value_to_json(scope, args.get(0)) {
            Ok(Some(JsonValue::Object(arguments))) => JsonValue::Object(arguments),
            Ok(_) => {
                throw_error(scope, "tool arguments must be a JSON object");
                return;
            }
            Err(message) => {
                throw_error(scope, &message);
                return;
            }
        }
    };

    let Some(resolver) = v8::PromiseResolver::new(scope) else {
        throw_error(scope, "failed to create nested tool Promise");
        return;
    };
    let promise = resolver.get_promise(scope);
    let resolver = v8::Global::new(scope, resolver);

    let (event_tx, tool_name, id, ordinal) = {
        let Some(state) = scope.get_slot_mut::<RuntimeState>() else {
            throw_error(scope, "code mode runtime state unavailable");
            return;
        };
        if state.tool_calls >= MAX_TOOL_CALLS {
            state.fatal_error = Some(RuntimeFailure {
                kind: RuntimeFailureKind::ToolCallBudgetExceeded,
                message: format!("code mode nested tool call limit ({MAX_TOOL_CALLS}) exceeded"),
                child_failure: None,
                limit: Some(CodeModeLimit {
                    kind: "nested_tool_calls".to_string(),
                    allowed: MAX_TOOL_CALLS,
                    current: state.tool_calls,
                    attempted: state.tool_calls.saturating_add(1),
                }),
            });
            throw_error(scope, "code mode nested tool call limit exceeded");
            return;
        }
        let Some(tool_name) = state.allowed_tools.get(tool_index).cloned() else {
            throw_error(scope, "tool callback identity is out of range");
            return;
        };
        state.tool_calls += 1;
        let ordinal = state.tool_calls;
        let id = format!("nested-{}", state.next_tool_call_id);
        state.next_tool_call_id = state.next_tool_call_id.saturating_add(1);
        state.pending_tool_calls.insert(id.clone(), resolver);
        (state.event_tx.clone(), tool_name, id, ordinal)
    };
    let _ = event_tx.send(RuntimeEvent::ToolCall {
        id,
        ordinal,
        tool_name,
        arguments,
    });
    retval.set(promise.into());
}

fn text_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue<v8::Value>,
) {
    let value = if args.length() == 0 {
        v8::undefined(scope).into()
    } else {
        args.get(0)
    };
    let text = match serialize_text(scope, value) {
        Ok(text) => text,
        Err(message) => {
            throw_error(scope, &message);
            return;
        }
    };
    let Some(state) = scope.get_slot_mut::<RuntimeState>() else {
        throw_error(scope, "code mode runtime state unavailable");
        return;
    };
    if state.emitted_items >= MAX_OUTPUT_ITEMS {
        state.fatal_error = Some(RuntimeFailure {
            kind: RuntimeFailureKind::OutputLimitExceeded,
            message: format!("code mode text output exceeds the {MAX_OUTPUT_ITEMS}-emission limit"),
            child_failure: None,
            limit: Some(CodeModeLimit {
                kind: "text_output_items".to_string(),
                allowed: MAX_OUTPUT_ITEMS,
                current: state.emitted_items,
                attempted: state.emitted_items.saturating_add(1),
            }),
        });
        throw_error(scope, "code mode text emission limit exceeded");
        return;
    }
    let next_bytes = state.emitted_bytes.saturating_add(text.len());
    if next_bytes > MAX_OUTPUT_BYTES {
        state.fatal_error = Some(RuntimeFailure {
            kind: RuntimeFailureKind::OutputLimitExceeded,
            message: format!("code mode text output exceeds the {MAX_OUTPUT_BYTES}-byte limit"),
            child_failure: None,
            limit: Some(CodeModeLimit {
                kind: "text_output_bytes".to_string(),
                allowed: MAX_OUTPUT_BYTES,
                current: state.emitted_bytes,
                attempted: next_bytes,
            }),
        });
        throw_error(scope, "code mode text output limit exceeded");
        return;
    }
    state.emitted_bytes = next_bytes;
    state.emitted_items += 1;
    let _ = state.event_tx.send(RuntimeEvent::Text(text));
    retval.set(v8::undefined(scope).into());
}

fn resolve_tool_response(
    scope: &mut v8::PinScope<'_, '_>,
    id: &str,
    response: Result<JsonValue, CodeModeChildFailure>,
) -> Result<(), String> {
    let resolver = scope
        .get_slot_mut::<RuntimeState>()
        .and_then(|state| state.pending_tool_calls.remove(id))
        .ok_or_else(|| format!("unknown nested tool call `{id}`"))?;
    let tc = std::pin::pin!(v8::TryCatch::new(scope));
    let mut tc = tc.init();
    let resolver = v8::Local::new(&tc, &resolver);
    match response {
        Ok(response) => {
            let value = json_to_v8(&mut tc, &response)
                .ok_or_else(|| "failed to serialize nested tool response".to_string())?;
            resolver.resolve(&tc, value);
        }
        Err(failure) => {
            // Preserve the original JavaScript rejected-Error ergonomics while
            // attaching enumerable structured fields for outer failure classification.
            let message = v8::String::new(&tc, &failure.message)
                .ok_or_else(|| "failed to allocate nested tool host error".to_string())?;
            let rejection = v8::Exception::error(&tc, message);
            let rejection_object = v8::Local::<v8::Object>::try_from(rejection)
                .map_err(|_| "failed to create nested tool host error object".to_string())?;
            let failure_kind_key = v8::String::new(&tc, "failure_kind")
                .ok_or_else(|| "failed to allocate child failure key".to_string())?;
            let failure_kind = v8::String::new(&tc, "child_call_failed")
                .ok_or_else(|| "failed to allocate child failure kind".to_string())?;
            if rejection_object.set(&tc, failure_kind_key.into(), failure_kind.into()) != Some(true)
            {
                return Err("failed to attach child failure kind".to_string());
            }
            let child_failure_key = v8::String::new(&tc, "child_failure")
                .ok_or_else(|| "failed to allocate child failure key".to_string())?;
            let child_failure = serde_json::to_value(&failure)
                .ok()
                .and_then(|failure| json_to_v8(&mut tc, &failure))
                .ok_or_else(|| "failed to serialize nested tool host failure".to_string())?;
            if rejection_object.set(&tc, child_failure_key.into(), child_failure) != Some(true) {
                return Err("failed to attach child failure detail".to_string());
            }
            resolver.reject(&tc, rejection);
        }
    }
    if tc.has_caught() {
        let message = tc
            .exception()
            .map(|exception| value_to_error_text(&mut tc, exception))
            .unwrap_or_else(|| "nested tool Promise resolution failed".to_string());
        return Err(message);
    }
    Ok(())
}

fn serialize_text(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> Result<String, String> {
    if value.is_string() {
        return Ok(value.to_rust_string_lossy(scope));
    }
    let tc = std::pin::pin!(v8::TryCatch::new(scope));
    let mut tc = tc.init();
    let Some(stringified) = v8::json::stringify(&tc, value) else {
        let message = tc
            .exception()
            .map(|exception| value_to_error_text(&mut tc, exception))
            .unwrap_or_else(|| "text(value) requires a string or JSON-safe value".to_string());
        return Err(message);
    };
    Ok(stringified.to_rust_string_lossy(&tc))
}

fn v8_value_to_json(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> Result<Option<JsonValue>, String> {
    let tc = std::pin::pin!(v8::TryCatch::new(scope));
    let mut tc = tc.init();
    let Some(stringified) = v8::json::stringify(&tc, value) else {
        if tc.has_caught() {
            let message = tc
                .exception()
                .map(|exception| value_to_error_text(&mut tc, exception))
                .unwrap_or_else(|| "failed to serialize JavaScript value".to_string());
            return Err(message);
        }
        return Ok(None);
    };
    serde_json::from_str(&stringified.to_rust_string_lossy(&tc))
        .map(Some)
        .map_err(|error| format!("failed to serialize JavaScript value: {error}"))
}

fn json_to_v8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: &JsonValue,
) -> Option<v8::Local<'s, v8::Value>> {
    match value {
        JsonValue::Null => Some(v8::null(scope).into()),
        JsonValue::Bool(value) => Some(v8::Boolean::new(scope, *value).into()),
        JsonValue::Number(value) => Some(v8::Number::new(scope, value.as_f64()?).into()),
        JsonValue::String(value) => Some(v8::String::new(scope, value)?.into()),
        JsonValue::Array(values) => {
            i32::try_from(values.len()).ok()?;
            let elements = values
                .iter()
                .map(|value| json_to_v8(scope, value))
                .collect::<Option<Vec<_>>>()?;
            // Build dense own elements without allocating a string key per index.
            // Unlike indexed assignment, construction cannot invoke inherited setters.
            Some(v8::Array::new_with_elements(scope, &elements).into())
        }
        JsonValue::Object(values) => {
            let object = v8::Object::new(scope);
            for (key, value) in values {
                let key = v8::String::new(scope, key)?;
                let value = json_to_v8(scope, value)?;
                if object.create_data_property(scope, key.into(), value) != Some(true) {
                    return None;
                }
            }
            Some(object.into())
        }
    }
}

fn value_to_error_text(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> String {
    if value.is_object() {
        if let Ok(object) = v8::Local::<v8::Object>::try_from(value) {
            if let Some(key) = v8::String::new(scope, "stack") {
                if let Some(stack) = object.get(scope, key.into()) {
                    if stack.is_string() {
                        return stack.to_rust_string_lossy(scope);
                    }
                }
            }
        }
    }
    value.to_rust_string_lossy(scope)
}

fn throw_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    if let Some(message) = v8::String::new(scope, message) {
        scope.throw_exception(message.into());
    }
}

fn set_global<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    global: v8::Local<'s, v8::Object>,
    name: &str,
    value: v8::Local<'s, v8::Value>,
) -> Result<(), String> {
    let key = v8::String::new(scope, name)
        .ok_or_else(|| format!("failed to allocate global `{name}`"))?;
    if global.set(scope, key.into(), value) == Some(true) {
        Ok(())
    } else {
        Err(format!("failed to set global `{name}`"))
    }
}

fn delete_global<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    global: v8::Local<'s, v8::Object>,
    name: &str,
) -> Result<(), String> {
    let key = v8::String::new(scope, name)
        .ok_or_else(|| format!("failed to allocate global `{name}`"))?;
    if global.delete(scope, key.into()) == Some(true) {
        Ok(())
    } else {
        Err(format!("failed to remove global `{name}`"))
    }
}

fn bounded_child_failure_message(message: &str) -> String {
    const MAX_CHILD_FAILURE_MESSAGE_BYTES: usize = 2_048;
    if message.len() <= MAX_CHILD_FAILURE_MESSAGE_BYTES {
        return message.to_string();
    }
    let suffix = "...[truncated]";
    let mut end = MAX_CHILD_FAILURE_MESSAGE_BYTES.saturating_sub(suffix.len());
    while end > 0 && !message.is_char_boundary(end) {
        end -= 1;
    }
    let mut bounded = message[..end].to_string();
    bounded.push_str(suffix);
    bounded
}

fn runtime_failure(message: impl Into<String>) -> RuntimeFailure {
    RuntimeFailure {
        kind: RuntimeFailureKind::Runtime,
        message: message.into(),
        child_failure: None,
        limit: None,
    }
}

fn dynamic_import_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _host_defined_options: v8::Local<'s, v8::Data>,
    _resource_name: v8::Local<'s, v8::Value>,
    _specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let resolver = v8::PromiseResolver::new(scope)?;
    let error = v8::String::new(scope, "imports are not available in WebCodex Code Mode")?;
    resolver.reject(scope, error.into());
    Some(resolver.get_promise(scope))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CodeModeHostError, CodeModeHostFuture};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;
    use tokio::sync::{Barrier, Notify, Semaphore};

    #[derive(Default)]
    struct RecordingHost {
        calls: Mutex<Vec<String>>,
    }

    impl CodeModeHost for RecordingHost {
        fn invoke_tool(
            &self,
            request: CodeModeToolRequest,
        ) -> CodeModeHostFuture<'_, Result<CodeModeToolResponse, CodeModeHostError>> {
            Box::pin(async move {
                self.calls.lock().unwrap().push(request.tool_name.clone());
                let output = match request.tool_name.as_str() {
                    "fake_search" => json!({"needs_detail": true}),
                    "fake_read" => json!({"detail": "found"}),
                    _ => json!({"ok": true}),
                };
                Ok(CodeModeToolResponse {
                    success: true,
                    output,
                    error: None,
                })
            })
        }
    }

    fn request(source: &str, allowed_tools: &[&str]) -> CodeModeExecuteRequest {
        CodeModeExecuteRequest {
            source: source.to_string(),
            allowed_tools: allowed_tools
                .iter()
                .map(|tool| (*tool).to_string())
                .collect(),
            timeout_ms: Some(2_000),
        }
    }

    fn startup_test_hook(
        before_readiness: Option<Box<dyn FnOnce() + Send>>,
        fail_before_readiness: bool,
        ready_published: Option<Arc<Notify>>,
    ) -> (
        RuntimeStartupTestHook,
        Arc<AtomicBool>,
        Arc<Notify>,
        Arc<Notify>,
    ) {
        let run_started = Arc::new(AtomicBool::new(false));
        let run_started_notify = Arc::new(Notify::new());
        let reaped = Arc::new(Notify::new());
        (
            RuntimeStartupTestHook {
                before_readiness,
                fail_before_readiness,
                ready_published,
                run_started: Arc::clone(&run_started),
                run_started_notify: Arc::clone(&run_started_notify),
                reaped: Arc::clone(&reaped),
            },
            run_started,
            run_started_notify,
            reaped,
        )
    }

    #[tokio::test(start_paused = true)]
    async fn startup_readiness_uses_callers_runtime_clock() {
        tokio::time::advance(Duration::from_secs(60)).await;
        let now = tokio::time::Instant::now();
        let slots = Arc::new(Semaphore::new(1));
        let permit = Arc::clone(&slots).acquire_owned().await.unwrap();
        let mut startup = spawn_runtime(
            "text('must not run')".to_string(),
            Vec::new(),
            permit,
            RuntimeStartupConfig::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let signal = loop {
            match startup.readiness_rx.try_recv() {
                Ok(signal) => break signal,
                Err(oneshot::error::TryRecvError::Empty) => {}
                Err(error) => panic!("startup readiness closed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "startup readiness was not published"
            );
            tokio::task::yield_now().await;
        };
        assert!(signal.result.is_ok());
        assert_eq!(signal.observed_at, now);

        // Close the activation gate and join the native thread without relying
        // on a virtual timeout to schedule its cleanup.
        startup.activation_tx.take();
        join_runtime(startup.join.take().unwrap()).await;
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn startup_handshake_runs_source_and_reaps_runtime() {
        let slots = Arc::new(Semaphore::new(1));
        let (hook, run_started, _run_started_notify, reaped) = startup_test_hook(None, false, None);
        let result = execute_with_termination_mode_inner(
            Arc::new(RecordingHost::default()),
            request("text('ready')", &[]),
            CodeModeTerminationMode::ReturnAtFrontendDeadline,
            Arc::clone(&slots),
            RuntimeStartupConfig {
                test_hook: Some(hook),
            },
        )
        .await
        .unwrap();

        assert_eq!(result.content, ["ready"]);
        assert!(run_started.load(Ordering::SeqCst));
        assert_eq!(slots.available_permits(), 1);
        tokio::time::timeout(Duration::from_secs(1), reaped.notified())
            .await
            .expect("normal runtime must be joined before completion");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delayed_startup_deadline_is_async_fenced_reaped_and_releases_slot() {
        ensure_v8_initialized().unwrap();
        let entered = Arc::new(Notify::new());
        let entered_for_thread = Arc::clone(&entered);
        let (release_tx, release_rx) = std_mpsc::channel();
        let before_readiness = Box::new(move || {
            entered_for_thread.notify_one();
            let _ = release_rx.recv();
        });
        let (hook, run_started, _run_started_notify, reaped) =
            startup_test_hook(Some(before_readiness), false, None);
        let slots = Arc::new(Semaphore::new(1));
        let slots_for_task = Arc::clone(&slots);
        let mut req = request("text('must not run')", &[]);
        req.timeout_ms = Some(100);
        let task = tokio::spawn(async move {
            execute_with_termination_mode_inner(
                Arc::new(RecordingHost::default()),
                req,
                CodeModeTerminationMode::ReturnAtFrontendDeadline,
                slots_for_task,
                RuntimeStartupConfig {
                    test_hook: Some(hook),
                },
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(1), entered.notified())
            .await
            .expect("runtime thread must reach the deterministic startup gate");
        let error = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("startup deadline must not block the Tokio worker")
            .unwrap()
            .unwrap_err();
        assert_eq!(error.kind, CodeModeErrorKind::Timeout);
        assert!(error.message.contains("while starting the runtime"));
        assert!(!run_started.load(Ordering::SeqCst));
        assert_eq!(slots.available_permits(), 0);

        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), reaped.notified())
            .await
            .expect("late startup thread must be reaped after its gate is released");
        assert!(!run_started.load(Ordering::SeqCst));
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn startup_task_cancellation_before_readiness_fences_js_and_releases_slot() {
        ensure_v8_initialized().unwrap();
        let entered = Arc::new(Notify::new());
        let entered_for_thread = Arc::clone(&entered);
        let (release_tx, release_rx) = std_mpsc::channel();
        let before_readiness = Box::new(move || {
            entered_for_thread.notify_one();
            let _ = release_rx.recv();
        });
        let (hook, run_started, _run_started_notify, reaped) =
            startup_test_hook(Some(before_readiness), false, None);
        let slots = Arc::new(Semaphore::new(1));
        let slots_for_task = Arc::clone(&slots);
        let task = tokio::spawn(async move {
            execute_with_termination_mode_inner(
                Arc::new(RecordingHost::default()),
                request("text('must not run')", &[]),
                CodeModeTerminationMode::ReturnAtFrontendDeadline,
                slots_for_task,
                RuntimeStartupConfig {
                    test_hook: Some(hook),
                },
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(1), entered.notified())
            .await
            .expect("runtime thread must reach the startup gate");
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(slots.available_permits(), 0);
        assert!(!run_started.load(Ordering::SeqCst));

        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), reaped.notified())
            .await
            .expect("cancelled startup must retain a reaper owner");
        assert!(!run_started.load(Ordering::SeqCst));
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_after_ready_publication_before_activation_fences_js() {
        ensure_v8_initialized().unwrap();
        let ready_published = Arc::new(Notify::new());
        let (hook, run_started, _run_started_notify, reaped) =
            startup_test_hook(None, false, Some(Arc::clone(&ready_published)));
        let slots = Arc::new(Semaphore::new(1));
        let execution_slot = Arc::clone(&slots).acquire_owned().await.unwrap();
        let startup = spawn_runtime(
            "text('must not run')".to_string(),
            Vec::new(),
            execution_slot,
            RuntimeStartupConfig {
                test_hook: Some(hook),
            },
        )
        .unwrap();

        tokio::time::timeout(Duration::from_secs(1), ready_published.notified())
            .await
            .expect("runtime must publish readiness before cancellation");
        assert_eq!(slots.available_permits(), 0);
        drop(startup);
        tokio::time::timeout(Duration::from_secs(1), reaped.notified())
            .await
            .expect("ready-but-not-activated runtime must be terminated and reaped");
        assert!(!run_started.load(Ordering::SeqCst));
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_after_activation_terminates_reaps_and_releases_slot() {
        ensure_v8_initialized().unwrap();
        let (hook, run_started, run_started_notify, reaped) = startup_test_hook(None, false, None);
        let slots = Arc::new(Semaphore::new(1));
        let slots_for_task = Arc::clone(&slots);
        let task = tokio::spawn(async move {
            execute_with_termination_mode_inner(
                Arc::new(RecordingHost::default()),
                request("while (true) {}", &[]),
                CodeModeTerminationMode::ReturnAtFrontendDeadline,
                slots_for_task,
                RuntimeStartupConfig {
                    test_hook: Some(hook),
                },
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(1), run_started_notify.notified())
            .await
            .expect("runtime must cross the activation fence before cancellation");
        assert!(run_started.load(Ordering::SeqCst));
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(slots.available_permits(), 0);
        tokio::time::timeout(Duration::from_secs(1), reaped.notified())
            .await
            .expect("activated runtime must retain its slot until it is reaped");
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failure_before_readiness_is_bounded_runtime_error_and_releases_slot() {
        ensure_v8_initialized().unwrap();
        let (hook, run_started, _run_started_notify, reaped) = startup_test_hook(None, true, None);
        let slots = Arc::new(Semaphore::new(1));
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            execute_with_termination_mode_inner(
                Arc::new(RecordingHost::default()),
                request("text('must not run')", &[]),
                CodeModeTerminationMode::ReturnAtFrontendDeadline,
                Arc::clone(&slots),
                RuntimeStartupConfig {
                    test_hook: Some(hook),
                },
            ),
        )
        .await
        .expect("startup thread failure must not hang")
        .unwrap_err();

        assert_eq!(result.kind, CodeModeErrorKind::Runtime);
        assert!(result.message.contains("failed before isolate readiness"));
        assert!(!run_started.load(Ordering::SeqCst));
        tokio::time::timeout(Duration::from_secs(1), reaped.notified())
            .await
            .expect("failed startup thread must be reaped");
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn panic_before_readiness_closes_handshake_reaps_and_releases_slot() {
        ensure_v8_initialized().unwrap();
        let before_readiness = Box::new(|| panic!("deterministic startup panic"));
        let (hook, run_started, _run_started_notify, reaped) =
            startup_test_hook(Some(before_readiness), false, None);
        let slots = Arc::new(Semaphore::new(1));
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            execute_with_termination_mode_inner(
                Arc::new(RecordingHost::default()),
                request("text('must not run')", &[]),
                CodeModeTerminationMode::ReturnAtFrontendDeadline,
                Arc::clone(&slots),
                RuntimeStartupConfig {
                    test_hook: Some(hook),
                },
            ),
        )
        .await
        .expect("startup thread panic must close readiness without hanging")
        .unwrap_err();

        assert_eq!(result.kind, CodeModeErrorKind::Runtime);
        assert!(result.message.contains("failed before isolate readiness"));
        assert!(!run_started.load(Ordering::SeqCst));
        tokio::time::timeout(Duration::from_secs(1), reaped.notified())
            .await
            .expect("panicked startup thread must be joined by the reaper");
        assert_eq!(slots.available_permits(), 1);
    }

    #[test]
    fn slot_wait_is_diagnostic_only_and_not_serialized_in_model_stats() {
        let stats = finish_stats(Instant::now(), 2, 1, 3, 17);
        assert_eq!(stats.slot_wait_ms, 17);
        let serialized = serde_json::to_value(stats).unwrap();
        let mut keys = serialized
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "duration_ms",
                "max_in_flight",
                "returned_bytes",
                "tool_calls"
            ]
        );
    }

    #[tokio::test]
    async fn emits_basic_json_text() {
        let result = execute(
            Arc::new(RecordingHost::default()),
            request(r#"text({hello: "world"});"#, &[]),
        )
        .await
        .unwrap();
        assert_eq!(result.content, vec![r#"{"hello":"world"}"#]);
        assert_eq!(result.stats.tool_calls, 0);
    }

    #[tokio::test]
    async fn awaits_one_nested_tool_call() {
        let host = Arc::new(RecordingHost::default());
        let result = execute(
            host.clone(),
            request(
                r#"const r = await tools.fake_read({}); text(r);"#,
                &["fake_read"],
            ),
        )
        .await
        .unwrap();
        assert_eq!(host.calls.lock().unwrap().as_slice(), ["fake_read"]);
        assert_eq!(result.stats.tool_calls, 1);
        let emitted: JsonValue = serde_json::from_str(&result.content[0]).unwrap();
        assert_eq!(emitted["success"], true);
        assert_eq!(emitted["output"]["detail"], "found");
    }

    #[tokio::test]
    async fn adaptive_second_call_depends_on_first_result() {
        let host = Arc::new(RecordingHost::default());
        let result = execute(
            host.clone(),
            request(
                r#"
                const search = await tools.fake_search({});
                if (search.output.needs_detail) {
                    const detail = await tools.fake_read({path: "chosen-after-search"});
                    text(detail.output);
                }
                "#,
                &["fake_search", "fake_read"],
            ),
        )
        .await
        .unwrap();
        assert_eq!(
            host.calls.lock().unwrap().as_slice(),
            ["fake_search", "fake_read"]
        );
        assert_eq!(result.content, vec![r#"{"detail":"found"}"#]);
    }

    struct ConcurrentHost {
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        barrier: Barrier,
        completion_order: Mutex<Vec<usize>>,
    }

    impl ConcurrentHost {
        fn new() -> Self {
            Self {
                in_flight: AtomicUsize::new(0),
                max_in_flight: AtomicUsize::new(0),
                barrier: Barrier::new(3),
                completion_order: Mutex::new(Vec::new()),
            }
        }
    }

    impl CodeModeHost for ConcurrentHost {
        fn invoke_tool(
            &self,
            request: CodeModeToolRequest,
        ) -> CodeModeHostFuture<'_, Result<CodeModeToolResponse, CodeModeHostError>> {
            Box::pin(async move {
                let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_in_flight.fetch_max(now, Ordering::SeqCst);
                self.barrier.wait().await;
                let delay_ms = 10 * (3usize.saturating_sub(request.ordinal)) as u64;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                self.completion_order.lock().unwrap().push(request.ordinal);
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                Ok(CodeModeToolResponse {
                    success: true,
                    output: json!({"ok": true, "ordinal": request.ordinal, "tool": request.tool_name}),
                    error: None,
                })
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn promise_all_overlaps_independent_calls() {
        let host = Arc::new(ConcurrentHost::new());
        let result = execute(
            host.clone(),
            request(
                r#"
                const [a,b,c] = await Promise.all([
                    tools.a({}), tools.b({}), tools.c({})
                ]);
                text([a.output.ordinal, b.output.ordinal, c.output.ordinal]);
                "#,
                &["a", "b", "c"],
            ),
        )
        .await
        .unwrap();
        assert!(host.max_in_flight.load(Ordering::SeqCst) >= 2);
        assert!(result.stats.max_in_flight >= 2);
        assert_eq!(result.stats.tool_calls, 3);
        assert_eq!(result.content, vec!["[1,2,3]"]);
        assert_eq!(host.completion_order.lock().unwrap().as_slice(), [3, 2, 1]);
    }

    struct FailingHost {
        failure_kind: &'static str,
    }

    impl CodeModeHost for FailingHost {
        fn invoke_tool(
            &self,
            _request: CodeModeToolRequest,
        ) -> CodeModeHostFuture<'_, Result<CodeModeToolResponse, CodeModeHostError>> {
            Box::pin(async move {
                Err(CodeModeHostError::with_kind(
                    self.failure_kind,
                    "bounded child host failure",
                ))
            })
        }
    }

    #[tokio::test]
    async fn child_host_failure_preserves_ordinal_tool_and_stable_kind() {
        for failure_kind in ["invalid_arguments", "insufficient_scope"] {
            let error = execute(
                Arc::new(FailingHost { failure_kind }),
                request("await tools.fake_read({});", &["fake_read"]),
            )
            .await
            .unwrap_err();
            assert_eq!(error.kind, CodeModeErrorKind::ChildCallFailed);
            let child = error.child_failure.expect("structured child failure");
            assert_eq!(child.ordinal, 1);
            assert_eq!(child.tool, "fake_read");
            assert_eq!(child.failure_kind, failure_kind);
            assert_eq!(child.message, "bounded child host failure");
            assert!(error.limit.is_none());
        }
    }

    #[tokio::test]
    async fn caught_child_host_failure_remains_stringifiable_and_structured() {
        let result = execute(
            Arc::new(FailingHost {
                failure_kind: "invalid_arguments",
            }),
            request(
                r#"
                try {
                    await tools.fake_read({});
                } catch (error) {
                    text({
                        rendered: String(error),
                        failure_kind: error.failure_kind,
                        child_kind: error.child_failure.failure_kind
                    });
                }
                "#,
                &["fake_read"],
            ),
        )
        .await
        .unwrap();
        assert_eq!(
            result.content,
            vec![
                r#"{"rendered":"Error: bounded child host failure","failure_kind":"child_call_failed","child_kind":"invalid_arguments"}"#
            ]
        );
    }

    struct PanickingHost;

    impl CodeModeHost for PanickingHost {
        fn invoke_tool(
            &self,
            _request: CodeModeToolRequest,
        ) -> CodeModeHostFuture<'_, Result<CodeModeToolResponse, CodeModeHostError>> {
            Box::pin(async move { panic!("intentional host task panic") })
        }
    }

    #[tokio::test]
    async fn host_task_failure_preserves_exact_child_identity() {
        let error = execute(
            Arc::new(PanickingHost),
            request("await tools.fake_read({});", &["fake_read"]),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, CodeModeErrorKind::ChildCallFailed);
        let child = error
            .child_failure
            .expect("structured host-task child failure");
        assert_eq!(child.ordinal, 1);
        assert_eq!(child.tool, "fake_read");
        assert_eq!(child.failure_kind, "host_task_failure");
        assert!(child.message.contains("nested tool host task failed"));
    }

    struct BusinessFailureHost;

    impl CodeModeHost for BusinessFailureHost {
        fn invoke_tool(
            &self,
            _request: CodeModeToolRequest,
        ) -> CodeModeHostFuture<'_, Result<CodeModeToolResponse, CodeModeHostError>> {
            Box::pin(async move {
                Ok(CodeModeToolResponse {
                    success: false,
                    output: json!({
                        "reason": "known_business_failure",
                        "nested": {"items": [1, true, null, "x"]},
                        "__proto__": {"polluted": true}
                    }),
                    error: Some("business failure".to_string()),
                })
            })
        }
    }

    #[tokio::test]
    async fn ordinary_business_failure_remains_a_javascript_value() {
        let result = execute(
            Arc::new(BusinessFailureHost),
            request(
                "const r = await tools.fake_read({}); text({success:r.success, reason:r.output.reason, nested:r.output.nested.items, proto_own:Object.prototype.hasOwnProperty.call(r.output,'__proto__'), polluted:r.output.polluted ?? null});",
                &["fake_read"],
            ),
        )
        .await
        .unwrap();
        assert_eq!(
            result.content,
            vec![
                r#"{"success":false,"reason":"known_business_failure","nested":[1,true,null,"x"],"proto_own":true,"polluted":null}"#
            ]
        );
    }

    #[tokio::test]
    async fn nested_tool_call_budget_fails_boundedly() {
        let error = execute(
            Arc::new(RecordingHost::default()),
            request(
                "await Promise.all(Array.from({length: 33}, () => tools.fake_read({})));",
                &["fake_read"],
            ),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, CodeModeErrorKind::ToolCallBudgetExceeded);
        assert_eq!(error.stats.tool_calls, MAX_TOOL_CALLS);
        assert_eq!(
            error.limit,
            Some(CodeModeLimit {
                kind: "nested_tool_calls".to_string(),
                allowed: MAX_TOOL_CALLS,
                current: MAX_TOOL_CALLS,
                attempted: MAX_TOOL_CALLS + 1,
            })
        );
    }

    #[tokio::test]
    async fn unknown_tool_is_not_callable() {
        let error = execute(
            Arc::new(RecordingHost::default()),
            request("await tools.not_allowed({});", &["fake_read"]),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, CodeModeErrorKind::Runtime);
        assert!(error.message.contains("not_allowed"));
        assert!(error.child_failure.is_none());
        assert!(error.limit.is_none());
    }

    #[tokio::test]
    async fn emitted_text_is_hard_bounded() {
        let error = execute(
            Arc::new(RecordingHost::default()),
            request(&format!("text('x'.repeat({}));", MAX_OUTPUT_BYTES + 1), &[]),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, CodeModeErrorKind::OutputLimitExceeded);
        assert_eq!(error.stats.returned_bytes, 0);
        assert_eq!(
            error.limit,
            Some(CodeModeLimit {
                kind: "text_output_bytes".to_string(),
                allowed: MAX_OUTPUT_BYTES,
                current: 0,
                attempted: MAX_OUTPUT_BYTES + 1,
            })
        );
    }

    #[tokio::test]
    async fn emitted_text_item_count_is_hard_bounded() {
        let error = execute(
            Arc::new(RecordingHost::default()),
            request(
                &format!(
                    "for (let i = 0; i < {}; i++) text('');",
                    MAX_OUTPUT_ITEMS + 1
                ),
                &[],
            ),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, CodeModeErrorKind::OutputLimitExceeded);
        assert_eq!(error.stats.returned_bytes, 0);
        assert_eq!(
            error.limit,
            Some(CodeModeLimit {
                kind: "text_output_items".to_string(),
                allowed: MAX_OUTPUT_ITEMS,
                current: MAX_OUTPUT_ITEMS,
                attempted: MAX_OUTPUT_ITEMS + 1,
            })
        );
    }

    struct LifecycleHost {
        started: AtomicUsize,
        completed: AtomicUsize,
        stopped: AtomicBool,
        state_changed: Notify,
        release: Semaphore,
    }

    impl LifecycleHost {
        fn new() -> Self {
            Self {
                started: AtomicUsize::new(0),
                completed: AtomicUsize::new(0),
                stopped: AtomicBool::new(false),
                state_changed: Notify::new(),
                release: Semaphore::new(0),
            }
        }

        async fn wait_for_started(&self, expected: usize) {
            loop {
                let changed = self.state_changed.notified();
                if self.started.load(Ordering::SeqCst) >= expected {
                    return;
                }
                changed.await;
            }
        }

        async fn wait_for_stopped(&self) {
            loop {
                let changed = self.state_changed.notified();
                if self.stopped.load(Ordering::SeqCst) {
                    return;
                }
                changed.await;
            }
        }
    }

    impl CodeModeHost for LifecycleHost {
        fn invoke_tool(
            &self,
            _request: CodeModeToolRequest,
        ) -> CodeModeHostFuture<'_, Result<CodeModeToolResponse, CodeModeHostError>> {
            Box::pin(async move {
                self.started.fetch_add(1, Ordering::SeqCst);
                self.state_changed.notify_waiters();
                let permit = self.release.acquire().await.map_err(|_| {
                    CodeModeHostError::new("lifecycle test release semaphore closed")
                })?;
                permit.forget();
                self.completed.fetch_add(1, Ordering::SeqCst);
                self.state_changed.notify_waiters();
                Ok(CodeModeToolResponse {
                    success: true,
                    output: json!({"ok": true}),
                    error: None,
                })
            })
        }

        fn stop_accepting_calls(&self) {
            self.stopped.store(true, Ordering::SeqCst);
            self.state_changed.notify_waiters();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn effectful_timeout_closes_frontend_and_drains_started_host_call() {
        let host = Arc::new(LifecycleHost::new());
        let mut req = request(
            "const child = tools.effect({}); while (true) {}",
            &["effect"],
        );
        req.timeout_ms = Some(1_000);
        let host_for_execute = host.clone();
        let task = tokio::spawn(async move {
            execute_with_termination_mode_inner(
                host_for_execute,
                req,
                CodeModeTerminationMode::DrainStartedChildren {
                    max_drain_ms: 5_000,
                },
                Arc::new(Semaphore::new(1)),
                RuntimeStartupConfig::default(),
            )
            .await
        });

        host.wait_for_started(1).await;
        host.wait_for_stopped().await;
        assert_eq!(host.started.load(Ordering::SeqCst), 1);
        assert!(
            !task.is_finished(),
            "effect-aware return must wait for started host work"
        );
        host.release.add_permits(1);
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("drain must finish after the started host call completes")
            .unwrap()
            .unwrap_err();
        assert_eq!(result.kind, CodeModeErrorKind::Timeout);
        assert_eq!(host.completed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn effectful_timeout_bounds_drain_and_cancels_stuck_host_work() {
        let host = Arc::new(LifecycleHost::new());
        let mut req = request(
            "const child = tools.effect({}); while (true) {}",
            &["effect"],
        );
        req.timeout_ms = Some(50);
        let host_for_execute = host.clone();
        let task = tokio::spawn(async move {
            execute_with_termination_mode_inner(
                host_for_execute,
                req,
                CodeModeTerminationMode::DrainStartedChildren { max_drain_ms: 50 },
                Arc::new(Semaphore::new(1)),
                RuntimeStartupConfig::default(),
            )
            .await
        });

        host.wait_for_started(1).await;
        host.wait_for_stopped().await;
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("bounded effect-aware drain must not wait forever")
            .unwrap()
            .unwrap_err();
        assert_eq!(result.kind, CodeModeErrorKind::Timeout);
        assert_eq!(host.completed.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn effectful_frontend_success_does_not_hide_stuck_host_work() {
        let host = Arc::new(LifecycleHost::new());
        let req = request("tools.effect({}); text('frontend done');", &["effect"]);
        let host_for_execute = host.clone();
        let task = tokio::spawn(async move {
            execute_with_termination_mode_inner(
                host_for_execute,
                req,
                CodeModeTerminationMode::DrainStartedChildren { max_drain_ms: 50 },
                Arc::new(Semaphore::new(1)),
                RuntimeStartupConfig::default(),
            )
            .await
        });

        host.wait_for_started(1).await;
        host.wait_for_stopped().await;
        let error = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("bounded drain must terminate a frontend-success call with stuck host work")
            .unwrap()
            .unwrap_err();
        assert_eq!(error.kind, CodeModeErrorKind::Runtime);
        assert!(error.message.contains("started-child drain exceeded 50 ms"));
        assert_eq!(host.completed.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn effectful_timeout_discards_runtime_queue_before_new_host_calls_start() {
        let host = Arc::new(LifecycleHost::new());
        let mut req = request(
            r#"
            for (let i = 0; i < 9; i++) tools.effect({i});
            while (true) {}
            "#,
            &["effect"],
        );
        req.timeout_ms = Some(1_000);
        let host_for_execute = host.clone();
        let task = tokio::spawn(async move {
            execute_with_termination_mode_inner(
                host_for_execute,
                req,
                CodeModeTerminationMode::DrainStartedChildren {
                    max_drain_ms: 5_000,
                },
                Arc::new(Semaphore::new(1)),
                RuntimeStartupConfig::default(),
            )
            .await
        });

        host.wait_for_started(MAX_CONCURRENT_TOOL_CALLS).await;
        host.wait_for_stopped().await;
        assert_eq!(
            host.started.load(Ordering::SeqCst),
            MAX_CONCURRENT_TOOL_CALLS,
            "the runtime-queued ninth call must not start after frontend closure"
        );
        host.release.add_permits(MAX_CONCURRENT_TOOL_CALLS);
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("started host calls must drain")
            .unwrap()
            .unwrap_err();
        assert_eq!(result.kind, CodeModeErrorKind::Timeout);
        assert_eq!(
            host.started.load(Ordering::SeqCst),
            MAX_CONCURRENT_TOOL_CALLS
        );
        assert_eq!(
            host.completed.load(Ordering::SeqCst),
            MAX_CONCURRENT_TOOL_CALLS
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e1_timeout_does_not_wait_for_blocked_host_call() {
        let host = Arc::new(LifecycleHost::new());
        let mut req = request(
            "const child = tools.fake_read({}); while (true) {}",
            &["fake_read"],
        );
        req.timeout_ms = Some(1_000);
        let host_for_execute = host.clone();
        let task = tokio::spawn(async move {
            execute_with_termination_mode_inner(
                host_for_execute,
                req,
                CodeModeTerminationMode::ReturnAtFrontendDeadline,
                Arc::new(Semaphore::new(1)),
                RuntimeStartupConfig::default(),
            )
            .await
        });

        host.wait_for_started(1).await;
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("E1 timeout must remain the return boundary")
            .unwrap()
            .unwrap_err();
        assert_eq!(result.kind, CodeModeErrorKind::Timeout);
        assert!(!host.stopped.load(Ordering::SeqCst));
        assert_eq!(host.completed.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cpu_bound_infinite_loop_is_terminated_by_deadline() {
        let mut request = request("while (true) {}", &[]);
        request.timeout_ms = Some(50);
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            execute(Arc::new(RecordingHost::default()), request),
        )
        .await
        .expect("V8 isolate termination must stop the runtime thread")
        .unwrap_err();
        assert_eq!(result.kind, CodeModeErrorKind::Timeout);
    }

    #[tokio::test]
    async fn ambient_host_authority_is_absent() {
        let result = execute(
            Arc::new(RecordingHost::default()),
            request(
                r#"
                text({
                    console: typeof console,
                    wasm: typeof WebAssembly,
                    atomics: typeof Atomics,
                    shared: typeof SharedArrayBuffer,
                    process: typeof process,
                    deno: typeof Deno,
                    fetch: typeof fetch,
                    require: typeof require
                });
                "#,
                &[],
            ),
        )
        .await
        .unwrap();
        let emitted: JsonValue = serde_json::from_str(&result.content[0]).unwrap();
        for key in [
            "console", "wasm", "atomics", "shared", "process", "deno", "fetch", "require",
        ] {
            assert_eq!(emitted[key], "undefined", "{key}");
        }
    }
}
