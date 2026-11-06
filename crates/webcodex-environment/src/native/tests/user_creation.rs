use super::*;
use webcodex_core::authority::validate_username;

#[test]
fn generated_account_usernames_satisfy_the_server_contract() {
    for (account, expected) in [
        (r"DESKTOP\Haruya", "haruya"),
        (r"DOMAIN\Administrator", "administrator"),
        ("alice.smith", "alice_smith"),
        ("alice", "alice"),
        ("bob_123-4", "bob_123-4"),
        ("用户", "user"),
        ("", "user"),
    ] {
        let username = account_username(account);
        assert_eq!(username, expected);
        assert_eq!(validate_username(&username).unwrap(), username);
    }
    let username = account_username(&"A".repeat(100));
    assert_eq!(username.len(), 48);
    assert_eq!(validate_username(&username).unwrap(), username);
}

fn create_record(url: String) -> EnvironmentRecord {
    let mut environment = record(
        url,
        None,
        EnvironmentMode::Create {
            listen: "127.0.0.1:8080".into(),
        },
    );
    environment.request.account.name = r"DESKTOP\Haruya".into();
    environment
}

fn prepare_bootstrap(store: &EnvironmentStore) {
    ensure_private_directory(&store.root().join("server")).unwrap();
    atomic_private_write(
        &store.root().join("server/webcodex.env"),
        b"WEBCODEX_TOKEN=wc_boot_fixture\n",
    )
    .unwrap();
}

// Exercise the production client's HTTP flow with the same validator as the
// Server. No service installation or real credentials are involved.
fn user_server(
    expected: usize,
    saved: Option<EnvironmentStore>,
    username: &'static str,
) -> (
    String,
    Arc<Mutex<Vec<HttpRequest>>>,
    std::thread::JoinHandle<()>,
) {
    let registered = Mutex::new(false);
    fixture(expected, move |request| match request.path.as_str() {
        "/api/users/create" => {
            let username = request.body["username"].as_str().unwrap();
            if let Some(store) = &saved {
                let journal = store.load_journal().unwrap().unwrap();
                assert_eq!(journal.environment.username.as_deref(), Some(username));
                assert_eq!(
                    journal.steps[&SetupStep::UserAuthentication],
                    StepState::Started
                );
            }
            match validate_username(username) {
                Ok(username) => (
                    200,
                    vec![],
                    json!({"success": true, "user": {"username": username}}),
                ),
                Err(_) => (400, vec![], json!({"error": "invalid username"})),
            }
        }
        "/api/runtime-console/overview" if !*registered.lock().unwrap() => {
            (401, vec![], json!({"error": "unauthorized"}))
        }
        "/api/runtime-console/overview" => (
            200,
            vec![],
            json!({"authenticated_user": username, "service": "webcodex"}),
        ),
        "/api/tokens/register_hash" => {
            assert_eq!(request.body["username"], username);
            *registered.lock().unwrap() = true;
            (
                200,
                vec![],
                json!({"success": true, "token": {"token_prefix": request.body["token_prefix"]}}),
            )
        }
        other => panic!("unexpected user provisioning route: {other}"),
    })
}

#[tokio::test]
async fn mixed_case_account_completes_first_user_creation() {
    for (account, username) in [
        (r"DESKTOP\Haruya", "haruya"),
        (r"DOMAIN\Alice.Smith", "alice_smith"),
    ] {
        let (url, requests, server) = user_server(4, None, username);
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        prepare_bootstrap(&store);
        let mut environment = create_record(url);
        environment.request.account.name = account.into();
        NativeEnvironment::new()
            .unwrap()
            .create_user(&store, &mut environment)
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(environment.username.as_deref(), Some(username));
        assert!(store.root().join("webcodex-user-token").is_file());
        assert_eq!(requests.lock().unwrap()[0].body["username"], username);
    }
}

fn failed_journal(environment: EnvironmentRecord) -> SetupJournal {
    SetupJournal {
        schema_version: ENVIRONMENT_SCHEMA,
        operation_id: "original-operation".into(),
        environment,
        steps: BTreeMap::from([
            (SetupStep::Preflight, StepState::Complete),
            (SetupStep::ServerConfiguration, StepState::Complete),
            (SetupStep::ServerServiceInstall, StepState::Complete),
            (SetupStep::ServerServiceStart, StepState::Complete),
            (SetupStep::ServerReachability, StepState::Complete),
            (SetupStep::UserAuthentication, StepState::Started),
        ]),
        last_diagnostic: Some(diagnostic(
            "server_request_failed",
            "The Server rejected the operation",
        )),
    }
}

// Keep the real setup journal and HTTP authentication adapter, replacing only
// unrelated platform service effects and final health observation.
struct UserSetup(NativeEnvironment);

impl EnvironmentBackend for UserSetup {
    async fn reconcile(
        &mut self,
        store: &EnvironmentStore,
        record: &mut EnvironmentRecord,
        step: SetupStep,
    ) -> SetupResultValue<Reconciliation> {
        if matches!(step, SetupStep::Preflight | SetupStep::UserAuthentication) {
            self.0.reconcile(store, record, step).await
        } else {
            Ok(Reconciliation::Complete)
        }
    }

    async fn apply(
        &mut self,
        store: &EnvironmentStore,
        record: &mut EnvironmentRecord,
        step: SetupStep,
        secrets: &SetupSecrets,
    ) -> SetupResultValue<()> {
        if step == SetupStep::Preflight {
            return Ok(());
        }
        assert_eq!(step, SetupStep::UserAuthentication);
        self.0.apply(store, record, step, secrets).await
    }

    async fn observe(
        &mut self,
        _: &EnvironmentStore,
        _: &EnvironmentRecord,
    ) -> SetupResultValue<RuntimeObservation> {
        Ok(RuntimeObservation {
            server_reachable: true,
            authenticated: true,
            ..Default::default()
        })
    }
}

#[tokio::test]
async fn resume_repairs_only_the_uncreated_username_and_reuses_the_credential() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    prepare_bootstrap(&store);
    let (url, requests, server) = user_server(5, Some(store.clone()), "haruya");
    let mut environment = create_record(url);
    environment.username = Some("Haruya".into());
    environment.runner_client_id = Some("Haruya-original-runner".into());
    let journal = failed_journal(environment.clone());
    store.save_journal(&journal).unwrap();

    let mut setup = EnvironmentSetup::new(UserSetup(NativeEnvironment::new().unwrap()));
    let first = setup
        .resume(&store, &SetupSecrets::default(), |_| {})
        .await
        .unwrap();
    let token = read_secret(&store.root().join("webcodex-user-token")).unwrap();
    let second = setup
        .resume(&store, &SetupSecrets::default(), |_| {})
        .await
        .unwrap();
    server.join().unwrap();

    for result in [first, second] {
        assert_eq!(result.environment.username.as_deref(), Some("haruya"));
        assert_eq!(
            result.environment.environment_id,
            environment.environment_id
        );
        assert_eq!(
            result.environment.runner_client_id,
            environment.runner_client_id
        );
        assert_eq!(result.environment.request, environment.request);
    }
    assert_eq!(
        read_secret(&store.root().join("webcodex-user-token"))
            .unwrap()
            .expose(),
        token.expose()
    );
    let saved = store.load_journal().unwrap().unwrap();
    assert_eq!(saved.operation_id, journal.operation_id);
    assert_eq!(saved.environment.username.as_deref(), Some("haruya"));
    assert!(saved.environment.configured);
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "/api/users/create",
            "/api/runtime-console/overview",
            "/api/tokens/register_hash",
            "/api/runtime-console/overview",
            "/api/runtime-console/overview",
        ]
    );
}

#[cfg(unix)]
#[test]
fn username_recovery_does_not_treat_a_dangling_credential_link_as_absent() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut environment = create_record("http://127.0.0.1:9".into());
    environment.username = Some("Haruya".into());
    store
        .save_journal(&failed_journal(environment.clone()))
        .unwrap();
    std::os::unix::fs::symlink(
        store.root().join("missing"),
        store.root().join("webcodex-user-token"),
    )
    .unwrap();
    assert_eq!(
        recover_uncreated_username(&store, &mut environment)
            .unwrap_err()
            .code,
        "user_identity_conflict"
    );
    assert_eq!(environment.username.as_deref(), Some("Haruya"));
}

#[test]
fn username_recovery_rejects_established_or_uncertain_identity_without_writes() {
    for case in [
        "configured",
        "environment_record",
        "completed_auth",
        "later_step",
        "other_name",
        "other_environment",
        "other_server",
        "uncertain_response",
        "missing_journal",
        "webcodex-user-token",
        "runner.toml",
        "enrollment-recovery.json",
    ] {
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let mut environment = create_record("http://127.0.0.1:9".into());
        environment.username = Some("Haruya".into());
        let mut journal = failed_journal(environment.clone());
        match case {
            "configured" => environment.configured = true,
            "environment_record" => store.save_environment(&environment).unwrap(),
            "completed_auth" => {
                journal
                    .steps
                    .insert(SetupStep::UserAuthentication, StepState::Complete);
            }
            "later_step" => {
                journal
                    .steps
                    .insert(SetupStep::RunnerConfiguration, StepState::Started);
            }
            "other_name" => {
                environment.username = Some("OtherUser".into());
                journal.environment = environment.clone();
            }
            "other_environment" => journal.environment.environment_id = "other-environment".into(),
            "other_server" => journal.environment.request.server_url = "http://127.0.0.1:10".into(),
            "uncertain_response" => {
                journal.last_diagnostic =
                    Some(diagnostic("response_uncertain", "Interrupted response"))
            }
            "missing_journal" => {}
            file => atomic_private_write(&store.root().join(file), b"PRIVATE_CANARY").unwrap(),
        }
        if case != "missing_journal" {
            store.save_journal(&journal).unwrap();
        }
        let before = serde_json::to_value(&environment).unwrap();
        let saved = store
            .load_journal()
            .unwrap()
            .map(|value| serde_json::to_value(value).unwrap());
        let error = recover_uncreated_username(&store, &mut environment).unwrap_err();
        assert_eq!(error.code, "user_identity_conflict", "{case}");
        assert_eq!(
            serde_json::to_value(&environment).unwrap(),
            before,
            "{case}"
        );
        assert_eq!(
            store
                .load_journal()
                .unwrap()
                .map(|value| serde_json::to_value(value).unwrap()),
            saved,
            "{case}"
        );
        if [
            "webcodex-user-token",
            "runner.toml",
            "enrollment-recovery.json",
        ]
        .contains(&case)
        {
            assert_eq!(
                read_private(&store.root().join(case)).unwrap(),
                b"PRIVATE_CANARY"
            );
        }
    }
}

#[tokio::test]
async fn existing_user_identity_is_not_derived_again_from_the_os_account() {
    let (url, requests, server) = fixture(1, |_| {
        (
            200,
            vec![],
            json!({"authenticated_user": "existing_owner", "service": "webcodex"}),
        )
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    atomic_private_write(
        &store.root().join("webcodex-user-token"),
        b"wc_pat_existing",
    )
    .unwrap();
    let mut environment = create_record(url);
    environment.username = Some("existing_owner".into());
    assert_eq!(
        NativeEnvironment::new()
            .unwrap()
            .reconcile(&store, &mut environment, SetupStep::UserAuthentication)
            .await
            .unwrap(),
        Reconciliation::Complete
    );
    server.join().unwrap();
    assert_eq!(environment.username.as_deref(), Some("existing_owner"));
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert_eq!(
        read_secret(&store.root().join("webcodex-user-token"))
            .unwrap()
            .expose(),
        "wc_pat_existing"
    );
}

#[tokio::test]
async fn http_rejection_diagnostics_preserve_codes_and_exclude_response_secrets() {
    for (status, expected) in [
        (400, "server_request_failed"),
        (500, "server_request_failed"),
        (503, "server_request_failed"),
        (401, "authentication_required"),
        (403, "permission_denied"),
        (409, "server_conflict"),
    ] {
        let (url, _, server) = fixture(1, move |_| {
            (status, vec![], json!({"error": "PRIVATE_RESPONSE_CANARY"}))
        });
        let error = NativeEnvironment::new()
            .unwrap()
            .post(
                &url,
                "/api/users/create",
                Some("PRIVATE_TOKEN_CANARY"),
                json!({"username": "haruya"}),
            )
            .await
            .unwrap_err();
        server.join().unwrap();
        assert_eq!(error.code, expected);
        assert!(error.message.contains(&format!("HTTP {status}")));
        let encoded = serde_json::to_string(&error).unwrap();
        assert!(!encoded.contains("CANARY"));
        assert!(!encoded.contains(&url));
    }
}

#[tokio::test]
async fn user_creation_failure_identifies_the_stage_without_writing_a_token() {
    let (url, _, server) = fixture(1, |_| {
        (400, vec![], json!({"error": "PRIVATE_RESPONSE_CANARY"}))
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    prepare_bootstrap(&store);
    let mut environment = create_record(url);
    let error = NativeEnvironment::new()
        .unwrap()
        .create_user(&store, &mut environment)
        .await
        .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, "server_request_failed");
    assert!(error.message.contains("Local user creation failed"));
    assert!(error.message.contains("HTTP 400"));
    assert!(!error.message.contains("CANARY"));
    assert!(!store.root().join("webcodex-user-token").exists());
}
