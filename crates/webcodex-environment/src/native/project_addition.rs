//! Disk policy is not proof that the live Runner applied it. Keep the exact
//! pending candidate and generation in the existing project-addition journal.
use super::*;

#[derive(Serialize, Deserialize)]
pub(super) struct ProjectConfigChange {
    before_sha256: String,
    candidate_sha256: String,
    generation: Option<u64>,
}

fn read_configuration(path: &Path) -> SetupResultValue<Secret> {
    // Unlike credential reads, candidate fences compare exact file bytes.
    String::from_utf8(read_private(path)?)
        .map(Secret::new)
        .map_err(|_| SetupDiagnostic::io())
}
fn digest(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}
fn conflict() -> SetupDiagnostic {
    diagnostic("config_concurrent_change", "Runner configuration changed during project addition; reconcile the saved candidate before retrying")
}
fn candidate(mut config: toml::Value, path: &Path) -> SetupResultValue<String> {
    let roots = config
        .get_mut("policy")
        .and_then(|value| value.get_mut("allowed_roots"))
        .and_then(toml::Value::as_array_mut)
        .ok_or_else(|| diagnostic("runner_policy", "Runner allowed roots are missing"))?;
    roots.push(toml::Value::String(path.to_string_lossy().into_owned()));
    toml::to_string(&config).map_err(|_| SetupDiagnostic::io())
}

impl NativeEnvironment {
    pub(super) async fn ensure_project_authority(
        &self,
        store: &EnvironmentStore,
        record: &EnvironmentRecord,
        client_id: &str,
        path: &Path,
        token: &str,
        pending: &mut ProjectAddition,
    ) -> SetupResultValue<()> {
        let config_path = store.root().join("runner.toml");
        let before = read_configuration(&config_path)?;
        let config: toml::Value = toml::from_str(before.expose())
            .map_err(|_| diagnostic("runner_configuration", "Runner configuration is invalid"))?;
        if config.get("client_id").and_then(toml::Value::as_str) != Some(client_id)
            || config.get("server_url").and_then(toml::Value::as_str)
                != Some(record.request.server_url.as_str())
        {
            return Err(diagnostic(
                "runner_binding_conflict",
                "The local Runner binding has changed",
            ));
        }
        let current_hash = digest(before.expose());
        let replacement = if let Some(change) = &pending.config_change {
            if current_hash == change.candidate_sha256 {
                None
            } else if current_hash == change.before_sha256 && change.generation.is_none() {
                // Resume a crash between recording intent and writing its candidate.
                let next = candidate(config, path)?;
                if digest(&next) != change.candidate_sha256 {
                    return Err(conflict());
                }
                Some(next)
            } else {
                return Err(conflict());
            }
        } else {
            if !project_requires_authority(&config, path)? {
                return Ok(());
            }
            let next = candidate(config, path)?;
            pending.config_change = Some(ProjectConfigChange {
                before_sha256: current_hash,
                candidate_sha256: digest(&next),
                generation: None,
            });
            // Persist intent BEFORE changing the disk file. A retry must not
            // mistake that file for already-applied live authority.
            store.write_json("add-project.json", pending)?;
            Some(next)
        };
        if let Some(next) = replacement {
            if read_configuration(&config_path)?.expose() != before.expose() {
                return Err(conflict());
            }
            atomic_private_write(&config_path, next.as_bytes())?;
        }
        let expected_hash = pending
            .config_change
            .as_ref()
            .unwrap()
            .candidate_sha256
            .clone();
        if digest(read_configuration(&config_path)?.expose()) != expected_hash {
            return Err(conflict());
        }
        let checked = self
            .post(
                &record.request.server_url,
                "/api/tools/call",
                Some(token),
                json!({"tool":"runner_config_check","params":{"client_id":client_id}}),
            )
            .await?;
        let checked = tool_output(&checked)?;
        if checked.get("valid").and_then(Value::as_bool) != Some(true)
            || checked.get("restart_required").and_then(Value::as_bool) == Some(true)
        {
            return Err(diagnostic(
                "config_requires_attention",
                "Runner configuration cannot be hot reloaded",
            ));
        }
        let generation = checked
            .get("current_generation")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                diagnostic(
                    "config_generation",
                    "Runner did not report its configuration generation",
                )
            })?;
        if digest(read_configuration(&config_path)?.expose()) != expected_hash {
            return Err(conflict());
        }
        if let Some(previous) = pending.config_change.as_ref().unwrap().generation {
            // Match the existing CLI reconciliation contract. Never replay a
            // possibly dispatched reload, including when the generation stayed put.
            if previous.checked_add(1) != Some(generation) {
                return Err(diagnostic("config_reconcile_required", "Runner reload outcome is uncertain; reconcile the exact candidate and generation before retrying"));
            }
        } else {
            let applied = generation.checked_add(1).ok_or_else(|| {
                diagnostic(
                    "config_generation",
                    "Runner configuration generation is exhausted",
                )
            })?;
            pending.config_change.as_mut().unwrap().generation = Some(generation);
            store.write_json("add-project.json", pending)?;
            let reload = self.post(&record.request.server_url, "/api/tools/call", Some(token),
                json!({"tool":"runner_config_reload","params":{"client_id":client_id,"expected_generation":generation}})).await?;
            if reload.get("success").and_then(Value::as_bool) != Some(true)
                && reload
                    .pointer("/output/execution_state")
                    .and_then(Value::as_str)
                    == Some("not_started")
            {
                // Only proven non-dispatch permits another explicit attempt.
                pending.config_change.as_mut().unwrap().generation = None;
                store.write_json("add-project.json", pending)?;
            }
            let reloaded = tool_output(&reload)?;
            if reloaded.get("current_generation").and_then(Value::as_u64) != Some(applied)
                || reloaded.get("valid").and_then(Value::as_bool) == Some(false)
                || reloaded.get("restart_required").and_then(Value::as_bool) == Some(true)
            {
                return Err(diagnostic(
                    "config_reconcile_required",
                    "Runner did not confirm the expected hot-reload generation",
                ));
            }
            if digest(read_configuration(&config_path)?.expose()) != expected_hash {
                return Err(conflict());
            }
        }
        pending.config_change = None;
        store.write_json("add-project.json", pending)
    }
}
