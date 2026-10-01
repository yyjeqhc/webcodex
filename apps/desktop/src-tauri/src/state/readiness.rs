async fn sleep_or_cancel_until(
    duration: Duration,
    cancellation: &CancellationContext,
    deadline: Deadline,
) -> DesktopResult<()> {
    cancellation.check()?;
    if deadline.is_elapsed() {
        return Ok(());
    }
    let wake_at = std::cmp::min(deadline.instant(), tokio::time::Instant::now() + duration);
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(cancelled_error()),
        _ = tokio::time::sleep_until(wake_at) => Ok(()),
    }
}

fn readiness_timeout_error(
    code: &'static str,
    message: &'static str,
    action: &'static str,
) -> DesktopError {
    DesktopError::new(code, message, action)
        .with_details(serde_json::json!({ "category": "readiness_timeout" }))
}

// These are Desktop packaging defaults, not process-environment overrides.
// The Server loads its env file only into keys that are absent from the process
// environment, so the effective precedence remains built-in defaults < this
// config file < explicit environment variables.
fn ensure_desktop_server_defaults(path: &Path) -> DesktopResult<()> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        desktop_state_unavailable("Desktop could not inspect its local Server configuration")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    if !metadata.is_file() || metadata.len() > DESKTOP_SERVER_ENV_MAX_BYTES {
        return Err(desktop_state_unavailable(
            "Desktop local Server configuration is not a bounded regular file",
        ));
    }
    let content = std::fs::read_to_string(path).map_err(|error| {
        desktop_state_unavailable("Desktop could not read its local Server configuration")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    let has_key = |key: &str| {
        content.lines().any(|line| {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line).trim();
            line.split_once('=')
                .is_some_and(|(candidate, _)| candidate.trim() == key)
        })
    };
    let mut additions = Vec::new();
    let has_host_profile = has_key("WEBCODEX_MCP_HOST_PROFILE");
    if !has_host_profile {
        additions.push(format!(
            "WEBCODEX_MCP_HOST_PROFILE={DESKTOP_MCP_HOST_PROFILE}"
        ));
        if !has_key("WEBCODEX_MCP_HOST_BUDGET_SECS") {
            additions.push(format!(
                "WEBCODEX_MCP_HOST_BUDGET_SECS={DESKTOP_MCP_HOST_BUDGET_SECS}"
            ));
        }
    }
    if !has_key("WEBCODEX_MCP_COMPACT_SCHEMAS") {
        additions.push(format!(
            "WEBCODEX_MCP_COMPACT_SCHEMAS={DESKTOP_MCP_COMPACT_SCHEMAS}"
        ));
    }
    if !has_key("WEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT") {
        additions.push(format!(
            "WEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT={DESKTOP_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT}"
        ));
    }
    if additions.is_empty() {
        return Ok(());
    }
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            desktop_state_unavailable("Desktop could not update its local Server configuration")
                .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
        })?;
    if !content.is_empty() && !content.ends_with('\n') {
        file.write_all(b"\n").map_err(|error| {
            desktop_state_unavailable("Desktop could not update its local Server configuration")
                .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
        })?;
    }
    for addition in additions {
        writeln!(file, "{addition}").map_err(|error| {
            desktop_state_unavailable("Desktop could not update its local Server configuration")
                .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
        })?;
    }
    file.flush().map_err(|error| {
        desktop_state_unavailable("Desktop could not update its local Server configuration")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    Ok(())
}

fn machine_event_overflow_error(event: &Value) -> DesktopError {
    DesktopError::new(
        "machine_event_overflow",
        "Desktop could not retain every critical machine-readiness event",
        "Retry the operation and inspect Activity if the child keeps emitting excessive machine events.",
    )
    .with_details(serde_json::json!({
        "category": "machine_event_overflow",
        "dropped_critical": event
            .get("dropped_critical")
            .and_then(Value::as_u64)
            .unwrap_or(1),
    }))
}

pub(crate) fn local_runtime_paths(data_dir: &Path) -> (PathBuf, PathBuf) {
    let local_dir = data_dir.join("runtime").join("local");
    (local_dir.join("webcodex.env"), local_dir.join("data"))
}

fn local_enrollment_directory(data_dir: &Path, config: &StoredDesktopConfig) -> PathBuf {
    // Two reusable slots bound local credential storage while keeping login's
    // --overwrite away from the currently committed recovery identity. Resolve
    // native path aliases (notably /var on macOS) before comparing paths.
    let root = data_dir
        .canonicalize()
        .unwrap_or_else(|_| data_dir.to_path_buf())
        .join("local-connections");
    let first = root.join("a");
    let saved_runner = config
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.runner_config.as_ref());
    if saved_runner.is_some_and(|path| {
        path.canonicalize()
            .unwrap_or_else(|_| path.clone())
            .starts_with(&first)
    }) {
        root.join("b")
    } else {
        first
    }
}

fn project_snapshot(config: &StoredDesktopConfig) -> Option<ProjectSelection> {
    let mut project = config.project.clone()?;
    if identity_from_config(config).is_none() {
        project.runtime_project_id = None;
    }
    Some(project)
}

fn stored_runner_client_id(config: &StoredDesktopConfig) -> Option<String> {
    let runtime = config.runtime.as_ref()?;
    if let Some(client_id) = runtime
        .runner_client_id
        .as_deref()
        .map(str::trim)
        .filter(|client_id| !client_id.is_empty())
    {
        return Some(client_id.to_string());
    }
    // Pre-migration Desktop state did not persist client_id separately. Recover
    // it from the exact runtime Project identity instead of accepting whatever
    // client_id happens to be present in runner.toml during the first upgrade.
    let project_id = runtime.project_id.as_deref()?.trim();
    let runtime_project_id = runtime.runtime_project_id.as_deref()?.trim();
    if project_id.is_empty() || runtime_project_id.is_empty() {
        return None;
    }
    let suffix = format!(":{project_id}");
    runtime_project_id
        .strip_prefix("agent:")?
        .strip_suffix(&suffix)
        .map(str::trim)
        .filter(|client_id| !client_id.is_empty())
        .map(str::to_string)
}

fn runner_identity_from_config(config: &StoredDesktopConfig) -> Option<RunnerRuntimeIdentity> {
    let runtime = config.runtime.as_ref()?;
    let client_id = stored_runner_client_id(config)?;
    let runner_config = runtime.runner_config.clone()?;
    let user_token_file = runtime.user_token_file.clone()?;
    if !runner_config.is_file() || !user_token_file.is_file() {
        return None;
    }
    Some(RunnerRuntimeIdentity {
        client_id,
        runner_config,
        user_token_file,
        server_url: runtime.server_url.clone(),
    })
}

fn identity_from_config(config: &StoredDesktopConfig) -> Option<ProjectRuntimeIdentity> {
    let runtime = config.runtime.as_ref()?;
    let project = config.project.as_ref()?;
    let runner_config = runtime.runner_config.clone()?;
    let user_token_file = runtime.user_token_file.clone()?;
    let project_id = runtime.project_id.clone()?.trim().to_string();
    let runtime_project_id = runtime.runtime_project_id.clone()?.trim().to_string();
    if project_id.is_empty()
        || runtime_project_id.is_empty()
        || !runner_config.is_file()
        || !user_token_file.is_file()
    {
        return None;
    }
    Some(ProjectRuntimeIdentity {
        project_id,
        runtime_project_id,
        project_path: project.path.clone(),
        runner: RunnerRuntimeIdentity {
            client_id: stored_runner_client_id(config)?,
            runner_config,
            user_token_file,
            server_url: runtime.server_url.clone(),
        },
    })
}

