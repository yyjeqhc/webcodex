//! Unix process-group backend for [`ManagedChild`].
//!
//! The direct child is made the leader of a new process group (`process_group
//! (0)`), so `pgid == child pid`. Descendants inherit the group. Tree-wide
//! termination is `kill(-pgid, SIGKILL)`; an `ESRCH` result means the group no
//! longer exists and is treated as idempotent success.

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::{GracefulTermination, SpawnOptions};

/// Poll interval used by `wait_tree_exit` and bounded drop reaping.
const TREE_POLL: Duration = Duration::from_millis(20);
/// Maximum time `Drop` spends trying to reap the direct child after SIGKILL.
const DROP_REAP_TIMEOUT: Duration = Duration::from_millis(200);

/// A managed child process plus its entire descendant tree.
pub struct ManagedChild {
    child: Child,
    pgid: u32,
    // Sticky once whole-tree exit is confirmed. After the group disappears
    // and its leader is reaped, this numeric pgid can be reused by an
    // unrelated process group, so later operations must never probe or signal
    // it again.
    tree_exited: AtomicBool,
}

impl std::fmt::Debug for ManagedChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `std::process::Child` has no `Debug`; print identity only.
        f.debug_struct("ManagedChild")
            .field("id", &self.child.id())
            .finish()
    }
}

impl ManagedChild {
    /// Spawn `command` as the leader of a new private process group.
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        Self::spawn_with_options(command, SpawnOptions::default())
    }

    /// Like [`ManagedChild::spawn`]. `options` is accepted for API symmetry;
    /// it has no Unix effect.
    pub fn spawn_with_options(command: &mut Command, _options: SpawnOptions) -> io::Result<Self> {
        // The child becomes the leader of a new process group whose id equals
        // the child's pid; every descendant it spawns inherits the group.
        // Keep this on CommandExt rather than a pre_exec closure: adding any
        // pre_exec hook changes normal Command spawn semantics for ENOEXEC.
        // Callers that need a conflicting setsid pre_exec require a dedicated
        // future spawn mode rather than changing the default contract.
        command.process_group(0);
        let child = command.spawn()?;
        let pgid = child.id();
        Ok(Self {
            child,
            pgid,
            tree_exited: AtomicBool::new(false),
        })
    }

    /// PID of the direct child (which is also the process group id).
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Borrow the underlying [`Child`].
    pub fn child(&self) -> &Child {
        &self.child
    }

    /// Mutably borrow the underlying [`Child`].
    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Wait only for the direct child.
    ///
    /// This must not be read as "the tree has exited": grandchildren in the
    /// process group may still be running. To wait for the whole tree, use
    /// [`ManagedChild::wait_tree_exit`].
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }

    /// Non-blocking check for the direct child.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Forcefully terminate the entire owned process tree via `kill(-pgid,
    /// SIGKILL)`.
    ///
    /// `ESRCH` (group already gone) is treated as idempotent success.
    pub fn terminate_tree(&mut self) -> io::Result<()> {
        if self.tree_exited.load(Ordering::Acquire) {
            return Ok(());
        }
        match signal_group(self.pgid, libc::SIGKILL)? {
            true => Ok(()),
            false => {
                self.tree_exited.store(true, Ordering::Release);
                Ok(())
            }
        }
    }

    /// Non-blocking check whether the owned process tree has fully exited.
    ///
    /// This is the non-blocking counterpart of [`ManagedChild::wait_tree_exit`]:
    /// `Ok(true)` means the group contains no live process right now, `Ok(false)`
    /// means at least one member is still running. Zombies are not considered
    /// live (see [`group_has_live_members`]); callers that hold the only handle
    /// to the direct child should still call `try_wait` to reap it.
    pub fn try_tree_exit(&self) -> io::Result<bool> {
        if self.tree_exited.load(Ordering::Acquire) {
            return Ok(true);
        }
        let exited = !group_has_live_members(self.pgid);
        if exited {
            self.tree_exited.store(true, Ordering::Release);
        }
        Ok(exited)
    }

    /// Request graceful termination of the entire owned process tree.
    ///
    /// Sends `SIGTERM` to the private process group. Returns
    /// [`GracefulTermination::Requested`] when the signal was delivered,
    /// [`GracefulTermination::AlreadyExited`] when the group no longer exists
    /// (`ESRCH`), and an [`io::Error`] for any other failure. The process
    /// group id is deliberately never exposed to callers.
    pub fn request_terminate_tree(&mut self) -> io::Result<GracefulTermination> {
        if self.tree_exited.load(Ordering::Acquire) {
            return Ok(GracefulTermination::AlreadyExited);
        }
        match signal_group(self.pgid, libc::SIGTERM)? {
            true => Ok(GracefulTermination::Requested),
            false => {
                self.tree_exited.store(true, Ordering::Release);
                Ok(GracefulTermination::AlreadyExited)
            }
        }
    }

    /// Wait until the process group contains no live processes.
    ///
    /// Polls with a short bounded interval until the group is gone or `timeout`
    /// elapses. Zombie members are not considered live (see
    /// [`group_has_live_members`]).
    pub fn wait_tree_exit(&self, timeout: Duration) -> io::Result<bool> {
        if self.tree_exited.load(Ordering::Acquire) {
            return Ok(true);
        }
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "tree wait timeout is too large",
            )
        })?;
        loop {
            if !group_has_live_members(self.pgid) {
                self.tree_exited.store(true, Ordering::Release);
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(TREE_POLL);
        }
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        // Best-effort group kill as a fail-safe backstop only while this tree
        // has not already been confirmed gone. Never retarget a known-dead
        // numeric pgid, which may already belong to an unrelated group. `Child`
        // does not reap on drop, so make a short bounded effort to reap our
        // direct child and avoid accumulating zombies in a long-lived Runner.
        if !self.tree_exited.load(Ordering::Acquire)
            && matches!(signal_group(self.pgid, libc::SIGKILL), Ok(false))
        {
            self.tree_exited.store(true, Ordering::Release);
        }
        let Some(deadline) = Instant::now().checked_add(DROP_REAP_TIMEOUT) else {
            return;
        };
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(TREE_POLL),
                Ok(None) => return,
            }
        }
    }
}

/// Signal a whole process group, reporting whether it still existed.
///
/// Returns `Ok(true)` when the signal was delivered, `Ok(false)` for `ESRCH`
/// (the group is already gone), and `Err` for any other failure.
fn signal_group(pgid: u32, signal: i32) -> io::Result<bool> {
    let pgid = i32::try_from(pgid).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "process group id out of range")
    })?;
    // SAFETY: negative pid targets the whole process group. The pgid was
    // recorded from the private group created for this child at spawn time.
    if unsafe { libc::kill(-pgid, signal) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(false);
    }
    Err(error)
}

/// Whether the group still exists as a POSIX entity (any member, including a
/// zombie). The fast probe for [`group_has_live_members`].
fn group_exists(pgid: u32) -> bool {
    let Some(pgid) = i32::try_from(pgid).ok() else {
        return false;
    };
    // SAFETY: signal 0 performs an existence check without delivering a
    // signal to any process in the group.
    if unsafe { libc::kill(-pgid, 0) } == 0 {
        return true;
    }
    // EPERM means the group exists but belongs to another user — which cannot
    // happen for our own group, but conservatively report it as present. Any
    // other error (notably ESRCH) means the group is gone.
    matches!(
        io::Error::last_os_error().raw_os_error(),
        Some(code) if code == libc::EPERM
    )
}

/// Whether the group contains any member that can still execute code.
///
/// `kill(-pgid, 0)` reports a zombie as a live member because it still occupies
/// a process table entry — and in containers whose PID 1 does not reap orphaned
/// grandchildren, a terminated grandchild can linger as a zombie
/// indefinitely, so the group would appear non-empty forever. On Linux we
/// therefore walk `/proc` and ignore zombies (`Z` / `X` states), matching the
/// approach already used by `webcodex-persistent-shell`. On other Unix
/// platforms zombies are reaped promptly by the nearest ancestor or PID 1, so
/// the plain group-exists probe is sufficient.
fn group_has_live_members(pgid: u32) -> bool {
    if !group_exists(pgid) {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            // /proc unavailable (unusual for Linux); fall back to the probe.
            return true;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            // `/proc/<pid>/stat` is `pid (comm) state ppid pgrp ...`; `comm`
            // may contain spaces and parentheses, so take the tokens after the
            // last `)`: index 0 = state, 1 = ppid, 2 = pgrp.
            let Some((_, rest)) = stat.rsplit_once(')') else {
                continue;
            };
            let mut fields = rest.split_whitespace();
            let state = fields.next().unwrap_or("");
            fields.next(); // ppid, unused
            let Some(entry_pgid) = fields.next().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            if entry_pgid == pgid && state != "Z" && state != "X" {
                return true;
            }
        }
        false
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}
