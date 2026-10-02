//! Runner-owned output draining, bounded process-tree convergence and validation evidence.
//! Native process ownership remains in `webcodex-process`; this module does not own Jobs.
use super::config::ShellConfig;
use super::output_text::{OutputTextDecoder, OutputTextSource};
use super::shell::{configured_validation_job_command, PreparedShellProfile};
use super::shutdown::lock_unpoison;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use webcodex_core::runner_protocol::{
    validation_infrastructure_failure_code, ShellJobTestCountEvidence, ShellJobValidationStep,
    VALIDATION_STEP_WAIT_FAILED_CODE,
};
use webcodex_process::{GracefulTermination, ManagedChild};

#[derive(Debug)]
pub(crate) enum OutputChunk {
    Stdout(String),
    Stderr(String),
}

pub(crate) fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    tx: mpsc::SyncSender<OutputChunk>,
    stdout: bool,
    source: OutputTextSource,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // A bounded channel plus fixed-size reads prevents a fast child (or
        // one enormous line) from retaining unbounded output in the runner
        // while a transport send is slow.
        let mut buf = [0_u8; 8 * 1024];
        let mut decoder = OutputTextDecoder::new(source);
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let text = decoder.push(&[], true);
                    if !text.is_empty() {
                        let _ = if stdout {
                            tx.send(OutputChunk::Stdout(text))
                        } else {
                            tx.send(OutputChunk::Stderr(text))
                        };
                    }
                    break;
                }
                Ok(read) => {
                    let text = decoder.push(&buf[..read], false);
                    if !text.is_empty() {
                        let _ = if stdout {
                            tx.send(OutputChunk::Stdout(text))
                        } else {
                            tx.send(OutputChunk::Stderr(text))
                        };
                    }
                }
                Err(_) => {
                    let text = decoder.push(&[], true);
                    if !text.is_empty() {
                        let _ = if stdout {
                            tx.send(OutputChunk::Stdout(text))
                        } else {
                            tx.send(OutputChunk::Stderr(text))
                        };
                    }
                    break;
                }
            }
        }
    })
}

pub(crate) fn drain_output_chunks(
    rx: &mpsc::Receiver<OutputChunk>,
    stdout: &mut String,
    stderr: &mut String,
) {
    while let Ok(chunk) = rx.try_recv() {
        match chunk {
            OutputChunk::Stdout(text) => stdout.push_str(&text),
            OutputChunk::Stderr(text) => stderr.push_str(&text),
        }
    }
}

/// Drain the bounded output channel while joining reader threads until
/// `deadline`. Draining and joining must progress together: a reader can be
/// blocked in `SyncSender::send` after the child exits, so waiting for the
/// reader before draining the channel creates a terminal-output race and can
/// drop the final validation summary. Returns the number of readers detached
/// after the existing bounded cleanup deadline.
pub(crate) fn drain_and_join_reader_threads_until(
    mut readers: Vec<std::thread::JoinHandle<()>>,
    rx: &mpsc::Receiver<OutputChunk>,
    stdout: &mut String,
    stderr: &mut String,
    deadline: Instant,
) -> usize {
    loop {
        drain_output_chunks(rx, stdout, stderr);
        let mut index = 0;
        while index < readers.len() {
            if readers[index].is_finished() {
                let reader = readers.swap_remove(index);
                let _ = reader.join();
            } else {
                index += 1;
            }
        }
        if readers.is_empty() {
            // A finished reader may have sent its final chunk just before the
            // join became observable.
            drain_output_chunks(rx, stdout, stderr);
            return 0;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            // Dropping a JoinHandle detaches it. The output channel is bounded,
            // so an abnormal pipe holder cannot retain unbounded runner memory
            // or block process shutdown.
            drain_output_chunks(rx, stdout, stderr);
            return readers.len();
        }
        std::thread::sleep(Duration::from_millis(10).min(remaining));
    }
}

pub(crate) fn wait_failure_error(validation: bool, error: &std::io::Error) -> String {
    if validation {
        VALIDATION_STEP_WAIT_FAILED_CODE.to_string()
    } else {
        format!("failed to wait job: {error}")
    }
}

pub(crate) fn validation_failed_step(
    status: &str,
    error: Option<&str>,
    step_name: &str,
) -> Option<String> {
    (status == "failed"
        && error
            .and_then(validation_infrastructure_failure_code)
            .is_none())
    .then(|| step_name.to_string())
}

pub(crate) fn validation_module_available(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    cwd: &Path,
    step: &ShellJobValidationStep,
    shutdown: Option<&AtomicBool>,
) -> bool {
    if step.program != "python" {
        return true;
    }
    let Some(module) = step
        .args
        .windows(2)
        .find(|p| p[0] == "-m")
        .map(|p| p[1].as_str())
    else {
        return false;
    };
    const PROBE: &str =
        "import importlib.util,sys;sys.exit(0 if importlib.util.find_spec(sys.argv[1]) else 42)";
    let args = ["-I", "-c", PROBE, module].map(str::to_string);
    let Ok(mut command) =
        configured_validation_job_command(shell, profile, &step.program, &args, cwd)
    else {
        return false;
    };
    command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(child) = ManagedChild::spawn(&mut command) else {
        return false;
    };
    let child = Arc::new(Mutex::new(child));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let wait_result = {
            let mut child = lock_unpoison(&child);
            child.try_wait()
        };
        match wait_result {
            Ok(Some(status)) => {
                let success = status.success();
                let _ = terminate_managed_tree(&child);
                return success;
            }
            Ok(None) => {
                if shutdown.is_some_and(|flag| flag.load(Ordering::SeqCst))
                    || Instant::now() >= deadline
                {
                    let _ = terminate_managed_tree(&child);
                    return false;
                }
            }
            Err(_) => {
                let _ = terminate_managed_tree(&child);
                return false;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Forcefully terminate the entire process tree owned by a job.
///
/// The platform detail (SIGKILL to a private process group on Unix,
/// `TerminateJobObject` on Windows) stays inside `webcodex-process`.
pub(crate) fn terminate_managed_tree(child: &Arc<Mutex<ManagedChild>>) -> Result<(), String> {
    lock_unpoison(child)
        .terminate_tree()
        .map_err(|error| error.to_string())
}

/// Request graceful tree termination, escalating to force termination where the
/// platform cannot deliver a graceful signal to the whole tree (Windows Job
/// Objects). An already-exited tree is idempotent success.
pub(crate) fn request_terminate_managed_tree(
    child: &Arc<Mutex<ManagedChild>>,
) -> Result<(), String> {
    // Bind the result first so the temporary `MutexGuard` is released before
    // the match arms: the Unsupported arm re-locks the same mutex, and a guard
    // still alive across the match would deadlock that re-lock.
    let outcome = lock_unpoison(child).request_terminate_tree();
    match outcome {
        Ok(GracefulTermination::Requested | GracefulTermination::AlreadyExited) => Ok(()),
        Ok(GracefulTermination::Unsupported) => terminate_managed_tree(child),
        Err(error) => Err(error.to_string()),
    }
}

/// Wait, bounded by `deadline`, until the managed process tree is empty.
///
/// Returns `Ok(true)` when the tree exited within the budget, `Ok(false)` when
/// the deadline elapsed first, and `Err` on a platform failure. The lock is
/// held while polling, so a concurrent `terminate_tree` blocks only until this
/// bounded wait returns; nothing here waits on another thread's progress.
pub(crate) fn wait_managed_tree_exit(
    child: &Arc<Mutex<ManagedChild>>,
    deadline: Instant,
) -> Result<bool, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Ok(false);
    }
    lock_unpoison(child)
        .wait_tree_exit(remaining)
        .map_err(|error| error.to_string())
}

/// Non-blocking probe: is the managed process tree still running?
///
/// Opportunistically reaps the direct child so a Unix zombie is not mistaken
/// for a live tree member. A busy lock or a platform probe failure is treated
/// conservatively as "still running".
pub(crate) fn managed_tree_running(child: &Arc<Mutex<ManagedChild>>) -> bool {
    let mut guard = match child.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::WouldBlock) => return true,
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
    };
    if guard.try_wait().is_err() {
        return true;
    }
    match guard.try_tree_exit() {
        Ok(true) => false,
        Ok(false) => true,
        Err(_) => true,
    }
}

/// Complete a job step's tree lifecycle after the direct child's status has
/// been decided: give the tree a short bounded window to exit on its own,
/// force-terminate whatever remains, and confirm the tree emptied, so
/// pipe-holding descendants cannot stall the output readers forever.
pub(crate) fn cleanup_managed_tree(child: &Arc<Mutex<ManagedChild>>) {
    const NATURAL_EXIT_GRACE: Duration = Duration::from_millis(500);
    const FORCE_EXIT_GRACE: Duration = Duration::from_millis(500);
    let natural_deadline = Instant::now() + NATURAL_EXIT_GRACE;
    if !wait_managed_tree_exit(child, natural_deadline).unwrap_or(false) {
        let _ = terminate_managed_tree(child);
        let force_deadline = Instant::now() + FORCE_EXIT_GRACE;
        let _ = wait_managed_tree_exit(child, force_deadline);
    }
}

/// Best-effort bounded reap of the direct child. Returns `Ok(true)` once the
/// direct child has been reaped, `Ok(false)` if the deadline elapsed first
/// (including when another thread reaped it concurrently), and `Err` on a wait
/// failure. The lock is only ever taken briefly with `try_lock`.
pub(crate) fn reap_managed_direct_child(
    child: &Arc<Mutex<ManagedChild>>,
    deadline: Instant,
) -> Result<bool, String> {
    loop {
        let mut guard = match child.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        match guard.try_wait() {
            Ok(Some(_)) => return Ok(true),
            Ok(None) => {}
            Err(error) => return Err(error.to_string()),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(10).min(remaining));
    }
}

pub(crate) fn observe_cargo_test_count_chunks(
    accumulator: &mut Option<webcodex_core::cargo_test_count::CargoTestRunMetadataAccumulator>,
    stdout: &str,
    stderr: &str,
) {
    if let Some(accumulator) = accumulator.as_mut() {
        accumulator.push_stdout_chunk(stdout);
        accumulator.push_stderr_chunk(stderr);
    }
}

pub(crate) fn finish_cargo_test_count_evidence(
    accumulator: Option<webcodex_core::cargo_test_count::CargoTestRunMetadataAccumulator>,
) -> Option<ShellJobTestCountEvidence> {
    accumulator.map(|accumulator| {
        let metadata = accumulator.finish();
        ShellJobTestCountEvidence {
            tests_detected: metadata.tests_detected,
            tests_run_count: metadata.tests_run_count,
            status: metadata.count_evidence_status,
        }
    })
}
