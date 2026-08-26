use webcodex::{server_binary_action, ServerBinaryAction};

// The default macOS Tokio worker stack is too small for the deepest MCP request
// path exercised by the local Server. Keep this explicit instead of requiring
// callers to set RUST_MIN_STACK for `webcodex share`.
#[cfg(target_os = "macos")]
const MACOS_SERVER_RUNTIME_STACK_SIZE: usize = 8 * 1024 * 1024;

fn build_server_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    #[cfg(target_os = "macos")]
    builder.thread_stack_size(MACOS_SERVER_RUNTIME_STACK_SIZE);
    builder.build()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match server_binary_action(std::env::args().skip(1)) {
        ServerBinaryAction::Run => {
            webcodex::prepare_server_process_environment().map_err(std::io::Error::other)?;
            build_server_runtime()?.block_on(webcodex::run_server())
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
            stack_size >= MACOS_SERVER_RUNTIME_STACK_SIZE,
            "WebCodex Server worker stack was {stack_size} bytes; expected at least {MACOS_SERVER_RUNTIME_STACK_SIZE}"
        );
    }
}
