//! Stable build metadata contracts and formatting. Git/environment capture is
//! owned by webcodex-build-info at the executable composition boundary.
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BuildInfo {
    pub version: &'static str,
    pub git_commit: Option<&'static str>,
    pub git_dirty: Option<bool>,
    pub built_at: Option<&'static str>,
    pub target: Option<&'static str>,
    pub architecture: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeBuildInfo {
    pub git_commit: Option<&'static str>,
    pub git_dirty: Option<bool>,
    pub built_at: Option<&'static str>,
    pub target: Option<&'static str>,
    pub architecture: Option<&'static str>,
}

impl BuildInfo {
    pub fn runtime_build_info(&self) -> RuntimeBuildInfo {
        RuntimeBuildInfo {
            git_commit: self.git_commit,
            git_dirty: self.git_dirty,
            built_at: self.built_at,
            target: self.target,
            architecture: self.architecture,
        }
    }

    pub fn version_output(&self, binary: &str) -> String {
        let mut output = format!(
            "{} {} (commit {}",
            binary,
            self.version,
            self.git_commit.unwrap_or("unknown")
        );
        if let Some(git_dirty) = self.git_dirty {
            output.push_str(&format!(", dirty={git_dirty}"));
        }
        if let Some(built_at) = self.built_at {
            output.push_str(&format!(", built_at={built_at}"));
        }
        output.push_str(")\n");
        output
    }

    /// Stable machine-readable metadata, independent of runtime configuration.
    pub fn machine_build_info(
        &self,
        binary: &str,
    ) -> crate::desktop_runtime_contract::MachineBuildInfo {
        use crate::desktop_runtime_contract::{
            MachineBuildInfo, BUILD_INFO_SCHEMA_VERSION, DESKTOP_RUNTIME_CONTRACT,
        };
        MachineBuildInfo {
            schema_version: BUILD_INFO_SCHEMA_VERSION,
            binary: binary.to_string(),
            version: self.version.to_string(),
            git_commit: self.git_commit.map(str::to_string),
            git_dirty: self.git_dirty,
            built_at: self.built_at.map(str::to_string),
            target: self.target.unwrap_or("unknown").to_string(),
            architecture: self
                .architecture
                .unwrap_or(std::env::consts::ARCH)
                .to_string(),
            desktop_runtime_contract: DESKTOP_RUNTIME_CONTRACT,
            agent_protocol_generation: matches!(binary, "webcodex-server" | "webcodex-runner")
                .then_some(crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2.get()),
            environment_data_format: Some(1),
        }
    }

    pub fn build_info_json(&self, binary: &str) -> String {
        format!(
            "{}\n",
            serde_json::to_string(&self.machine_build_info(binary))
                .expect("build identity contains only JSON primitives")
        )
    }
}

#[cfg(test)]
pub(crate) fn fixture() -> BuildInfo {
    BuildInfo {
        version: "0.4.3",
        git_commit: Some("0123456789ab"),
        git_dirty: Some(true),
        built_at: Some("1234567890"),
        target: Some("x86_64-unknown-linux-gnu"),
        architecture: Some("x86_64"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_formatting_preserves_identity_and_unknowns() {
        let info = fixture();
        assert_eq!(
            info.version_output("webcodex-test"),
            "webcodex-test 0.4.3 (commit 0123456789ab, dirty=true, built_at=1234567890)\n"
        );
        let runtime = info.runtime_build_info();
        assert_eq!(runtime.git_commit, info.git_commit);
        assert_eq!(runtime.git_dirty, Some(true));
        let machine: serde_json::Value =
            serde_json::from_str(&info.build_info_json("webcodex-runner")).unwrap();
        assert_eq!(machine["git_commit"], "0123456789ab");
        assert_eq!(machine["agent_protocol_generation"], 2);
        let unknown = BuildInfo {
            git_commit: None,
            git_dirty: None,
            built_at: None,
            ..info
        };
        assert_eq!(
            unknown.version_output("webcodex-test"),
            "webcodex-test 0.4.3 (commit unknown)\n"
        );
        assert_eq!(unknown.machine_build_info("webcodex").git_dirty, None);
    }
}
