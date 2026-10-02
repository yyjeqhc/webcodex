use super::config::{configured_skill_root_identity, SkillsConfig};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;
use webcodex_core::runner_skill::{
    normalize_runner_skill_resource_path, RunnerSkillDescriptor, RunnerSkillExecutionRequest,
    RunnerSkillReadResponse, RunnerSkillSource, MAX_RUNNER_SKILL_READ_TEXT_BYTES,
    RUNNER_SKILL_RESPONSE_FORMAT,
};
use webcodex_core::skill_metadata::{parse_skill_metadata, MAX_SKILL_DEFINITION_BYTES};
use webcodex_workspace::file_read_range;

const SKILL_DEFINITION_FILE: &str = "SKILL.md";
const MAX_SKILL_PACKAGE_NAME_BYTES: usize = 160;
const MAX_CONFIGURED_SKILL_DIAGNOSTICS: usize = 8;
const MAX_CONFIGURED_SKILL_PACKAGES: usize = 256;
const MAX_CONFIGURED_SKILL_RESOURCE_FILE_BYTES: usize = 512 * 1024;
const MAX_CONFIGURED_SKILL_ROOT_SCAN_ENTRIES: usize = 1024;

#[derive(Debug)]
pub(super) struct LiveSkill {
    pub(super) descriptor: RunnerSkillDescriptor,
    package_root: PathBuf,
}

#[derive(Debug)]
pub(super) struct PreparedConfiguredSkillExecution {
    pub(super) target_path: PathBuf,
    pub(super) script: String,
}

#[derive(Debug, Default)]
pub(super) struct LiveDiscovery {
    pub(super) skills: Vec<LiveSkill>,
    pub(super) invalid_count: usize,
    pub(super) diagnostics: Vec<String>,
    pub(super) discovery_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfiguredSkillScanTrigger {
    CatalogList,
    ExactReadResolution,
}

impl ConfiguredSkillScanTrigger {
    const fn as_str(self) -> &'static str {
        match self {
            Self::CatalogList => "catalog_list",
            Self::ExactReadResolution => "exact_read_resolution",
        }
    }
}

#[derive(Debug, Default)]
struct ConfiguredSkillScanStats {
    roots_examined: usize,
    directory_entries_scanned: usize,
    definitions_attempted: usize,
    definitions_read: usize,
    definition_bytes_read: usize,
}

fn observe_configured_skill_scan(
    stats: &ConfiguredSkillScanStats,
    discovery: &LiveDiscovery,
    started: Instant,
    trigger: ConfiguredSkillScanTrigger,
    outcome_class: &'static str,
) {
    tracing::info!(
        event = "configured_skill_source_scan",
        source = "runner_configured",
        operation = "catalog_scan",
        trigger = trigger.as_str(),
        outcome_class,
        roots_examined = stats.roots_examined as u64,
        directory_entries_scanned = stats.directory_entries_scanned as u64,
        definitions_attempted = stats.definitions_attempted as u64,
        definitions_read = stats.definitions_read as u64,
        definition_bytes_read = stats.definition_bytes_read as u64,
        valid_count = discovery.skills.len() as u64,
        invalid_count = discovery.invalid_count as u64,
        truncated = discovery.discovery_truncated,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "configured_skill_source_scan"
    );
}

pub(super) fn discover(config: &SkillsConfig) -> Result<LiveDiscovery, String> {
    discover_with_trigger(config, ConfiguredSkillScanTrigger::CatalogList)
}

fn discover_with_trigger(
    config: &SkillsConfig,
    trigger: ConfiguredSkillScanTrigger,
) -> Result<LiveDiscovery, String> {
    let mut discovery = LiveDiscovery::default();
    let started = Instant::now();
    let mut stats = ConfiguredSkillScanStats::default();
    let mut seen_ids = BTreeSet::new();
    for configured_root in &config.roots {
        if discovery.skills.len() >= MAX_CONFIGURED_SKILL_PACKAGES {
            discovery.discovery_truncated = true;
            break;
        }
        stats.roots_examined = stats.roots_examined.saturating_add(1);
        let root_metadata = match fs::symlink_metadata(configured_root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                push_diagnostic(
                    &mut discovery.diagnostics,
                    "configured_skill_root_not_found",
                );
                continue;
            }
            Err(_) => {
                push_diagnostic(
                    &mut discovery.diagnostics,
                    "configured_skill_root_unavailable",
                );
                continue;
            }
        };
        if metadata_is_link_like(&root_metadata) {
            push_diagnostic(
                &mut discovery.diagnostics,
                "configured_skill_root_link_not_allowed",
            );
            continue;
        }
        if !root_metadata.is_dir() {
            push_diagnostic(
                &mut discovery.diagnostics,
                "configured_skill_root_not_directory",
            );
            continue;
        }
        let root = match configured_root.canonicalize() {
            Ok(root) if root.is_dir() => root,
            _ => {
                push_diagnostic(
                    &mut discovery.diagnostics,
                    "configured_skill_root_unavailable",
                );
                continue;
            }
        };
        let entries = match bounded_root_entries(&root, &mut stats) {
            Ok(entries) => entries,
            Err(code) => {
                if code == "configured_skill_root_scan_limit_exceeded" {
                    discovery.discovery_truncated = true;
                }
                push_diagnostic(&mut discovery.diagnostics, code);
                continue;
            }
        };
        for package_name in entries {
            if discovery.skills.len() >= MAX_CONFIGURED_SKILL_PACKAGES {
                discovery.discovery_truncated = true;
                break;
            }
            match load_live_skill(configured_root, &root, &package_name, &mut stats) {
                Ok(skill) => {
                    if !seen_ids.insert(skill.descriptor.skill_id().to_string()) {
                        observe_configured_skill_scan(
                            &stats, &discovery, started, trigger, "error",
                        );
                        return Err("configured_skill_identity_collision".to_string());
                    }
                    discovery.skills.push(skill);
                }
                Err(code) => {
                    discovery.invalid_count = discovery.invalid_count.saturating_add(1);
                    push_diagnostic(&mut discovery.diagnostics, code);
                }
            }
        }
    }
    discovery
        .skills
        .sort_by(|left, right| left.descriptor.skill_id().cmp(right.descriptor.skill_id()));
    observe_configured_skill_scan(&stats, &discovery, started, trigger, "success");
    Ok(discovery)
}

fn resolve_live_skill_by_id(
    config: &SkillsConfig,
    target_skill_id: &str,
) -> Result<(Option<LiveSkill>, ConfiguredSkillScanStats), String> {
    let mut discovery = LiveDiscovery::default();
    let started = Instant::now();
    let mut stats = ConfiguredSkillScanStats::default();
    for configured_root in &config.roots {
        stats.roots_examined = stats.roots_examined.saturating_add(1);
        let root_metadata = match fs::symlink_metadata(configured_root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                push_diagnostic(
                    &mut discovery.diagnostics,
                    "configured_skill_root_not_found",
                );
                continue;
            }
            Err(_) => {
                push_diagnostic(
                    &mut discovery.diagnostics,
                    "configured_skill_root_unavailable",
                );
                continue;
            }
        };
        if metadata_is_link_like(&root_metadata) {
            push_diagnostic(
                &mut discovery.diagnostics,
                "configured_skill_root_link_not_allowed",
            );
            continue;
        }
        if !root_metadata.is_dir() {
            push_diagnostic(
                &mut discovery.diagnostics,
                "configured_skill_root_not_directory",
            );
            continue;
        }
        let root = match configured_root.canonicalize() {
            Ok(root) if root.is_dir() => root,
            _ => {
                push_diagnostic(
                    &mut discovery.diagnostics,
                    "configured_skill_root_unavailable",
                );
                continue;
            }
        };
        let entries = match bounded_root_entries(&root, &mut stats) {
            Ok(entries) => entries,
            Err(code) => {
                if code == "configured_skill_root_scan_limit_exceeded" {
                    discovery.discovery_truncated = true;
                }
                push_diagnostic(&mut discovery.diagnostics, code);
                continue;
            }
        };
        for package_name in entries {
            if !valid_package_name(&package_name) {
                discovery.invalid_count = discovery.invalid_count.saturating_add(1);
                continue;
            }
            if webcodex_core::sensitive_paths::is_secret_path(&package_name) {
                discovery.invalid_count = discovery.invalid_count.saturating_add(1);
                continue;
            }
            if configured_skill_id(configured_root, &package_name) != target_skill_id {
                continue;
            }
            match load_live_skill(configured_root, &root, &package_name, &mut stats) {
                Ok(skill) => {
                    if !discovery.skills.is_empty() {
                        observe_configured_skill_scan(
                            &stats,
                            &discovery,
                            started,
                            ConfiguredSkillScanTrigger::ExactReadResolution,
                            "error",
                        );
                        return Err("configured_skill_identity_collision".to_string());
                    }
                    discovery.skills.push(skill);
                }
                Err(code) => {
                    discovery.invalid_count = discovery.invalid_count.saturating_add(1);
                    push_diagnostic(&mut discovery.diagnostics, code);
                }
            }
        }
    }
    observe_configured_skill_scan(
        &stats,
        &discovery,
        started,
        ConfiguredSkillScanTrigger::ExactReadResolution,
        "success",
    );
    Ok((discovery.skills.pop(), stats))
}

pub(super) fn resolve_live_skill(
    config: &SkillsConfig,
    target_skill_id: &str,
) -> Result<Option<LiveSkill>, String> {
    resolve_live_skill_by_id(config, target_skill_id).map(|(skill, _)| skill)
}

fn bounded_root_entries(
    root: &Path,
    stats: &mut ConfiguredSkillScanStats,
) -> Result<Vec<String>, &'static str> {
    let read_dir = fs::read_dir(root).map_err(|_| "configured_skill_root_unavailable")?;
    let mut candidates = Vec::new();
    let mut scanned = 0usize;
    for entry in read_dir {
        scanned = scanned.saturating_add(1);
        stats.directory_entries_scanned = stats.directory_entries_scanned.saturating_add(1);
        if scanned > MAX_CONFIGURED_SKILL_ROOT_SCAN_ENTRIES {
            return Err("configured_skill_root_scan_limit_exceeded");
        }
        let entry = entry.map_err(|_| "configured_skill_root_unavailable")?;
        let file_type = entry
            .file_type()
            .map_err(|_| "configured_skill_root_unavailable")?;
        if !file_type.is_dir() && !file_type.is_symlink() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            candidates.push(String::new());
            continue;
        };
        candidates.push(name);
    }
    candidates.sort();
    Ok(candidates)
}

fn load_live_skill(
    configured_root: &Path,
    canonical_root: &Path,
    package_name: &str,
    stats: &mut ConfiguredSkillScanStats,
) -> Result<LiveSkill, &'static str> {
    if !valid_package_name(package_name) {
        return Err("invalid_skill_package");
    }
    if webcodex_core::sensitive_paths::is_secret_path(package_name) {
        return Err("sensitive_skill_definition");
    }
    let package_path = canonical_root.join(package_name);
    let metadata = fs::symlink_metadata(&package_path).map_err(|_| "invalid_skill_package")?;
    if metadata_is_link_like(&metadata) || !metadata.is_dir() {
        return Err("invalid_skill_package");
    }
    let package_root = package_path
        .canonicalize()
        .map_err(|_| "invalid_skill_package")?;
    if !webcodex_runner_config::paths::path_is_within(&package_root, canonical_root) {
        return Err("invalid_skill_package");
    }
    stats.definitions_attempted = stats.definitions_attempted.saturating_add(1);
    let definition = resolve_regular_package_file(
        &package_root,
        canonical_root,
        SKILL_DEFINITION_FILE,
        "missing_skill_definition",
        "invalid_skill_package",
    )?;
    let bytes =
        read_bounded(&definition, MAX_SKILL_DEFINITION_BYTES).map_err(|code| match code {
            "too_large" => "skill_definition_too_large",
            "invalid_utf8" => "invalid_utf8_skill_definition",
            _ => "invalid_skill_package",
        })?;
    stats.definitions_read = stats.definitions_read.saturating_add(1);
    stats.definition_bytes_read = stats.definition_bytes_read.saturating_add(bytes.len());
    let text = std::str::from_utf8(&bytes).map_err(|_| "invalid_utf8_skill_definition")?;
    let skill_metadata = parse_skill_metadata(text)?;
    let definition_revision = sha256_hex(&bytes);
    let skill_id = configured_skill_id(configured_root, package_name);
    Ok(LiveSkill {
        descriptor: RunnerSkillDescriptor::Configured {
            skill_id,
            name: skill_metadata.name,
            description: skill_metadata.description,
            definition_revision,
        },
        package_root,
    })
}

pub(super) fn prepare_execution_target(
    config: &SkillsConfig,
    request: &RunnerSkillExecutionRequest,
) -> Result<PreparedConfiguredSkillExecution, String> {
    request
        .validate()
        .map_err(|_| "skill_invalid_execution_request".to_string())?;
    if request.expected_source != RunnerSkillSource::Configured {
        return Err("skill_source_changed".to_string());
    }
    let path = normalize_runner_skill_resource_path(&request.path)
        .map_err(|_| "skill_resource_path_invalid".to_string())?;
    if webcodex_core::sensitive_paths::is_secret_path(&path) {
        return Err("skill_sensitive_path".to_string());
    }
    let (skill, _scan_stats) = resolve_live_skill_by_id(config, &request.skill_id)?;
    let skill = skill.ok_or_else(|| "skill_not_found".to_string())?;
    if skill.descriptor.definition_revision() != request.expected_definition_revision {
        return Err("skill_definition_changed".to_string());
    }
    let canonical_root = skill
        .package_root
        .parent()
        .ok_or_else(|| "skill_resource_path_invalid".to_string())?;
    let target = resolve_regular_package_file(
        &skill.package_root,
        canonical_root,
        &path,
        "skill_resource_not_found",
        "skill_resource_path_invalid",
    )
    .map_err(str::to_string)?;
    let bytes =
        read_bounded(&target, MAX_RUNNER_SKILL_READ_TEXT_BYTES).map_err(|code| match code {
            "too_large" => "skill_resource_too_large".to_string(),
            "invalid_utf8" => "skill_resource_unsupported_encoding".to_string(),
            _ => "skill_resource_unavailable".to_string(),
        })?;
    if sha256_hex(&bytes) != request.expected_resource_sha256 {
        return Err("skill_resource_changed".to_string());
    }
    let script =
        String::from_utf8(bytes).map_err(|_| "skill_resource_unsupported_encoding".to_string())?;
    if script.contains('\0') {
        return Err("skill_resource_unsupported_encoding".to_string());
    }
    let definition_after = resolve_regular_package_file(
        &skill.package_root,
        canonical_root,
        SKILL_DEFINITION_FILE,
        "skill_definition_changed",
        "skill_definition_changed",
    )
    .map_err(str::to_string)?;
    let definition_after = read_bounded(&definition_after, MAX_SKILL_DEFINITION_BYTES)
        .map_err(|_| "skill_definition_changed".to_string())?;
    if sha256_hex(&definition_after) != request.expected_definition_revision {
        return Err("skill_definition_changed".to_string());
    }
    Ok(PreparedConfiguredSkillExecution {
        target_path: target,
        script,
    })
}

pub(super) fn read_resource(
    config: &SkillsConfig,
    skill_id: &str,
    requested_path: &str,
    start_line: usize,
    limit: usize,
    expected_definition_revision: Option<&str>,
) -> Result<RunnerSkillReadResponse, String> {
    let path = normalize_runner_skill_resource_path(requested_path)
        .map_err(|_| "skill_resource_path_invalid".to_string())?;
    if webcodex_core::sensitive_paths::is_secret_path(&path) {
        return Err("skill_sensitive_path".to_string());
    }
    let (skill, _scan_stats) = resolve_live_skill_by_id(config, skill_id)?;
    let skill = skill.ok_or_else(|| "skill_not_found".to_string())?;
    if expected_definition_revision
        .is_some_and(|expected| expected != skill.descriptor.definition_revision())
    {
        return Err("skill_definition_changed".to_string());
    }
    let canonical_root = skill
        .package_root
        .parent()
        .ok_or_else(|| "skill_resource_path_invalid".to_string())?;
    let target = resolve_regular_package_file(
        &skill.package_root,
        canonical_root,
        &path,
        "skill_resource_not_found",
        "skill_resource_path_invalid",
    )
    .map_err(str::to_string)?;
    let relative = target
        .strip_prefix(&skill.package_root)
        .map_err(|_| "skill_resource_path_invalid".to_string())?
        .to_string_lossy();
    if webcodex_core::sensitive_paths::is_secret_path(relative.as_ref()) {
        return Err("skill_sensitive_path".to_string());
    }
    let max_file_bytes = if path == SKILL_DEFINITION_FILE {
        MAX_SKILL_DEFINITION_BYTES
    } else {
        MAX_CONFIGURED_SKILL_RESOURCE_FILE_BYTES
    };
    // Enforce the file bound on the bytes actually read, not a metadata
    // snapshot: configured resources can grow or be replaced between reads.
    let bytes = read_bounded(&target, max_file_bytes).map_err(|code| match code {
        "too_large" => "skill_resource_too_large".to_string(),
        "invalid_utf8" => "skill_resource_unsupported_encoding".to_string(),
        _ => "skill_resource_unavailable".to_string(),
    })?;
    let range = file_read_range::EffectiveRange::new(Some(start_line), Some(limit));
    let read = file_read_range::read_range_from_with_budget(
        bytes.as_slice(),
        range,
        MAX_RUNNER_SKILL_READ_TEXT_BYTES,
    )
    .map_err(|error| match error.reason {
        file_read_range::ReadFileReason::InvalidUtf8 => {
            "skill_resource_unsupported_encoding".to_string()
        }
        file_read_range::ReadFileReason::NotFound => "skill_resource_not_found".to_string(),
        file_read_range::ReadFileReason::RangeTooLarge => "skill_read_result_too_large".to_string(),
        _ => "skill_resource_unavailable".to_string(),
    })?;
    let definition_after = resolve_regular_package_file(
        &skill.package_root,
        canonical_root,
        SKILL_DEFINITION_FILE,
        "skill_definition_changed",
        "skill_definition_changed",
    )
    .map_err(str::to_string)?;
    let definition_after = read_bounded(&definition_after, MAX_SKILL_DEFINITION_BYTES)
        .map_err(|_| "skill_definition_changed".to_string())?;
    if sha256_hex(&definition_after) != skill.descriptor.definition_revision()
        || (path == SKILL_DEFINITION_FILE && read.sha256 != skill.descriptor.definition_revision())
    {
        return Err("skill_definition_changed".to_string());
    }
    let response = RunnerSkillReadResponse {
        format: RUNNER_SKILL_RESPONSE_FORMAT.to_string(),
        skill: skill.descriptor,
        path: path.clone(),
        sha256: read.sha256,
        text: read.content,
        start_line: read.start_line,
        end_line: read.end_line,
        returned_lines: read.returned_lines,
        has_more: read.has_more,
        next_start_line: read.next_start_line,
    };
    response
        .validate_for_request(
            skill_id,
            RunnerSkillSource::Configured,
            &path,
            start_line,
            limit,
        )
        .map_err(|_| "configured_skill_response_invalid".to_string())?;
    Ok(response)
}

fn resolve_regular_package_file(
    package_root: &Path,
    canonical_root: &Path,
    relative: &str,
    missing_code: &'static str,
    invalid_code: &'static str,
) -> Result<PathBuf, &'static str> {
    let mut current = package_root.to_path_buf();
    let components = relative.split('/').collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                missing_code
            } else {
                invalid_code
            }
        })?;
        let last = index + 1 == components.len();
        if metadata_is_link_like(&metadata)
            || (last && !metadata.is_file())
            || (!last && !metadata.is_dir())
        {
            return Err(invalid_code);
        }
    }
    let target = current.canonicalize().map_err(|_| invalid_code)?;
    if !webcodex_runner_config::paths::path_is_within(&target, package_root)
        || !webcodex_runner_config::paths::path_is_within(&target, canonical_root)
    {
        return Err(invalid_code);
    }
    Ok(target)
}

pub(super) fn metadata_is_link_like(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn read_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>, &'static str> {
    let file = File::open(path).map_err(|_| "unavailable")?;
    let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "unavailable")?;
    if bytes.len() > max_bytes {
        return Err("too_large");
    }
    if std::str::from_utf8(&bytes).is_err() {
        return Err("invalid_utf8");
    }
    Ok(bytes)
}

fn configured_skill_id(root: &Path, package_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.configured-skill-id.v1\0");
    hasher.update(configured_skill_root_identity(root).as_bytes());
    hasher.update(b"\0");
    hasher.update(package_name.as_bytes());
    format!(
        "wc_skill_{}",
        webcodex_core::compact::encode(&hasher.finalize()[..16])
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_SKILL_PACKAGE_NAME_BYTES
        && !name.contains(['/', '\\'])
        && name != "."
        && name != ".."
        && !name.chars().any(char::is_control)
}

fn push_diagnostic(diagnostics: &mut Vec<String>, code: &str) {
    if diagnostics.len() < MAX_CONFIGURED_SKILL_DIAGNOSTICS {
        diagnostics.push(code.to_string());
    }
}

#[cfg(test)]
#[path = "configured_skills_tests.rs"]
mod tests;
