use salvo::conn::Acceptor;
use salvo::prelude::*;
use salvo::{Server, Service};
use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShutdownReason {
    Sigint,
    Sigterm,
    ParentEof,
}

impl ShutdownReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sigint => "SIGINT",
            Self::Sigterm => "SIGTERM",
            Self::ParentEof => "parent_stdin_eof",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShutdownState {
    Running = 0,
    Draining = 1,
    Stopped = 2,
}

#[derive(Debug)]
pub(crate) struct ShutdownCoordinator {
    state: AtomicU8,
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(ShutdownState::Running as u8),
        }
    }
}

impl ShutdownCoordinator {
    #[cfg(test)]
    fn state(&self) -> ShutdownState {
        match self.state.load(Ordering::Acquire) {
            0 => ShutdownState::Running,
            1 => ShutdownState::Draining,
            2 => ShutdownState::Stopped,
            value => panic!("invalid Server shutdown state {value}"),
        }
    }

    fn begin_draining(&self) -> bool {
        self.state
            .compare_exchange(
                ShutdownState::Running as u8,
                ShutdownState::Draining as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn is_running(&self) -> bool {
        self.state.load(Ordering::Acquire) == ShutdownState::Running as u8
    }

    fn mark_stopped(&self) {
        self.state
            .store(ShutdownState::Stopped as u8, Ordering::Release);
    }
}

#[derive(Clone)]
pub(crate) struct DrainAdmission {
    coordinator: Arc<ShutdownCoordinator>,
}

impl DrainAdmission {
    pub(crate) fn new(coordinator: Arc<ShutdownCoordinator>) -> Self {
        Self { coordinator }
    }
}

#[async_trait]
impl Handler for DrainAdmission {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if !self.coordinator.is_running() {
            res.status_code(StatusCode::SERVICE_UNAVAILABLE);
            res.headers_mut().insert(
                salvo::http::header::RETRY_AFTER,
                salvo::http::HeaderValue::from_static("1"),
            );
            res.render(Json(serde_json::json!({
                "error": "server_draining",
                "message": "Server is draining; retry the request against the replacement Server"
            })));
            ctrl.skip_rest();
            return;
        }
        ctrl.call_next(req, depot, res).await;
    }
}

#[cfg(unix)]
struct TerminationSignals {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl TerminationSignals {
    fn new() -> io::Result<Self> {
        use tokio::signal::unix::{signal, SignalKind};

        Ok(Self {
            interrupt: signal(SignalKind::interrupt())?,
            terminate: signal(SignalKind::terminate())?,
        })
    }

    async fn recv(&mut self) -> ShutdownReason {
        tokio::select! {
            _ = self.interrupt.recv() => ShutdownReason::Sigint,
            _ = self.terminate.recv() => ShutdownReason::Sigterm,
        }
    }
}

#[cfg(not(unix))]
struct TerminationSignals;

#[cfg(not(unix))]
impl TerminationSignals {
    fn new() -> io::Result<Self> {
        Ok(Self)
    }

    async fn recv(&mut self) -> ShutdownReason {
        match tokio::signal::ctrl_c().await {
            Ok(()) => ShutdownReason::Sigint,
            Err(error) => {
                tracing::error!(
                    ?error,
                    "failed to listen for Server Ctrl-C; forcing shutdown"
                );
                ShutdownReason::Sigint
            }
        }
    }
}

pub(crate) async fn serve_until_termination<A, S>(
    server: Server<A>,
    service: S,
    coordinator: Arc<ShutdownCoordinator>,
    graceful_timeout: Duration,
    stop_on_stdin_eof: bool,
) -> io::Result<()>
where
    A: Acceptor + Send,
    S: Into<Service> + Send,
{
    let mut signals = TerminationSignals::new()?;
    let parent_eof = if stop_on_stdin_eof {
        Some(parent_eof_signal()?)
    } else {
        None
    };
    let signal = async move {
        if let Some(parent_eof) = parent_eof {
            tokio::select! {
                reason = signals.recv() => reason,
                _ = parent_eof => ShutdownReason::ParentEof,
            }
        } else {
            signals.recv().await
        }
    };
    serve_with_signal(server, service, coordinator, signal, graceful_timeout).await
}

fn parent_eof_signal() -> io::Result<tokio::sync::oneshot::Receiver<()>> {
    use std::io::Read;

    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("webcodex-server-parent-lease".to_string())
        .spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buffer = [0_u8; 64];
            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(());
                        return;
                    }
                    Ok(_) => {}
                }
            }
        })?;
    Ok(rx)
}

async fn serve_with_signal<A, S, F>(
    server: Server<A>,
    service: S,
    coordinator: Arc<ShutdownCoordinator>,
    signal: F,
    graceful_timeout: Duration,
) -> io::Result<()>
where
    A: Acceptor + Send,
    S: Into<Service> + Send,
    F: Future<Output = ShutdownReason> + Send,
{
    let handle = server.handle();
    let serve = server.try_serve(service);
    tokio::pin!(serve);
    tokio::pin!(signal);

    let reason = tokio::select! {
        result = &mut serve => {
            coordinator.mark_stopped();
            return result;
        }
        reason = &mut signal => reason,
    };

    if coordinator.begin_draining() {
        tracing::info!(
            shutdown_signal = reason.as_str(),
            graceful_deadline_seconds = graceful_timeout.as_secs(),
            shutdown_state = "draining",
            "Server graceful shutdown requested"
        );
        handle.stop_graceful(Some(graceful_timeout));
    }

    let started = Instant::now();
    let deadline = tokio::time::sleep(graceful_timeout);
    tokio::pin!(deadline);
    let (result, deadline_elapsed) = tokio::select! {
        result = &mut serve => (result, false),
        _ = &mut deadline => {
            tracing::warn!(
                shutdown_signal = reason.as_str(),
                elapsed_ms = started.elapsed().as_millis() as u64,
                graceful_deadline_seconds = graceful_timeout.as_secs(),
                "Server graceful shutdown deadline reached; Salvo force-stop escalation is active"
            );
            // `stop_graceful(Some(timeout))` owns the force-stop timer. Keep
            // awaiting the same serve future instead of dropping/cancelling it
            // or duplicating Salvo's escalation lifecycle.
            (serve.await, true)
        }
    };
    coordinator.mark_stopped();
    let elapsed = started.elapsed();
    if deadline_elapsed {
        tracing::warn!(
            shutdown_signal = reason.as_str(),
            elapsed_ms = elapsed.as_millis() as u64,
            graceful_deadline_seconds = graceful_timeout.as_secs(),
            shutdown_state = "stopped",
            "Server shutdown completed after graceful deadline escalation"
        );
    } else {
        tracing::info!(
            shutdown_signal = reason.as_str(),
            elapsed_ms = elapsed.as_millis() as u64,
            graceful_deadline_seconds = graceful_timeout.as_secs(),
            shutdown_state = "stopped",
            "Server graceful shutdown completed"
        );
    }
    result
}

#[cfg(test)]
mod tests;
