mod disconnect;
mod oauth;
mod output;
mod probe;
mod process;
mod profile;

pub(crate) use disconnect::{run_disconnect, DisconnectOptions};
pub(crate) use process::{
    local_runner_profile_marker, local_runner_state_summary, run_hosted_log_writer,
    run_local_runner_logs, run_local_runner_service, LocalRunnerServiceAction,
};
pub(crate) use profile::{ConnectAuth, ConnectOptions};

use super::connections::{canonical_server_url, ensure_real_directory_tree};
use super::login::validate_client_id;
use super::profiles::{
    client_output_dir_for_profile, client_state_dir_for_profile, default_client_base_dir,
    default_client_state_base_dir, validate_client_profile,
};
use super::system::discover_internal_binary;
use std::io::Write;
use std::path::PathBuf;

use self::output::render_connect_output;
use self::probe::{preflight_shared_key, wait_for_connection};
use self::process::{
    ensure_runner_unlocked, load_runner_state, local_runner_log_path, process_matches,
    stop_runner_unlocked, RunnerStart,
};
use self::profile::{
    atomic_write, derived_profile, ensure_private_directory, generated_client_id,
    read_existing_agent_config, render_agent_document, render_project_file, resolve_key,
    resolve_project, validate_existing_profile, ProfileLock,
};

const DEFAULT_CONNECT_WAIT_MS: u64 = 15_000;

#[derive(Debug)]
pub(crate) struct ConnectResult {
    output: String,
    disclosure_marker: Option<PathBuf>,
}

pub(crate) fn write_connect_result(
    result: ConnectResult,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), String> {
    stdout
        .write_all(result.output.as_bytes())
        .map_err(|error| format!("failed to write connect output: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("failed to flush connect output: {error}"))?;
    if let Some(marker) = result.disclosure_marker {
        if let Err(error) = atomic_write(&marker, b"disclosed = true\n", false) {
            let _ = writeln!(
                stderr,
                "Warning: the connection is healthy, but WebCodex could not record that the generated key was displayed ({error}). The key may be displayed again on the next connect."
            );
            let _ = stderr.flush();
        }
    }
    Ok(())
}

pub(crate) async fn run_connect(opts: ConnectOptions) -> Result<ConnectResult, String> {
    if opts.auth == ConnectAuth::OAuth {
        return oauth::run_oauth_connect(opts).await;
    }
    run_shared_key_connect(opts).await
}

async fn run_shared_key_connect(opts: ConnectOptions) -> Result<ConnectResult, String> {
    let canonical_server = canonical_server_url(&opts.server_url)?;
    let canonical_project = opts.project.canonicalize().map_err(|error| {
        format!(
            "project path {} does not exist or cannot be resolved: {error}",
            opts.project.display()
        )
    })?;
    if !canonical_project.is_dir() {
        return Err(format!(
            "project path {} is not a directory",
            canonical_project.display()
        ));
    }
    let explicit_profile = opts
        .profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?;
    let config_base = opts
        .config_base
        .clone()
        .map(Ok)
        .unwrap_or_else(default_client_base_dir)?;
    let state_base = opts
        .state_base
        .clone()
        .map(Ok)
        .unwrap_or_else(default_client_state_base_dir)?;
    let resolved_key = resolve_key(
        &opts,
        &config_base,
        &canonical_server.url,
        &canonical_project,
    )?;
    let profile = explicit_profile
        .or(resolved_key.recovered_profile.clone())
        .unwrap_or_else(|| derived_profile(&canonical_server.url, &resolved_key.value));
    let profile = validate_client_profile(&profile)?;
    let config_base = ensure_real_directory_tree(&config_base)?;
    let state_base = ensure_real_directory_tree(&state_base)?;
    let profile_dir =
        ensure_private_directory(&client_output_dir_for_profile(&config_base, &profile))?;
    let state_dir = ensure_private_directory(&client_state_dir_for_profile(&state_base, &profile))?;
    let _lock = ProfileLock::acquire(&state_dir)?;

    let config_path = profile_dir.join("agent.toml");
    let projects_dir = ensure_private_directory(&profile_dir.join("projects.d"))?;
    let log_path = local_runner_log_path(&state_dir);
    let existing_config = read_existing_agent_config(&config_path)?;
    validate_existing_profile(
        existing_config.as_ref(),
        &canonical_server.url,
        &resolved_key.value,
    )?;
    let existing_summary = local_runner_state_summary(&state_dir)?;
    let client_id = match (&opts.client_id, existing_config.as_ref()) {
        (Some(requested), Some(existing)) => {
            let requested = validate_client_id(requested)?;
            if requested != existing.client_id && existing_summary.running {
                return Err(
                    "--client-id differs from the active profile; stop that Runner before changing its identity"
                        .to_string(),
                );
            }
            requested
        }
        (Some(requested), None) => validate_client_id(requested)?,
        (None, Some(existing)) => validate_client_id(&existing.client_id)?,
        (None, None) => generated_client_id(&canonical_server.url),
    };
    let (project_path, project, already_registered) = resolve_project(
        &projects_dir,
        &canonical_project,
        opts.project_id.as_deref(),
    )?;
    let runtime_project_id = format!("agent:{client_id}:{}", project.id);
    let runner_bin = opts
        .runner_bin
        .clone()
        .or_else(|| discover_internal_binary("webcodex-runner"))
        .ok_or_else(|| {
            "webcodex-runner was not found beside webcodex or in an absolute PATH entry".to_string()
        })?;

    // Fail before replacing a healthy profile when the destination cannot
    // authenticate this direct shared key at all.
    preflight_shared_key(
        &canonical_server.url,
        &opts.server_http,
        &resolved_key.value,
    )
    .await?;

    let project_changed = if already_registered {
        false
    } else {
        let project_content = render_project_file(&project)?;
        atomic_write(&project_path, project_content.as_bytes(), false)?
    };
    let agent_content = render_agent_document(
        &config_path,
        &canonical_server.url,
        &resolved_key.value,
        &client_id,
        &projects_dir,
        &canonical_project,
    )?;
    atomic_write(&config_path, agent_content.as_bytes(), true)?;
    atomic_write(
        &local_runner_profile_marker(&state_dir),
        format!("profile = {profile:?}\n").as_bytes(),
        false,
    )?;

    if project_changed
        && load_runner_state(&state_dir)?
            .as_ref()
            .is_some_and(process_matches)
    {
        stop_runner_unlocked(&state_dir)?;
    }
    let start = ensure_runner_unlocked(&runner_bin, &config_path, &state_dir).map_err(|error| {
        format!(
            "{error}. Runner logs: {}",
            local_runner_log_path(&state_dir).display()
        )
    })?;
    if let Err(error) = wait_for_connection(
        &canonical_server.url,
        &opts.server_http,
        &resolved_key.value,
        &client_id,
        &runtime_project_id,
        &state_dir,
        if opts.wait_timeout_ms == 0 {
            DEFAULT_CONNECT_WAIT_MS
        } else {
            opts.wait_timeout_ms
        },
    )
    .await
    {
        if start == RunnerStart::Started {
            let _ = stop_runner_unlocked(&state_dir);
        }
        return Err(format!("{error}. Runner logs: {}", log_path.display()));
    }
    Ok(ConnectResult {
        output: render_connect_output(
            &canonical_server.url,
            &profile,
            &client_id,
            &runtime_project_id,
            &config_path,
            &log_path,
            &resolved_key,
        ),
        disclosure_marker: resolved_key
            .generated
            .then(|| profile_dir.join(profile::KEY_DISCLOSED_FILE)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct ControlledWriter {
        bytes: Vec<u8>,
        fail_after: Option<usize>,
        fail_flush: bool,
    }

    impl ControlledWriter {
        fn successful() -> Self {
            Self {
                bytes: Vec::new(),
                fail_after: None,
                fail_flush: false,
            }
        }

        fn failing_after(limit: usize) -> Self {
            Self {
                bytes: Vec::new(),
                fail_after: Some(limit),
                fail_flush: false,
            }
        }
    }

    impl Write for ControlledWriter {
        fn write(&mut self, content: &[u8]) -> io::Result<usize> {
            if self
                .fail_after
                .is_some_and(|limit| self.bytes.len() >= limit)
            {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "injected write failure",
                ));
            }
            let available = self
                .fail_after
                .map(|limit| limit.saturating_sub(self.bytes.len()))
                .unwrap_or(content.len());
            let written = available.min(content.len());
            self.bytes.extend_from_slice(&content[..written]);
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.fail_flush {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "injected flush failure",
                ))
            } else {
                Ok(())
            }
        }
    }

    fn generated_result(marker: PathBuf) -> ConnectResult {
        ConnectResult {
            output: "Connected\nMCP key: wck_test_generated_secret\n".to_string(),
            disclosure_marker: Some(marker),
        }
    }

    #[test]
    fn generated_key_marker_is_committed_only_after_write_and_flush() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join(profile::KEY_DISCLOSED_FILE);
        let mut stdout = ControlledWriter::successful();
        let mut stderr = Vec::new();
        write_connect_result(generated_result(marker.clone()), &mut stdout, &mut stderr).unwrap();
        assert!(marker.is_file());
        assert!(stderr.is_empty());

        for fail_after in [0, 5] {
            let marker = tmp.path().join(format!(
                "failed-{fail_after}-{}",
                profile::KEY_DISCLOSED_FILE
            ));
            let mut stdout = ControlledWriter::failing_after(fail_after);
            let error = write_connect_result(
                generated_result(marker.clone()),
                &mut stdout,
                &mut Vec::new(),
            )
            .unwrap_err();
            assert!(error.contains("write connect output"));
            assert!(!marker.exists());
        }

        let marker = tmp
            .path()
            .join(format!("flush-{}", profile::KEY_DISCLOSED_FILE));
        let mut stdout = ControlledWriter::successful();
        stdout.fail_flush = true;
        let error = write_connect_result(
            generated_result(marker.clone()),
            &mut stdout,
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(error.contains("flush connect output"));
        assert!(!marker.exists());
    }

    #[test]
    fn marker_failure_warns_without_secret_and_keeps_connect_successful() {
        let tmp = tempfile::tempdir().unwrap();
        let not_directory = tmp.path().join("not-a-directory");
        std::fs::write(&not_directory, b"file").unwrap();
        let marker = not_directory.join(profile::KEY_DISCLOSED_FILE);
        let mut stdout = ControlledWriter::successful();
        let mut stderr = Vec::new();
        write_connect_result(generated_result(marker.clone()), &mut stdout, &mut stderr).unwrap();
        assert!(!marker.exists());
        let warning = String::from_utf8(stderr).unwrap();
        assert!(warning.contains("connection is healthy"));
        assert!(warning.contains("may be displayed again"));
        assert!(!warning.contains("wck_test_generated_secret"));
    }

    #[test]
    fn explicit_key_result_never_creates_a_disclosure_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join(profile::KEY_DISCLOSED_FILE);
        let result = ConnectResult {
            output: "Connected with an explicitly supplied key\n".to_string(),
            disclosure_marker: None,
        };
        write_connect_result(result, &mut Vec::new(), &mut Vec::new()).unwrap();
        assert!(!marker.exists());
    }
}
