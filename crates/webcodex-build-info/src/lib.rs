//! Checkout-dependent build identity for executable composition.
//!
//! Only this small crate owns Git/dirty build-script inputs. Core/domain crates
//! must not depend on it; their stable contracts must survive identity-only changes.

pub use webcodex_core::build_info::{BuildInfo, RuntimeBuildInfo};

pub fn current() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        git_commit: option_env!("WEBCODEX_BUILD_GIT_COMMIT").and_then(non_empty),
        git_dirty: option_env!("WEBCODEX_BUILD_GIT_DIRTY").and_then(parse_bool),
        built_at: option_env!("WEBCODEX_BUILD_BUILT_AT").and_then(non_empty),
        target: option_env!("WEBCODEX_BUILD_TARGET").and_then(non_empty),
        architecture: option_env!("WEBCODEX_BUILD_ARCHITECTURE").and_then(non_empty),
    }
}

pub fn runtime_build_info() -> RuntimeBuildInfo {
    current().runtime_build_info()
}

pub fn version_output(binary: &str) -> String {
    current().version_output(binary)
}

pub fn machine_build_info(
    binary: &str,
) -> webcodex_core::desktop_runtime_contract::MachineBuildInfo {
    current().machine_build_info(binary)
}

pub fn build_info_json(binary: &str) -> String {
    current().build_info_json(binary)
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
    fn compiled_identity_is_available_without_runtime_git_or_configuration() {
        let info = current();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert!(info.target.is_some_and(|value| !value.trim().is_empty()));
        assert!(info
            .architecture
            .is_some_and(|value| !value.trim().is_empty()));
        assert!(version_output("webcodex-test")
            .starts_with(&format!("webcodex-test {} (commit ", info.version)));
        for binary in [
            "webcodex",
            "webcodex-server",
            "webcodex-runner",
            "webcodex-desktop",
        ] {
            let machine = machine_build_info(binary);
            machine.validate(binary).unwrap();
            assert_eq!(machine.git_commit.as_deref(), info.git_commit);
            assert_eq!(machine.git_dirty, info.git_dirty);
        }
        assert_eq!(runtime_build_info(), info.runtime_build_info());
    }

    #[test]
    fn unknown_or_empty_override_does_not_claim_clean() {
        for value in ["", "unknown", "maybe"] {
            assert_eq!(parse_bool(value), None);
        }
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(non_empty("  "), None);
    }
}
