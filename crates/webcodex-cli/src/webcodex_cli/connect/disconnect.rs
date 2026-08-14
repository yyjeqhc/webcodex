use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use webcodex_admin::ServerHttpOptions;

use super::super::connections::{canonical_server_url, ensure_real_directory_tree};
use super::super::http::{post_json_authed, ApiCall};
use super::super::profiles::{
    client_output_dir_for_profile, client_state_dir_for_profile, default_client_base_dir,
    default_client_state_base_dir, validate_client_profile,
};
use super::process::{local_runner_state_summary, stop_runner_unlocked};
use super::profile::{
    read_existing_agent_config, read_project_files, stored_project_matches,
    validate_existing_regular_file, ExistingAgentConfig, ProfileLock, ProjectFile,
};

const MAX_AMBIGUOUS_PROFILES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisconnectOptions {
    pub(crate) project: PathBuf,
    pub(crate) profile: Option<String>,
    pub(crate) config_base: Option<PathBuf>,
    pub(crate) state_base: Option<PathBuf>,
    pub(crate) server_http: ServerHttpOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisconnectResult {
    pub(crate) outcome: String,
    pub(crate) profile: String,
    pub(crate) project: PathBuf,
    pub(crate) project_id: String,
    pub(crate) runner_action: String,
}

impl DisconnectResult {
    pub(crate) fn render(&self) -> String {
        format!(
            "Disconnected WebCodex project.\n  outcome: {}\n  profile: {}\n  project: {}\n  project_id: {}\n  runner: {}\n",
            self.outcome,
            self.profile,
            self.project.display(),
            self.project_id,
            self.runner_action
        )
    }
}

#[derive(Debug, Clone)]
struct ProjectRegistration {
    profile: String,
    profile_dir: PathBuf,
    state_dir: PathBuf,
    project_path: PathBuf,
    project: ProjectFile,
}

pub(crate) async fn run_disconnect(opts: DisconnectOptions) -> Result<DisconnectResult, String> {
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
    if !config_base.is_dir() || !state_base.is_dir() {
        return Err(format!(
            "no hosted `webcodex connect` profile registers project {}",
            canonical_project.display()
        ));
    }
    let config_base = ensure_real_directory_tree(&config_base)?;
    let state_base = ensure_real_directory_tree(&state_base)?;

    let candidate = resolve_registration(
        &config_base,
        &state_base,
        &canonical_project,
        explicit_profile.as_deref(),
    )?;
    let _lock = ProfileLock::acquire(&candidate.state_dir)?;

    // Re-read under the same profile lock used by connect. The exact TOML must
    // still be a regular file and still name this canonical repository before
    // either the remote or local registration is changed.
    validate_existing_regular_file(&candidate.project_path)?;
    let current_projects = read_project_files(&candidate.profile_dir.join("projects.d"))?;
    let current = current_projects
        .iter()
        .find(|(path, project)| {
            path == &candidate.project_path && stored_project_matches(project, &canonical_project)
        })
        .ok_or_else(|| {
            "hosted project registration changed while disconnect was starting; retry after inspecting the profile"
                .to_string()
        })?;
    if current.1.id != candidate.project.id {
        return Err(
            "hosted project registration changed while disconnect was starting; refusing to guess"
                .to_string(),
        );
    }

    let config_path = candidate.profile_dir.join("agent.toml");
    let config = read_existing_agent_config(&config_path)?
        .ok_or_else(|| format!("hosted profile {} has no agent.toml", candidate.profile))?;
    let runtime_project_id = format!("agent:{}:{}", config.client_id, current.1.id);
    let runner = local_runner_state_summary(&candidate.state_dir)?;
    if !runner.managed {
        return Err(format!(
            "profile {} is not a hosted `webcodex connect` profile",
            candidate.profile
        ));
    }

    let outcome = if runner.running {
        let remote_outcome =
            unregister_live_project(&config, &opts.server_http, &runtime_project_id).await?;
        // Only remove the local registration after the Server/Runner returned a
        // terminal structured unregister outcome. Indeterminate remote outcomes
        // leave this file intact.
        remove_exact_registration(&candidate.project_path)?;
        remote_outcome
    } else {
        remove_exact_registration(&candidate.project_path)?;
        "local_unregistered".to_string()
    };

    let remaining = read_project_files(&candidate.profile_dir.join("projects.d"))?.len();
    let runner_action =
        runner_action_after_disconnect(runner.running, remaining, &candidate.state_dir);

    Ok(DisconnectResult {
        outcome,
        profile: candidate.profile,
        project: canonical_project,
        project_id: runtime_project_id,
        runner_action: runner_action.to_string(),
    })
}

fn runner_action_after_disconnect(
    runner_running: bool,
    remaining: usize,
    state_dir: &Path,
) -> &'static str {
    if runner_running && remaining == 0 {
        return match stop_runner_unlocked(state_dir) {
            Ok(true) => "stopped",
            Ok(false) => "already_stopped",
            Err(_) => "stop_failed",
        };
    }
    if runner_running {
        "kept_running"
    } else {
        "not_running"
    }
}

fn resolve_registration(
    config_base: &Path,
    state_base: &Path,
    canonical_project: &Path,
    explicit_profile: Option<&str>,
) -> Result<ProjectRegistration, String> {
    let names = if let Some(profile) = explicit_profile {
        vec![profile.to_string()]
    } else {
        let clients = config_base.join("clients");
        let mut names = match std::fs::read_dir(&clients) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .filter(|name| validate_client_profile(name).is_ok())
                .collect::<Vec<_>>(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                return Err(format!(
                    "failed to inspect hosted profiles {}: {error}",
                    clients.display()
                ))
            }
        };
        names.sort();
        names
    };

    let mut matches = Vec::new();
    for profile in names {
        let profile_dir = client_output_dir_for_profile(config_base, &profile);
        let state_dir = client_state_dir_for_profile(state_base, &profile);
        let marker = super::process::local_runner_profile_marker(&state_dir);
        if !marker.exists() {
            if explicit_profile.is_some() {
                return Err(format!(
                    "profile {profile} is not a hosted `webcodex connect` profile"
                ));
            }
            continue;
        }
        validate_existing_regular_file(&marker)?;
        let project_matches = read_project_files(&profile_dir.join("projects.d"))?
            .into_iter()
            .filter(|(_, project)| stored_project_matches(project, canonical_project))
            .collect::<Vec<_>>();
        if project_matches.len() > 1 {
            return Err(format!(
                "profile {profile} contains more than one registration for the same canonical repository; refusing to guess"
            ));
        }
        if let Some((project_path, project)) = project_matches.into_iter().next() {
            matches.push(ProjectRegistration {
                profile,
                profile_dir,
                state_dir,
                project_path,
                project,
            });
        } else if explicit_profile.is_some() {
            return Err(format!(
                "profile {profile} does not register project {}",
                canonical_project.display()
            ));
        }
    }
    if matches.len() > 1 {
        let mut names = matches
            .iter()
            .map(|candidate| candidate.profile.clone())
            .collect::<Vec<_>>();
        names.sort();
        let shown = names
            .iter()
            .take(MAX_AMBIGUOUS_PROFILES)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if names.len() > MAX_AMBIGUOUS_PROFILES {
            format!(", +{} more", names.len() - MAX_AMBIGUOUS_PROFILES)
        } else {
            String::new()
        };
        return Err(format!(
            "more than one hosted profile registers this exact repository ({shown}{suffix}); rerun with --profile NAME"
        ));
    }
    matches.pop().ok_or_else(|| {
        format!(
            "no hosted `webcodex connect` profile registers project {}",
            canonical_project.display()
        )
    })
}

async fn unregister_live_project(
    config: &ExistingAgentConfig,
    server_http: &ServerHttpOptions,
    runtime_project_id: &str,
) -> Result<String, String> {
    let server = canonical_server_url(&config.server_url)?;
    let list = post_json_authed(ApiCall {
        server_url: &server.url,
        server_http,
        token: &config.token,
        path: "/api/projects/list",
        body: json!({}),
    })
    .await?;
    let projects = list
        .pointer("/output/projects")
        .or_else(|| list.get("projects"))
        .and_then(Value::as_array)
        .ok_or_else(|| "Server returned an invalid project inventory".to_string())?;
    let project = projects
        .iter()
        .find(|project| project.get("id").and_then(Value::as_str) == Some(runtime_project_id))
        .ok_or_else(|| {
            "live Runner project is not present in the authenticated Server inventory; local registration was preserved"
                .to_string()
        })?;
    let expected_revision = project
        .get("revision")
        .and_then(Value::as_str)
        .filter(|revision| valid_revision(revision))
        .ok_or_else(|| "Server project inventory omitted a valid revision".to_string())?;
    let response = post_json_authed(ApiCall {
        server_url: &server.url,
        server_http,
        token: &config.token,
        path: "/api/projects/unregister",
        body: json!({
            "project": runtime_project_id,
            "expected_revision": expected_revision,
        }),
    })
    .await?;
    if response.get("project").and_then(Value::as_str) != Some(runtime_project_id) {
        return Err(
            "Server unregister response did not identify the requested project".to_string(),
        );
    }
    let outcome = response
        .get("outcome")
        .and_then(Value::as_str)
        .filter(|outcome| matches!(*outcome, "unregistered" | "already_unregistered"))
        .ok_or_else(|| "Server did not return a terminal unregister outcome".to_string())?;
    Ok(outcome.to_string())
}

fn valid_revision(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn remove_exact_registration(path: &Path) -> Result<(), String> {
    validate_existing_regular_file(path)?;
    std::fs::remove_file(path).map_err(|error| {
        format!(
            "failed to remove hosted project registration {}: {error}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn write_profile(
        config_base: &Path,
        state_base: &Path,
        profile: &str,
        project: &Path,
        project_id: &str,
    ) -> (PathBuf, PathBuf) {
        let profile_dir = client_output_dir_for_profile(config_base, profile);
        let state_dir = client_state_dir_for_profile(state_base, profile);
        std::fs::create_dir_all(profile_dir.join("projects.d")).unwrap();
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::write(
            profile_dir.join("agent.toml"),
            "server_url = \"http://127.0.0.1:1\"\ntoken = \"shared-key\"\nclient_id = \"client\"\n",
        )
        .unwrap();
        std::fs::write(
            profile_dir
                .join("projects.d")
                .join(format!("{project_id}.toml")),
            format!(
                "id = \"{project_id}\"\npath = {:?}\n",
                project.to_string_lossy()
            ),
        )
        .unwrap();
        std::fs::write(
            super::super::process::local_runner_profile_marker(&state_dir),
            "profile = \"test\"\n",
        )
        .unwrap();
        (profile_dir, state_dir)
    }

    #[test]
    fn canonical_matching_and_multi_profile_ambiguity_are_exact() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        let state = tmp.path().join("state");
        let project = tmp.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(config.join("clients")).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        write_profile(&config, &state, "one", &project, "repo");
        let canonical = project.canonicalize().unwrap();
        let one = resolve_registration(&config, &state, &canonical, None).unwrap();
        assert_eq!(one.profile, "one");
        write_profile(&config, &state, "two", &project, "different-id");
        let error = resolve_registration(&config, &state, &canonical, None).unwrap_err();
        assert!(error.contains("one, two"), "{error}");
        let explicit = resolve_registration(&config, &state, &canonical, Some("two")).unwrap();
        assert_eq!(explicit.project.id, "different-id");
    }

    #[cfg(unix)]
    #[test]
    fn project_registration_symlink_fails_closed() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        let state = tmp.path().join("state");
        let project = tmp.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(config.join("clients")).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        let (profile_dir, _) = write_profile(&config, &state, "one", &project, "repo");
        let actual = profile_dir.join("projects.d/repo.toml");
        let target = profile_dir.join("actual.toml");
        std::fs::rename(&actual, &target).unwrap();
        symlink(&target, &actual).unwrap();
        let error = resolve_registration(&config, &state, &project.canonicalize().unwrap(), None)
            .unwrap_err();
        assert!(error.contains("not a regular file"), "{error}");
    }

    #[test]
    fn non_regular_project_registration_fails_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        let state = tmp.path().join("state");
        let project = tmp.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(config.join("clients/one/projects.d/bad.toml")).unwrap();
        std::fs::create_dir_all(state.join("clients/one")).unwrap();
        std::fs::write(
            super::super::process::local_runner_profile_marker(&state.join("clients/one")),
            "profile = \"one\"\n",
        )
        .unwrap();
        let error = resolve_registration(&config, &state, &project.canonicalize().unwrap(), None)
            .unwrap_err();
        assert!(error.contains("not a regular file"), "{error}");
    }

    #[tokio::test]
    async fn offline_unregister_preserves_other_project_and_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        let state = tmp.path().join("state");
        let project = tmp.path().join("repo");
        let other = tmp.path().join("other");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::write(project.join("keep.txt"), "keep").unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::create_dir_all(config.join("clients")).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        let (profile_dir, _) = write_profile(&config, &state, "one", &project, "repo");
        std::fs::write(
            profile_dir.join("projects.d/other.toml"),
            format!("id = \"other\"\npath = {:?}\n", other.to_string_lossy()),
        )
        .unwrap();
        let result = run_disconnect(DisconnectOptions {
            project: project.clone(),
            profile: None,
            config_base: Some(config),
            state_base: Some(state),
            server_http: ServerHttpOptions::default(),
        })
        .await
        .unwrap();
        assert_eq!(result.outcome, "local_unregistered");
        assert_eq!(result.runner_action, "not_running");
        assert!(profile_dir.join("projects.d/other.toml").is_file());
        assert!(profile_dir.join("agent.toml").is_file());
        assert_eq!(
            std::fs::read_to_string(project.join("keep.txt")).unwrap(),
            "keep"
        );
        assert!(project.join(".git").is_dir());
    }

    #[tokio::test]
    async fn live_unregister_uses_structured_endpoint_and_revision() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for index in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = vec![0u8; 16 * 1024];
                let read = stream.read(&mut request).unwrap();
                let text = String::from_utf8_lossy(&request[..read]);
                let (path, body) = if index == 0 {
                    assert!(text.starts_with("POST /api/projects/list "), "{text}");
                    (
                        "/api/projects/list",
                        json!({"success":true,"output":{"projects":[{"id":"agent:client:repo","revision":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}}),
                    )
                } else {
                    assert!(text.starts_with("POST /api/projects/unregister "), "{text}");
                    assert!(text.contains("agent:client:repo"), "{text}");
                    assert!(text.contains("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), "{text}");
                    (
                        "/api/projects/unregister",
                        json!({"operation":"unregister","project":"agent:client:repo","outcome":"unregistered","changed":true}),
                    )
                };
                let _ = path;
                let payload = body.to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                )
                .unwrap();
            }
        });
        let config = ExistingAgentConfig {
            server_url: format!("http://{address}"),
            token: "shared-key".to_string(),
            client_id: "client".to_string(),
        };
        let outcome = unregister_live_project(
            &config,
            &ServerHttpOptions {
                proxy: None,
                no_system_proxy: true,
            },
            "agent:client:repo",
        )
        .await
        .unwrap();
        assert_eq!(outcome, "unregistered");
        server.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn last_project_disconnect_stops_managed_runner() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let runner = tmp.path().join("webcodex-runner");
        std::fs::write(
            &runner,
            "#!/bin/sh\ntrap 'exit 0' TERM INT\nwhile :; do sleep 1; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = tmp.path().join("agent.toml");
        std::fs::write(&config, "server_url='http://example.test'\n").unwrap();
        let state = tmp.path().join("state");
        std::fs::create_dir(&state).unwrap();
        std::fs::write(
            super::super::process::local_runner_profile_marker(&state),
            "profile = \"disconnect-test\"\n",
        )
        .unwrap();
        assert_eq!(
            super::super::process::ensure_runner_unlocked(&runner, &config, &state).unwrap(),
            super::super::process::RunnerStart::Started
        );
        assert!(local_runner_state_summary(&state).unwrap().running);

        assert_eq!(runner_action_after_disconnect(true, 0, &state), "stopped");
        assert!(!local_runner_state_summary(&state).unwrap().running);
    }

    #[test]
    fn output_is_bounded_non_secret_metadata() {
        let result = DisconnectResult {
            outcome: "unregistered".to_string(),
            profile: "demo".to_string(),
            project: PathBuf::from("/tmp/repo"),
            project_id: "agent:client:repo".to_string(),
            runner_action: "kept_running".to_string(),
        };
        let rendered = result.render();
        assert!(rendered.contains("profile: demo"));
        assert!(rendered.contains("project_id: agent:client:repo"));
        assert!(!rendered.contains("shared-key"));
        assert!(!rendered.contains("agent.toml"));
    }

    #[test]
    fn project_revision_validation_is_exact() {
        assert!(valid_revision(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        assert!(!valid_revision("sha256:abc"));
    }
}
