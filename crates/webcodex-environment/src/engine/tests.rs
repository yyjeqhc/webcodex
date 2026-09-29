use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
struct Host {
    completed: BTreeSet<SetupStep>,
    effects: Vec<SetupStep>,
    fail_after: Option<SetupStep>,
    lose_enrollment: bool,
}
impl EnvironmentBackend for Host {
    async fn reconcile(
        &mut self,
        _: &EnvironmentStore,
        _: &mut EnvironmentRecord,
        step: SetupStep,
    ) -> SetupResultValue<Reconciliation> {
        Ok(if self.completed.contains(&step) {
            Reconciliation::Complete
        } else {
            Reconciliation::Missing
        })
    }
    async fn apply(
        &mut self,
        _: &EnvironmentStore,
        _: &mut EnvironmentRecord,
        step: SetupStep,
        _: &SetupSecrets,
    ) -> SetupResultValue<()> {
        self.effects.push(step);
        if !(self.lose_enrollment && step == SetupStep::RunnerEnrollment) {
            self.completed.insert(step);
        }
        if self.fail_after == Some(step) {
            self.fail_after = None;
            return Err(SetupDiagnostic::new(
                "interrupted",
                "Operation response was lost",
                "Reconcile",
            ));
        }
        Ok(())
    }
    async fn observe(
        &mut self,
        _: &EnvironmentStore,
        record: &EnvironmentRecord,
    ) -> SetupResultValue<RuntimeObservation> {
        Ok(RuntimeObservation {
            server_reachable: true,
            authenticated: true,
            runner_online: record.request.local_runner().then_some(true),
            ..Default::default()
        })
    }
}
fn request(create: bool, project: bool) -> SetupRequest {
    SetupRequest {
        service_scope: service::ServiceScope::System,
        mode: if create {
            EnvironmentMode::Create {
                listen: "127.0.0.1:8080".into(),
            }
        } else {
            EnvironmentMode::Join
        },
        server_url: "http://127.0.0.1:8080".into(),
        project: project.then(|| "/projects/one".into()),
        runner: None,
        account: LocalAccount {
            name: "alice".into(),
            identity: "1000".into(),
            home: "/home/alice".into(),
        },
        binaries: RuntimeBinaries {
            cli: "/bin/webcodex".into(),
            server: "/bin/webcodex-server".into(),
            runner: "/bin/webcodex-runner".into(),
        },
    }
}

#[tokio::test]
async fn four_paths_share_one_core_and_viewers_never_enroll() {
    for create in [false, true] {
        for project in [false, true] {
            let dir = crate::test_tempdir().unwrap();
            let store = EnvironmentStore::open(dir.path().join("environment")).unwrap();
            let intent = request(create, project);
            let mut setup = EnvironmentSetup::new(Host::default());
            let result = setup
                .configure(&store, intent.clone(), &SetupSecrets::default(), |_| {})
                .await
                .unwrap();
            assert_eq!(result.observation.runner_online, project.then_some(true));
            assert_eq!(
                setup.backend.effects.contains(&SetupStep::RunnerEnrollment),
                project && !create
            );
            assert_eq!(
                setup
                    .backend
                    .effects
                    .contains(&SetupStep::ServerServiceInstall),
                create
            );
            let effects = setup.backend.effects.clone();
            setup
                .configure(&store, intent, &SetupSecrets::default(), |_| {})
                .await
                .unwrap();
            assert_eq!(setup.backend.effects, effects);
        }
    }
}

#[tokio::test]
async fn projectless_runners_install_without_registering_a_default_project() {
    for create in [false, true] {
        let dir = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(dir.path().join("environment")).unwrap();
        let mut intent = request(create, false);
        intent.runner = Some(true);
        let mut setup = EnvironmentSetup::new(Host::default());
        let result = setup
            .configure(&store, intent, &SetupSecrets::default(), |_| {})
            .await
            .unwrap();
        assert_eq!(result.observation.runner_online, Some(true));
        assert!(result.environment.projects.is_empty());
        assert!(setup
            .backend
            .effects
            .contains(&SetupStep::RunnerConfiguration));
        assert!(setup
            .backend
            .effects
            .contains(&SetupStep::RunnerServiceStart));
        assert!(!setup
            .backend
            .effects
            .contains(&SetupStep::ProjectRegistration));
        assert_eq!(
            setup.backend.effects.contains(&SetupStep::RunnerEnrollment),
            !create
        );
        let effects = setup.backend.effects.clone();
        setup
            .resume(&store, &SetupSecrets::default(), |_| {})
            .await
            .unwrap();
        assert_eq!(setup.backend.effects, effects);
    }
}

#[test]
fn old_setup_journals_preserve_their_machine_role() {
    for create in [false, true] {
        for project in [false, true] {
            let original = request(create, project);
            let value = serde_json::to_value(&original).unwrap();
            assert!(value.get("runner").is_none());
            let restored: SetupRequest = serde_json::from_value(value).unwrap();
            assert_eq!(restored, original);
            assert_eq!(restored.local_runner(), project);
        }
    }
}

#[tokio::test]
async fn every_effect_boundary_is_reconciled_before_recovery() {
    for intent in [
        request(true, true),
        request(false, true),
        request(false, false),
    ] {
        for boundary in intent.steps() {
            let dir = crate::test_tempdir().unwrap();
            let store = EnvironmentStore::open(dir.path().join("environment")).unwrap();
            let mut setup = EnvironmentSetup::new(Host {
                fail_after: Some(boundary),
                ..Default::default()
            });
            assert!(setup
                .configure(&store, intent.clone(), &SetupSecrets::default(), |_| {})
                .await
                .is_err());
            assert_eq!(
                store.load_journal().unwrap().unwrap().steps.get(&boundary),
                Some(&StepState::Started)
            );
            setup
                .resume(&store, &SetupSecrets::default(), |_| {})
                .await
                .unwrap();
            assert_eq!(
                setup
                    .backend
                    .effects
                    .iter()
                    .filter(|step| **step == boundary)
                    .count(),
                1
            );
        }
    }
}

#[tokio::test]
async fn unpersisted_enrollment_never_automatically_redeems_again() {
    let dir = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(dir.path().join("environment")).unwrap();
    let mut setup = EnvironmentSetup::new(Host {
        fail_after: Some(SetupStep::RunnerEnrollment),
        lose_enrollment: true,
        ..Default::default()
    });
    let secrets = SetupSecrets {
        pairing_code: Some(Secret::new("sensitive-one-shot-code".into())),
        ..Default::default()
    };
    assert!(setup
        .configure(&store, request(false, true), &secrets, |_| {})
        .await
        .is_err());
    assert_eq!(
        setup
            .resume(&store, &secrets, |_| {})
            .await
            .unwrap_err()
            .code,
        "pairing_recovery_required"
    );
    assert_eq!(
        setup
            .backend
            .effects
            .iter()
            .filter(|step| **step == SetupStep::RunnerEnrollment)
            .count(),
        1
    );
    let bytes = std::fs::read_to_string(dir.path().join("environment/setup.json")).unwrap();
    assert!(!bytes.contains("sensitive-one-shot-code"));
    assert!(!format!("{secrets:?}").contains("sensitive-one-shot-code"));
    setup.backend.lose_enrollment = false;
    setup
        .resume(
            &store,
            &SetupSecrets {
                pairing_code: Some(Secret::new("replacement".into())),
                replacement_pairing_code: true,
                ..Default::default()
            },
            |_| {},
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn user_scope_is_persisted_and_never_reinterprets_an_existing_system_setup() {
    let dir = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(dir.path().join("environment")).unwrap();
    assert_eq!(
        resolve_service_scope(&store, None).unwrap(),
        service::ServiceScope::User
    );
    let mut setup = EnvironmentSetup::new(Host::default());
    let mut original = request(true, false);
    original.runner = Some(true);
    original.service_scope = service::ServiceScope::User;
    setup
        .configure(&store, original.clone(), &SetupSecrets::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(
        store
            .load_environment()
            .unwrap()
            .unwrap()
            .request
            .service_scope,
        service::ServiceScope::User
    );
    assert_eq!(
        resolve_service_scope(&store, None).unwrap(),
        service::ServiceScope::User
    );
    assert_eq!(
        resolve_service_scope(&store, Some(service::ServiceScope::System))
            .unwrap_err()
            .code,
        "service_scope_conflict"
    );
    assert!(!setup.backend.effects.contains(&SetupStep::RunnerEnrollment));
    let old = serde_json::to_value(request(true, false)).unwrap();
    assert!(old.get("service_scope").is_none());
    let restored: SetupRequest = serde_json::from_value(old).unwrap();
    assert_eq!(restored.service_scope, service::ServiceScope::System);
    let before = setup.backend.effects.len();
    let mut changed = original;
    changed.service_scope = service::ServiceScope::System;
    assert_eq!(
        setup
            .configure(&store, changed, &SetupSecrets::default(), |_| {})
            .await
            .unwrap_err()
            .code,
        "environment_conflict"
    );
    assert_eq!(setup.backend.effects.len(), before);
    std::fs::write(store.root().join("setup.json"), "broken").unwrap();
    assert!(resolve_service_scope(&store, None).is_err());
}

#[tokio::test]
async fn another_target_or_account_cannot_rebind_saved_setup() {
    let dir = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(dir.path().join("environment")).unwrap();
    let mut setup = EnvironmentSetup::new(Host::default());
    let original = request(false, true);
    setup
        .configure(&store, original.clone(), &SetupSecrets::default(), |_| {})
        .await
        .unwrap();
    let mut server = original.clone();
    server.server_url = "https://another.example".into();
    let mut project = original.clone();
    project.project = Some("/projects/another".into());
    let mut account = original.clone();
    account.account.identity = "2000".into();
    for changed in [server, project, account] {
        assert_eq!(
            setup
                .configure(&store, changed, &SetupSecrets::default(), |_| {})
                .await
                .unwrap_err()
                .code,
            "environment_conflict"
        );
    }
    assert_eq!(store.load_environment().unwrap().unwrap().request, original);
}

#[test]
fn independent_frontends_cannot_hold_the_same_setup_lock() {
    let dir = crate::test_tempdir().unwrap();
    let desktop = EnvironmentStore::open(dir.path().join("environment")).unwrap();
    let cli = EnvironmentStore::open(dir.path().join("environment")).unwrap();
    let _held = desktop.lock().unwrap();
    assert_eq!(cli.lock().err().unwrap().code, "setup_busy");
}

#[cfg(unix)]
#[test]
fn linked_state_is_not_followed() {
    let dir = crate::test_tempdir().unwrap();
    let foreign = crate::test_tempdir().unwrap();
    std::os::unix::fs::symlink(foreign.path(), dir.path().join("linked")).unwrap();
    assert!(EnvironmentStore::open(dir.path().join("linked")).is_err());
}
