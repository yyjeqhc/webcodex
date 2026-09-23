use super::*;

#[tokio::test]
async fn registry_allows_session_scoped_run_without_ssh_resource() {
    let registry = RunnerRegistry::default();
    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "xrh".to_string(),
            runner_instance_id: "inst".to_string(),
            runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                RunnerCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();

    let (request_id, _rx) = registry
        .enqueue_run_with_ssh(
            ShellRunRequest {
                login: false,
                client_id: "xrh".to_string(),
                cwd: None,
                command: "echo local".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
            None,
            Some("wc_sess_local".to_string()),
            None,
        )
        .await
        .unwrap();
    assert!(!registry.cancel_request(&request_id).await);

    let error = registry
        .enqueue_run_with_ssh(
            ShellRunRequest {
                login: false,
                client_id: "xrh".to_string(),
                cwd: None,
                command: "echo remote".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
            Some("tmp".to_string()),
            None,
            None,
        )
        .await
        .unwrap_err();
    assert!(error.contains("ssh_session_required"), "{error}");
}

#[tokio::test]
async fn script_language_extensions_require_independent_additive_capabilities() {
    let registry = RunnerRegistry::default();
    let registration = |javascript: bool, typescript: bool| {
        let mut capabilities =
            crate::test_support::current_runner_capabilities(RunnerCapabilities::default());
        capabilities.structured_script_javascript = javascript;
        capabilities.structured_script_typescript = typescript;
        current_runner_registration(RunnerRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: "script-cap".to_string(),
            runner_instance_id: "inst".to_string(),
            runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities,
            policy: None,
        })
    };

    // Model a mixed-version Runner that already understands JavaScript typed
    // scripts but predates the additive TypeScript wire variant.
    registry.register(registration(true, false)).await.unwrap();
    let typescript = ShellScriptPayload {
        language: ShellScriptLanguage::Typescript,
        script: "const value: string = 'hello';\nconsole.log(value);\n".to_string(),
        args: Vec::new(),
    };
    let error = registry
        .enqueue_script(
            "script-cap".to_string(),
            None,
            typescript.clone(),
            None,
            10,
            1,
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(error.contains("structured_script_typescript"), "{error}");

    // TypeScript's new fence must not block JavaScript when the independent JS
    // capability is present, nor older canonical shell languages.
    for payload in [
        ShellScriptPayload {
            language: ShellScriptLanguage::Javascript,
            script: "console.log('hello');\n".to_string(),
            args: Vec::new(),
        },
        ShellScriptPayload {
            language: ShellScriptLanguage::Bash,
            script: "printf ok\n".to_string(),
            args: Vec::new(),
        },
    ] {
        let (request_id, _rx) = registry
            .enqueue_script(
                "script-cap".to_string(),
                None,
                payload,
                None,
                10,
                1,
                "test".to_string(),
            )
            .await
            .unwrap();
        assert!(!registry.cancel_request(&request_id).await);
    }

    registry.register(registration(true, true)).await.unwrap();
    let (request_id, _rx) = registry
        .enqueue_script(
            "script-cap".to_string(),
            None,
            typescript,
            None,
            10,
            1,
            "test".to_string(),
        )
        .await
        .unwrap();
    assert!(!registry.cancel_request(&request_id).await);
}

#[tokio::test]
async fn python_direct_script_requires_additive_capability_before_enqueue() {
    let registry = RunnerRegistry::default();
    let registration = |python: bool| RunnerRegisterRequest {
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
        client_id: "python-cap".to_string(),
        runner_instance_id: "inst".to_string(),
        runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities: {
            let mut capabilities = v2_baseline_capabilities();
            capabilities.structured_script_python = python;
            capabilities
        },
        policy: None,
    };
    registry.register(registration(false)).await.unwrap();
    let payload = ShellScriptPayload {
        language: ShellScriptLanguage::Python,
        script: "print('雪')\n".to_string(),
        args: Vec::new(),
    };
    let error = registry
        .enqueue_script(
            "python-cap".to_string(),
            None,
            payload.clone(),
            None,
            10,
            1,
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(error.contains("structured_script_python"), "{error}");
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "python-cap".to_string(),
            runner_instance_id: "inst".to_string()
        })
        .await
        .unwrap()
        .is_none());
    registry.register(registration(true)).await.unwrap();
    let (_id, _rx) = registry
        .enqueue_script(
            "python-cap".to_string(),
            None,
            payload,
            None,
            10,
            1,
            "test".to_string(),
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "python-cap".to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        request.script.unwrap().language,
        ShellScriptLanguage::Python
    );
}

#[tokio::test]
async fn bash_login_requires_additive_capability_before_enqueue() {
    use webcodex_core::workflow_session_contract::ExecutionShell;
    let registry = RunnerRegistry::default();
    let registration = |login: bool| RunnerRegisterRequest {
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
        client_id: "bash-login-cap".to_string(),
        runner_instance_id: "inst".to_string(),
        runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities: {
            let mut capabilities = v2_baseline_capabilities();
            capabilities.shell = true;
            capabilities.explicit_shell_selection = true;
            capabilities.bash_login_shell = login;
            capabilities
        },
        policy: None,
    };
    let request = ShellRunRequest {
        client_id: "bash-login-cap".to_string(),
        cwd: None,
        command: "printf ok".to_string(),
        login: true,
        stdin: None,
        timeout_secs: 10,
        wait_timeout_secs: 1,
    };
    registry.register(registration(false)).await.unwrap();
    let error = registry
        .enqueue_run_with_ssh(
            request.clone(),
            "test".to_string(),
            None,
            None,
            Some(ExecutionShell::Bash),
        )
        .await
        .unwrap_err();
    assert!(error.contains("bash_login_shell"), "{error}");
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "bash-login-cap".to_string(),
            runner_instance_id: "inst".to_string()
        })
        .await
        .unwrap()
        .is_none());
    registry.register(registration(true)).await.unwrap();
    let (_id, _rx) = registry
        .enqueue_run_with_ssh(
            request.clone(),
            "test".to_string(),
            None,
            None,
            Some(ExecutionShell::Bash),
        )
        .await
        .unwrap();
    let wire = registry
        .poll(RunnerPollRequest {
            client_id: "bash-login-cap".to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert!(wire.login);
    assert_eq!(wire.shell, Some(ExecutionShell::Bash));
    let error = registry
        .enqueue_run_with_ssh(
            request,
            "test".to_string(),
            None,
            None,
            Some(ExecutionShell::Sh),
        )
        .await
        .unwrap_err();
    assert!(error.contains("requires local shell=bash"), "{error}");
}

#[tokio::test]
async fn registry_rejects_unknown_client_run() {
    let registry = RunnerRegistry::default();
    let err = registry
        .enqueue_run(
            ShellRunRequest {
                login: false,
                client_id: "missing".to_string(),
                cwd: None,
                command: "pwd".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 1,
            },
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(err.contains("unknown shell client"));
}
