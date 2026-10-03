//! Real candidate binary acceptance; never launches a configured service.
use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn run(config: &Path, check: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_webcodex-runner"));
    if check {
        command.arg("--check-config");
    }
    let mut child = command
        .arg("--config")
        .arg(config)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("configuration preflight tried to keep running");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn native_preflight_checks_current_config_without_network_or_registry_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let config = temp.path().join("runner.toml");
    let registry = temp.path().join("not-created");
    let text=format!("server_url='http://{}'\nclient_id='preflight'\ntoken='PRIVATE_TEST_TOKEN'\nproject_registry_dir={}\n[policy]\nallow_cwd_anywhere=true\n",
        listener.local_addr().unwrap(),serde_json::to_string(&registry.to_string_lossy()).unwrap());
    std::fs::write(&config, &text).unwrap();
    let output = run(&config, true);
    assert!(
        output.status.success(),
        "preflight failed with {:?}",
        output.status.code()
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "WebCodex Runner configuration valid"
    );
    assert!(output.stderr.is_empty());
    assert!(!registry.exists());
    assert_eq!(std::fs::read_to_string(&config).unwrap(), text);
    assert_eq!(listener.accept().unwrap_err().kind(), ErrorKind::WouldBlock);
}

#[test]
fn native_deterministic_config_errors_exit_two_and_preflight_redacts_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("runner.toml");
    for text in [
        "server_url='http://127.0.0.1:9'\nclient_id='preflight'\nprojects_dir='SECRET_RETIRED_PATH'\n",
        "server_url='http://127.0.0.1:9'\nclient_id='preflight'\npoll_interval_ms=0\ntoken='SECRET_TOKEN'\n",
        "server_url = [ SECRET_BROKEN_TOML",
    ] {
        std::fs::write(&config,text).unwrap();
        let output=run(&config,true);
        assert_eq!(output.status.code(),Some(2));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("SECRET_"));
        assert!(output.stdout.is_empty());
        assert_eq!(std::fs::read_to_string(&config).unwrap(),text);
        // systemd observes the same deterministic code from the actual entrypoint.
        assert_eq!(run(&config,false).status.code(),Some(2));
    }
    std::fs::remove_file(&config).unwrap();
    assert_eq!(run(&config, true).status.code(), Some(2));
    eprintln!("RUNNER_CONFIG_PREFLIGHT native_validation=true config_invalid_exit=2 starts=0 network_calls=0");
}
