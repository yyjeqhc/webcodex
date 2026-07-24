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
