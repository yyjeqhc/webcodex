//! Native argv + open input using the existing managed tree/streaming worker.
//! No PTY, implicit shell state, detached supervisor or terminal Session.
use super::shell_worker::PreparedShellJob;
use super::*;

impl JobManager {
    pub(super) fn start_interactive_process(&self, start: PendingJobStart) {
        let RunnerJobOperation::StartInteractiveProcess(request) = &start.operation else {
            unreachable!()
        };
        let prepared = (|| {
            if request.stdin.is_some() {
                return Err("interactive_process_requires_job_write_input".into());
            }
            let launch = prepare_detached_process_launch(
                start.generation,
                &start.policy,
                &start.shell,
                &start.project_registry_dir,
                &self.prepared_profiles,
                request.cwd.as_deref(),
                &request.process.executable,
                &request.process.args,
                request.timeout_secs,
                Some(self.shutting_down.as_ref()),
            )?;
            let mut command = super::super::shell::structured_process_command(
                std::ffi::OsStr::new(&launch.process.executable),
                &launch.process.args,
                Some(Path::new(&launch.cwd)),
            )?;
            command
                .env_clear()
                .envs(launch.env)
                .current_dir(&launch.cwd)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            Ok::<_, String>((command, launch.timeout_secs))
        })();
        let (command, timeout_secs) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.fail_job(&start.operation, error, None);
                return;
            }
        };
        self.start_prepared_shell_job(PreparedShellJob {
            job_id: request.job_id.clone(),
            operation: start.operation,
            policy: start.policy,
            steps: vec![],
            timeout_secs,
            validation: false,
            capture_cargo_test_count: false,
            step_count: 1,
            commands: VecDeque::from([command]),
            prepared_profile: None,
        });
    }
}
