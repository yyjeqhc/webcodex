//! Non-Unix fallback; process-tree policy remains the platform adapter's job.
use std::io::{self, Read};
use std::process::{Child, Command, Output};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Owner(Child);
impl Drop for Owner {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}
fn reader(stream: impl Read + Send + 'static, limit: usize) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stream.take(limit.saturating_add(1) as u64).read_to_end(&mut bytes)
            .and_then(|_| if bytes.len() > limit { Err(io::Error::other("process output exceeded its bound")) } else { Ok(bytes) });
        let _ = send.send(result);
    });
    receive
}
pub(super) fn capture(command: &mut Command, timeout: Duration, limit: usize) -> io::Result<Output> {
    let mut child = Owner(command.spawn()?);
    let stdout = reader(child.0.stdout.take().expect("piped stdout"), limit);
    let stderr = reader(child.0.stderr.take().expect("piped stderr"), limit);
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.0.try_wait()? { break status; }
        if Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "process deadline exceeded")); }
        std::thread::sleep(Duration::from_millis(20));
    };
    let out = stdout.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "process stdout did not close"))??;
    let err = stderr.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "process stderr did not close"))??;
    Ok(Output { status, stdout: out, stderr: err })
}
