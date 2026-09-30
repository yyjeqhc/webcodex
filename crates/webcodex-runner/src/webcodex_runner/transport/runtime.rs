//! Process-wide Runner resources and ordered shutdown.

use super::*;

impl RunnerRuntimeState {
    pub(crate) fn new(cfg: &RunnerConfig, path: PathBuf) -> Self {
        Self::with_shutdown_budget(cfg, path, DEFAULT_SHUTDOWN_BUDGET)
    }

    pub(super) fn with_shutdown_budget(
        cfg: &RunnerConfig,
        path: PathBuf,
        budget: Duration,
    ) -> Self {
        let jobs = JobManager::new(max_concurrent_jobs(cfg))
            .with_detached_profile_identity(&cfg.server_url);
        // Persistent shells reuse the same authenticated OpenSSH multiplex pool
        // as async jobs: one transport per (session, resource, generation),
        // never a second SSH configuration or connection pool.
        let persistent_shells = PersistentShellManager::new(&cfg.shell, jobs.ssh_pool().clone());
        Self {
            lsp: LspSupervisor::default(),
            browser: BrowserSupervisor::new(),
            config: Arc::new(ReloadableRunnerConfig::new(cfg.clone(), path)),
            jobs,
            persistent_shells,
            coordinator: Arc::new(ShutdownCoordinator::new(budget)),
            reload_threads: Arc::new(BackgroundThreads::default()),
            background_threads: Arc::new(BackgroundThreads::default()),
            dispatches: ActivityTracker::default(),
            #[cfg(windows)]
            exit_diagnostics: None,
        }
    }

    pub(super) fn request_shutdown_signal(&self) {
        self.coordinator.request_signal();
        #[cfg(windows)]
        if let Some(diagnostics) = self.exit_diagnostics.as_ref() {
            // Persist signal provenance before any cleanup work. If the process
            // disappears while cleanup is in progress, the lifecycle record must
            // still distinguish an interrupted graceful shutdown from an exit
            // that never entered Rust shutdown handling.
            diagnostics.mark_shutdown_signal_received();
        }
        let deadline = self.coordinator.deadline().instant();
        self.config.begin_shutdown();
        self.jobs.stop_accepting_work();
        self.persistent_shells.close_all("runner_shutdown");
        self.browser.begin_shutdown();
        self.lsp.begin_shutdown_until(deadline);
    }

    pub(super) fn shutdown_flag(&self) -> Arc<AtomicBool> {
        self.coordinator.requested_flag()
    }

    pub(super) fn shutdown_requested(&self) -> bool {
        self.coordinator.is_requested()
    }

    pub(super) fn project_summaries(
        &self,
        cache: &mut RunnerProjectCache,
        cfg: &RunnerConfig,
    ) -> Vec<RunnerProjectSummary> {
        let shutdown = self.shutdown_flag();
        cache.get_with_shutdown(cfg, Some(shutdown.as_ref()))
    }

    pub(super) fn transport_runtime_shutdown_timeout(&self) -> Duration {
        if self.shutdown_requested() {
            RUNTIME_SHUTDOWN_TIMEOUT.min(
                self.coordinator
                    .deadline()
                    .instant()
                    .saturating_duration_since(Instant::now()),
            )
        } else {
            RUNTIME_SHUTDOWN_TIMEOUT
        }
    }

    pub(super) async fn wait_for_shutdown(&self) {
        self.coordinator.wait_requested().await;
    }

    #[cfg(any(unix, test))]
    pub(super) fn register_reload_thread(&self, handle: std::thread::JoinHandle<()>) {
        self.reload_threads.register(handle);
    }

    pub(super) fn register_background_thread(&self, handle: std::thread::JoinHandle<()>) {
        self.background_threads.register(handle);
    }

    pub(super) fn shutdown(&self) -> ShutdownReport {
        self.coordinator.run_once(|deadline| self.cleanup(deadline))
    }

    pub(super) fn cleanup(&self, deadline: ShutdownDeadline) -> Vec<ShutdownPhaseResult> {
        let mut phases = Vec::with_capacity(11);

        let started = Instant::now();
        phases.push(if self.coordinator.signal_received() {
            ShutdownPhaseResult::completed("signal_received", started, 0)
        } else {
            ShutdownPhaseResult::skipped("signal_received", started)
        });

        let started = Instant::now();
        self.config.begin_shutdown();
        self.jobs.stop_accepting_work();
        self.persistent_shells.close_all("runner_shutdown");
        self.browser.begin_shutdown();
        self.lsp.begin_shutdown_until(deadline.instant());
        phases.push(ShutdownPhaseResult::completed(
            "stop_accepting_work",
            started,
            0,
        ));

        let started = Instant::now();
        let reload_resources = self.reload_threads.pending();
        let reload = self
            .reload_threads
            .join_until(deadline.phase_deadline(CONFIG_RELOAD_JOIN_BUDGET));
        phases.push(shutdown_phase(
            "config_reload_stop",
            started,
            reload_resources,
            reload.timed_out,
            reload.panicked,
            "reload_thread_panicked",
        ));

        let started = Instant::now();
        let cancelled = self.jobs.cancel_queued_for_shutdown();
        phases.push(if cancelled == 0 {
            ShutdownPhaseResult::skipped("queued_jobs_cancel", started)
        } else {
            ShutdownPhaseResult::completed("queued_jobs_cancel", started, cancelled)
        });

        let started = Instant::now();
        let job_batch = self.jobs.signal_all_for_shutdown();
        let active_jobs = job_batch.running();
        let signal_failures = job_batch.failures();
        phases.push(shutdown_phase(
            "active_jobs_signal",
            started,
            active_jobs,
            0,
            signal_failures,
            "job_signal_failed",
        ));

        let started = Instant::now();
        let jobs = self
            .jobs
            .drain_shutdown(job_batch, deadline.phase_deadline(JOB_DRAIN_BUDGET));
        phases.push(shutdown_phase(
            "active_jobs_drain",
            started,
            jobs.resources(),
            jobs.timed_out(),
            jobs.failures().saturating_sub(signal_failures),
            "job_reap_failed",
        ));

        let started = Instant::now();
        let provider_deadline = deadline.phase_deadline(PROVIDER_SHUTDOWN_BUDGET);
        let mut provider_connections = 0usize;
        let mut provider_timeouts = 0usize;
        let mut provider_failures = 0usize;
        for router in self.config.external_routers() {
            let outcome = router.shutdown_until(provider_deadline);
            provider_connections = provider_connections.saturating_add(outcome.connections);
            provider_timeouts = provider_timeouts.saturating_add(outcome.timed_out);
            provider_failures = provider_failures.saturating_add(outcome.failures);
        }
        phases.push(shutdown_phase(
            "external_providers_stop",
            started,
            provider_connections,
            provider_timeouts,
            provider_failures,
            "provider_shutdown_failed",
        ));

        let started = Instant::now();
        let browser = self
            .browser
            .shutdown_until(deadline.phase_deadline(BROWSER_SHUTDOWN_BUDGET));
        phases.push(shutdown_phase(
            "browser_runtimes_stop",
            started,
            browser.browsers,
            browser.timed_out,
            browser.failures,
            "browser_shutdown_failed",
        ));

        let started = Instant::now();
        let lsp = self
            .lsp
            .shutdown_until(deadline.phase_deadline(LSP_SHUTDOWN_BUDGET));
        phases.push(shutdown_phase(
            "lsp_servers_stop",
            started,
            lsp.servers,
            lsp.timed_out + usize::from(lsp.reaper_timed_out),
            lsp.failures,
            "lsp_shutdown_failed",
        ));

        let started = Instant::now();
        let background_deadline = deadline.phase_deadline(BACKGROUND_JOIN_BUDGET);
        let background_resources = self.reload_threads.pending()
            + self.background_threads.pending()
            + self.jobs.worker_count()
            + self.dispatches.active();
        let reload_retry = self.reload_threads.join_until(background_deadline);
        let joined = self.background_threads.join_until(background_deadline);
        let workers_done = self.jobs.wait_for_workers(background_deadline);
        let dispatches_done = self.dispatches.wait_until(background_deadline);
        let coding_agents = self
            .config
            .coding_agents()
            .map(|manager| manager.drain_workers_until(background_deadline))
            .unwrap_or_default();
        let background_timeouts = reload_retry.timed_out
            + joined.timed_out
            + usize::from(!workers_done)
            + usize::from(!dispatches_done)
            + coding_agents.timed_out;
        phases.push(shutdown_phase(
            "background_threads_join",
            started,
            background_resources + coding_agents.resources,
            background_timeouts,
            reload_retry.panicked + joined.panicked + coding_agents.panicked,
            "background_thread_panicked",
        ));

        phases.push(ShutdownPhaseResult::completed(
            "shutdown_complete",
            Instant::now(),
            0,
        ));
        phases
    }
}

pub(super) fn shutdown_phase(
    phase: &'static str,
    started: Instant,
    resources: usize,
    timed_out: usize,
    failures: usize,
    failure_code: &'static str,
) -> ShutdownPhaseResult {
    if timed_out > 0 {
        ShutdownPhaseResult::timed_out(phase, started, resources)
    } else if failures > 0 {
        ShutdownPhaseResult::failed(phase, started, resources, failure_code)
    } else if resources == 0 {
        ShutdownPhaseResult::skipped(phase, started)
    } else {
        ShutdownPhaseResult::completed(phase, started, resources)
    }
}
