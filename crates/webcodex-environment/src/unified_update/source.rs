//! Read-only validation of #697 native build provenance. An update source is
//! not an UpgradeCandidate: installation still requires Core to verify every
//! extracted payload byte, executable, owner receipt and upgrade transaction.
use super::*;
use webcodex_core::desktop_runtime_contract::{DesktopRuntimeContract, MachineBuildInfo};

#[derive(Debug, Clone)]
pub struct UpdateSource {
    pub flavor: PackageFlavor,
    pub version: String,
    pub source_sha: String,
    pub platform: RuntimePlatform,
    pub manifest_sha256: String,
    components: BTreeMap<String, Component>,
    windows_guarded_bootstrap_contract: Option<u16>,
}
impl UpdateSource {
    /// Capability belongs to the verified raw CLI metadata from the official
    /// same-source bootstrap build, not to the generic MachineBuildInfo wire.
    pub fn supports_guarded_windows_handoff(&self) -> bool {
        matches!(
            self.platform,
            RuntimePlatform::Win32X64 | RuntimePlatform::Win32Arm64
        ) && self.windows_guarded_bootstrap_contract == Some(WINDOWS_GUARDED_HANDOFF_VERSION)
    }
    pub fn component_build(&self, name: &str) -> Option<&MachineBuildInfo> {
        self.components
            .get(name)
            .map(|component| &component.build_info)
    }
    pub fn component_sha256(&self, name: &str) -> Option<&str> {
        self.components
            .get(name)
            .map(|component| component.sha256.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Component {
    path: String,
    sha256: String,
    build_info: MachineBuildInfo,
    build_info_sha256: String,
    probe: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedFile {
    path: String,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DesktopPayload {
    path: String,
    sha256: String,
    executable: String,
    #[serde(default)]
    managed_files: Option<Vec<ManagedFile>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    schema_version: u16,
    #[serde(default)]
    package_flavor: PackageFlavor,
    version: String,
    source_sha: String,
    platform: RuntimePlatform,
    target: String,
    architecture: String,
    source_workflow_run_id: u64,
    source_workflow_ref: String,
    desktop_runtime_contract: DesktopRuntimeContract,
    artifacts: BTreeMap<String, Component>,
    #[serde(default)]
    desktop_payload: Option<DesktopPayload>,
}

fn relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value
            .chars()
            .any(|c| c.is_control() || c == '\\' || c == ':')
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn verify_source_manifest(
    bytes: &[u8],
    version: &str,
    platform: RuntimePlatform,
) -> UpdateResult<UpdateSource> {
    verify_source_manifest_for_flavor(bytes, version, platform, PackageFlavor::Full)
}

pub fn verify_source_manifest_for_flavor(
    bytes: &[u8],
    version: &str,
    platform: RuntimePlatform,
    flavor: PackageFlavor,
) -> UpdateResult<UpdateSource> {
    let bad = UpdateError::SourceManifestInvalid;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err(bad);
    }
    let raw = strict_json(bytes).map_err(|_| bad)?;
    let source: Source = serde_json::from_value(raw.clone()).map_err(|_| bad)?;
    if (flavor.is_full() && raw.get("package_flavor").is_some())
        || source.schema_version != flavor.source_schema()
        || source.package_flavor != flavor
        || (!flavor.is_full()
            && !matches!(
                platform,
                RuntimePlatform::LinuxX64 | RuntimePlatform::LinuxArm64
            ))
        || !stable_version(version)
        || source.version != version
        || source.platform != platform
        || source.target != platform.target()
        || source.architecture != platform.architecture()
        || !lowercase_hex(&source.source_sha, 40)
        || source.source_workflow_run_id == 0
        || source.source_workflow_ref
            != format!(
                "{OFFICIAL_REPOSITORY}/.github/workflows/release-build.yml@refs/tags/v{version}"
            )
        || !source.desktop_runtime_contract.is_valid()
        || source.artifacts.len() != flavor.components().len()
    {
        return Err(bad);
    }
    let mut windows_guarded_bootstrap_contract = None;
    let mut data_format = None;
    let mut runner_generation = None;
    for &name in flavor.components() {
        let component = source.artifacts.get(name).ok_or(bad)?;
        let info = &component.build_info;
        info.validate(name).map_err(|_| bad)?;
        // Hash the original JSON object, not a reserialized wire struct that
        // might omit additive fields. This matches Core's candidate verifier.
        let encoded = serde_json::to_vec(&raw["artifacts"][name]["build_info"]).map_err(|_| bad)?;
        if (!flavor.is_full() && component.path != format!("artifacts/bin/{name}"))
            || !relative_path(&component.path)
            || !valid_sha256(&component.sha256)
            || !valid_sha256(&component.build_info_sha256)
            || sha256(&encoded) != component.build_info_sha256
            || component.probe != "native-build-job"
            || info.version != version
            || info.git_commit.as_deref() != Some(source.source_sha.as_str())
            || info.git_dirty != Some(false)
            || info.target != platform.target()
            || info.architecture != platform.architecture()
            || info.desktop_runtime_contract != source.desktop_runtime_contract
            || info
                .built_at
                .as_deref()
                .is_none_or(|v| v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()))
            || info.environment_data_format.is_none_or(|v| v == 0)
        {
            return Err(bad);
        }
        if let Some(marker) =
            raw["artifacts"][name]["build_info"].get(WINDOWS_GUARDED_BOOTSTRAP_BUILD_INFO_FIELD)
        {
            if name != "webcodex"
                || !matches!(
                    platform,
                    RuntimePlatform::Win32X64 | RuntimePlatform::Win32Arm64
                )
            {
                return Err(bad);
            }
            windows_guarded_bootstrap_contract = Some(
                marker
                    .as_u64()
                    .filter(|value| *value > 0 && *value <= u16::MAX as u64)
                    .ok_or(bad)? as u16,
            );
        }
        if data_format.is_some_and(|v| Some(v) != info.environment_data_format) {
            return Err(bad);
        }
        data_format = info.environment_data_format;
        if name == "webcodex-server" || name == "webcodex-runner" {
            if info.agent_protocol_generation.is_none_or(|v| v == 0)
                || runner_generation.is_some_and(|v| Some(v) != info.agent_protocol_generation)
            {
                return Err(bad);
            }
            runner_generation = info.agent_protocol_generation;
        }
    }
    if !flavor.is_full() {
        if source.desktop_payload.is_some() || raw.get("desktop_payload").is_some() {
            return Err(bad);
        }
        return Ok(UpdateSource {
            flavor,
            version: source.version,
            source_sha: source.source_sha,
            platform,
            manifest_sha256: sha256(bytes),
            components: source.artifacts,
            windows_guarded_bootstrap_contract,
        });
    }
    let payload = source.desktop_payload.as_ref().ok_or(bad)?;
    if !relative_path(&payload.path)
        || !relative_path(&payload.executable)
        || !valid_sha256(&payload.sha256)
    {
        return Err(bad);
    }
    let desktop = &source.artifacts["webcodex-desktop"];
    match platform {
        RuntimePlatform::DarwinX64 | RuntimePlatform::DarwinArm64 => {
            if !payload.path.ends_with(".app")
                || !payload.executable.starts_with("Contents/MacOS/")
                || desktop.path != format!("{}/{}", payload.path, payload.executable)
                || payload.managed_files.is_some()
            {
                return Err(bad);
            }
        }
        RuntimePlatform::LinuxX64 | RuntimePlatform::LinuxArm64 => {
            if desktop.path != payload.path
                || payload.path.rsplit('/').next() != Some(payload.executable.as_str())
                || payload.sha256 != desktop.sha256
                || payload.managed_files.is_some()
            {
                return Err(bad);
            }
        }
        RuntimePlatform::Win32X64 | RuntimePlatform::Win32Arm64 => {
            if desktop.path != payload.path
                || payload.executable != "WebCodex.exe"
                || payload.sha256 != desktop.sha256
            {
                return Err(bad);
            }
            let files = payload.managed_files.as_ref().ok_or(bad)?;
            if files.len() != 4 {
                return Err(bad);
            }
            let mut declared = BTreeMap::new();
            for file in files {
                if !valid_sha256(&file.sha256)
                    || declared
                        .insert(file.path.as_str(), file.sha256.as_str())
                        .is_some()
                {
                    return Err(bad);
                }
            }
            for (path, name) in [
                ("WebCodex.exe", "webcodex-desktop"),
                ("webcodex-runtime/webcodex.exe", "webcodex"),
                ("webcodex-runtime/webcodex-server.exe", "webcodex-server"),
                ("webcodex-runtime/webcodex-runner.exe", "webcodex-runner"),
            ] {
                if declared.get(path).copied()
                    != source.artifacts.get(name).map(|item| item.sha256.as_str())
                {
                    return Err(bad);
                }
            }
        }
    }
    Ok(UpdateSource {
        flavor,
        version: source.version,
        source_sha: source.source_sha,
        platform,
        manifest_sha256: sha256(bytes),
        components: source.artifacts,
        windows_guarded_bootstrap_contract,
    })
}
