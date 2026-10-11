//! Shell/validation admission, profile resolution and stdin-safe command preparation.

use super::shell_worker::PreparedShellJob;
use super::*;
impl JobManager {
    pub(super) fn start_shell_job(&self, start: PendingJobStart) {
        let PendingJobStart {
            generation,
            policy,
            shell,
            ssh,
            project_registry_dir,
            metadata: _,
            operation,
            ..
        } = start;
        let (
            job_id,
            cwd,
            raw_command,
            explicit_shell,
            login,
            steps,
            timeout_secs,
            context,
            validation,
        ) = match &operation {
            RunnerJobOperation::StartShell(request) => (
                request.job_id.clone(),
                request.cwd.clone(),
                Some(request.command.clone()),
                request.shell,
                request.login,
                Vec::new(),
                request.timeout_secs,
                request.context.clone(),
                false,
            ),
            RunnerJobOperation::StartValidation(request) => (
                request.job_id.clone(),
                request.cwd.clone(),
                None,
                None,
                false,
                request.steps.clone(),
                request.timeout_secs,
                request.context.clone(),
                true,
            ),
            _ => unreachable!("shell Job starter received non shell/validation operation"),
        };
        let project_go_single_module = validation
            && context
                .validation
                .as_ref()
                .and_then(|metadata| metadata.project_validation.as_ref())
                .is_some_and(|provenance| provenance.backend == "go");
        if validation {
            // A project validation can wait in the local queue after its
            // admission fence. Recheck the retained project plan after that
            // wait and before entering the native validation process path.
            if let Err(error) = crate::webcodex_runner::validation::project::fence(
                &policy,
                &project_registry_dir,
                &operation,
            ) {
                self.fail_job(&operation, error, None);
                return;
            }
        }
        let capture_cargo_test_count = context.validation.as_ref().is_some_and(|metadata| {
            metadata.adapter == "cargo_test"
                && metadata.kind == "test"
                && metadata.no_run != Some(true)
        });
        if login && explicit_shell != Some(ExecutionShell::Bash) {
            self.fail_job(
                &operation,
                "bash login mode requires shell=bash".to_string(),
                None,
            );
            return;
        }
        if !policy.allow_raw_shell {
            self.fail_job(
                &operation,
                "raw shell is disabled by local Runner policy".to_string(),
                None,
            );
            return;
        }
        if context.ssh_resource.is_some() {
            self.start_ssh_shell_job(generation, policy, ssh, operation);
            return;
        }
        let cwd_path = cwd
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        if let Err(e) = cwd_allowed(&policy, &cwd_path) {
            self.fail_job(&operation, e, None);
            return;
        }
        let prepared_profile = match resolve_prepared_shell_profile(
            generation,
            &shell,
            &project_registry_dir,
            &cwd_path,
            cwd.is_some(),
            &self.prepared_profiles,
            Some(self.shutting_down.as_ref()),
        ) {
            Ok(profile) => profile,
            Err(e) => {
                self.fail_job(&operation, e, None);
                return;
            }
        };
        if validation
            && steps.iter().any(|step| {
                !step.is_structured_pytest()
                    && !step.is_structured_ruff()
                    && !step.is_structured_node_check()
                    && !step.is_structured_node_tap_test()
                    && !validation_module_available(
                        &shell,
                        prepared_profile.as_deref(),
                        &cwd_path,
                        step,
                        Some(self.shutting_down.as_ref()),
                    )
            })
        {
            self.fail_job(
                &operation,
                VALIDATION_TOOL_UNAVAILABLE_CODE.to_string(),
                Some(ShellJobValidationProgress {
                    completed: 0,
                    current_step: None,
                    failed_step: None,
                }),
            );
            return;
        }
        let step_count = if validation { steps.len() } else { 1 };
        let mut commands = VecDeque::with_capacity(step_count);
        for index in 0..step_count {
            let configured = if validation && steps[index].is_structured_pytest() {
                crate::webcodex_runner::shell::configured_pytest_job_command(
                    &shell,
                    prepared_profile.as_deref(),
                    &steps[index].args,
                    &cwd_path,
                    Some(self.shutting_down.as_ref()),
                )
            } else if validation
                && (steps[index].is_structured_node_check()
                    || steps[index].is_structured_node_tap_test())
            {
                crate::webcodex_runner::shell::configured_node_project_check_job_command(
                    &shell,
                    prepared_profile.as_deref(),
                    &steps[index],
                    &cwd_path,
                    Some(self.shutting_down.as_ref()),
                )
            } else if validation && steps[index].is_structured_ruff() {
                crate::webcodex_runner::shell::configured_ruff_job_command(
                    &shell,
                    prepared_profile.as_deref(),
                    &steps[index],
                    &cwd_path,
                    Some(self.shutting_down.as_ref()),
                )
            } else if validation {
                configured_validation_job_command(
                    &shell,
                    prepared_profile.as_deref(),
                    &steps[index].program,
                    &steps[index].args,
                    &cwd_path,
                )
            } else {
                let raw_command = raw_command
                    .as_deref()
                    .expect("typed raw shell Job carries command text");
                match explicit_shell {
                    Some(selection) => configured_explicit_shell_command(
                        &shell,
                        prepared_profile.as_deref(),
                        selection,
                        login,
                        raw_command,
                    ),
                    None => match prepared_profile.as_deref() {
                        Some(profile) => {
                            configured_prepared_shell_job_command(profile, raw_command)
                        }
                        None => configured_shell_job_command(&shell, raw_command),
                    },
                }
            };
            let mut command = match configured {
                Ok(command) => command,
                Err(error) => {
                    self.fail_job(&operation, error, None);
                    return;
                }
            };
            if validation {
                command.envs(
                    steps[index]
                        .env
                        .iter()
                        .map(|(key, value)| (key.as_str(), value.as_str())),
                );
                if project_go_single_module {
                    command.envs(GO_PROJECT_SINGLE_MODULE_ENV);
                }
            }
            // Raw Shell Jobs and every validation step have no stdin payload.
            // Never inherit the Runner's parent-liveness pipe: it stays open
            // while Desktop is alive and can stall native commands or let a
            // child consume input owned by the Runner. Match the sync path.
            command
                .current_dir(&cwd_path)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            commands.push_back(command);
        }
        // Profile initialization and interpreter probes may themselves change
        // project files. For native Node scripts repeat the authoritative
        // manifest/argv/cwd fence after ALL preparation, immediately before
        // handing the prepared command to the managed process worker.
        // Node's later manifest read is not an atomic source snapshot.
        if validation
            && steps
                .iter()
                .any(|step| step.is_structured_node_check() || step.is_structured_node_tap_test())
        {
            if let Err(error) = crate::webcodex_runner::validation::project::fence(
                &policy,
                &project_registry_dir,
                &operation,
            ) {
                self.fail_job(&operation, error, None);
                return;
            }
        }
        self.start_prepared_shell_job(PreparedShellJob {
            job_id,
            operation,
            policy,
            steps,
            timeout_secs,
            validation,
            capture_cargo_test_count,
            step_count,
            commands,
            prepared_profile,
        });
    }
}
