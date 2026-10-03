//! Preflight the exact candidate binary's own configuration parser. This is a
//! read-only subprocess, never an Agent loop or service control operation.
use crate::process::CommandOutputExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Execute as the caller. Privileged adapters must not run an unprivileged
/// service's candidate binary with elevated authority just to validate config.
pub fn preflight_runner_configuration(program: &Path, config: &Path) -> Result<(), String> {
    let program = program
        .canonicalize()
        .map_err(|_| "Runner preflight executable is unavailable".to_string())?;
    let config = config
        .canonicalize()
        .map_err(|_| "Runner preflight configuration is unavailable".to_string())?;
    check_command(
        Command::new(program)
            .arg("--check-config")
            .arg("--config")
            .arg(config),
    )
}

pub(crate) fn check_command(command: &mut Command) -> Result<(), String> {
    let output = command.output_with_limits(Duration::from_secs(15), 8192)
        .map_err(|_| "Runner configuration preflight could not complete within its bounds; the current Runner was not stopped".to_string())?;
    if !output.status.success()
        || String::from_utf8_lossy(&output.stdout).trim() != "WebCodex Runner configuration valid"
    {
        // Never include stderr/config contents; either can contain credentials.
        return Err(format!("Runner configuration preflight failed (exit {:?}); verify the candidate supports --check-config and uses the current configuration contract. The current Runner was not stopped", output.status.code()));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn preflight_uses_exact_candidate_and_does_not_expose_parser_secrets() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::create_dir_all(&root).unwrap();
        let tmp = tempfile::tempdir_in(root).unwrap();
        let config = tmp.path().join("runner.toml");
        std::fs::write(&config, "token='SECRET_CONFIG'\n").unwrap();
        let runner = tmp.path().join("runner");
        let write = |body: &str| {
            std::fs::write(&runner, body).unwrap();
            std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        write("#!/bin/sh\n[ \"$1\" = --check-config ] && [ \"$2\" = --config ] && [ -f \"$3\" ] || exit 3\nprintf 'WebCodex Runner configuration valid\\n'\n");
        preflight_runner_configuration(&runner, &config).unwrap();
        write("#!/bin/sh\necho SECRET_CONFIG >&2\nexit 2\n");
        let error = preflight_runner_configuration(&runner, &config).unwrap_err();
        assert!(error.contains("Some(2)"));
        assert!(!error.contains("SECRET_CONFIG"));
        write("#!/bin/sh\nexit 0\n");
        assert!(
            preflight_runner_configuration(&runner, &config).is_err(),
            "arbitrary successful programs are not a config validator"
        );
        assert_eq!(
            std::fs::read_to_string(config).unwrap(),
            "token='SECRET_CONFIG'\n"
        );
    }
}
