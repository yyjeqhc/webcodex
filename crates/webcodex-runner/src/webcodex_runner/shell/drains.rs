//! Continuous stdout/stderr drains and bounded collection.

use super::*;

pub(super) struct ContinuousPipeDrain {
    pub(super) stdout_rx: mpsc::Receiver<Result<BoundedPipeTail, String>>,
    pub(super) stderr_rx: mpsc::Receiver<Result<BoundedPipeTail, String>>,
    pub(super) stdout_handle: std::thread::JoinHandle<()>,
    pub(super) stderr_handle: std::thread::JoinHandle<()>,
}

impl ContinuousPipeDrain {
    /// Take both child pipes and start independent bounded readers immediately.
    ///
    /// The readers own the OS pipe handles for the full child lifetime. They do
    /// not forward per-chunk data through another bounded queue, so Server/model
    /// observation cannot backpressure either stream. Each reader retains only
    /// the existing bounded tail plus UTF-8/BOM validation state.
    pub(super) fn start(child: &mut ManagedChild, max_output_bytes: usize) -> Result<Self, String> {
        let stdout = child
            .child_mut()
            .stdout
            .take()
            .ok_or_else(|| "stdout pipe missing".to_string())?;
        let stderr = match child.child_mut().stderr.take() {
            Some(stderr) => stderr,
            None => {
                drop(stdout);
                return Err("stderr pipe missing".to_string());
            }
        };
        let (stdout_tx, stdout_rx) = mpsc::sync_channel(1);
        let stdout_handle = std::thread::spawn(move || {
            let _ = stdout_tx.send(read_bounded_pipe_tail(stdout, max_output_bytes, "stdout"));
        });
        let (stderr_tx, stderr_rx) = mpsc::sync_channel(1);
        let stderr_handle = std::thread::spawn(move || {
            let _ = stderr_tx.send(read_bounded_pipe_tail(stderr, max_output_bytes, "stderr"));
        });
        Ok(Self {
            stdout_rx,
            stderr_rx,
            stdout_handle,
            stderr_handle,
        })
    }

    pub(super) fn finish_until(
        self,
        deadline: Instant,
    ) -> Result<(BoundedPipeTail, BoundedPipeTail), String> {
        let Self {
            stdout_rx,
            stderr_rx,
            stdout_handle,
            stderr_handle,
        } = self;
        let receive = |rx: mpsc::Receiver<Result<BoundedPipeTail, String>>,
                       stream_name: &'static str|
         -> Result<BoundedPipeTail, String> {
            let remaining = deadline.saturating_duration_since(Instant::now());
            rx.recv_timeout(remaining).map_err(|_| {
                format!("{stream_name} reader did not finish before cleanup deadline")
            })?
        };
        // Always give both already-running readers the same drain deadline.
        // A read error on one stream must not short-circuit collection of the
        // other stream and turn an otherwise bounded reader into a detached one.
        let stdout = receive(stdout_rx, "stdout");
        let stderr = receive(stderr_rx, "stderr");
        if stdout_handle.is_finished() {
            let _ = stdout_handle.join();
        }
        if stderr_handle.is_finished() {
            let _ = stderr_handle.join();
        }
        match (stdout, stderr) {
            (Ok(stdout), Ok(stderr)) => Ok((stdout, stderr)),
            (Err(stdout), Ok(_)) => Err(stdout),
            (Ok(_), Err(stderr)) => Err(stderr),
            (Err(stdout), Err(stderr)) => Err(format!("{stdout}; {stderr}")),
        }
    }
}

/// Finish one command whose stdout/stderr readers have already been draining
/// continuously since immediately after spawn. Tree termination still happens
/// before waiting for reader EOF so a descendant that inherited a pipe cannot
/// keep the readers alive after direct-child exit, stop, or timeout.
///
/// Tree cleanup/reaping and pipe drain are separate bounded phases. A saturated
/// CI/runtime scheduler can consume the tree-cleanup budget after the child has
/// already exited; reusing that expired deadline for readers would incorrectly
/// turn a known terminal status into `OutcomeUnknown`. stdout/stderr still share
/// one drain deadline, so neither stream receives an independently reset budget.
pub(super) fn terminate_and_collect_pipes(
    mut child: ManagedChild,
    drains: ContinuousPipeDrain,
) -> Result<(std::process::ExitStatus, BoundedPipeTail, BoundedPipeTail), String> {
    let cleanup_deadline = Instant::now() + PROCESS_TREE_CLEANUP_TIMEOUT;
    let cleanup = terminate_child_process_tree_until(&mut child, cleanup_deadline).err();
    let status = wait_child_until(&mut child, cleanup_deadline);
    let drain_deadline = Instant::now() + PROCESS_PIPE_DRAIN_TIMEOUT;
    let output = drains.finish_until(drain_deadline);
    let mut errors = Vec::new();
    if let Some(cleanup) = cleanup {
        errors.push(format!(
            "failed to terminate command process tree: {cleanup}"
        ));
    }
    let status = match status {
        Ok(status) => Some(status),
        Err(error) => {
            errors.push(error);
            None
        }
    };
    let output = match output {
        Ok(output) => Some(output),
        Err(error) => {
            errors.push(format!("failed to collect output: {error}"));
            None
        }
    };
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    let (stdout, stderr) = output.expect("output exists when no collection error was recorded");
    Ok((
        status.expect("status exists when no wait error was recorded"),
        stdout,
        stderr,
    ))
}

/// Test/error-path convenience wrapper. Production structured execution starts
/// the drain immediately after spawn rather than waiting until termination.
#[cfg(test)]
pub(super) fn terminate_and_read_pipes(
    mut child: ManagedChild,
    max_output_bytes: usize,
) -> Result<(std::process::ExitStatus, BoundedPipeTail, BoundedPipeTail), String> {
    let drains = match ContinuousPipeDrain::start(&mut child, max_output_bytes) {
        Ok(drains) => drains,
        Err(error) => {
            let cleanup = terminate_child_process_tree(&mut child).err();
            return Err(with_cleanup_error(error, cleanup));
        }
    };
    terminate_and_collect_pipes(child, drains)
}
