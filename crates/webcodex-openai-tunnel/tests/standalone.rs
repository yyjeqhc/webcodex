#![cfg(feature = "cli")]
mod support;
use serde_json::{json, Value};
use std::{path::Path, process::Stdio, time::Duration};
use support::*;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};
use webcodex_openai_tunnel::HealthSnapshot;

fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_webcodex-tunnel"));
    // Never use workstation credentials or route fixture traffic through its proxy.
    for name in [
        "CONTROL_PLANE_API_KEY",
        "CONTROL_PLANE_TUNNEL_ID",
        "CONTROL_PLANE_BASE_URL",
        "MCP_SERVER_URL",
        "MCP_AUTHORIZATION",
        "HEALTH_LISTEN_ADDR",
        "WEBCODEX_TUNNEL_STATE_DIR",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        command.env_remove(name);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}
fn configured(cp: &Server, mcp: &Server, root: &Path) -> Command {
    let mut command = cli();
    command
        .args([
            "run",
            "--json",
            "--stop-on-stdin-eof",
            "--health.listen-addr",
            "127.0.0.1:0",
            "--state-dir",
        ])
        .arg(root)
        .env("CONTROL_PLANE_BASE_URL", &cp.url)
        .env("CONTROL_PLANE_TUNNEL_ID", "tunnel_fixture")
        .env("CONTROL_PLANE_API_KEY", "control-secret")
        .env("MCP_SERVER_URL", format!("{}/mcp", mcp.url));
    command.stdin(Stdio::piped());
    command
}
async fn launch(command: &mut Command) -> (Child, String) {
    let mut child = command.spawn().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(5), output.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    assert!(line.len() < 1024);
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["event"], "started");
    let address = value["health_address"].as_str().unwrap().to_string();
    // Retain the stdout pipe with the owned child; shutdown drains the bounded
    // lifecycle records alongside stderr without inheriting the terminal.
    child.stdout = Some(output.into_inner());
    (child, address)
}
async fn shutdown(mut child: Child) -> std::process::Output {
    drop(child.stdin.take());
    // The CLI now grants admitted work a ten-second graceful drain budget.
    // Keep a bounded outer deadline above it, including process reaping.
    tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .unwrap()
        .unwrap()
}
fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}
async fn snapshot(address: &str) -> HealthSnapshot {
    let bytes = http()
        .get(format!("http://{address}/health"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
async fn ready(address: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !snapshot(address).await.ready {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn settled_cli(address: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while snapshot(address).await.unsettled_commands != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn markers(root: &Path) -> usize {
    // Stable .lock files carry mutual exclusion, not unconfirmed execution.
    std::fs::read_dir(root)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "active")
        })
        .count()
}
fn no_secret(output: &std::process::Output) {
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        for secret in ["control-secret", "local-secret", "fixture-shard"] {
            assert!(!text.contains(secret));
        }
    }
}

#[tokio::test]
async fn doctor_help_and_version_do_not_need_network_or_create_state() {
    let cp = Server::new().await;
    let mcp = Server::new().await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("unused");
    for flag in ["--help", "--version"] {
        assert!(cli().arg(flag).output().await.unwrap().status.success());
    }
    let out = cli()
        .args(["doctor", "--json", "--state-dir"])
        .arg(&root)
        .env("CONTROL_PLANE_API_KEY", "control-secret")
        .env("CONTROL_PLANE_TUNNEL_ID", "tunnel_fixture")
        .env("CONTROL_PLANE_BASE_URL", &cp.url)
        .env("MCP_SERVER_URL", &mcp.url)
        .output()
        .await
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["network_checked"],
        false
    );
    assert!(!root.exists());
    no_secret(&out);
}

#[tokio::test]
async fn standalone_forwards_without_local_auth_and_reports_status_then_stops_cleanly() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let tmp = tempfile::tempdir().unwrap();
    let mut command = configured(&cp, &mcp, tmp.path());
    let mut polls = Polls::start(&mut cp, vec![json!([command_for_test()])]);
    let (child, address) = launch(&mut command).await;
    let request = mcp.next().await;
    assert!(!request.headers.contains_key("authorization"));
    request.respond(200, r#"{"jsonrpc":"2.0","id":"one","result":{}}"#);
    polls.response().await.respond(200, "{}");
    ready(&address).await;
    settled_cli(&address).await;
    let status = cli()
        .args(["status", "--json", "--health.listen-addr", &address])
        .output()
        .await
        .unwrap();
    assert!(status.status.success());
    no_secret(&status);
    assert_eq!(
        serde_json::from_slice::<Value>(&status.stdout).unwrap()["health"]["ready"],
        true
    );
    assert_eq!(markers(tmp.path()), 1);
    let stopped = shutdown(child).await;
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    assert_eq!(markers(tmp.path()), 0);
    no_secret(&stopped);
    assert!(mcp.requests.try_recv().is_err());
}
fn command_for_test() -> Value {
    support::command("one")
}

#[tokio::test]
async fn local_health_is_passive_and_second_owner_is_refused() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let tmp = tempfile::tempdir().unwrap();
    let (child, address) = launch(&mut configured(&cp, &mcp, tmp.path())).await;
    let held_poll = cp.next().await;
    assert_eq!(
        http()
            .get(format!("http://{address}/healthz"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        http()
            .get(format!("http://{address}/readyz"))
            .send()
            .await
            .unwrap()
            .status(),
        503
    );
    assert_eq!(
        http()
            .post(format!("http://{address}/health"))
            .send()
            .await
            .unwrap()
            .status(),
        405
    );
    assert_eq!(
        http()
            .get(format!("http://{address}/anything"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert!(!snapshot(&address).await.ready);
    assert!(mcp.requests.try_recv().is_err());
    let other = tokio::time::timeout(
        Duration::from_secs(5),
        configured(&cp, &mcp, tmp.path()).output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!other.status.success());
    no_secret(&other);
    assert!(String::from_utf8_lossy(&other.stderr).contains("already running"));
    assert!(cp.requests.try_recv().is_err());
    held_poll.respond(204, "");
    let stopped = shutdown(child).await;
    assert!(stopped.status.success());
    assert_eq!(markers(tmp.path()), 0);
}

#[tokio::test]
async fn interrupted_effect_retains_restart_latch_and_cannot_dispatch_again() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let tmp = tempfile::tempdir().unwrap();
    let mut command = configured(&cp, &mcp, tmp.path());
    let _polls = Polls::start(&mut cp, vec![json!([command_for_test()])]);
    let (child, _address) = launch(&mut command).await;
    let held = mcp.next().await;
    let stopped = shutdown(child).await;
    assert!(!stopped.status.success());
    assert_eq!(markers(tmp.path()), 1);
    no_secret(&stopped);
    let second = configured(&cp, &mcp, tmp.path()).output().await.unwrap();
    assert!(!second.status.success());
    no_secret(&second);
    assert!(mcp.requests.try_recv().is_err());
    drop(held);
}

#[tokio::test]
async fn explicit_file_authorization_only_reaches_the_local_target() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let tmp = tempfile::tempdir().unwrap();
    let secret = tmp.path().join("local-auth");
    std::fs::write(&secret, "Bearer local-secret\n").unwrap();
    let root = tmp.path().join("runs");
    let mut command = configured(&cp, &mcp, &root);
    command.args(["--mcp.authorization", &format!("file:{}", secret.display())]);
    let mut polls = Polls::start(&mut cp, vec![json!([command_for_test()])]);
    let (child, address) = launch(&mut command).await;
    let request = mcp.next().await;
    assert_eq!(request.headers["authorization"], "Bearer local-secret");
    request.respond(200, r#"{"jsonrpc":"2.0","id":"one","result":{}}"#);
    polls.response().await.respond(200, "{}");
    settled_cli(&address).await;
    let stopped = shutdown(child).await;
    assert!(stopped.status.success());
    no_secret(&stopped);
    assert_eq!(markers(&root), 0);
}
