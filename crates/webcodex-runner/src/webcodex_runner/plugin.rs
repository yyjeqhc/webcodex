//! Runner-owned native stdio Tool Plugin runtime.
//!
//! Startup providers are eagerly initialized exactly once and their admitted
//! catalog is frozen for the Runner process lifetime. Explicit reloads build a
//! separate dynamic overlay; they never replace startup instances or direct
//! first-class bindings.

use super::config::{load_config, PluginConfig, PluginProviderConfig, RunnerConfig, ShellConfig};
use super::shell::{PreparedExecutionEnvironment, PreparedShellProfileCache};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};
use webcodex_core::plugin::{
    validate_request, validate_startup_catalog, validate_startup_tool, validate_tool_result,
    validate_tools, PluginDispatchState, PluginGatewayRequest, PluginGatewayResponse,
    PluginGatewayResponsePayload, PluginPlane, PluginProviderView, PluginReloadFailure,
    PluginSchemaObservation, PluginTool, PluginToolResult, StartupPluginProvider,
    PLUGIN_MAX_MESSAGE_BYTES, PLUGIN_PROTOCOL_VERSION, PLUGIN_STARTUP_CATALOG_MAX_BYTES,
    PLUGIN_STARTUP_MAX_DIRECT_TOOLS,
};
use webcodex_process::ManagedChild;

const PLUGIN_READER_QUEUE: usize = 16;
const PLUGIN_WRITER_QUEUE: usize = 1;
const PLUGIN_STOP_POLL: Duration = Duration::from_millis(25);
const PLUGIN_TERMINATION_BUDGET: Duration = Duration::from_secs(1);

pub(crate) struct PluginManager {
    startup: BTreeMap<String, Arc<ProviderEntry>>,
    startup_catalog: Vec<StartupPluginProvider>,
    startup_config: PluginConfig,
    startup_shell: ShellConfig,
    dynamic: Mutex<DynamicState>,
    reload_gate: Mutex<()>,
    config_path: PathBuf,
    prepared_profiles: PreparedShellProfileCache,
    next_generation: AtomicU64,
    stopping: Arc<AtomicBool>,
}

struct DynamicState {
    overlay: BTreeMap<String, DynamicEntry>,
    first_class_restart_required: bool,
}

enum DynamicEntry {
    Provider(Arc<ProviderEntry>),
    Removed,
}

struct ProviderEntry {
    config: PluginProviderConfig,
    instance_id: String,
    timeout: Duration,
    failed: AtomicBool,
    error_code: Mutex<Option<String>>,
    process: Arc<ProviderProcess>,
    session: Mutex<Option<ProviderConnection>>,
}

struct ProviderProcess {
    child: Mutex<Option<ManagedChild>>,
}

struct ProviderConnection {
    process: Arc<ProviderProcess>,
    stopping: Arc<AtomicBool>,
    writer: mpsc::SyncSender<WriteRequest>,
    incoming: mpsc::Receiver<ReaderEvent>,
    next_id: u64,
}

struct WriteRequest {
    frame: Vec<u8>,
    ack: mpsc::Sender<WriteAck>,
}

enum WriteAck {
    Written,
    Failed,
}

enum ReaderEvent {
    Message(Result<Value, ReaderFault>),
    Eof,
}

#[derive(Clone, Copy)]
enum ReaderFault {
    Malformed,
    TooLarge,
    Io,
}

struct ProviderFailure {
    dispatch_state: PluginDispatchState,
    code: &'static str,
    fatal: bool,
}

impl ProviderFailure {
    fn not_started(code: &'static str) -> Self {
        Self {
            dispatch_state: PluginDispatchState::NotStarted,
            code,
            fatal: true,
        }
    }

    fn after_send(code: &'static str, effectful: bool) -> Self {
        Self {
            dispatch_state: if effectful {
                PluginDispatchState::OutcomeUnknown
            } else {
                PluginDispatchState::NotStarted
            },
            code,
            fatal: true,
        }
    }

    fn before_send_timeout() -> Self {
        Self {
            dispatch_state: PluginDispatchState::NotStarted,
            code: "plugin_timeout",
            fatal: false,
        }
    }

    fn completed(code: &'static str, fatal: bool) -> Self {
        Self {
            dispatch_state: PluginDispatchState::Completed,
            code,
            fatal,
        }
    }
}

impl PluginManager {
    pub(crate) fn new(startup: &RunnerConfig, config_path: PathBuf) -> Self {
        let prepared_profiles = PreparedShellProfileCache::default();
        let stopping = Arc::new(AtomicBool::new(false));
        let request_timeout = Duration::from_secs(startup.plugins.request_timeout_secs);
        let mut startup_entries = BTreeMap::new();
        let mut startup_catalog = Vec::with_capacity(startup.plugins.providers.len());
        let mut direct_tool_count = 0usize;

        for provider in &startup.plugins.providers {
            let (entry, listed_tools, failure) = prepare_provider(
                provider,
                &startup.shell,
                1,
                request_timeout,
                &prepared_profiles,
                &stopping,
            );
            let instance_id = entry.instance_id.clone();
            let mut advertised = StartupPluginProvider {
                provider_id: provider.id.clone(),
                provider_instance_id: instance_id,
                name: provider.name.clone(),
                status: if failure.is_some() {
                    "failed".to_string()
                } else {
                    "ready".to_string()
                },
                error_code: failure.clone(),
                tools: Vec::new(),
            };

            if let Some(tools) = listed_tools {
                let provider_direct_admissible =
                    tools.iter().all(|tool| validate_startup_tool(tool).is_ok())
                        && direct_tool_count.saturating_add(tools.len())
                            <= PLUGIN_STARTUP_MAX_DIRECT_TOOLS;
                if provider_direct_admissible {
                    advertised.tools = tools;
                    let mut tentative = startup_catalog.clone();
                    tentative.push(advertised.clone());
                    let within_aggregate = serde_json::to_vec(&tentative)
                        .is_ok_and(|encoded| encoded.len() <= PLUGIN_STARTUP_CATALOG_MAX_BYTES)
                        && validate_startup_catalog(&tentative).is_ok();
                    if within_aggregate {
                        direct_tool_count += advertised.tools.len();
                    } else {
                        advertised.tools.clear();
                        advertised.status = "ready_secondary".to_string();
                        advertised.error_code = Some("first_class_catalog_too_large".to_string());
                    }
                } else {
                    advertised.status = "ready_secondary".to_string();
                    advertised.error_code = Some("first_class_catalog_too_large".to_string());
                }
            }
            startup_entries.insert(provider.id.clone(), entry);
            startup_catalog.push(advertised);
        }

        debug_assert!(validate_startup_catalog(&startup_catalog).is_ok());
        Self {
            startup: startup_entries,
            startup_catalog,
            startup_config: startup.plugins.clone(),
            startup_shell: startup.shell.clone(),
            dynamic: Mutex::new(DynamicState {
                overlay: BTreeMap::new(),
                first_class_restart_required: false,
            }),
            reload_gate: Mutex::new(()),
            config_path,
            prepared_profiles,
            next_generation: AtomicU64::new(2),
            stopping,
        }
    }

    pub(crate) fn startup_catalog(&self) -> Vec<StartupPluginProvider> {
        self.startup_catalog.clone()
    }

    pub(crate) fn handle(&self, request: PluginGatewayRequest) -> PluginGatewayResponse {
        if let Err(error) = validate_request(&request) {
            tracing::warn!(error = %error, "rejected invalid Plugin gateway request");
            return gateway_error(
                PluginDispatchState::NotStarted,
                "invalid_plugin_request",
                "Plugin gateway request was invalid and was not dispatched",
            );
        }
        if self.stopping.load(Ordering::SeqCst) {
            return gateway_error(
                PluginDispatchState::NotStarted,
                "plugin_manager_stopping",
                "Plugin manager is stopping; request was not dispatched",
            );
        }
        match request {
            PluginGatewayRequest::Reload => self.reload_dynamic(),
            PluginGatewayRequest::ProvidersList => {
                PluginGatewayResponse::success(PluginGatewayResponsePayload::Providers {
                    providers: self.provider_views(),
                    first_class_restart_required: self
                        .dynamic
                        .lock()
                        .unwrap()
                        .first_class_restart_required,
                })
            }
            PluginGatewayRequest::ToolsList {
                plane,
                provider_id,
                provider_instance_id,
            } => {
                let Some(provider) =
                    self.resolve_provider(plane, &provider_id, &provider_instance_id)
                else {
                    return stale_provider();
                };
                match provider.with_connection(|connection, timeout| connection.tools_list(timeout))
                {
                    Ok(tools) => {
                        PluginGatewayResponse::success(PluginGatewayResponsePayload::Tools {
                            tools,
                        })
                    }
                    Err(error) => provider_failure_response(error),
                }
            }
            PluginGatewayRequest::ToolsCall {
                plane,
                provider_id,
                provider_instance_id,
                name,
                arguments,
                expected_schema,
            } => {
                let Some(provider) =
                    self.resolve_provider(plane, &provider_id, &provider_instance_id)
                else {
                    return stale_provider();
                };
                match provider.call_tool(&name, arguments, &expected_schema) {
                    Ok(result) => {
                        PluginGatewayResponse::success(PluginGatewayResponsePayload::ToolResult {
                            result,
                        })
                    }
                    Err(error) => provider_failure_response(error),
                }
            }
        }
    }

    fn resolve_provider(
        &self,
        plane: PluginPlane,
        provider_id: &str,
        provider_instance_id: &str,
    ) -> Option<Arc<ProviderEntry>> {
        let provider = match plane {
            PluginPlane::Startup => self.startup.get(provider_id).cloned(),
            PluginPlane::Effective => {
                let dynamic = self.dynamic.lock().unwrap();
                match dynamic.overlay.get(provider_id) {
                    Some(DynamicEntry::Provider(provider)) => Some(Arc::clone(provider)),
                    Some(DynamicEntry::Removed) => None,
                    None => self.startup.get(provider_id).cloned(),
                }
            }
        }?;
        (provider.instance_id == provider_instance_id).then_some(provider)
    }

    fn provider_views(&self) -> Vec<PluginProviderView> {
        let dynamic = self.dynamic.lock().unwrap();
        let mut ids: BTreeSet<String> = self.startup.keys().cloned().collect();
        ids.extend(dynamic.overlay.keys().cloned());
        ids.into_iter()
            .filter_map(|provider_id| {
                let (provider, plane) = match dynamic.overlay.get(&provider_id) {
                    Some(DynamicEntry::Provider(provider)) => {
                        (Arc::clone(provider), PluginPlane::Effective)
                    }
                    Some(DynamicEntry::Removed) => return None,
                    None => (
                        Arc::clone(self.startup.get(&provider_id)?),
                        PluginPlane::Startup,
                    ),
                };
                let direct_count = self
                    .startup_catalog
                    .iter()
                    .find(|entry| entry.provider_id == provider_id)
                    .map_or(0, |entry| entry.tools.len());
                Some(provider.view(plane, direct_count))
            })
            .collect()
    }

    fn reload_dynamic(&self) -> PluginGatewayResponse {
        let _reload_guard = match self.reload_gate.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => {
                return gateway_error(
                    PluginDispatchState::NotStarted,
                    "plugin_reload_busy",
                    "Another Plugin reload is already preparing candidates; this reload was not started",
                )
            }
            Err(TryLockError::Poisoned(_)) => {
                return gateway_error(
                    PluginDispatchState::NotStarted,
                    "plugin_reload_state_failed",
                    "Plugin reload state is unavailable; dynamic state was unchanged",
                )
            }
        };
        let candidate = match load_config(&self.config_path) {
            Ok(candidate) => candidate,
            Err(error) => {
                let code = if error.starts_with("failed to read config") {
                    "plugin_config_read_failed"
                } else if error.starts_with("failed to parse config") {
                    "plugin_config_parse_failed"
                } else {
                    "plugin_config_invalid"
                };
                return gateway_error(
                    PluginDispatchState::NotStarted,
                    code,
                    "Runner-owned Plugin configuration could not be loaded; dynamic state was unchanged",
                );
            }
        };
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
        let mut prepared = BTreeMap::new();
        let mut failures = Vec::new();
        for provider in &candidate.plugins.providers {
            let (entry, _tools, failure) = prepare_provider(
                provider,
                &candidate.shell,
                generation,
                Duration::from_secs(candidate.plugins.request_timeout_secs),
                &self.prepared_profiles,
                &self.stopping,
            );
            if let Some(code) = failure {
                failures.push(PluginReloadFailure {
                    provider_id: provider.id.clone(),
                    code,
                });
            } else {
                prepared.insert(provider.id.clone(), entry);
            }
        }

        let configured: BTreeSet<_> = candidate
            .plugins
            .providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect();
        let mut dynamic = self.dynamic.lock().unwrap();
        if self.stopping.load(Ordering::SeqCst) {
            drop(dynamic);
            return gateway_error(
                PluginDispatchState::NotStarted,
                "plugin_manager_stopping",
                "Plugin manager began stopping before reload commit; dynamic state was unchanged",
            );
        }
        // Replacing an entry may drop the last ProviderEntry Arc, whose Drop
        // performs bounded process-tree termination. Keep that cleanup outside
        // the dynamic-state mutex so unrelated list/describe/call operations do
        // not wait on old-provider process teardown during an otherwise atomic
        // reload commit.
        let mut retired = Vec::new();
        let previous_ids: BTreeSet<_> = self
            .startup
            .keys()
            .chain(dynamic.overlay.keys())
            .cloned()
            .collect();
        for provider_id in previous_ids {
            if !configured.contains(&provider_id) {
                if let Some(previous) = dynamic.overlay.insert(provider_id, DynamicEntry::Removed) {
                    retired.push(previous);
                }
            }
        }
        for (provider_id, provider) in prepared {
            if let Some(previous) = dynamic
                .overlay
                .insert(provider_id, DynamicEntry::Provider(provider))
            {
                retired.push(previous);
            }
        }
        dynamic.first_class_restart_required = candidate.plugins != self.startup_config
            || candidate.shell != self.startup_shell
            || dynamic
                .overlay
                .values()
                .any(|entry| matches!(entry, DynamicEntry::Provider(_) | DynamicEntry::Removed));
        let restart_required = dynamic.first_class_restart_required;
        drop(dynamic);
        drop(retired);

        PluginGatewayResponse::success(PluginGatewayResponsePayload::Reloaded {
            providers: self.provider_views(),
            failures,
            first_class_restart_required: restart_required,
        })
    }

    pub(crate) fn shutdown(&self) {
        if self.stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        for provider in self.startup.values() {
            provider.shutdown();
        }
        let dynamic_providers = {
            let dynamic = self.dynamic.lock().unwrap();
            dynamic
                .overlay
                .values()
                .filter_map(|entry| match entry {
                    DynamicEntry::Provider(provider) => Some(Arc::clone(provider)),
                    DynamicEntry::Removed => None,
                })
                .collect::<Vec<_>>()
        };
        for provider in dynamic_providers {
            provider.shutdown();
        }
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl ProviderEntry {
    fn view(&self, plane: PluginPlane, startup_direct_tool_count: usize) -> PluginProviderView {
        let failed = self.failed.load(Ordering::SeqCst);
        PluginProviderView {
            provider_id: self.config.id.clone(),
            provider_instance_id: self.instance_id.clone(),
            name: self.config.name.clone(),
            plane,
            status: if failed { "failed" } else { "ready" }.to_string(),
            error_code: self.error_code.lock().unwrap().clone(),
            startup_direct_tool_count,
        }
    }

    fn retire(&self, code: &str) {
        self.failed.store(true, Ordering::SeqCst);
        *self.error_code.lock().unwrap() = Some(code.to_string());
    }

    fn with_connection<T>(
        &self,
        f: impl FnOnce(&mut ProviderConnection, Duration) -> Result<T, ProviderFailure>,
    ) -> Result<T, ProviderFailure> {
        if self.failed.load(Ordering::SeqCst) {
            return Err(ProviderFailure::not_started("plugin_provider_unavailable"));
        }
        let mut session = match self.session.try_lock() {
            Ok(session) => session,
            Err(TryLockError::WouldBlock) => {
                return Err(ProviderFailure {
                    dispatch_state: PluginDispatchState::NotStarted,
                    code: "plugin_provider_busy",
                    fatal: false,
                })
            }
            Err(TryLockError::Poisoned(_)) => {
                self.retire("plugin_provider_state_failed");
                return Err(ProviderFailure::not_started("plugin_provider_state_failed"));
            }
        };
        let Some(connection) = session.as_mut() else {
            self.retire("plugin_provider_unavailable");
            return Err(ProviderFailure::not_started("plugin_provider_unavailable"));
        };
        let timeout = self.timeout;
        match f(connection, timeout) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.fatal {
                    self.retire(error.code);
                    self.process.terminate();
                    *session = None;
                }
                Err(error)
            }
        }
    }

    fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        expected_schema: &PluginSchemaObservation,
    ) -> Result<PluginToolResult, ProviderFailure> {
        self.with_connection(|connection, timeout| {
            let started = Instant::now();
            let tools = connection.tools_list(timeout)?;
            let Some(tool) = tools.iter().find(|tool| tool.name == name) else {
                return Err(ProviderFailure {
                    dispatch_state: PluginDispatchState::NotStarted,
                    code: "plugin_tool_unavailable",
                    fatal: false,
                });
            };
            if &tool.schema_observation() != expected_schema {
                return Err(ProviderFailure {
                    dispatch_state: PluginDispatchState::NotStarted,
                    code: "plugin_schema_changed",
                    fatal: false,
                });
            }
            let Some(remaining) = timeout
                .checked_sub(started.elapsed())
                .filter(|remaining| !remaining.is_zero())
            else {
                // The schema preflight completed successfully, so the provider
                // connection is still trustworthy, but the model-requested
                // effect must not start after the provider's total call budget
                // has already been consumed.
                return Err(ProviderFailure {
                    dispatch_state: PluginDispatchState::NotStarted,
                    code: "plugin_timeout",
                    fatal: false,
                });
            };
            connection.tools_call(name, arguments, remaining)
        })
    }

    fn shutdown(&self) {
        self.process.terminate();
        if let Ok(mut session) = self.session.try_lock() {
            *session = None;
        }
    }
}

impl Drop for ProviderEntry {
    fn drop(&mut self) {
        self.process.terminate();
    }
}

impl ProviderProcess {
    fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }

    fn install(&self, child: ManagedChild) {
        let mut slot = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        debug_assert!(slot.is_none());
        *slot = Some(child);
    }

    fn terminate(&self) {
        let child = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(mut child) = child {
            terminate_provider_process(&mut child);
        }
    }
}

impl Drop for ProviderProcess {
    fn drop(&mut self) {
        let child = self
            .child
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(mut child) = child {
            terminate_provider_process(&mut child);
        }
    }
}

fn prepare_provider(
    config: &PluginProviderConfig,
    shell: &ShellConfig,
    generation: u64,
    request_timeout: Duration,
    prepared_profiles: &PreparedShellProfileCache,
    stopping: &Arc<AtomicBool>,
) -> (Arc<ProviderEntry>, Option<Vec<PluginTool>>, Option<String>) {
    let process = Arc::new(ProviderProcess::new());
    let entry = Arc::new(ProviderEntry {
        config: config.clone(),
        instance_id: uuid::Uuid::new_v4().simple().to_string(),
        timeout: config
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(request_timeout),
        failed: AtomicBool::new(false),
        error_code: Mutex::new(None),
        process: Arc::clone(&process),
        session: Mutex::new(None),
    });
    let cwd = config
        .cwd
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let environment = match PreparedExecutionEnvironment::prepare(
        generation,
        shell,
        config.profile.as_deref(),
        &cwd,
        prepared_profiles,
        Some(stopping.as_ref()),
    ) {
        Ok(environment) => environment,
        Err(_) => {
            entry.retire("plugin_environment_prepare_failed");
            return (
                entry,
                None,
                Some("plugin_environment_prepare_failed".to_string()),
            );
        }
    };
    let mut command = match environment.native_command(&config.command, &config.args, &cwd) {
        Ok(command) => command,
        Err(error) => {
            let code = if error.starts_with("unsupported_executable_type:") {
                "plugin_executable_unsupported"
            } else {
                "plugin_executable_unavailable"
            };
            entry.retire(code);
            return (entry, None, Some(code.to_string()));
        }
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match ManagedChild::spawn(&mut command) {
        Ok(child) => child,
        Err(_) => {
            entry.retire("plugin_spawn_failed");
            return (entry, None, Some("plugin_spawn_failed".to_string()));
        }
    };
    let stdin = match child.child_mut().stdin.take() {
        Some(stdin) => stdin,
        None => {
            let _ = child.terminate_tree();
            entry.retire("plugin_stdio_unavailable");
            return (entry, None, Some("plugin_stdio_unavailable".to_string()));
        }
    };
    let stdout = match child.child_mut().stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.terminate_tree();
            entry.retire("plugin_stdio_unavailable");
            return (entry, None, Some("plugin_stdio_unavailable".to_string()));
        }
    };
    let stderr = match child.child_mut().stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = child.terminate_tree();
            entry.retire("plugin_stdio_unavailable");
            return (entry, None, Some("plugin_stdio_unavailable".to_string()));
        }
    };
    let stderr_thread_name = format!("wc-plugin-stderr-{}", config.id);
    if std::thread::Builder::new()
        .name(stderr_thread_name)
        .spawn(move || {
            let mut stderr = stderr;
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        })
        .is_err()
    {
        let _ = child.terminate_tree();
        entry.retire("plugin_reader_unavailable");
        return (entry, None, Some("plugin_reader_unavailable".to_string()));
    }
    let (sender, incoming) = mpsc::sync_channel(PLUGIN_READER_QUEUE);
    let stdout_thread_name = format!("wc-plugin-{}", config.id);
    if std::thread::Builder::new()
        .name(stdout_thread_name)
        .spawn(move || provider_stdout_reader(stdout, sender))
        .is_err()
    {
        let _ = child.terminate_tree();
        entry.retire("plugin_reader_unavailable");
        return (entry, None, Some("plugin_reader_unavailable".to_string()));
    }
    let (writer, write_requests) = mpsc::sync_channel(PLUGIN_WRITER_QUEUE);
    let stdin_thread_name = format!("wc-plugin-stdin-{}", config.id);
    if std::thread::Builder::new()
        .name(stdin_thread_name)
        .spawn(move || provider_stdin_writer(stdin, write_requests))
        .is_err()
    {
        let _ = child.terminate_tree();
        entry.retire("plugin_writer_unavailable");
        return (entry, None, Some("plugin_writer_unavailable".to_string()));
    }
    process.install(child);
    if stopping.load(Ordering::SeqCst) {
        process.terminate();
        entry.retire("plugin_manager_stopping");
        return (entry, None, Some("plugin_manager_stopping".to_string()));
    }
    let mut connection = ProviderConnection {
        process: Arc::clone(&process),
        stopping: Arc::clone(stopping),
        writer,
        incoming,
        next_id: 1,
    };
    let timeout = config
        .timeout_secs
        .map(Duration::from_secs)
        .unwrap_or(request_timeout);
    if let Err(failure) = connection.initialize(timeout) {
        process.terminate();
        entry.retire(failure.code);
        return (entry, None, Some(failure.code.to_string()));
    }
    let tools = match connection.tools_list(timeout) {
        Ok(tools) => tools,
        Err(failure) => {
            process.terminate();
            entry.retire(failure.code);
            return (entry, None, Some(failure.code.to_string()));
        }
    };
    *entry.session.lock().unwrap() = Some(connection);
    (entry, Some(tools), None)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginToolsListResult {
    tools: Vec<PluginTool>,
}

impl ProviderConnection {
    fn initialize(&mut self, timeout: Duration) -> Result<(), ProviderFailure> {
        let result = self.request(
            "initialize",
            json!({"protocolVersion": PLUGIN_PROTOCOL_VERSION}),
            timeout,
            false,
        )?;
        webcodex_core::plugin::validate_json_value(
            &result,
            PLUGIN_MAX_MESSAGE_BYTES,
            "plugin initialize result",
        )
        .map_err(|_| ProviderFailure::not_started("plugin_initialize_invalid"))?;
        if result
            .as_object()
            .and_then(|object| object.get("protocolVersion"))
            .and_then(Value::as_str)
            != Some(PLUGIN_PROTOCOL_VERSION)
        {
            return Err(ProviderFailure::not_started(
                "plugin_protocol_version_mismatch",
            ));
        }
        Ok(())
    }

    fn tools_list(&mut self, timeout: Duration) -> Result<Vec<PluginTool>, ProviderFailure> {
        let result = self.request("tools/list", json!({}), timeout, false)?;
        let result: PluginToolsListResult = serde_json::from_value(result)
            .map_err(|_| ProviderFailure::not_started("plugin_tools_list_invalid"))?;
        validate_tools(&result.tools)
            .map_err(|_| ProviderFailure::not_started("plugin_tools_list_invalid"))?;
        Ok(result.tools)
    }

    fn tools_call(
        &mut self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<PluginToolResult, ProviderFailure> {
        let result = self.request(
            "tools/call",
            json!({"name": name, "arguments": arguments}),
            timeout,
            true,
        )?;
        let result: PluginToolResult = serde_json::from_value(result)
            .map_err(|_| ProviderFailure::completed("plugin_result_invalid", true))?;
        validate_tool_result(&result)
            .map_err(|_| ProviderFailure::completed("plugin_result_invalid", true))?;
        Ok(result)
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        effectful: bool,
    ) -> Result<Value, ProviderFailure> {
        // The deadline is created before any request-side validation or
        // serialization. Queue admission, the complete write+flush ack, and the
        // response wait all consume this same absolute budget.
        let deadline = Instant::now() + timeout;
        match self.incoming.try_recv() {
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) | Ok(ReaderEvent::Eof) => {
                return Err(ProviderFailure::not_started("plugin_eof"));
            }
            Ok(ReaderEvent::Message(Ok(_))) => {
                return Err(ProviderFailure::not_started("plugin_unexpected_message"));
            }
            Ok(ReaderEvent::Message(Err(fault))) => {
                return Err(ProviderFailure::not_started(reader_fault_code(fault)));
            }
        }

        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut encoded = serde_json::to_vec(&message)
            .map_err(|_| ProviderFailure::not_started("plugin_request_invalid"))?;
        if encoded.len() > PLUGIN_MAX_MESSAGE_BYTES {
            return Err(ProviderFailure::not_started("plugin_request_too_large"));
        }
        encoded.push(b'\n');
        if deadline.saturating_duration_since(Instant::now()).is_zero() {
            return Err(ProviderFailure::before_send_timeout());
        }

        let (ack_sender, ack_receiver) = mpsc::channel();
        let mut write_request = WriteRequest {
            frame: encoded,
            ack: ack_sender,
        };
        loop {
            if self.stopping.load(Ordering::SeqCst) {
                return Err(ProviderFailure::not_started("plugin_manager_stopping"));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ProviderFailure::before_send_timeout());
            }
            match self.writer.try_send(write_request) {
                Ok(()) => break,
                Err(mpsc::TrySendError::Full(request)) => {
                    write_request = request;
                    std::thread::sleep(Duration::from_millis(1).min(remaining));
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err(ProviderFailure::not_started("plugin_stdin_failed"));
                }
            }
        }

        // Once the bounded queue accepted the frame, an effectful request may
        // already have started writing. A timeout or transport failure before
        // the full write+flush ack is therefore OutcomeUnknown for effects.
        loop {
            if self.stopping.load(Ordering::SeqCst) {
                self.process.terminate();
                return Err(ProviderFailure::after_send(
                    "plugin_manager_stopping",
                    effectful,
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ProviderFailure::after_send("plugin_timeout", effectful));
            }
            match ack_receiver.recv_timeout(remaining.min(PLUGIN_STOP_POLL)) {
                Ok(WriteAck::Written) => break,
                Ok(WriteAck::Failed) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(ProviderFailure::after_send(
                        "plugin_stdin_failed",
                        effectful,
                    ));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }

        loop {
            if self.stopping.load(Ordering::SeqCst) {
                self.process.terminate();
                return Err(ProviderFailure::after_send(
                    "plugin_manager_stopping",
                    effectful,
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ProviderFailure::after_send("plugin_timeout", effectful));
            }
            let response = match self.incoming.recv_timeout(remaining.min(PLUGIN_STOP_POLL)) {
                Ok(ReaderEvent::Message(Ok(response))) => response,
                Ok(ReaderEvent::Message(Err(fault))) => {
                    return Err(ProviderFailure::after_send(
                        reader_fault_code(fault),
                        effectful,
                    ));
                }
                Ok(ReaderEvent::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(ProviderFailure::after_send("plugin_eof", effectful));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            };
            return validate_rpc_response(response, id, effectful);
        }
    }
}

fn terminate_provider_process(child: &mut ManagedChild) {
    let deadline = Instant::now() + PLUGIN_TERMINATION_BUDGET;
    let _ = child.terminate_tree();
    let _ = child.wait_tree_exit(deadline.saturating_duration_since(Instant::now()));
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10).min(remaining));
            }
        }
    }
}

fn provider_stdin_writer(mut stdin: ChildStdin, requests: mpsc::Receiver<WriteRequest>) {
    while let Ok(request) = requests.recv() {
        let result = stdin.write_all(&request.frame).and_then(|_| stdin.flush());
        let failed = result.is_err();
        let _ = request.ack.send(if failed {
            WriteAck::Failed
        } else {
            WriteAck::Written
        });
        if failed {
            return;
        }
    }
}

fn provider_stdout_reader(
    stdout: std::process::ChildStdout,
    sender: mpsc::SyncSender<ReaderEvent>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let message = match read_bounded_line(&mut reader) {
            Ok(Some(message)) => ReaderEvent::Message(
                serde_json::from_slice(&message).map_err(|_| ReaderFault::Malformed),
            ),
            Ok(None) => ReaderEvent::Eof,
            Err(fault) => ReaderEvent::Message(Err(fault)),
        };
        let terminal = !matches!(&message, ReaderEvent::Message(Ok(_)));
        if sender.send(message).is_err() || terminal {
            return;
        }
    }
}

fn read_bounded_line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, ReaderFault> {
    let mut line = Vec::new();
    let read = (&mut *reader)
        .take((PLUGIN_MAX_MESSAGE_BYTES + 2) as u64)
        .read_until(b'\n', &mut line)
        .map_err(|_| ReaderFault::Io)?;
    if read == 0 {
        return Ok(None);
    }
    if line.last() != Some(&b'\n') {
        return Err(if line.len() > PLUGIN_MAX_MESSAGE_BYTES {
            ReaderFault::TooLarge
        } else {
            ReaderFault::Malformed
        });
    }
    line.pop();
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    if line.is_empty() || line.len() > PLUGIN_MAX_MESSAGE_BYTES {
        return Err(if line.len() > PLUGIN_MAX_MESSAGE_BYTES {
            ReaderFault::TooLarge
        } else {
            ReaderFault::Malformed
        });
    }
    Ok(Some(line))
}

fn validate_rpc_response(
    response: Value,
    expected_id: u64,
    effectful: bool,
) -> Result<Value, ProviderFailure> {
    let object = response
        .as_object()
        .ok_or_else(|| ProviderFailure::after_send("plugin_protocol_error", effectful))?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || object.contains_key("method")
    {
        return Err(ProviderFailure::after_send(
            "plugin_protocol_error",
            effectful,
        ));
    }
    let response_id = object
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| ProviderFailure::after_send("plugin_response_id_invalid", effectful))?;
    if response_id != expected_id {
        return Err(ProviderFailure::after_send(
            if response_id < expected_id {
                "plugin_duplicate_response_id"
            } else {
                "plugin_unknown_response_id"
            },
            effectful,
        ));
    }
    match (object.get("result"), object.get("error")) {
        (Some(result), None) => Ok(result.clone()),
        (None, Some(Value::Object(error)))
            if error.get("code").and_then(Value::as_i64).is_some()
                && error
                    .get("message")
                    .and_then(Value::as_str)
                    .is_some_and(|message| !message.is_empty() && message.len() <= 512) =>
        {
            Err(ProviderFailure::completed("plugin_rpc_error", false))
        }
        _ => Err(ProviderFailure::after_send(
            "plugin_protocol_error",
            effectful,
        )),
    }
}

fn reader_fault_code(fault: ReaderFault) -> &'static str {
    match fault {
        ReaderFault::Malformed => "plugin_malformed_json",
        ReaderFault::TooLarge => "plugin_message_too_large",
        ReaderFault::Io => "plugin_stdout_failed",
    }
}

fn provider_failure_response(error: ProviderFailure) -> PluginGatewayResponse {
    let message = match error.dispatch_state {
        PluginDispatchState::NotStarted => {
            "Plugin request was not started; no downstream effect was dispatched"
        }
        PluginDispatchState::OutcomeUnknown => {
            "Plugin request may have been dispatched; outcome is unknown and must not be retried automatically"
        }
        PluginDispatchState::Completed => {
            "Plugin completed the request-response exchange but returned an error or invalid bounded result"
        }
    };
    gateway_error(error.dispatch_state, error.code, message)
}

fn gateway_error(
    state: PluginDispatchState,
    code: &'static str,
    message: &'static str,
) -> PluginGatewayResponse {
    PluginGatewayResponse::error(state, code, message)
}

fn stale_provider() -> PluginGatewayResponse {
    gateway_error(
        PluginDispatchState::NotStarted,
        "stale_plugin_provider",
        "Exact Plugin provider instance is unavailable; request was not routed elsewhere",
    )
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;
