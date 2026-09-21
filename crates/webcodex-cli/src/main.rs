fn main() -> Result<(), Box<dyn std::error::Error>> {
    webcodex_runner_config::isolate_webpi_process_environment();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(webcodex_cli::run())
}
