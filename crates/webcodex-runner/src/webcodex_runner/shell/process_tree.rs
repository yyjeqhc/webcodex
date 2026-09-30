//! Managed process-tree termination and absolute cleanup deadlines.

use super::*;

pub(super) fn with_cleanup_error(base: impl Into<String>, cleanup: Option<String>) -> String {
    match cleanup {
        Some(cleanup) => format!("{}; cleanup failed: {}", base.into(), cleanup),
        None => base.into(),
    }
}

/// Terminate a command and its entire managed process tree, then confirm the
/// whole tree exited and reap the direct child. Callers do this before waiting
/// for output pipes, so descendants cannot keep them open after a timeout,
/// stop, executor failure, or direct-child exit.
///
/// The whole tree is owned by the [`ManagedChild`]: a private process group on
/// Unix, a kill-on-close Job Object on Windows. No pid/pgid is handled here
/// directly.
pub(super) fn terminate_child_process_tree(child: &mut ManagedChild) -> Result<(), String> {
    terminate_child_process_tree_until(child, Instant::now() + PROCESS_TREE_CLEANUP_TIMEOUT)
}

/// Terminate the managed command tree within one overall cleanup deadline.
///
/// Every phase recomputes its remaining budget from the single deadline, so an
/// expired graceful phase can never silently reuse a stale deadline for the
/// force-confirmation wait.
///
/// 1. Graceful request ([`GracefulTermination`]): on Unix this delivers SIGTERM
///    to the whole managed process group, which gets a bounded grace
///    ([`PROCESS_GROUP_TERMINATION_GRACE`]) to exit on its own; on Windows the
///    request reports `Unsupported` and the next phase escalates immediately.
/// 2. Force phase: `terminate_tree` for anything still alive.
/// 3. Whole-tree exit confirmation: `wait_tree_exit`, not just the direct
///    child (a direct-child exit never proves the tree is gone).
/// 4. Direct-child reap within the remaining budget.
pub(super) fn terminate_child_process_tree_until(
    child: &mut ManagedChild,
    deadline: Instant,
) -> Result<(), String> {
    let mut errors = Vec::new();
    let mut tree_exited = false;
    let mut force_tree = false;
    match child.request_terminate_tree() {
        Ok(GracefulTermination::Requested) => {
            let grace_deadline = deadline.min(Instant::now() + PROCESS_GROUP_TERMINATION_GRACE);
            let grace = grace_deadline.saturating_duration_since(Instant::now());
            match child.wait_tree_exit(grace) {
                Ok(true) => tree_exited = true,
                Ok(false) => force_tree = true,
                Err(error) => {
                    errors.push(format!(
                        "failed to wait for command process tree graceful exit: {error}"
                    ));
                    force_tree = true;
                }
            }
        }
        // Once the backend reports that the tree is already gone, do not
        // probe the numeric Unix process-group id again. The direct child may
        // already have been reaped, allowing that id to be reused by an
        // unrelated process group between calls.
        Ok(GracefulTermination::AlreadyExited) => tree_exited = true,
        Ok(GracefulTermination::Unsupported) => force_tree = true,
        Err(error) => {
            errors.push(format!(
                "failed to request graceful command process tree termination: {error}"
            ));
            force_tree = true;
        }
    }
    if force_tree {
        if let Err(error) = child.terminate_tree() {
            errors.push(format!("failed to terminate command process tree: {error}"));
        }
    }
    if !tree_exited {
        // Confirm the complete tree exited, not just the direct child.
        // Forceful termination can complete asynchronously (notably Job Object
        // teardown on Windows), so use the remaining cleanup budget.
        let remaining = deadline.saturating_duration_since(Instant::now());
        match child.wait_tree_exit(remaining) {
            Ok(true) => {}
            Ok(false) => {
                errors.push("command process tree did not exit before deadline".to_string())
            }
            Err(error) => errors.push(format!(
                "failed to wait for command process tree exit: {error}"
            )),
        }
    }
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    errors.push("command child reap timed out".to_string());
                    break;
                }
                std::thread::sleep(Duration::from_millis(10).min(remaining));
            }
            Err(error) => {
                errors.push(format!("failed to reap command child: {error}"));
                break;
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

pub(super) fn terminate_child_without_output(mut child: ManagedChild) -> Result<(), String> {
    let result = terminate_child_process_tree(&mut child);
    // The direct child has been reaped above. Closing the local pipe handles
    // is sufficient on error paths where the response intentionally has no
    // command output.
    drop(child.child_mut().stdout.take());
    drop(child.child_mut().stderr.take());
    result
}

pub(super) fn wait_child_until(
    child: &mut ManagedChild,
    deadline: Instant,
) -> Result<std::process::ExitStatus, String> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err("command child wait timed out".to_string());
                }
                std::thread::sleep(Duration::from_millis(10).min(remaining));
            }
            Err(error) => return Err(format!("failed to wait command: {error}")),
        }
    }
}
