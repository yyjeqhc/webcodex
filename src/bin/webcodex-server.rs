use webcodex::{server_binary_action, ServerBinaryAction};

// The default Tokio worker stack on macOS and Windows is too small for the
// deepest Server request paths exercised by local/project runtimes. Keep this
// explicit instead of requiring callers to set RUST_MIN_STACK for
// `webcodex share` or packaged Server launches.
#[cfg(any(target_os = "macos", target_os = "windows"))]
const SERVER_RUNTIME_STACK_SIZE: usize = 8 * 1024 * 1024;

fn server_runtime_stack_size() -> Option<usize> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        Some(SERVER_RUNTIME_STACK_SIZE)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

fn build_server_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    if let Some(stack_size) = server_runtime_stack_size() {
        builder.thread_stack_size(stack_size);
    }
    builder.build()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    webcodex_runner_config::isolate_webpi_process_environment();
    match server_binary_action(std::env::args().skip(1)) {
        ServerBinaryAction::Run { stop_on_stdin_eof } => {
            webcodex::prepare_server_process_environment().map_err(std::io::Error::other)?;
            build_server_runtime()?
                .block_on(webcodex::run_server_with_parent_liveness(stop_on_stdin_eof))
        }
        ServerBinaryAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }
            std::process::exit(code);
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn server_runtime_uses_large_worker_stack() {
        let runtime = build_server_runtime().expect("build WebCodex Server runtime");
        let stack_size = runtime.block_on(async {
            tokio::spawn(async {
                // SAFETY: pthread_self returns the current worker thread and
                // pthread_get_stacksize_np only queries that thread's stack metadata.
                unsafe { libc::pthread_get_stacksize_np(libc::pthread_self()) }
            })
            .await
            .expect("observe WebCodex Server worker")
        });
        assert!(
            stack_size >= SERVER_RUNTIME_STACK_SIZE,
            "WebCodex Server worker stack was {stack_size} bytes; expected at least {SERVER_RUNTIME_STACK_SIZE}"
        );
    }
}
#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;

    #[test]
    fn server_runtime_uses_large_worker_stack_policy() {
        assert_eq!(server_runtime_stack_size(), Some(SERVER_RUNTIME_STACK_SIZE));
        build_server_runtime().expect("build WebCodex Server runtime");
    }
}
