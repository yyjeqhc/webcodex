//! Bounded subprocess capture for native service adapters and metadata probes.
use std::io;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub(crate) trait CommandOutputExt {
    fn bounded_output(&mut self) -> io::Result<Output>;
    fn output_with_limits(&mut self, deadline: Duration, limit: usize) -> io::Result<Output>;
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
            unix::capture(self, timeout, limit)
        }
        #[cfg(windows)]
        {
            windows::capture(self, timeout, limit)
        }
        #[cfg(not(any(unix, windows)))]
        {
            compile_error!("native capture requires Unix or Windows");
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
