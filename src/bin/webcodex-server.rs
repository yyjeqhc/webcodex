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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let service = webcodex_environment::runtime_entry::split_windows_service_args(&args)
        .map_err(std::io::Error::other)?;
    if let Some((name, service_args)) = service {
        #[cfg(windows)]
        {
            if service_args.len() != 2
                || service_args[0] != "--env-file"
                || service_args[1].is_empty()
            {
                return Err(
                    std::io::Error::other("Server service requires only --env-file PATH").into(),
                );
            }
            let env_file = std::path::PathBuf::from(&service_args[1]);
            webcodex_environment::runtime_entry::validate_service_env_file(&env_file)
                .map_err(std::io::Error::other)?;
            return webcodex_environment::service::runtime::run_windows_service(
                &name,
                move |stop| {
                    // SCM owns this thread; Server owns a single Tokio runtime and its drain path.
                    let log_dir = env_file
                        .parent()
                        .ok_or("Server service env file has no parent directory")?;
                    let mut service_log = webcodex_environment::service::ServiceLogGuard::open(
                        log_dir,
                        webcodex_environment::service::Component::Server,
                    )?;
                    std::env::set_var("WEBCODEX_SERVICE_LOG_DIR", log_dir);
                    std::env::set_var("WEBCODEX_ENV_FILE", &env_file);
                    webcodex::prepare_server_process_environment()?;
                    let result = build_server_runtime()
                        .map_err(|error| error.to_string())?
                        .block_on(webcodex::run_server_with_shutdown(false, stop.cancelled()))
                        .map_err(|error| error.to_string());
                    if result.is_ok() {
                        service_log.stopped()?;
                    }
                    result
                },
            )
            .map_err(Into::into);
        }
        #[cfg(not(windows))]
        {
            let _ = (name, service_args);
            unreachable!("service prefix rejected on non-Windows");
        }
    }
    match server_binary_action(args) {
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
