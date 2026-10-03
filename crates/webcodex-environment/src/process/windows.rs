//! Reuse the native pre-resume Job Object owner. Peek before each pipe read,
//! rather than spawning blocking reader threads which can outlive their caller.
use std::io::{self, Read};
use std::os::windows::io::AsRawHandle;
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use webcodex_process::{ManagedChild, SpawnOptions};
use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

fn drain(
    stream: &mut (impl Read + AsRawHandle),
    bytes: &mut Vec<u8>,
    limit: usize,
) -> io::Result<bool> {
    let mut buffer = [0; 8192];
    for _ in 0..16 {
        let mut available = 0u32;
        // Anonymous stdio pipes support PeekNamedPipe. We are their only reader,
        // so reading at most the observed available bytes cannot await new input.
        let ok = unsafe {
            PeekNamedPipe(
                stream.as_raw_handle().cast(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let error = io::Error::last_os_error();
            return match error.raw_os_error().map(|n| n as u32) {
                Some(ERROR_BROKEN_PIPE | ERROR_NO_DATA | ERROR_PIPE_NOT_CONNECTED) => Ok(true),
                _ => Err(error),
            };
        }
        if available == 0 {
            return Ok(false);
        }
        let n = (available as usize)
            .min(buffer.len())
            .min(limit.saturating_sub(bytes.len()).saturating_add(1));
        match stream.read(&mut buffer[..n]) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                bytes.extend_from_slice(&buffer[..n]);
                if bytes.len() > limit {
                    return Err(io::Error::other("process output exceeded its bound"));
                }
            }
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
    // All current native service/probe callers are noninteractive; their only
    // extra creation flag is CREATE_NO_WINDOW. ManagedChild owns spawn flags.
    let mut child = ManagedChild::spawn_with_options(
        command,
        SpawnOptions {
            windows_creation_flags: CREATE_NO_WINDOW,
            ..SpawnOptions::default()
        },
    )?;
    let mut stdout = child.child_mut().stdout.take().expect("piped stdout");
    let mut stderr = child.child_mut().stderr.take().expect("piped stderr");
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (mut out_closed, mut err_closed) = (false, false);
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut,"process or output deadline exceeded; reconcile its external result before retrying"));
        }
        if !out_closed {
            out_closed = drain(&mut stdout, &mut out, limit)?;
        }
        if !err_closed {
            err_closed = drain(&mut stderr, &mut err, limit)?;
        }
        if out_closed && err_closed {
            if let Some(status) = child.try_wait()? {
                return Ok(Output {
                    status,
                    stdout: out,
                    stderr: err,
                });
            }
        }
        std::thread::sleep(
            Duration::from_millis(2).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    // ManagedChild's kill-on-close job guard runs on success/error/unwind and
    // owns even descendants of an already-exited leader. No reader threads exist.
}

#[cfg(test)]
mod tests;
