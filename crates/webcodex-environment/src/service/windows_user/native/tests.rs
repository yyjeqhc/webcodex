use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn plan() -> TaskPlan {
    TaskPlan {
        id: "native-observer-fixture".into(), home: r"C:\Users\Fixture".into(),
        name: "WebCodex-native-observer-fixture".into(),
        description: "webcodex fixture scope=user".into(), sid: "S-1-5-21-1-2-3-1001".into(),
        program: r"C:\WebCodex\webcodex-runtime\server.exe".into(),
        arguments: r#""--config" "C:\Fixture\环境.toml""#.into(),
        host: Some(r"C:\WebCodex\WebCodex.exe".into()),
        host_arguments: r#"--user-task "server" "fixture" "C:\Fixture" "C:\WebCodex\webcodex-runtime\server.exe" "--config" "C:\Fixture\环境.toml""#.into(),
        directory: r"C:\Fixture".into(),
    }
}

// Real COM interfaces and setters, but never RegisterTask/Run: the definition
// stays in memory and cannot affect an installed task or start a process.
fn definition(scheduler: &ITaskService, plan: &TaskPlan) -> ITaskDefinition {
    unsafe {
        let definition = scheduler.NewTask(0).unwrap();
        definition
            .RegistrationInfo()
            .unwrap()
            .SetDescription(&BSTR::from(plan.description.as_str()))
            .unwrap();
        let principal = definition.Principal().unwrap();
        principal.SetUserId(&BSTR::from(plan.sid.as_str())).unwrap();
        principal
            .SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)
            .unwrap();
        principal.SetRunLevel(TASK_RUNLEVEL_LUA).unwrap();
        let action: IExecAction = definition
            .Actions()
            .unwrap()
            .Create(TASK_ACTION_EXEC)
            .unwrap()
            .cast()
            .unwrap();
        action.SetPath(&BSTR::from(plan.program.as_str())).unwrap();
        action
            .SetArguments(&BSTR::from(plan.arguments.as_str()))
            .unwrap();
        action
            .SetWorkingDirectory(&BSTR::from(plan.directory.as_str()))
            .unwrap();
        let trigger: ILogonTrigger = definition
            .Triggers()
            .unwrap()
            .Create(TASK_TRIGGER_LOGON)
            .unwrap()
            .cast()
            .unwrap();
        trigger.SetUserId(&BSTR::from(plan.sid.as_str())).unwrap();
        trigger.SetEnabled(VARIANT_BOOL(-1)).unwrap();
        let settings = definition.Settings().unwrap();
        settings
            .SetMultipleInstances(TASK_INSTANCES_IGNORE_NEW)
            .unwrap();
        settings.SetExecutionTimeLimit(&BSTR::from("PT0S")).unwrap();
        settings.SetAllowHardTerminate(VARIANT_BOOL(-1)).unwrap();
        settings.SetRestartCount(3).unwrap();
        settings.SetRestartInterval(&BSTR::from("PT1M")).unwrap();
        settings.SetStartWhenAvailable(VARIANT_BOOL(-1)).unwrap();
        settings
            .SetDisallowStartIfOnBatteries(VARIANT_BOOL(0))
            .unwrap();
        settings.SetStopIfGoingOnBatteries(VARIANT_BOOL(0)).unwrap();
        definition
    }
}

fn scheduler() -> ITaskService {
    let service: ITaskService =
        unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }.unwrap();
    let empty = VARIANT::default();
    unsafe { service.Connect(&empty, &empty, &empty, &empty) }.unwrap();
    service
}

#[test]
fn native_task_definition_checks_every_existing_ownership_boundary() {
    let _apartment = Apartment::new().unwrap();
    let scheduler = scheduler();
    let plan = plan();
    let deadline = Instant::now() + Duration::from_secs(10);
    let original = definition(&scheduler, &plan);
    assert!(owned_action(&original, &plan, "FIXTURE", deadline)
        .unwrap()
        .is_some());

    let changes: &[fn(&ITaskDefinition)] = &[
        |d| unsafe {
            d.RegistrationInfo()
                .unwrap()
                .SetDescription(&BSTR::from("foreign"))
                .unwrap();
        },
        |d| unsafe {
            d.Principal()
                .unwrap()
                .SetUserId(&BSTR::from("S-1-5-21-1-2-3-1002"))
                .unwrap();
        },
        |d| unsafe {
            d.Principal()
                .unwrap()
                .SetLogonType(TASK_LOGON_PASSWORD)
                .unwrap();
        },
        |d| unsafe {
            d.Principal()
                .unwrap()
                .SetRunLevel(TASK_RUNLEVEL_HIGHEST)
                .unwrap();
        },
        |d| unsafe {
            let _: IAction = d.Actions().unwrap().Create(TASK_ACTION_EXEC).unwrap();
        },
        |d| unsafe {
            let actions = d.Actions().unwrap();
            actions.Clear().unwrap();
            let _: IAction = actions.Create(TASK_ACTION_COM_HANDLER).unwrap();
        },
        |d| unsafe {
            d.Actions()
                .unwrap()
                .get_Item(1)
                .unwrap()
                .cast::<IExecAction>()
                .unwrap()
                .SetPath(&BSTR::from(r"C:\Other\server.exe"))
                .unwrap();
        },
        |d| unsafe {
            d.Actions()
                .unwrap()
                .get_Item(1)
                .unwrap()
                .cast::<IExecAction>()
                .unwrap()
                .SetArguments(&BSTR::from("--other"))
                .unwrap();
        },
        |d| unsafe {
            d.Actions()
                .unwrap()
                .get_Item(1)
                .unwrap()
                .cast::<IExecAction>()
                .unwrap()
                .SetWorkingDirectory(&BSTR::from(r"C:\Other"))
                .unwrap();
        },
        |d| unsafe {
            let _: ITrigger = d.Triggers().unwrap().Create(TASK_TRIGGER_LOGON).unwrap();
        },
        |d| unsafe {
            let triggers = d.Triggers().unwrap();
            triggers.Clear().unwrap();
            let _: ITrigger = triggers.Create(TASK_TRIGGER_TIME).unwrap();
        },
        |d| unsafe {
            d.Triggers()
                .unwrap()
                .get_Item(1)
                .unwrap()
                .SetEnabled(VARIANT_BOOL(0))
                .unwrap();
        },
        |d| unsafe {
            d.Triggers()
                .unwrap()
                .get_Item(1)
                .unwrap()
                .cast::<ILogonTrigger>()
                .unwrap()
                .SetUserId(&BSTR::from("S-1-5-21-1-2-3-1002"))
                .unwrap();
        },
        |d| unsafe {
            d.Settings()
                .unwrap()
                .SetMultipleInstances(TASK_INSTANCES_PARALLEL)
                .unwrap();
        },
        |d| unsafe {
            d.Settings()
                .unwrap()
                .SetExecutionTimeLimit(&BSTR::from("PT1H"))
                .unwrap();
        },
        |d| unsafe {
            d.Settings()
                .unwrap()
                .SetAllowHardTerminate(VARIANT_BOOL(0))
                .unwrap();
        },
        |d| unsafe {
            d.Settings().unwrap().SetRestartCount(0).unwrap();
        },
        |d| unsafe {
            d.Settings()
                .unwrap()
                .SetRestartInterval(&BSTR::from("PT2M"))
                .unwrap();
        },
        |d| unsafe {
            d.Settings()
                .unwrap()
                .SetStartWhenAvailable(VARIANT_BOOL(0))
                .unwrap();
        },
        |d| unsafe {
            d.Settings()
                .unwrap()
                .SetDisallowStartIfOnBatteries(VARIANT_BOOL(-1))
                .unwrap();
        },
        |d| unsafe {
            d.Settings()
                .unwrap()
                .SetStopIfGoingOnBatteries(VARIANT_BOOL(-1))
                .unwrap();
        },
    ];
    for (index, change) in changes.iter().enumerate() {
        let changed = definition(&scheduler, &plan);
        change(&changed);
        assert!(
            owned_action(&changed, &plan, "FIXTURE", deadline)
                .unwrap()
                .is_none(),
            "changed ownership field {index}"
        );
    }
    let hosted = definition(&scheduler, &plan);
    let action: IExecAction = unsafe {
        hosted
            .Actions()
            .unwrap()
            .get_Item(1)
            .unwrap()
            .cast()
            .unwrap()
    };
    unsafe {
        action
            .SetPath(&BSTR::from(plan.host.as_deref().unwrap()))
            .unwrap();
        action
            .SetArguments(&BSTR::from(plan.host_arguments.as_str()))
            .unwrap();
    }
    assert!(owned_action(&hosted, &plan, "FIXTURE", deadline)
        .unwrap()
        .is_some());
    let mut untrusted_host = plan.clone();
    untrusted_host.host = None;
    assert!(owned_action(&hosted, &untrusted_host, "FIXTURE", deadline)
        .unwrap()
        .is_none());
}

#[test]
fn native_scheduler_confirms_absence_by_reading_hidden_task_collection() {
    let _apartment = Apartment::new().unwrap();
    let scheduler = scheduler();
    let folder = unsafe { scheduler.GetFolder(&BSTR::from("\\")) }.unwrap();
    let name = format!("WebCodex-unregistered-test-{}", uuid::Uuid::new_v4());
    assert!(
        read_task(&folder, &name, Instant::now() + Duration::from_secs(10))
            .unwrap()
            .is_none()
    );
}

#[test]
fn native_task_name_comparison_does_not_classify_a_case_variant_as_absent() {
    assert!(process::equal_ignore_case(
        "WebCodex-user-abcd-Runner",
        "WEBCODEX-USER-ABCD-RUNNER"
    ));
    assert!(!process::equal_ignore_case(
        "WebCodex-user-abcd-Runner",
        "WebCodex-user-abce-Runner"
    ));
}

#[test]
fn native_task_absence_requires_explicit_file_not_found_not_access_or_rpc_failure() {
    assert!(missing_task_hresult(0x80070002u32 as i32));
    for code in [
        0x80070005u32,
        0x800706bau32,
        0x80004005u32,
        0x80070003u32,
        0,
    ] {
        assert!(!missing_task_hresult(code as i32));
    }
}

#[test]
fn native_worker_timeout_does_not_spawn_replacements_or_observe_expired_queue_entries() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = calls.clone();
    let (started, receive_started) = mpsc::sync_channel(1);
    let (release, wait_release) = mpsc::sync_channel(1);
    let worker = spawn_worker(move |_, _| {
        if observed_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            started.send(()).unwrap();
            wait_release.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        vec![Ok((ServiceStatus::absent("fixture"), None))]
    })
    .unwrap();
    let (response, _first_receive) = mpsc::sync_channel(1);
    assert!(worker
        .try_send(Request {
            probes: vec![],
            deadline: Instant::now() + Duration::from_secs(2),
            response,
        })
        .is_ok());
    receive_started
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    // The worker is already inside the blocked observation before starting the
    // caller deadline; thread-start scheduling cannot invalidate this fixture.
    assert!(request(&worker, vec![], Duration::from_millis(20)).is_err());
    for _ in 0..MAX_QUEUED_BATCHES - 2 {
        let (response, _) = mpsc::sync_channel(1);
        assert!(worker
            .try_send(Request {
                probes: vec![],
                deadline: Instant::now(),
                response
            })
            .is_ok());
    }
    let (response, receive) = mpsc::sync_channel(1);
    assert!(worker
        .try_send(Request {
            probes: vec![],
            deadline: Instant::now() + Duration::from_secs(1),
            response
        })
        .is_ok());
    assert!(
        request(&worker, vec![], Duration::from_millis(20)).is_err(),
        "a full queue returns unknown without waiting"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "timeouts do not replace the blocked worker"
    );
    release.send(()).unwrap();
    let result = receive.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(result[0].is_ok());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "expired queue entries are never observed"
    );
}

#[test]
fn native_plan_rejects_environment_injection_and_oversized_batches_without_observation() {
    let mut spec = super::super::super::tests::sample_spec();
    spec.environment
        .insert("NATIVE_TEST_ONLY".into(), "value".into());
    assert!(matches!(
        make_probe(&spec, true),
        Err(ServiceError {
            code: ServiceErrorCode::InvalidSpec,
            ..
        })
    ));
    let probes = vec![(&spec, true); MAX_BATCH_PROBES + 1];
    let errors = inspect_many(&probes);
    assert_eq!(errors.len(), probes.len());
    assert!(errors.iter().all(|result| matches!(
        result,
        Err(ServiceError {
            code: ServiceErrorCode::InvalidSpec,
            ..
        })
    )));
}
