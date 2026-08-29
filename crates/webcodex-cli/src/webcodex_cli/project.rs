use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::connect::profile::{atomic_write, render_project_file, resolve_project};
use super::shell_command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRegisterOptions {
    pub(crate) config: PathBuf,
    pub(crate) project: PathBuf,
    pub(crate) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRegistration {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) record_path: PathBuf,
    pub(crate) already_registered: bool,
}

#[derive(Debug, Default, Deserialize)]
struct RegistrationPolicy {
    #[serde(default)]
    allow_cwd_anywhere: bool,
    #[serde(default)]
    allowed_roots: Vec<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct RegistrationAgentConfig {
    projects_dir: Option<PathBuf>,
    #[serde(default)]
    policy: RegistrationPolicy,
}

fn canonical_existing_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let canonical = path.canonicalize().map_err(|error| {
        format!(
            "{label} {} does not exist or cannot be resolved: {error}",
            path.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "{label} {} is not a directory",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn effective_canonical_roots(
    configured: &[PathBuf],
    allow_cwd_anywhere: bool,
) -> Result<Vec<PathBuf>, String> {
    webcodex_runner_config::effective_allowed_roots(configured, allow_cwd_anywhere)?
        .into_iter()
        .map(|root| canonical_existing_directory(&root, "allowed root"))
        .collect()
}

fn validate_project_authority(
    project: &Path,
    configured_roots: &[PathBuf],
    allow_cwd_anywhere: bool,
) -> Result<Vec<PathBuf>, String> {
    let roots = effective_canonical_roots(configured_roots, allow_cwd_anywhere)?;
    if !allow_cwd_anywhere
        && !roots
            .iter()
            .any(|root| webcodex_runner_config::paths::path_is_within(project, root))
    {
        return Err(format!(
            "project {} is outside the Runner allowed_roots; --allowed-root controls registration authority and does not register a workspace",
            project.display()
        ));
    }
    Ok(roots)
}

fn ensure_registry_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!(
            "projects_dir {} must be an absolute path; projects_dir is the Runner project registry directory, not a workspace root",
            path.display()
        ));
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(format!(
                    "projects_dir {} is not a real directory; projects_dir is the Runner project registry directory, not a workspace root",
                    path.display()
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(path).map_err(|error| {
                format!("failed to create projects_dir {}: {error}", path.display())
            })?;
        }
        Err(error) => {
            return Err(format!(
                "failed to inspect projects_dir {}: {error}",
                path.display()
            ));
        }
    }
    Ok(())
}

pub(crate) fn register_existing_project(
    projects_dir: &Path,
    project: &Path,
    configured_roots: &[PathBuf],
    allow_cwd_anywhere: bool,
    explicit_id: Option<&str>,
) -> Result<(ProjectRegistration, Vec<PathBuf>), String> {
    let canonical_project = canonical_existing_directory(project, "project path")?;
    let roots =
        validate_project_authority(&canonical_project, configured_roots, allow_cwd_anywhere)?;
    ensure_registry_directory(projects_dir)?;
    let (record_path, project_file, already_registered) =
        resolve_project(projects_dir, &canonical_project, explicit_id)?;
    if !already_registered {
        let content = render_project_file(&project_file)?;
        atomic_write(&record_path, content.as_bytes(), false)?;
    }
    Ok((
        ProjectRegistration {
            id: project_file.id,
            path: canonical_project,
            record_path,
            already_registered,
        },
        roots,
    ))
}

fn read_registration_config(path: &Path) -> Result<RegistrationAgentConfig, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect agent config {}: {error}", path.display()))?;
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(format!(
            "agent config {} is not a regular file",
            path.display()
        ));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read agent config {}: {error}", path.display()))?;
    toml::from_str(&content)
        .map_err(|error| format!("failed to parse agent config {}: {error}", path.display()))
}

fn registration_projects_dir(config: &RegistrationAgentConfig) -> Result<PathBuf, String> {
    config.projects_dir.clone().map(Ok).unwrap_or_else(|| {
        webcodex_runner_config::paths::default_client_config_base_dir()
            .map(|base| base.join("projects.d"))
    })
}

pub(crate) fn run_project_register(opts: ProjectRegisterOptions) -> Result<String, String> {
    let config = read_registration_config(&opts.config)?;
    // Mirror Runner config loading exactly: a minimal agent.toml may omit
    // projects_dir, in which case the Runner materializes the shared per-user
    // config-base projects.d path. The local registration CLI must write to that
    // same registry rather than rejecting a config the Runner itself accepts.
    let projects_dir = registration_projects_dir(&config)?;
    let (registration, roots) = register_existing_project(
        &projects_dir,
        &opts.project,
        &config.policy.allowed_roots,
        config.policy.allow_cwd_anywhere,
        None,
    )?;
    if opts.json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "agent_config": opts.config.to_string_lossy(),
            "projects_dir": projects_dir.to_string_lossy(),
            "project": {
                "id": registration.id,
                "path": registration.path.to_string_lossy(),
                "record": registration.record_path.to_string_lossy(),
                "already_registered": registration.already_registered,
            },
            "policy": {
                "allow_cwd_anywhere": config.policy.allow_cwd_anywhere,
                "allowed_roots": roots.iter().map(|root| root.to_string_lossy().to_string()).collect::<Vec<_>>(),
            },
            "runner_reload_required": !registration.already_registered,
        }))
        .map_err(|error| error.to_string());
    }
    let runner_command = shell_command(&[
        "webcodex-runner".to_string(),
        "--config".to_string(),
        opts.config.to_string_lossy().into_owned(),
    ]);
    let reload_guidance = if registration.already_registered {
        "The project registry was unchanged; no Runner reload is required.\n".to_string()
    } else {
        format!(
            "Restart or reload the existing Runner that uses this config so the registry change is loaded.\nIf it is a foreground Runner, stop that existing process first, then run:\n  {runner_command}\nIf it is installed as a service, use the matching `webcodex runner restart ...` command instead; do not start a second foreground Runner with the same client_id.\n"
        )
    };
    Ok(format!(
        "Project {}.\n\n  id:                {}\n  path:              {}\n  agent config:      {}\n  projects registry: {}\n  project record:    {}\n\nAllowed roots are registration authority; they are not workspace registrations.\n{}",
        if registration.already_registered {
            "already registered"
        } else {
            "registered"
        },
        registration.id,
        registration.path.display(),
        opts.config.display(),
        projects_dir.display(),
        registration.record_path.display(),
        reload_guidance,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(path: &Path, projects_dir: &Path, root: &Path) {
        std::fs::write(
            path,
            format!(
                "server_url = \"https://example.test\"\ntoken = \"secret-not-printed\"\nclient_id = \"client\"\nprojects_dir = {:?}\n\n[policy]\nallowed_roots = [{:?}]\n",
                projects_dir.to_string_lossy(),
                root.to_string_lossy()
            ),
        )
        .unwrap();
    }

    #[test]
    fn register_existing_directory_is_idempotent_and_respects_registry_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let project = root.join("demo");
        let registry = tmp.path().join("registry");
        std::fs::create_dir_all(&project).unwrap();
        let config_path = tmp.path().join("agent.toml");
        config(&config_path, &registry, &root);

        let first = run_project_register(ProjectRegisterOptions {
            config: config_path.clone(),
            project: project.clone(),
            json: true,
        })
        .unwrap();
        let first: serde_json::Value = serde_json::from_str(&first).unwrap();
        assert_eq!(first["project"]["id"], "demo");
        assert_eq!(first["project"]["already_registered"], false);
        assert_eq!(first["runner_reload_required"], true);
        assert!(registry.join("demo.toml").is_file());
        assert!(!first.to_string().contains("secret-not-printed"));

        let second = run_project_register(ProjectRegisterOptions {
            config: config_path,
            project,
            json: true,
        })
        .unwrap();
        let second: serde_json::Value = serde_json::from_str(&second).unwrap();
        assert_eq!(second["project"]["id"], "demo");
        assert_eq!(second["project"]["already_registered"], true);
        assert_eq!(second["runner_reload_required"], false);
    }

    #[test]
    fn omitted_projects_dir_uses_the_same_default_as_runner_config_loading() {
        let config = RegistrationAgentConfig {
            projects_dir: None,
            policy: RegistrationPolicy::default(),
        };
        let expected = webcodex_runner_config::paths::default_client_config_base_dir()
            .unwrap()
            .join("projects.d");
        assert_eq!(registration_projects_dir(&config).unwrap(), expected);
    }

    #[test]
    fn outside_allowed_roots_is_rejected_without_registry_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let outside = tmp.path().join("outside");
        let registry = tmp.path().join("registry");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let config_path = tmp.path().join("agent.toml");
        config(&config_path, &registry, &root);
        let error = run_project_register(ProjectRegisterOptions {
            config: config_path,
            project: outside,
            json: false,
        })
        .unwrap_err();
        assert!(
            error.contains("outside the Runner allowed_roots"),
            "{error}"
        );
        assert!(!registry.exists());
    }

    #[test]
    fn different_paths_with_same_basename_get_stable_collision_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let one = root.join("one/demo");
        let two = root.join("two/demo");
        let registry = tmp.path().join("registry");
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        let first = register_existing_project(&registry, &one, &[root.clone()], false, None)
            .unwrap()
            .0;
        let second = register_existing_project(&registry, &two, &[root], false, None)
            .unwrap()
            .0;
        assert_eq!(first.id, "demo");
        assert!(second.id.starts_with("demo-"), "{}", second.id);
        assert_ne!(first.id, second.id);
    }
}
