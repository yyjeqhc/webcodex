use super::*;

#[tokio::test]
async fn cloudflare_expired_start_deadline_does_not_spawn_a_child() {
    let result = CloudflareTransport::start_quick(
        Path::new("nonexistent-cloudflared"),
        "http://127.0.0.1:2345",
        Instant::now(),
    )
    .await;
    assert!(result.err().unwrap().message.contains("timed out"));
}

#[test]
fn cloudflare_output_discards_oversized_lines_and_keeps_draining() {
    let mut bytes = vec![b'x'; MAX_LINE_BYTES + 1];
    bytes.extend_from_slice(b"https://poison.trycloudflare.com\n");
    bytes.extend_from_slice(b"cloudflared version 2026.7.3 (built 2026-07-01)\n");
    bytes.extend_from_slice(b"https://correct.trycloudflare.com");
    let mut lines = Vec::new();
    read_bounded_lines(bytes.as_slice(), |line| lines.push(line.to_string()));
    assert_eq!(lines.len(), 2);
    assert_eq!(parse_version(&lines[0]), Some((2026, 7, 3)));
    assert_eq!(
        super::super::share_service::parse_quick_tunnel_url(&lines[1]),
        Some("https://correct.trycloudflare.com".to_string())
    );
    assert!(!lines.iter().any(|line| line.contains("poison")));
}

#[test]
fn cloudflare_output_bounds_unterminated_and_invalid_utf8_lines() {
    let mut bytes = vec![0xff, b'\n'];
    bytes.extend_from_slice(&vec![b'x'; MAX_LINE_BYTES]);
    let mut sizes = Vec::new();
    read_bounded_lines(bytes.as_slice(), |line| sizes.push(line.len()));
    assert_eq!(sizes, vec![MAX_LINE_BYTES]);
    sizes.clear();
    bytes.push(b'x');
    read_bounded_lines(bytes.as_slice(), |line| sizes.push(line.len()));
    assert!(sizes.is_empty());
}

#[test]
fn cloudflare_environment_is_a_positive_allowlist() {
    let directory = Path::new("/isolated-runtime");
    let inputs = [
        ("PATH", "/bin"),
        ("SystemRoot", "C:\\Windows"),
        ("HOME", "/ambient-home"),
        ("USERPROFILE", "ambient-profile"),
        ("TUNNEL_TOKEN", "secret-tunnel"),
        ("TUNNEL_TOKEN_FILE", "ambient-token"),
        ("CLOUDFLARE_API_TOKEN", "secret-cloudflare"),
        ("OPENAI_API_KEY", "secret-openai"),
        ("WEBCODEX_BOOTSTRAP_KEY", "secret-webcodex"),
        ("HTTP_PROXY", "https://secret-proxy"),
        ("LD_PRELOAD", "/untrusted-library"),
    ];
    let command = isolated_command_with(
        Path::new("cloudflared"),
        directory,
        inputs
            .into_iter()
            .map(|(name, value)| (name.into(), value.into())),
    );
    let selected = command
        .get_envs()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.unwrap().to_string_lossy().into_owned(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(selected["PATH"], "/bin");
    assert_eq!(selected["SystemRoot"], "C:\\Windows");
    for key in ["HOME", "USERPROFILE", "APPDATA", "XDG_CONFIG_HOME"] {
        assert_eq!(selected[key], directory.to_string_lossy());
    }
    assert_eq!(selected.len(), 6);
    assert!(!format!("{selected:?}").contains("secret-"));
}

#[test]
fn cloudflare_origins_and_versions_fail_closed() {
    for value in [
        "http://127.0.0.1:2345",
        "http://localhost:2345",
        "http://[::1]:2345",
    ] {
        assert!(validate_ingress(value).is_ok());
    }
    for value in [
        "http://example.com",
        "http://127.0.0.1/mcp",
        "http://secret@localhost",
        "http://localhost?token=secret",
    ] {
        assert!(validate_ingress(value).is_err());
    }
    assert!(validate_origin("https://mcp.example.com").is_ok());
    for value in [
        "http://mcp.example.com",
        "https://token@mcp.example.com",
        "https://mcp.example.com/mcp",
        "https://mcp.example.com?secret",
    ] {
        assert!(validate_origin(value).is_err());
    }
    assert_eq!(
        parse_version("cloudflared version 2025.4.0 (built yesterday)"),
        Some(MIN_VERSION)
    );
    assert_eq!(parse_version("cloudflared version 2025.4.0-evil"), None);
    assert_eq!(parse_version("anything version 2026.7.3"), None);
    assert_eq!(parse_version("cloudflared version 2026.7.3.4"), None);
    for value in [
        "https://user:secret@a.trycloudflare.com",
        "https://a.trycloudflare.com:8443",
        "https://a.trycloudflare.com/mcp",
        "https://a.trycloudflare.com?token=secret",
    ] {
        assert!(super::super::share_service::parse_quick_tunnel_url(value).is_none());
    }
}

#[cfg(unix)]
mod real_process {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fixture(body: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cloudflared");
        std::fs::write(&path, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'cloudflared version 2026.7.3'; exit 0; fi\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        (directory, path)
    }

    fn live(pid: i32) -> bool {
        #[cfg(target_os = "linux")]
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            if let Some((_, rest)) = stat.rsplit_once(')') {
                if matches!(rest.split_whitespace().next(), Some("Z" | "X")) {
                    return false;
                }
            }
        }
        unsafe { libc::kill(pid, 0) == 0 }
    }

    async fn descendant(path: &Path) -> i32 {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(pid) = std::fs::read_to_string(path) {
                if let Ok(pid) = pid.trim().parse() {
                    return pid;
                }
            }
            assert!(
                Instant::now() < deadline,
                "fixture did not publish descendant identity"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn assert_dead(pid: i32) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while live(pid) {
            assert!(
                Instant::now() < deadline,
                "owned descendant survived cleanup"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn descendant_body(pid_file: &Path, suffix: &str) -> String {
        // Fixture paths are generated private temp paths, never external input.
        format!(
            "(while :; do sleep 1; done) &\necho $! > '{}'\n{suffix}",
            pid_file.display()
        )
    }

    #[tokio::test]
    #[ignore = "real process lifecycle evidence; run cloudflare_transport::tests::real_process with --ignored --test-threads=1"]
    async fn cloudflare_real_process_stop_and_eof_drain_owned_tree() {
        let pid_dir = tempfile::tempdir().unwrap();
        let pid_file = pid_dir.path().join("pid");
        let body = descendant_body(&pid_file, "if read input; then exit 88; fi\n[ -z \"${TUNNEL_TOKEN+x}${OPENAI_API_KEY+x}${WEBCODEX_BOOTSTRAP_KEY+x}\" ] || exit 89\necho https://ready.trycloudflare.com\necho 'Registered tunnel connection' >&2\nwhile :; do sleep 1; done");
        let (_directory, binary) = fixture(&body);
        let (url, mut transport) = CloudflareTransport::start_quick(
            &binary,
            "http://127.0.0.1:2345",
            Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap();
        assert_eq!(url, "https://ready.trycloudflare.com");
        let pid = descendant(&pid_file).await;
        assert!(transport.is_alive().unwrap());
        transport.stop().await.unwrap();
        assert_dead(pid).await;
        assert!(!transport.is_alive().unwrap());
        transport.stop().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "real process lifecycle evidence; run cloudflare_transport::tests::real_process with --ignored --test-threads=1"]
    async fn cloudflare_real_process_cancelled_startup_reaps_descendants() {
        let pid_dir = tempfile::tempdir().unwrap();
        let pid_file = pid_dir.path().join("pid");
        let (_directory, binary) =
            fixture(&descendant_body(&pid_file, "while :; do sleep 1; done"));
        let task = tokio::spawn(async move {
            CloudflareTransport::start_quick(
                &binary,
                "http://localhost:2345",
                Instant::now() + Duration::from_secs(30),
            )
            .await
        });
        let pid = descendant(&pid_file).await;
        task.abort();
        assert!(matches!(task.await, Err(error) if error.is_cancelled()));
        assert_dead(pid).await;
    }

    #[tokio::test]
    #[ignore = "real process lifecycle evidence; run cloudflare_transport::tests::real_process with --ignored --test-threads=1"]
    async fn cloudflare_real_process_timeout_and_parent_exit_reap_descendants() {
        for parent_exits in [false, true] {
            let pid_dir = tempfile::tempdir().unwrap();
            let pid_file = pid_dir.path().join("pid");
            let suffix = if parent_exits {
                "echo secret-token-and-authorization >&2\nexit 9"
            } else {
                "while :; do sleep 1; done"
            };
            let (_directory, binary) = fixture(&descendant_body(&pid_file, suffix));
            let start = CloudflareTransport::start_quick(
                &binary,
                "http://localhost:2345",
                Instant::now() + Duration::from_millis(400),
            );
            let (result, pid) = tokio::join!(start, descendant(&pid_file));
            let error = result.err().expect("fixture should fail startup");
            assert!(!serde_json::to_string(&error)
                .unwrap()
                .contains("secret-token"));
            assert_dead(pid).await;
        }
    }

    #[tokio::test]
    #[ignore = "real process lifecycle evidence; run cloudflare_transport::tests::real_process with --ignored --test-threads=1"]
    async fn cloudflare_real_process_named_uses_only_private_token_file_and_empty_config() {
        let (_directory,binary) = fixture("case \"$*\" in *'--no-autoupdate run --token-file '*) ;; *) exit 90;; esac\n[ \"$1\" = tunnel ] && [ \"$2\" = --config ] || exit 91\n[ \"$(cat \"$3\")\" = '{}' ] || exit 92\nfor arg; do last=\"$arg\"; done\n[ \"$(cat \"$last\")\" = 'private-fixture-token' ] || exit 93\necho 'Registered tunnel connection' >&2\nwhile :; do sleep 1; done");
        let token_dir = tempfile::tempdir().unwrap();
        let token_file = token_dir.path().join("token");
        write_new_private(&token_file, b"private-fixture-token").unwrap();
        let mut transport = CloudflareTransport::start_named(
            &binary,
            "https://mcp.example.test",
            &token_file,
            Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap();
        let owned_dir = transport._directory.path().to_path_buf();
        assert!(owned_dir.join("tunnel-token").is_file());
        transport.stop().await.unwrap();
        drop(transport);
        assert!(!owned_dir.exists());
    }

    #[tokio::test]
    #[ignore = "real process lifecycle evidence; run cloudflare_transport::tests::real_process with --ignored --test-threads=1"]
    async fn cloudflare_real_process_version_drains_both_streams_and_cancellation_owns_tree() {
        let (_directory, binary) = fixture("");
        std::fs::write(&binary, "#!/bin/sh\ni=0\nwhile [ \"$i\" -lt 100 ]; do printf '%4096s\\n' x; printf '%4096s\\n' x >&2; i=$((i+1)); done\necho 'cloudflared version 2026.7.3'\n").unwrap();
        assert_eq!(
            cloudflared_binary_version(&binary).await.unwrap(),
            (2026, 7, 3)
        );

        let pid_dir = tempfile::tempdir().unwrap();
        let pid_file = pid_dir.path().join("pid");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\n{}",
                descendant_body(&pid_file, "while :; do sleep 1; done")
            ),
        )
        .unwrap();
        let task = tokio::spawn(async move { cloudflared_binary_version(&binary).await });
        let pid = descendant(&pid_file).await;
        task.abort();
        assert!(matches!(task.await, Err(error) if error.is_cancelled()));
        assert_dead(pid).await;
    }
}
