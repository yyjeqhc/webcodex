use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BuildInfo {
    pub version: &'static str,
    pub git_commit: Option<&'static str>,
    pub git_dirty: Option<bool>,
    pub built_at: Option<&'static str>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeBuildInfo {
    pub git_commit: Option<&'static str>,
    pub git_dirty: Option<bool>,
    pub built_at: Option<&'static str>,
}

pub fn current() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        git_commit: option_env!("WEBCODEX_BUILD_GIT_COMMIT").and_then(non_empty),
        git_dirty: option_env!("WEBCODEX_BUILD_GIT_DIRTY").and_then(parse_bool),
        built_at: option_env!("WEBCODEX_BUILD_BUILT_AT").and_then(non_empty),
    }
}

#[allow(dead_code)]
pub fn runtime_build_info() -> RuntimeBuildInfo {
    let info = current();
    RuntimeBuildInfo {
        git_commit: info.git_commit,
        git_dirty: info.git_dirty,
        built_at: info.built_at,
    }
}

pub fn version_output(binary: &str) -> String {
    let info = current();
    let mut output = format!(
        "{} {} (commit {}",
        binary,
        info.version,
        info.git_commit.unwrap_or("unknown")
    );
    if let Some(git_dirty) = info.git_dirty {
        output.push_str(&format!(", dirty={git_dirty}"));
    }
    if let Some(built_at) = info.built_at {
        output.push_str(&format!(", built_at={built_at}"));
    }
    output.push_str(")\n");
    output
}

fn non_empty(value: &'static str) -> Option<&'static str> {
    (!value.trim().is_empty()).then_some(value)
}

fn parse_bool(value: &'static str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_includes_package_version() {
        let info = current();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert!(!info.version.trim().is_empty());
    }

    #[test]
    fn build_info_version_output_includes_build_commit_or_unknown() {
        let output = version_output("webcodex-test");
        assert!(output.starts_with(&format!(
            "webcodex-test {} (commit ",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(output.trim_end().ends_with(')'));
        assert_ne!(
            output,
            format!("webcodex-test {}\n", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn build_info_runtime_build_metadata_is_safe() {
        let build = runtime_build_info();
        if let Some(commit) = build.git_commit {
            assert!(!commit.contains('/'));
            assert!(!commit.contains('\\'));
            assert!(!commit.trim().is_empty());
        }
    }
}
