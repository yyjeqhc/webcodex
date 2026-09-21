use super::*;
use salvo::conn::{Acceptor, Listener, TcpListener};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::Instant;

const CONFORMANCE_URL_FILE_ENV: &str = "WEBPI_MCP_CONFORMANCE_URL_FILE";
const CONFORMANCE_STOP_FILE_ENV: &str = "WEBPI_MCP_CONFORMANCE_STOP_FILE";
const CONFORMANCE_FIXTURE_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// External MCP conformance uses a real loopback socket, but the endpoint is
/// still the same production Salvo/AuthMiddleware/mcp_post composition used by
/// the in-process HTTP tests. It is ignored because it deliberately waits for an
/// external referee process; `scripts/mcp_conformance.sh` owns its lifecycle.
#[tokio::test]
#[ignore = "started only by scripts/mcp_conformance.sh with the pinned external MCP referee"]
async fn mcp_conformance_fixture_server() {
    let url_file = PathBuf::from(
        std::env::var_os(CONFORMANCE_URL_FILE_ENV)
            .expect("conformance fixture requires WEBPI_MCP_CONFORMANCE_URL_FILE"),
    );
    let stop_file = PathBuf::from(
        std::env::var_os(CONFORMANCE_STOP_FILE_ENV)
            .expect("conformance fixture requires WEBPI_MCP_CONFORMANCE_STOP_FILE"),
    );

    let config = test_config(None);
    let (_db_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let router = build_test_router(config, db, runtime);
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let addr = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .expect("MCP conformance listener must be TCP");
    let server_task = tokio::spawn(async move {
        Server::new(acceptor).serve(router).await;
    });

    let temporary_url_file = url_file.with_extension("tmp");
    std::fs::write(&temporary_url_file, format!("http://{addr}/mcp\n"))
        .expect("write MCP conformance fixture URL");
    std::fs::rename(&temporary_url_file, &url_file)
        .expect("publish MCP conformance fixture URL atomically");

    let deadline = Instant::now() + CONFORMANCE_FIXTURE_LIFETIME;
    loop {
        if stop_file.exists() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "external MCP conformance fixture exceeded its bounded lifetime"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    server_task.abort();
    let _ = server_task.await;
}
