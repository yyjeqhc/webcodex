use std::path::{Path, PathBuf};

use super::output::RuntimeBuildMetadata;
use crate::{cli_action, CliAction};

pub(crate) fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

pub(crate) fn build_metadata(commit: Option<&str>) -> RuntimeBuildMetadata {
    RuntimeBuildMetadata {
        version: Some("0.1.0".to_string()),
        git_commit: commit.map(str::to_string),
        git_dirty: Some(false),
        built_at: Some("1782739890".to_string()),
    }
}

pub(crate) fn cli_exit<I, S>(args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    match cli_action(args) {
        CliAction::Exit {
            code: 0, stdout, ..
        } => Ok(stdout),
        CliAction::Exit { stderr, .. } => Err(stderr),
        other => Err(format!("expected exit, got {other:?}")),
    }
}

pub(crate) fn write_doctor_agent_config(
    dir: &Path,
    projects_dir: &Path,
    default_profile: Option<&str>,
) -> PathBuf {
    let default_line = default_profile
        .map(|p| format!("default_profile = {:?}\n", p))
        .unwrap_or_default();
    let agent_toml = format!(
        "server_url = \"http://127.0.0.1:8000\"\n\
         token = \"test-token\"\n\
         client_id = \"oe\"\n\
         projects_dir = {:?}\n\
         [shell]\n\
         {default_line}\
         [shell.profiles.rust]\n\
         program = \"sh\"\n\
         args = [\"-c\"]\n\
         init_script = \"export SECRET=DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY\"\n\
         [shell.profiles.rust.env]\n\
         CARGO_HOME = \"/root/.cargo\"\n\
         SECRET_ENV = \"DO_NOT_LEAK_THIS_ENV_VALUE\"\n",
        projects_dir
    );
    let path = dir.join("agent.toml");
    std::fs::write(&path, agent_toml).unwrap();
    path
}

pub(crate) fn write_doctor_project(
    projects_dir: &Path,
    id: &str,
    path: &Path,
    shell_profile: Option<&str>,
) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let shell_line = shell_profile
        .map(|p| format!("shell_profile = {:?}\n", p))
        .unwrap_or_default();
    std::fs::write(
        projects_dir.join(format!("{id}.toml")),
        format!(
            "id = {:?}\npath = {:?}\nname = {:?}\n{shell_line}",
            id,
            path.to_string_lossy(),
            id
        ),
    )
    .unwrap();
}
