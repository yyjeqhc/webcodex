use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-env-changed=WEBCODEX_GIT_COMMIT");
    println!("cargo:rerun-if-env-changed=WEBCODEX_GIT_DIRTY");
    println!("cargo:rerun-if-env-changed=WEBCODEX_BUILT_AT");
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Some(head_ref) = current_head_ref() {
        println!("cargo:rerun-if-changed=.git/{head_ref}");
    }

    let git_commit = env_value("WEBCODEX_GIT_COMMIT").unwrap_or_else(git_commit_from_git);
    let git_dirty = env_value("WEBCODEX_GIT_DIRTY").unwrap_or_else(git_dirty_from_git);
    let built_at = env_value("WEBCODEX_BUILT_AT").unwrap_or_else(current_unix_timestamp);

    println!("cargo:rustc-env=WEBCODEX_BUILD_GIT_COMMIT={git_commit}");
    println!("cargo:rustc-env=WEBCODEX_BUILD_GIT_DIRTY={git_dirty}");
    println!("cargo:rustc-env=WEBCODEX_BUILD_BUILT_AT={built_at}");
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn git_commit_from_git() -> String {
    command_stdout(["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".to_string())
}

fn current_head_ref() -> Option<String> {
    let head = std::fs::read_to_string(".git/HEAD").ok()?;
    let head = head.trim();
    head.strip_prefix("ref: ")
        .filter(|value| !value.contains(".."))
        .filter(|value| !value.starts_with('/'))
        .map(ToOwned::to_owned)
}

fn git_dirty_from_git() -> String {
    match Command::new("git")
        .args(["diff-index", "--quiet", "HEAD", "--"])
        .status()
    {
        Ok(status) if status.success() => "false".to_string(),
        Ok(status) if status.code() == Some(1) => "true".to_string(),
        _ => "unknown".to_string(),
    }
}

fn command_stdout<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let value = stdout.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn current_unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}
