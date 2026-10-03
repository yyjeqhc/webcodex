//! Poll owned nonblocking pipes: no detached reader threads. Keep the leader
//! waitable until process-group cleanup so its PID/PGID cannot be recycled.
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

struct Owner {
    child: Child,
    group_owned: bool,
}
impl Owner {
    fn exited(&self) -> io::Result<bool> {
        // SAFETY: valid owned child PID and writable siginfo. WNOWAIT observes
        // without reaping; cleanup retains the process-group identity anchor.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id() as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { info.si_pid() } != 0)
    }
    fn stop_group(&self) {
        // The leader has not been reaped anywhere in this module. Negative PID
        // addresses only the process group created for this exact invocation.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if self.group_owned {
            self.stop_group();
        }
        let _ = self.child.wait();
    }
}

fn nonblocking(fd: i32) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn drain(stream: &mut impl Read, bytes: &mut Vec<u8>, limit: usize) -> io::Result<bool> {
    let mut buffer = [0; 8192];
    // Yield after bounded work so a continuously writing child cannot postpone
    // deadline checks or starve the other pipe.
    for _ in 0..16 {
        let remaining = limit
            .saturating_sub(bytes.len())
            .saturating_add(1)
            .min(buffer.len());
        match stream.read(&mut buffer[..remaining]) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                bytes.extend_from_slice(&buffer[..n]);
                if bytes.len() > limit {
                    return Err(io::Error::other("process output exceeded its bound"));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

pub(super) fn capture(
    command: &mut Command,
    timeout: Duration,
    limit: usize,
) -> io::Result<Output> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| io::Error::other("invalid process deadline"))?;
    command.process_group(0);
    let mut owner = Owner {
        child: command.spawn()?,
        group_owned: true,
    };
    let mut stdout = owner.child.stdout.take().expect("piped stdout");
    let mut stderr = owner.child.stderr.take().expect("piped stderr");
    nonblocking(stdout.as_raw_fd())?;
    nonblocking(stderr.as_raw_fd())?;
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (mut out_closed, mut err_closed) = (false, false);
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut,
                "process or output deadline exceeded; reconcile its external result before retrying"));
        }
        if !out_closed {
            out_closed = drain(&mut stdout, &mut out, limit)?;
        }
        if !err_closed {
            err_closed = drain(&mut stderr, &mut err, limit)?;
        }
        if out_closed && err_closed && owner.exited()? {
            owner.stop_group();
            // Cleanup was completed before wait/reaping. Do not let Drop signal
            // a potentially recycled PGID after that point.
            owner.group_owned = false;
            let status = owner.child.wait()?;
            return Ok(Output {
                status,
                stdout: out,
                stderr: err,
            });
        }
        let mut fds = [
            libc::pollfd {
                fd: if out_closed { -1 } else { stdout.as_raw_fd() },
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: if err_closed { -1 } else { stderr.as_raw_fd() },
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let wait_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(20) as i32;
        if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, wait_ms) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}
