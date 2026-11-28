//! Fresh, read-only Task Scheduler observations. No shell, WMI, or PS fallback.
//!
//! COM can block in a local RPC. Exactly one worker and a bounded queue contain
//! that failure: callers have a deadline, expired requests are discarded, and a
//! timed-out worker is never replaced. A permanently blocked COM call therefore
//! leaves observations unknown until this process is restarted, not an unbounded
//! number of abandoned threads. COM interfaces never cross the worker boundary.
use super::{
    observed_status, task_plan, ServiceError, ServiceErrorCode, ServiceSpec, ServiceStatus,
    TaskPlan,
};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};
use windows::core::{Interface, BSTR};
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::TaskScheduler::*;
use windows::Win32::System::Variant::VARIANT;

mod process;
#[cfg(test)]
mod tests;

type Observation = Result<(ServiceStatus, Option<u32>), ServiceError>;
const MAX_BATCH_PROBES: usize = 18; // Server, Runner, and sixteen Tunnel profiles.
const MAX_QUEUED_BATCHES: usize = 8;
const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(55);
const MAX_TASKS: i32 = 4096;
const MAX_TEXT: usize = 32767;

struct Probe {
    plan: TaskPlan,
    with_pid: bool,
}

struct Request {
    probes: Vec<Result<Probe, ServiceError>>,
    deadline: Instant,
    response: mpsc::SyncSender<Vec<Observation>>,
}

fn unknown() -> ServiceError {
    ServiceError::new(
        ServiceErrorCode::OutcomeUnknown,
        "Native Task Scheduler observation unavailable or incomplete; inspect again before control",
    )
}

fn check_deadline(deadline: Instant) -> Result<(), ServiceError> {
    if Instant::now() < deadline {
        Ok(())
    } else {
        Err(unknown())
    }
}

fn make_probe(spec: &ServiceSpec, with_pid: bool) -> Result<Probe, ServiceError> {
    if !spec.environment.is_empty() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "User Task Scheduler does not inject environment values",
        ));
    }
    let plan = task_plan(spec)?;
    let fields = [
        &plan.id,
        &plan.name,
        &plan.description,
        &plan.sid,
        &plan.program,
        &plan.arguments,
        &plan.host_arguments,
        &plan.directory,
    ];
    let size = fields
        .iter()
        .map(|value| value.encode_utf16().count())
        .sum::<usize>()
        + plan
            .host
            .as_deref()
            .map_or(0, |host| host.encode_utf16().count())
        + plan
            .home
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .count();
    if size > 24_000
        || fields.iter().any(|value| value.contains('\0'))
        || plan.host.as_deref().is_some_and(|host| host.contains('\0'))
    {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Task plan exceeds its native input budget or contains a NUL",
        ));
    }
    Ok(Probe { plan, with_pid })
}

pub(super) fn inspect_many(probes: &[(&ServiceSpec, bool)]) -> Vec<Observation> {
    if probes.is_empty() {
        return Vec::new();
    }
    if probes.len() > MAX_BATCH_PROBES {
        return probes
            .iter()
            .map(|_| {
                Err(ServiceError::new(
                    ServiceErrorCode::InvalidSpec,
                    "Task observation batch exceeds its native input budget",
                ))
            })
            .collect();
    }
    let prepared = probes
        .iter()
        .map(|(spec, pid)| make_probe(spec, *pid))
        .collect();
    static WORKER: OnceLock<Result<mpsc::SyncSender<Request>, ()>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| spawn_worker(observe_batch).map_err(|_| ()));
    let result = match worker {
        Ok(worker) => request(worker, prepared, OBSERVATION_TIMEOUT),
        Err(()) => Err(unknown()),
    };
    result.unwrap_or_else(|error| probes.iter().map(|_| Err(error.clone())).collect())
}

fn request(
    worker: &mpsc::SyncSender<Request>,
    probes: Vec<Result<Probe, ServiceError>>,
    timeout: Duration,
) -> Result<Vec<Observation>, ServiceError> {
    let deadline = Instant::now() + timeout;
    let (response, receive) = mpsc::sync_channel(1);
    // Never block trying to enqueue behind a stalled COM call.
    worker
        .try_send(Request {
            probes,
            deadline,
            response,
        })
        .map_err(|_| unknown())?;
    let result = receive
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| unknown())?;
    check_deadline(deadline)?;
    Ok(result)
}

fn spawn_worker(
    mut observe: impl FnMut(Vec<Result<Probe, ServiceError>>, Instant) -> Vec<Observation>
        + Send
        + 'static,
) -> std::io::Result<mpsc::SyncSender<Request>> {
    let (send, receive) = mpsc::sync_channel::<Request>(MAX_QUEUED_BATCHES);
    std::thread::Builder::new()
        .name("webcodex-task-observer".into())
        .spawn(move || {
            while let Ok(request) = receive.recv() {
                if check_deadline(request.deadline).is_err() {
                    continue;
                }
                let result = observe(request.probes, request.deadline);
                if check_deadline(request.deadline).is_ok() {
                    let _ = request.response.try_send(result);
                }
            }
        })?;
    Ok(send)
}

struct Apartment;
impl Apartment {
    fn new() -> Result<Self, ServiceError> {
        // This is our dedicated thread, never a borrowed UI/STA apartment.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }.map_err(|_| unknown())?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

struct Context {
    _scheduler: ITaskService,
    folder: ITaskFolder,
    owner: super::super::CurrentAccount,
    computer: String,
}

fn context() -> Result<Context, ServiceError> {
    let owner = super::super::current_account()?;
    let computer = process::computer_name().ok_or_else(unknown)?;
    let scheduler: ITaskService =
        unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }
            .map_err(|_| unknown())?;
    let empty = VARIANT::default();
    unsafe { scheduler.Connect(&empty, &empty, &empty, &empty) }.map_err(|_| unknown())?;
    let folder = unsafe { scheduler.GetFolder(&BSTR::from("\\")) }.map_err(|_| unknown())?;
    Ok(Context {
        _scheduler: scheduler,
        folder,
        owner,
        computer,
    })
}

fn observe_batch(probes: Vec<Result<Probe, ServiceError>>, deadline: Instant) -> Vec<Observation> {
    let apartment = Apartment::new();
    // Apartment is dropped only after every COM object (including Context).
    let context = apartment
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|_| context());
    probes.into_iter().map(|probe| {
        let probe = probe?;
        check_deadline(deadline)?;
        let context = context.as_ref().map_err(Clone::clone)?;
        if probe.plan.sid != context.owner.identity || probe.plan.home != context.owner.home {
            return Err(ServiceError::new(ServiceErrorCode::PermissionDenied,
                "Run this user service operation as the saved environment owner; no system fallback is allowed"));
        }
        observe(context, &probe, deadline)
    }).collect()
}

fn read_task(
    folder: &ITaskFolder,
    name: &str,
    deadline: Instant,
) -> Result<Option<IRegisteredTask>, ServiceError> {
    check_deadline(deadline)?;
    match unsafe { folder.GetTask(&BSTR::from(name)) } {
        Ok(task) => return Ok(Some(task)),
        Err(error) if missing_task_hresult(error.code().0) => {}
        Err(_) => return Err(unknown()),
    }
    // Enumeration may omit inaccessible tasks. It can corroborate an explicit
    // file-not-found result, but must never turn access/RPC failures into absence.
    let tasks = unsafe { folder.GetTasks(TASK_ENUM_HIDDEN.0) }.map_err(|_| unknown())?;
    let count = unsafe { tasks.Count() }.map_err(|_| unknown())?;
    if !(0..=MAX_TASKS).contains(&count) {
        return Err(unknown());
    }
    for index in 1..=count {
        check_deadline(deadline)?;
        let task = unsafe { tasks.get_Item(&VARIANT::from(index)) }.map_err(|_| unknown())?;
        if process::equal_ignore_case(&text(unsafe { task.Name() })?, name) {
            return Err(unknown());
        }
    }
    if unsafe { tasks.Count() }.map_err(|_| unknown())? != count {
        return Err(unknown());
    }
    Ok(None)
}

fn missing_task_hresult(code: i32) -> bool {
    code == 0x80070002u32 as i32 // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)
}

fn out<T: Default>(
    get: impl FnOnce(*mut T) -> windows::core::Result<()>,
) -> Result<T, ServiceError> {
    let mut value = T::default();
    get(&mut value).map_err(|_| unknown())?;
    Ok(value)
}

fn text(value: windows::core::Result<BSTR>) -> Result<String, ServiceError> {
    let value = value.map_err(|_| unknown())?;
    if value.len() > MAX_TEXT || value.iter().any(|ch| *ch == 0) {
        return Err(unknown());
    }
    String::from_utf16(&value).map_err(|_| unknown())
}

fn string(
    get: impl FnOnce(*mut BSTR) -> windows::core::Result<()>,
) -> Result<String, ServiceError> {
    let value = out(get)?;
    text(Ok(value))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Action {
    program: String,
    arguments: String,
}

fn owned_action(
    definition: &ITaskDefinition,
    plan: &TaskPlan,
    computer: &str,
    deadline: Instant,
) -> Result<Option<Action>, ServiceError> {
    check_deadline(deadline)?;
    let info = unsafe { definition.RegistrationInfo() }.map_err(|_| unknown())?;
    let principal = unsafe { definition.Principal() }.map_err(|_| unknown())?;
    if string(|v| unsafe { info.Description(v) })? != plan.description
        || !process::account_matches(
            &string(|v| unsafe { principal.UserId(v) })?,
            &plan.sid,
            computer,
        )
        || out(|v| unsafe { principal.LogonType(v) })? != TASK_LOGON_INTERACTIVE_TOKEN
        || out(|v| unsafe { principal.RunLevel(v) })? != TASK_RUNLEVEL_LUA
    {
        return Ok(None);
    }
    let actions = unsafe { definition.Actions() }.map_err(|_| unknown())?;
    if out(|v| unsafe { actions.Count(v) })? != 1 {
        return Ok(None);
    }
    let action = unsafe { actions.get_Item(1) }.map_err(|_| unknown())?;
    if out(|v| unsafe { action.Type(v) })? != TASK_ACTION_EXEC {
        return Ok(None);
    }
    let action: IExecAction = action.cast().map_err(|_| unknown())?;
    let observed = Action {
        program: string(|v| unsafe { action.Path(v) })?,
        arguments: string(|v| unsafe { action.Arguments(v) })?,
    };
    let direct = observed.program == plan.program && observed.arguments == plan.arguments;
    let hosted =
        plan.host.as_ref() == Some(&observed.program) && observed.arguments == plan.host_arguments;
    if (!direct && !hosted) || string(|v| unsafe { action.WorkingDirectory(v) })? != plan.directory
    {
        return Ok(None);
    }
    check_deadline(deadline)?;
    let triggers = unsafe { definition.Triggers() }.map_err(|_| unknown())?;
    if out(|v| unsafe { triggers.Count(v) })? != 1 {
        return Ok(None);
    }
    let trigger = unsafe { triggers.get_Item(1) }.map_err(|_| unknown())?;
    if out(|v| unsafe { trigger.Type(v) })? != TASK_TRIGGER_LOGON
        || !boolean(|v| unsafe { trigger.Enabled(v) })?
    {
        return Ok(None);
    }
    let trigger: ILogonTrigger = trigger.cast().map_err(|_| unknown())?;
    if !process::account_matches(
        &string(|v| unsafe { trigger.UserId(v) })?,
        &plan.sid,
        computer,
    ) {
        return Ok(None);
    }
    let settings = unsafe { definition.Settings() }.map_err(|_| unknown())?;
    if out(|v| unsafe { settings.MultipleInstances(v) })? != TASK_INSTANCES_IGNORE_NEW
        || string(|v| unsafe { settings.ExecutionTimeLimit(v) })? != "PT0S"
        || !boolean(|v| unsafe { settings.AllowHardTerminate(v) })?
        || out(|v| unsafe { settings.RestartCount(v) })? != 3
        || string(|v| unsafe { settings.RestartInterval(v) })? != "PT1M"
        || !boolean(|v| unsafe { settings.StartWhenAvailable(v) })?
        || boolean(|v| unsafe { settings.DisallowStartIfOnBatteries(v) })?
        || boolean(|v| unsafe { settings.StopIfGoingOnBatteries(v) })?
    {
        return Ok(None);
    }
    Ok(Some(observed))
}

fn boolean(
    get: impl FnOnce(*mut VARIANT_BOOL) -> windows::core::Result<()>,
) -> Result<bool, ServiceError> {
    Ok(out(get)?.0 != 0)
}

fn observe(context: &Context, probe: &Probe, deadline: Instant) -> Observation {
    let plan = &probe.plan;
    let Some(task) = read_task(&context.folder, &plan.name, deadline)? else {
        return Ok((ServiceStatus::absent(&plan.id), None));
    };
    let definition = unsafe { task.Definition() }.map_err(|_| unknown())?;
    let action = owned_action(&definition, plan, &context.computer, deadline)?;
    let state = unsafe { task.State() }.map_err(|_| unknown())?;
    let enabled = unsafe { task.Enabled() }.map_err(|_| unknown())?.0 != 0;
    let last_result = unsafe { task.LastTaskResult() }.map_err(|_| unknown())?;
    let status = observed_status(
        &plan.id,
        action.is_some(),
        Some(i64::from(state.0)),
        Some(enabled),
        Some(i64::from(last_result)),
    );
    let pid = if probe.with_pid && state == TASK_STATE_RUNNING {
        match action {
            Some(action) => running_process(context, &task, &action, plan, deadline)?,
            None => None,
        }
    } else {
        None
    };
    check_deadline(deadline)?;
    Ok((status, pid))
}

#[derive(Debug, PartialEq, Eq)]
struct Instance {
    guid: uuid::Uuid,
    engine_pid: u32,
}

fn instance(task: &IRegisteredTask) -> Result<Option<Instance>, ServiceError> {
    let instances = unsafe { task.GetInstances(0) }.map_err(|_| unknown())?;
    if unsafe { instances.Count() }.map_err(|_| unknown())? != 1 {
        return Ok(None);
    }
    let instance = unsafe { instances.get_Item(&VARIANT::from(1)) }.map_err(|_| unknown())?;
    if unsafe { instance.State() }.map_err(|_| unknown())? != TASK_STATE_RUNNING {
        return Ok(None);
    }
    let guid = uuid::Uuid::parse_str(&text(unsafe { instance.InstanceGuid() })?)
        .ok()
        .filter(|id| !id.is_nil());
    let engine_pid = unsafe { instance.EnginePID() }.map_err(|_| unknown())?;
    Ok(guid
        .filter(|_| engine_pid != 0)
        .map(|guid| Instance { guid, engine_pid }))
}

fn running_process(
    context: &Context,
    task: &IRegisteredTask,
    action: &Action,
    plan: &TaskPlan,
    deadline: Instant,
) -> Result<Option<u32>, ServiceError> {
    let Some(initial) = instance(task)? else {
        return Ok(None);
    };
    // EnginePID may be taskeng/svchost. Never search the machine for some other
    // matching action: it must itself be our exact process (or packaged host).
    let Some(host) = process::Process::open(initial.engine_pid) else {
        return Ok(None);
    };
    let hosted = plan.host.as_ref() == Some(&action.program);
    let worker_action = Action {
        program: plan.program.clone(),
        arguments: plan.arguments.clone(),
    };
    let mut first_host = None;
    let mut first_worker = None;
    let mut pinned_worker = None;
    for _ in 0..2 {
        check_deadline(deadline)?;
        let Some(host_read) = host.read() else {
            return Ok(None);
        };
        if !host_read.matches(action, &plan.sid) {
            return Ok(None);
        }
        if first_host.is_some_and(|birth| birth != host_read.created) {
            return Ok(None);
        }
        first_host = Some(host_read.created);
        let worker_read = if hosted {
            let Some(worker) =
                process::matching_child(initial.engine_pid, &worker_action, deadline)
            else {
                return Ok(None);
            };
            let Some(observed) = worker.read() else {
                return Ok(None);
            };
            if !observed.matches(&worker_action, &plan.sid) || observed.created < host_read.created
            {
                return Ok(None);
            }
            pinned_worker = Some(worker);
            observed
        } else {
            host_read
        };
        let identity = (worker_read.pid, worker_read.created);
        if first_worker.is_some_and(|first| first != identity) {
            return Ok(None);
        }
        first_worker = Some(identity);
    }
    let Some(current) = read_task(&context.folder, &plan.name, deadline)? else {
        return Ok(None);
    };
    let definition = unsafe { current.Definition() }.map_err(|_| unknown())?;
    if owned_action(&definition, plan, &context.computer, deadline)?.as_ref() != Some(action)
        || unsafe { current.State() }.map_err(|_| unknown())? != TASK_STATE_RUNNING
        || instance(&current)?.as_ref() != Some(&initial)
        || !host.is_alive()
        || pinned_worker
            .as_ref()
            .is_some_and(|worker| !worker.is_alive())
    {
        return Ok(None);
    }
    Ok(first_worker.map(|(pid, _)| pid))
}
