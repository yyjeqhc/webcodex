//! Bounded subprocess capture for native service adapters and metadata probes.
use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub(crate) trait CommandOutputExt {
    fn bounded_output(&mut self) -> io::Result<Output>;
    fn output_with_limits(&mut self, deadline: Duration, limit: usize) -> io::Result<Output>;
}
fn reader(stream: impl Read + Send + 'static, limit: usize) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stream
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut bytes)
            .and_then(|_| {
                if bytes.len() > limit {
                    Err(io::Error::other("process output exceeded its bound"))
                } else {
                    Ok(bytes)
                }
            });
        let _ = send.send(result);
    });
    receive
}
impl CommandOutputExt for Command {
    fn bounded_output(&mut self) -> io::Result<Output> {
        self.output_with_limits(Duration::from_secs(55), 128 * 1024)
    }
    fn output_with_limits(&mut self, timeout: Duration, limit: usize) -> io::Result<Output> {
        self.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            self.process_group(0);
        }
        let mut child = self.spawn()?;
        let stdout = reader(child.stdout.take().expect("piped stdout"), limit);
        let stderr = reader(child.stderr.take().expect("piped stderr"), limit);
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                #[cfg(unix)]
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "process deadline exceeded; reconcile its external result before retrying",
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let out = stdout.recv_timeout(remaining).map_err(|_| {
            io::Error::new(io::ErrorKind::TimedOut, "process stdout did not close")
        })??;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let err = stderr.recv_timeout(remaining).map_err(|_| {
            io::Error::new(io::ErrorKind::TimedOut, "process stderr did not close")
        })??;
        Ok(Output {
            status,
            stdout: out,
            stderr: err,
        })
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn deadlines_and_output_bounds_are_enforced() {
        assert!(Command::new("/bin/sh")
            .args(["-c", "sleep 2"])
            .output_with_limits(Duration::from_millis(40), 1024)
            .is_err());
        assert!(Command::new("/bin/sh")
            .args(["-c", "printf 123456789"])
            .output_with_limits(Duration::from_secs(1), 4)
            .is_err());
    }
}
