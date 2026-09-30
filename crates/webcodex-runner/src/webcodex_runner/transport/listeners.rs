//! Shutdown, reload, service-stop, and parent-liveness listeners.

use super::*;

pub(super) async fn async_sleep_or_shutdown(delay: Duration, runtime: &RunnerRuntimeState) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        _ = runtime.wait_for_shutdown() => true,
    }
}

pub(super) async fn future_or_shutdown<F>(
    future: F,
    runtime: &RunnerRuntimeState,
) -> Option<F::Output>
where
    F: std::future::Future,
{
    tokio::select! {
        result = future => Some(result),
        _ = runtime.wait_for_shutdown() => None,
    }
}

pub(super) fn install_shutdown_listener(
    runtime: RunnerRuntimeState,
) -> Result<std::thread::JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("webcodex-runner-shutdown".to_string())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async {
                tokio::select! {
                    _ = shutdown_signal() => runtime.request_shutdown_signal(),
                    _ = runtime.wait_for_shutdown() => {}
                }
            });
        })
        .map_err(|_| "failed to start process shutdown signal listener".to_string())
}

#[cfg(windows)]
pub(super) fn install_service_stop_listener(
    runtime: RunnerRuntimeState,
    stop: webcodex_environment::service::runtime::ServiceStop,
) -> Result<std::thread::JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("webcodex-runner-scm-stop".to_string())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                runtime.request_shutdown_signal();
                return;
            };
            rt.block_on(async {
                tokio::select! {
                    _ = stop.cancelled() => runtime.request_shutdown_signal(),
                    _ = runtime.wait_for_shutdown() => {},
                }
            });
        })
        .map_err(|_| "failed to start SCM stop listener".to_string())
}

#[cfg(windows)]
pub(super) const PARENT_PIPE_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[cfg(windows)]
pub(super) fn install_parent_liveness_listener(runtime: RunnerRuntimeState) -> Result<(), String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_PIPE};

    let stdin = std::io::stdin();
    let handle = stdin.as_raw_handle() as usize;
    if unsafe { GetFileType(handle as _) } == FILE_TYPE_PIPE {
        let listener = spawn_windows_pipe_parent_liveness_listener(handle, runtime)?;
        // The process owns stdin for its whole lifetime. The listener stops on
        // pipe EOF/error or an already-requested Runner shutdown, so detaching
        // the JoinHandle does not transfer ownership of any external resource.
        drop(listener);
        return Ok(());
    }

    install_blocking_parent_liveness_listener(runtime)
}

#[cfg(not(windows))]
pub(super) fn install_parent_liveness_listener(runtime: RunnerRuntimeState) -> Result<(), String> {
    install_blocking_parent_liveness_listener(runtime)
}

pub(super) fn install_blocking_parent_liveness_listener(
    runtime: RunnerRuntimeState,
) -> Result<(), String> {
    use std::io::Read;

    let listener = std::thread::Builder::new()
        .name("webcodex-runner-parent-lease".to_string())
        .spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buffer = [0_u8; 64];
            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) | Err(_) => {
                        runtime.request_shutdown_signal();
                        return;
                    }
                    Ok(_) => {}
                }
            }
        })
        .map_err(|_| "failed to start parent-liveness listener".to_string())?;
    // Non-pipe stdin may require a genuinely blocking read. Keep the legacy
    // detached behavior for consoles and Unix streams; process exit reclaims
    // the listener while EOF still triggers exact-generation shutdown.
    drop(listener);
    Ok(())
}

#[cfg(windows)]
pub(super) fn spawn_windows_pipe_parent_liveness_listener(
    pipe_handle: usize,
    runtime: RunnerRuntimeState,
) -> Result<std::thread::JoinHandle<()>, String> {
    use windows_sys::Win32::Storage::FileSystem::ReadFile;
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;

    std::thread::Builder::new()
        .name("webcodex-runner-parent-lease".to_string())
        .spawn(move || {
            let pipe = pipe_handle as windows_sys::Win32::Foundation::HANDLE;
            let mut discard = [0_u8; 64];
            loop {
                if runtime.shutdown_requested() {
                    return;
                }

                let mut available = 0_u32;
                let peeked = unsafe {
                    PeekNamedPipe(
                        pipe,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                };
                if peeked == 0 {
                    // Preserve the historical lease contract: any stdin read
                    // failure is equivalent to parent EOF and requests Runner
                    // shutdown. Broken anonymous pipes land here without a
                    // blocking ReadFile on the Windows startup path.
                    runtime.request_shutdown_signal();
                    return;
                }

                if available == 0 {
                    std::thread::sleep(PARENT_PIPE_POLL_INTERVAL);
                    continue;
                }

                let to_read = available.min(discard.len() as u32);
                let mut bytes_read = 0_u32;
                let read = unsafe {
                    ReadFile(
                        pipe,
                        discard.as_mut_ptr(),
                        to_read,
                        &mut bytes_read,
                        std::ptr::null_mut(),
                    )
                };
                if read == 0 || bytes_read == 0 {
                    runtime.request_shutdown_signal();
                    return;
                }
            }
        })
        .map_err(|_| "failed to start parent-liveness listener".to_string())
}

#[cfg(unix)]
pub(super) async fn shutdown_signal() {
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = async {
            if let Some(signal) = sigterm.as_mut() {
                let _ = signal.recv().await;
            } else {
                std::future::pending::<()>().await;
            }
        } => {}
    }
}

#[cfg(not(unix))]
pub(super) async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(unix)]
pub(crate) fn install_reload_listener(
    runtime: Arc<ReloadableRunnerConfig>,
) -> Result<std::thread::JoinHandle<()>, String> {
    let signal_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "failed to initialize config reload signal listener".to_string())?;
    let mut sighup = {
        let _guard = signal_runtime.enter();
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .map_err(|_| "failed to install config reload signal listener".to_string())?
    };
    std::thread::Builder::new()
        .name("webcodex-runner-reload".to_string())
        .spawn(move || {
            signal_runtime.block_on(async move {
                while !runtime.is_stopping() {
                    match tokio::time::timeout(Duration::from_millis(100), sighup.recv()).await {
                        Ok(Some(_)) => {
                            runtime.reload();
                        }
                        Ok(None) => break,
                        Err(_) => {}
                    }
                }
            });
        })
        .map_err(|_| "failed to start config reload signal listener".to_string())
}
