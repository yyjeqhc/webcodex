//! Polling registration, recovery, request dispatch, and shutdown.

use super::*;

pub(super) fn sleep_or_shutdown(delay: Duration, shutdown: &AtomicBool) -> bool {
    let start = Instant::now();
    while start.elapsed() < delay {
        if shutdown.load(Ordering::SeqCst) {
            return true;
        }
        let remaining = delay.saturating_sub(start.elapsed());
        std::thread::sleep(remaining.min(POLLING_SHUTDOWN_SLEEP_SLICE));
    }
    shutdown.load(Ordering::SeqCst)
}

pub(super) fn send_polling_offline_best_effort(
    client: &Client,
    cfg: &RunnerConfig,
    runner_instance_id: &str,
) {
    let url = format!(
        "{}{}",
        cfg.server_url.trim_end_matches('/'),
        RUNNER_OFFLINE_PATH
    );
    let mut request = client.post(url).timeout(POLLING_OFFLINE_TIMEOUT);
    if !cfg.token.trim().is_empty() {
        request = request.bearer_auth(cfg.token.trim());
    }
    let body = RunnerOfflineRequest {
        client_id: cfg.client_id.clone(),
        runner_instance_id: runner_instance_id.to_string(),
    };
    match request.json(&body).send() {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => tracing::debug!(
            status = %response.status(),
            "webcodex-runner polling offline notice was not accepted"
        ),
        Err(error) => tracing::debug!(
            error = %concise_log_error(&error.to_string(), &cfg.token),
            "webcodex-runner polling offline notice failed"
        ),
    }
}

pub(super) fn complete_polling_shutdown(
    client: &Client,
    cfg: &RunnerConfig,
    runner_instance_id: &str,
    registered: bool,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    runtime.request_shutdown_signal();
    runtime.shutdown();
    // Publish offline only after local workers have drained/stopped. Sending it
    // earlier would let a final polling result or Job update refresh last_seen
    // after the Server had already marked the Runner offline. A replacement
    // process never waits for this cleanup because registration takeover is
    // immediate and the delayed notice is instance-scoped.
    if registered {
        send_polling_offline_best_effort(client, cfg, runner_instance_id);
    }
    Ok(())
}

pub(super) fn run_polling_runner(
    cfg: RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    let shutdown = runtime.shutdown_flag();
    run_polling_runner_with_shutdown(cfg, once, runner_instance_id, shutdown, runtime)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PollFailureDirective {
    Continue,
    Shutdown,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_poll_failure(
    error: crate::PollError,
    cfg: &RunnerConfig,
    runtime: &RunnerRuntimeState,
    shutdown: &AtomicBool,
    registered: &mut bool,
    recovering: &mut bool,
    session_refreshed_during_recovery: &mut bool,
    recovery_backoff: &mut RetryBackoff,
) -> Result<PollFailureDirective, String> {
    // Polling has no durable transport lease to prove a Server still knows
    // these process handles. Fail closed at the first recovery boundary
    // instead of advertising false survival after a Server restart or lost
    // registration.
    runtime
        .persistent_shells
        .close_all("runner_transport_disconnected");
    match error.recovery_action() {
        PollingRecoveryAction::Shutdown => Ok(PollFailureDirective::Shutdown),
        PollingRecoveryAction::Fatal => Err(error.into_message()),
        PollingRecoveryAction::RetryPoll => {
            *recovering = true;
            // Refresh the same-instance registration once per recovery
            // episode. This covers a server restart that lost in-memory
            // session state without registering on every repeated 5xx.
            if !*session_refreshed_during_recovery {
                *registered = false;
            }
            let delay = recovery_backoff.next_delay();
            eprintln!(
                "webcodex-runner transient poll failure; retrying delay={} error={}",
                format_delay(delay),
                concise_log_error(&error.to_string(), &cfg.token)
            );
            if sleep_or_shutdown(delay, shutdown) {
                Ok(PollFailureDirective::Shutdown)
            } else {
                Ok(PollFailureDirective::Continue)
            }
        }
        PollingRecoveryAction::ReRegister => {
            *recovering = true;
            *registered = false;
            *session_refreshed_during_recovery = false;
            let delay = recovery_backoff.next_delay();
            eprintln!(
                "webcodex-runner polling session lost; re-registering delay={} error={}",
                format_delay(delay),
                concise_log_error(&error.to_string(), &cfg.token)
            );
            if sleep_or_shutdown(delay, shutdown) {
                Ok(PollFailureDirective::Shutdown)
            } else {
                Ok(PollFailureDirective::Continue)
            }
        }
    }
}

pub(super) fn complete_polling_after_shutdown(
    client: &Client,
    cfg: &RunnerConfig,
    runner_instance_id: &str,
    registered: bool,
    runtime: &RunnerRuntimeState,
    polling_dispatches: &mut PollingDispatchSupervisor,
    project_cache: &mut RunnerProjectCache,
) -> Result<(), String> {
    // Match the former synchronous dispatch race: a fatal submission response
    // that has already reached a worker gets a bounded chance to reach polling
    // control instead of being masked by a simultaneous clean shutdown.
    if let Err(error) =
        polling_dispatches.wait_for_shutdown_outcome(project_cache, Duration::from_millis(500))
    {
        if error.recovery_action() == PollingRecoveryAction::Fatal {
            return Err(error.into_message());
        }
    }
    complete_polling_shutdown(client, cfg, runner_instance_id, registered, runtime)
}

pub(super) fn run_polling_runner_with_shutdown(
    cfg: RunnerConfig,
    once: bool,
    runner_instance_id: &str,
    shutdown: Arc<AtomicBool>,
    runtime: &RunnerRuntimeState,
) -> Result<(), String> {
    let client = Client::builder()
        .timeout(POLLING_HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("failed to create http client: {}", e))?;
    let jobs = runtime.jobs.clone();
    let mut project_cache = RunnerProjectCache::default();
    let mut idle_backoff = PollingIdleBackoff::new(Duration::from_millis(cfg.poll_interval_ms));
    let mut project_refresh = PollingProjectRefresh::new(Instant::now());
    let mut polling_dispatches = PollingDispatchSupervisor::new(
        Arc::clone(&runtime.background_threads),
        runtime.dispatches.clone(),
    );
    let mut registered = false;
    let mut recovering = false;
    let mut session_refreshed_during_recovery = false;
    let mut recovery_backoff = RetryBackoff::new(&POLLING_RECOVERY_BACKOFF_STEPS);
    let mut lease_conflict_started: Option<Instant> = None;
    let mut project_inventory_sync: Option<ProjectInventorySync> = None;
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return complete_polling_after_shutdown(
                &client,
                &cfg,
                runner_instance_id,
                registered,
                runtime,
                &mut polling_dispatches,
                &mut project_cache,
            );
        }
        if let Err(error) = polling_dispatches.drain_completed(&mut project_cache) {
            match handle_poll_failure(
                error,
                &cfg,
                runtime,
                shutdown.as_ref(),
                &mut registered,
                &mut recovering,
                &mut session_refreshed_during_recovery,
                &mut recovery_backoff,
            )? {
                PollFailureDirective::Continue => continue,
                PollFailureDirective::Shutdown => {
                    return complete_polling_after_shutdown(
                        &client,
                        &cfg,
                        runner_instance_id,
                        registered,
                        runtime,
                        &mut polling_dispatches,
                        &mut project_cache,
                    );
                }
            }
        }
        if !registered {
            match register(
                &client,
                &cfg,
                &runtime.config,
                &mut project_cache,
                Some(shutdown.as_ref()),
                runner_instance_id,
                jobs.prepared_profiles().len(),
                &jobs,
            ) {
                Ok((projects_count, registered_jobs, registered_projects, _inventory_status)) => {
                    registered = true;
                    lease_conflict_started = None;
                    recovery_backoff.reset();
                    idle_backoff.reset();
                    project_refresh.mark_sent(Instant::now());
                    project_inventory_sync =
                        Some(paged_sync_after_registration(registered_projects));
                    let sink = RunnerSink::Http(HttpSendConfig {
                        client: client.clone(),
                        server_url: cfg.server_url.clone(),
                        token: cfg.token.clone(),
                        client_id: cfg.client_id.clone(),
                        runner_instance_id: runner_instance_id.to_string(),
                        shutdown: Arc::clone(&shutdown),
                    });
                    jobs.install_sink(sink);
                    jobs.replay_snapshots_since(&registered_jobs);
                    if recovering {
                        eprintln!(
                            "webcodex-runner polling session refreshed during recovery client_id={}",
                            concise_log_error(&cfg.client_id, &cfg.token)
                        );
                        session_refreshed_during_recovery = true;
                    }
                    eprintln!(
                        "{}",
                        registered_log_line(&cfg, TRANSPORT_POLLING, projects_count)
                    );
                }
                Err(error) => match error.recovery_action() {
                    RegisterRecoveryAction::Fatal => return Err(error.into_message()),
                    RegisterRecoveryAction::Retry => {
                        recovering = true;
                        let delay = recovery_backoff.next_delay();
                        eprintln!(
                            "webcodex-runner transient register failure; retrying delay={} error={}",
                            format_delay(delay),
                            concise_log_error(&error.to_string(), &cfg.token)
                        );
                        if sleep_or_shutdown(delay, shutdown.as_ref()) {
                            return complete_polling_after_shutdown(
                                &client,
                                &cfg,
                                runner_instance_id,
                                registered,
                                runtime,
                                &mut polling_dispatches,
                                &mut project_cache,
                            );
                        }
                    }
                    RegisterRecoveryAction::WaitForLease => {
                        recovering = true;
                        let started = lease_conflict_started.get_or_insert_with(Instant::now);
                        let elapsed = started.elapsed();
                        let Some(delay) = next_lease_conflict_delay(&mut recovery_backoff, elapsed)
                        else {
                            return Err(format!(
                                "active-instance lease conflict for client_id={} did not clear within {}",
                                concise_log_error(&cfg.client_id, &cfg.token),
                                format_delay(POLLING_LEASE_CONFLICT_MAX_WAIT)
                            ));
                        };
                        eprintln!(
                            "webcodex-runner active-instance lease conflict; waiting client_id={} delay={}",
                            concise_log_error(&cfg.client_id, &cfg.token),
                            format_delay(delay)
                        );
                        if sleep_or_shutdown(delay, shutdown.as_ref()) {
                            return complete_polling_after_shutdown(
                                &client,
                                &cfg,
                                runner_instance_id,
                                registered,
                                runtime,
                                &mut polling_dispatches,
                                &mut project_cache,
                            );
                        }
                    }
                },
            }
            continue;
        }
        if !once {
            if let Err(error) = polling_dispatches
                .wait_for_capacity_or_shutdown(&mut project_cache, shutdown.as_ref())
            {
                match handle_poll_failure(
                    error,
                    &cfg,
                    runtime,
                    shutdown.as_ref(),
                    &mut registered,
                    &mut recovering,
                    &mut session_refreshed_during_recovery,
                    &mut recovery_backoff,
                )? {
                    PollFailureDirective::Continue => continue,
                    PollFailureDirective::Shutdown => {
                        return complete_polling_after_shutdown(
                            &client,
                            &cfg,
                            runner_instance_id,
                            registered,
                            runtime,
                            &mut polling_dispatches,
                            &mut project_cache,
                        );
                    }
                }
            }
        }
        if shutdown.load(Ordering::SeqCst) {
            return complete_polling_after_shutdown(
                &client,
                &cfg,
                runner_instance_id,
                registered,
                runtime,
                &mut polling_dispatches,
                &mut project_cache,
            );
        }
        let refresh_projects = if project_inventory_sync.is_none() {
            polling_projects_for_poll(
                &project_refresh,
                &mut project_cache,
                &cfg,
                shutdown.as_ref(),
                Instant::now(),
            )
        } else {
            None
        };
        if let Some(projects) = refresh_projects {
            project_inventory_sync = Some(ProjectInventorySync::new(projects));
        }

        let mut project_inventory_page = None;
        let mut inventory_page_build_failed = false;
        if let Some(sync) = project_inventory_sync.as_mut() {
            match sync.current_page() {
                Ok(page) => project_inventory_page = page,
                Err(reason_code) => {
                    log_project_inventory_degraded(
                        TRANSPORT_POLLING,
                        sync.total_reported(),
                        reason_code,
                    );
                    inventory_page_build_failed = true;
                }
            }
        }
        if inventory_page_build_failed {
            project_inventory_sync = None;
            project_refresh.mark_sent(Instant::now());
        }
        let sent_inventory_page = project_inventory_page.is_some();
        let mut poll_result = handle_one_poll(
            &client,
            &cfg,
            &runtime.config,
            &jobs,
            &runtime.persistent_shells,
            &mut project_cache,
            project_inventory_page,
            runner_instance_id,
            &runtime.lsp,
            &runtime.browser,
            &shutdown,
            &runtime.dispatches,
            &mut polling_dispatches,
            once,
        );
        // A background fatal result submission takes precedence over an
        // unrelated poll response from the same turn.
        if let Err(error) = polling_dispatches.drain_completed(&mut project_cache) {
            poll_result = Err(error);
        }
        match poll_result {
            Ok((ran_request, inventory_status)) => {
                if sent_inventory_page {
                    match inventory_status {
                        Some(status) => {
                            if let Some(sync) = project_inventory_sync.as_mut() {
                                match sync.acknowledge(&status) {
                                    Ok(done) => {
                                        if done {
                                            project_inventory_sync = None;
                                            project_refresh.mark_sent(Instant::now());
                                        }
                                    }
                                    Err(reason_code) => {
                                        log_project_inventory_degraded(
                                            TRANSPORT_POLLING,
                                            sync.total_reported(),
                                            &reason_code,
                                        );
                                        project_inventory_sync = None;
                                        project_refresh.mark_sent(Instant::now());
                                    }
                                }
                            }
                        }
                        None => {
                            return Err(
                                "poll response missing canonical project_inventory acknowledgement; Server is incompatible with this 0.4 Runner"
                                    .to_string(),
                            );
                        }
                    }
                }
                recovery_backoff.reset();
                lease_conflict_started = None;
                if recovering {
                    idle_backoff.reset();
                    eprintln!(
                        "webcodex-runner polling recovery succeeded phase=poll client_id={}",
                        concise_log_error(&cfg.client_id, &cfg.token)
                    );
                    recovering = false;
                    session_refreshed_during_recovery = false;
                }
                if once {
                    // `--once` still completes a negotiated multi-page startup
                    // inventory. Exiting after page 0 would make once-mode a
                    // cardinality-dependent partial-sync path.
                    if project_inventory_sync.is_some() {
                        continue;
                    }
                    while jobs.has_work() {
                        if sleep_or_shutdown(
                            Duration::from_millis(cfg.poll_interval_ms),
                            shutdown.as_ref(),
                        ) {
                            return complete_polling_after_shutdown(
                                &client,
                                &cfg,
                                runner_instance_id,
                                registered,
                                runtime,
                                &mut polling_dispatches,
                                &mut project_cache,
                            );
                        }
                    }
                    return complete_polling_shutdown(
                        &client,
                        &cfg,
                        runner_instance_id,
                        registered,
                        runtime,
                    );
                }
                if let Some(delay) = polling_idle_delay(&mut idle_backoff, ran_request) {
                    if sleep_or_shutdown(delay, shutdown.as_ref()) {
                        return complete_polling_after_shutdown(
                            &client,
                            &cfg,
                            runner_instance_id,
                            registered,
                            runtime,
                            &mut polling_dispatches,
                            &mut project_cache,
                        );
                    }
                }
            }
            Err(error) => match handle_poll_failure(
                error,
                &cfg,
                runtime,
                shutdown.as_ref(),
                &mut registered,
                &mut recovering,
                &mut session_refreshed_during_recovery,
                &mut recovery_backoff,
            )? {
                PollFailureDirective::Continue => {}
                PollFailureDirective::Shutdown => {
                    return complete_polling_after_shutdown(
                        &client,
                        &cfg,
                        runner_instance_id,
                        registered,
                        runtime,
                        &mut polling_dispatches,
                        &mut project_cache,
                    );
                }
            },
        }
    }
}
