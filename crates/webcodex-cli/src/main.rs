fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let service = webcodex_environment::runtime_entry::split_windows_service_args(&args)
        .map_err(std::io::Error::other)?;
    if let Some((name, service_args)) = service {
        #[cfg(windows)]
        {
            // Parse and verify service-only arguments before entering SCM dispatch.
            webcodex_cli::validate_windows_tunnel_service_args(&service_args)
                .map_err(std::io::Error::other)?;
            return webcodex_environment::service::runtime::run_windows_service(
                &name,
                move |stop| {
                    tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())?
                        .block_on(webcodex_cli::run_windows_tunnel_service(service_args, stop))
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
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(webcodex_cli::run())
}
