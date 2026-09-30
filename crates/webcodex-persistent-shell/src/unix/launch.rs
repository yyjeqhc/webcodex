//! Unix child spawn guard and secure control descriptors.

use super::*;

#[cfg(unix)]
pub(super) struct SpawnedChildGuard {
    child: Option<Child>,
    pub(super) process_group_id: u32,
}

#[cfg(unix)]
impl SpawnedChildGuard {
    pub(super) fn new(child: Child) -> Self {
        let process_group_id = child.id();
        Self {
            child: Some(child),
            process_group_id,
        }
    }

    pub(super) fn child_mut(&mut self) -> &mut Child {
        self.child
            .as_mut()
            .expect("spawned shell child guard was already disarmed")
    }

    pub(super) fn disarm(mut self) -> Child {
        self.child
            .take()
            .expect("spawned shell child guard was already disarmed")
    }
}

#[cfg(unix)]
impl Drop for SpawnedChildGuard {
    fn drop(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        let _ = signal_process_group(self.process_group_id, libc::SIGTERM);
        let deadline = Instant::now() + PROCESS_SIGNAL_GRACE;
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let _ = signal_process_group(self.process_group_id, libc::SIGKILL);
        let kill_deadline = Instant::now() + PROCESS_SIGNAL_GRACE;
        while Instant::now() < kill_deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn set_close_on_exec(fd: RawFd) -> std::io::Result<()> {
    // `pipe2(O_CLOEXEC)` is unavailable on Darwin. Preserve any existing
    // descriptor flags and add FD_CLOEXEC immediately after creating the pipe.
    // SAFETY: `fd` is owned by a live `File` for the duration of both calls.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags == -1 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn create_control_pipe() -> Result<(File, File), ShellError> {
    let mut fds = [-1_i32; 2];
    // Linux and Android can set close-on-exec atomically. Darwin and other Unix
    // targets use `pipe` followed immediately by `fcntl(FD_CLOEXEC)` because
    // their libc does not expose `pipe2`.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    // SAFETY: `fds` points to two valid integers and `pipe2` initializes both
    // on success.
    let pipe_result = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    // SAFETY: `fds` points to two valid integers and `pipe` initializes both on
    // success.
    let pipe_result = unsafe { libc::pipe(fds.as_mut_ptr()) };

    if pipe_result == -1 {
        return Err(ShellError::new(
            "persistent_shell_spawn_failed",
            format!(
                "failed to create persistent shell control pipe: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    // SAFETY: both descriptors were created by the successful pipe call above
    // and ownership is transferred exactly once to these `File` values.
    let reader = unsafe { File::from_raw_fd(fds[0]) };
    // SAFETY: same ownership argument as for `reader`.
    let writer = unsafe { File::from_raw_fd(fds[1]) };

    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    for fd in [reader.as_raw_fd(), writer.as_raw_fd()] {
        set_close_on_exec(fd).map_err(|error| {
            ShellError::new(
                "persistent_shell_spawn_failed",
                format!("failed to secure persistent shell control pipe: {error}"),
            )
        })?;
    }

    Ok((reader, writer))
}

#[cfg(unix)]
pub(super) fn resolve_control_program(name: &str) -> Result<String, ShellError> {
    for directory in ["/usr/bin", "/bin"] {
        let path = Path::new(directory).join(name);
        if path.is_file() {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    Err(ShellError::new(
        "persistent_shell_spawn_failed",
        format!("required persistent shell control program '{name}' was not found"),
    ))
}
