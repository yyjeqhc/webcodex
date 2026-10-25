#![cfg(all(feature = "cli", unix))]
mod support;
#[path = "support/credential_transition.rs"]
mod transition_support;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use support::*;
use transition_support::*;

// Keep process creation in its own integration-test process. Fork/exec may
// temporarily inherit other threads' open file descriptions and their flock
// ownership even when those descriptors have close-on-exec set.
use std::{collections::BTreeMap, os::unix::fs::PermissionsExt, process::Stdio};
use tokio::process::Command;

fn private_header(path: &Path, value: &str) {
    std::fs::write(path, value).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn protected_files(root: &Path) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(protected_files(&path));
        } else {
            files.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    files
}
fn cli_command(cp: &Server, mcp: &Server, root: &Path, old_header: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_webcodex-tunnel"));
    // Explicit child-local fixture configuration: no workstation credentials,
    // proxy discovery, inherited terminal or persistent runtime is involved.
    command
        .env_clear()
        .env("CONTROL_PLANE_BASE_URL", &cp.url)
        .env("CONTROL_PLANE_TUNNEL_ID", "tunnel_fixture")
        .env("CONTROL_PLANE_API_KEY", "control-secret")
        .env("MCP_SERVER_URL", format!("{}/mcp", mcp.url))
        .args(["authorize-local-credential", "--json", "--state-dir"])
        .arg(root)
        .arg("--mcp.authorization")
        .arg(format!("file:{}", old_header.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}
async fn output(command: &mut Command) -> std::process::Output {
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .unwrap()
        .unwrap();
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        for secret in ["control-secret", "old-local", "new-local", "literal-secret"] {
            assert!(!text.contains(secret), "credential appeared in CLI output");
        }
    }
    output
}
fn options(command: &mut Command, reference: &str, time: &str, revision: &str, confirm: bool) {
    command.args([
        "--new-mcp-authorization",
        reference,
        "--enqueue-after",
        time,
        "--expected-revision",
        revision,
    ]);
    if confirm {
        command.arg("--confirm-future-enqueue-cutoff");
    }
}

#[tokio::test]
async fn cli_authorizes_only_explicit_private_input_without_updating_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let root = base.join("protected");
    let old_header = base.join("old-header");
    let new_header = base.join("new-header");
    private_header(&old_header, "Bearer old-local\n");
    private_header(&new_header, "Bearer new-local\n");
    let original_config = std::fs::read(&old_header).unwrap();
    let reference = format!("file:{}", new_header.display());
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let (initial, guard) = bound(&cp, &mcp, "old-local")
        .with_restart_recovery(&root, None, |_| true)
        .unwrap();
    drop(initial);
    guard.finish().unwrap();
    let original_history = protected_files(&root);

    for (new, time, revision, confirm) in [
        (reference.as_str(), CUTOFF, "0", false),
        (reference.as_str(), "invalid", "0", true),
        (reference.as_str(), CUTOFF, "invalid", true),
        (reference.as_str(), CUTOFF, "1", true),
        ("Bearer literal-secret", CUTOFF, "0", true),
    ] {
        let mut child = cli_command(&cp, &mcp, &root, &old_header);
        options(&mut child, new, time, revision, confirm);
        assert!(!output(&mut child).await.status.success());
        assert_eq!(protected_files(&root), original_history);
        assert!(cp.requests.try_recv().is_err());
        assert!(mcp.requests.try_recv().is_err());
    }
    // File provenance is checked before any persistent authorization append.
    let public = base.join("public-header");
    private_header(&public, "Bearer new-local\n");
    std::fs::set_permissions(&public, std::fs::Permissions::from_mode(0o644)).unwrap();
    let symlink = base.join("symlink-header");
    std::os::unix::fs::symlink(&new_header, &symlink).unwrap();
    let fifo = base.join("fifo-header");
    let fifo_name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
    for path in [&public, &symlink, &fifo] {
        let mut child = cli_command(&cp, &mcp, &root, &old_header);
        options(
            &mut child,
            &format!("file:{}", path.display()),
            CUTOFF,
            "0",
            true,
        );
        assert!(!output(&mut child).await.status.success());
        assert_eq!(protected_files(&root), original_history);
    }
    for replayed in [false, true] {
        let mut child = cli_command(&cp, &mcp, &root, &old_header);
        options(&mut child, &reference, CUTOFF, "0", true);
        let output = output(&mut child).await;
        assert!(output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["event"], "local_credential_authorized");
        assert_eq!(value["revision"], 1);
        assert_eq!(value["replayed"], replayed);
        assert_eq!(value["configuration_updated"], false);
        assert_eq!(value["network_checked"], false);
        assert_eq!(std::fs::read(&old_header).unwrap(), original_config);
        assert_eq!(std::fs::read(&new_header).unwrap(), b"Bearer new-local\n");
        assert!(cp.requests.try_recv().is_err());
        assert!(mcp.requests.try_recv().is_err());
    }
    assert!(bound(&cp, &mcp, "old-local")
        .with_restart_recovery(&root, None, |_| true)
        .is_err());
    let (new, guard) = bound(&cp, &mcp, "new-local")
        .with_restart_recovery(&root, None, |_| true)
        .unwrap();
    batch(
        new,
        &mut cp,
        &mut mcp,
        vec![
            stamped(support::command("stale"), Some(json!(CUTOFF))),
            stamped(support::command("fresh"), Some(json!(AFTER))),
        ],
        &[("POST", "fresh")],
        1,
        "Bearer new-local",
    )
    .await;
    guard.finish().unwrap();
}
