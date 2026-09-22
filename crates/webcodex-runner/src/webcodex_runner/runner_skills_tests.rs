use super::*;
use crate::webcodex_runner::config::{RunnerPolicy, SkillsConfig};
use crate::webcodex_runner::skill_store::SkillStore;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Write};
#[cfg(feature = "runner-real-process-tests")]
use std::process::{Command, Stdio};
use tempfile::TempDir;
use webcodex_core::runner_skill::{
    RunnerSkillDescriptor, RunnerSkillExecutionRequest, RunnerSkillSource,
};
use zip::write::SimpleFileOptions;

const CONFIGURED_PYTHON_HELPER: &str = "VALUE = 'configured-helper'\n";
const MANAGED_PYTHON_HELPER: &str = "VALUE = 'managed-helper'\n";
const PYTHON_PROBE: &str = concat!(
    "from helper import VALUE\n",
    "import os\n",
    "print('VALUE=' + VALUE)\n",
    "print('FILE=' + __file__)\n",
    "print('CWD=' + os.getcwd())\n",
);

fn write_configured_skill(root: &std::path::Path) {
    let package = root.join("configured");
    fs::create_dir_all(package.join("references")).unwrap();
    fs::create_dir_all(package.join("scripts")).unwrap();
    fs::write(
        package.join("SKILL.md"),
        "---\nname: configured\ndescription: configured guidance\n---\nbody\n",
    )
    .unwrap();
    fs::write(package.join("references/guide.md"), "configured resource\n").unwrap();
    fs::write(package.join("scripts/helper.py"), CONFIGURED_PYTHON_HELPER).unwrap();
    fs::write(package.join("scripts/probe.py"), PYTHON_PROBE).unwrap();
}

fn configured_fixture() -> (TempDir, SkillsConfig, RunnerSkillDescriptor) {
    let root = tempfile::tempdir().unwrap();
    write_configured_skill(root.path());
    let config = SkillsConfig {
        roots: vec![root.path().to_path_buf()],
    };
    let descriptor = configured_skills::discover(&config)
        .unwrap()
        .skills
        .remove(0)
        .descriptor;
    (root, config, descriptor)
}

fn archive_bytes() -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("SKILL.md", options).unwrap();
    writer
        .write_all(b"---\nname: managed\ndescription: managed guidance\n---\nbody\n")
        .unwrap();
    writer.start_file("references/guide.md", options).unwrap();
    writer.write_all(b"managed resource\n").unwrap();
    writer.start_file("scripts/helper.py", options).unwrap();
    writer.write_all(MANAGED_PYTHON_HELPER.as_bytes()).unwrap();
    writer.start_file("scripts/probe.py", options).unwrap();
    writer.write_all(PYTHON_PROBE.as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn execution_request(
    descriptor: &RunnerSkillDescriptor,
    path: &str,
    script: &str,
) -> RunnerSkillExecutionRequest {
    RunnerSkillExecutionRequest {
        skill_id: descriptor.skill_id().to_string(),
        expected_source: descriptor.source(),
        path: path.to_string(),
        expected_definition_revision: descriptor.definition_revision().to_string(),
        expected_package_revision: descriptor.package_revision().map(str::to_string),
        expected_resource_sha256: sha256_hex(script.as_bytes()),
        args: vec!["literal argument".to_string()],
    }
}

#[cfg(feature = "runner-real-process-tests")]
fn run_python_candidate(
    request: &RunnerSkillExecutionRequest,
    target: &std::path::Path,
    script: &str,
    cwd: &std::path::Path,
) -> std::process::Output {
    let target = target.to_str().expect("test Skill path must be UTF-8");
    for (executable, args) in skill_execution_candidates(request, target).unwrap() {
        #[cfg(windows)]
        let executable = {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let Some(resolved) =
                crate::webcodex_runner::util::resolve_program_in_path(&executable, &path)
            else {
                continue;
            };
            if !windows_skill_interpreter_path_available(resolved.path()) {
                continue;
            }
            resolved.path().to_string_lossy().into_owned()
        };
        let child = Command::new(&executable)
            .args(&args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("could not spawn {executable}: {error}"),
        };
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(script.as_bytes())
            .unwrap();
        return child.wait_with_output().unwrap();
    }
    panic!("no supported Python interpreter is available for the real-process regression test")
}

#[cfg(all(feature = "runner-real-process-tests", windows))]
#[test]
fn python_interpreter_admission_skips_windows_app_execution_aliases() {
    let path = std::env::var_os("PATH").unwrap_or_default();
    if let Some(python3) = crate::webcodex_runner::util::resolve_program_in_path("python3", &path) {
        if std::fs::symlink_metadata(python3.path())
            .ok()
            .is_some_and(|metadata| configured_skills::metadata_is_link_like(&metadata))
        {
            assert!(
                !windows_skill_interpreter_path_available(python3.path()),
                "App Execution Alias must not be admitted as a Skill interpreter: {}",
                python3.path().display()
            );
        }
    }
    if let Some(python) = crate::webcodex_runner::util::resolve_program_in_path("python", &path) {
        if std::fs::symlink_metadata(python.path())
            .ok()
            .is_some_and(|metadata| !configured_skills::metadata_is_link_like(&metadata))
        {
            assert!(windows_skill_interpreter_path_available(python.path()));
        }
    }
}

#[cfg(feature = "runner-real-process-tests")]
fn output_field<'a>(stdout: &'a str, name: &str) -> &'a str {
    let prefix = format!("{name}=");
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("missing {name} field in Skill Python output: {stdout}"))
}

#[cfg(feature = "runner-real-process-tests")]
fn canonical_output_path(stdout: &str, name: &str) -> std::path::PathBuf {
    let raw = output_field(stdout, name);
    fs::canonicalize(raw)
        .unwrap_or_else(|error| panic!("could not canonicalize {name} path {raw:?}: {error}"))
}

#[cfg(feature = "runner-real-process-tests")]
fn assert_python_package_context(
    config: &SkillsConfig,
    store: &SkillStore,
    request: &RunnerSkillExecutionRequest,
    expected_value: &str,
) {
    let prepared = prepare_skill_resource_execution_with_store(config, store, request).unwrap();
    let business = tempfile::tempdir().unwrap();
    let output = run_python_candidate(
        request,
        &prepared.target_path,
        &prepared.script,
        business.path(),
    );
    assert!(
        output.status.success(),
        "Skill Python wrapper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(output_field(&stdout, "VALUE"), expected_value, "{stdout}");

    let actual_file = canonical_output_path(&stdout, "FILE");
    let expected_file = fs::canonicalize(&prepared.target_path).unwrap();
    assert_eq!(
        actual_file, expected_file,
        "Skill __file__ must identify the prepared Runner-owned target: {stdout}"
    );

    let actual_cwd = canonical_output_path(&stdout, "CWD");
    let expected_cwd = business.path().canonicalize().unwrap();
    assert_eq!(
        actual_cwd, expected_cwd,
        "Skill cwd must identify the business working directory: {stdout}"
    );
}

fn managed_fixture() -> (TempDir, SkillStore, RunnerSkillDescriptor) {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let store = SkillStore::for_test(root.path().join("store"), "runner-test");
    let archive = archive_bytes();
    let archive_sha = sha256_hex(&archive);
    fs::write(source.path().join("skill.zip"), &archive).unwrap();
    let policy = RunnerPolicy {
        allow_cwd_anywhere: true,
        ..RunnerPolicy::default()
    };
    let installed = store
        .install(
            &policy,
            "managed",
            "project-test",
            source.path().to_string_lossy().as_ref(),
            "skill.zip",
            &archive_sha,
            "install-managed-fixture",
            true,
            None,
        )
        .unwrap();
    let descriptor = store
        .resolve_active(&installed.skill_id)
        .unwrap()
        .expect("installed Skill must be active");
    (root, store, descriptor)
}

#[test]
fn configured_only_runtime_list_resolve_and_read_use_canonical_source() {
    let (_root, config, configured) = configured_fixture();
    let store_root = tempfile::tempdir().unwrap();
    let store = SkillStore::for_test(store_root.path().join("store"), "runner-empty");

    let listed = list_runner_skills(&config, &store).unwrap();
    assert_eq!(listed.skills.len(), 1);
    assert_eq!(listed.skills[0].source(), RunnerSkillSource::Configured);

    let resolved = resolve_runner_skill(&config, &store, configured.skill_id())
        .unwrap()
        .expect("configured target");
    assert_eq!(resolved.source(), RunnerSkillSource::Configured);
    assert_eq!(resolved.skill_id(), configured.skill_id());

    let read = read_runner_skill(
        &config,
        &store,
        configured.skill_id(),
        RunnerSkillSource::Configured,
        "references/guide.md",
        1,
        20,
        None,
        Some(configured.definition_revision()),
    )
    .unwrap();
    assert_eq!(read.skill.source(), RunnerSkillSource::Configured);
    assert_eq!(read.text, "configured resource");
}

#[test]
fn managed_only_runtime_resolve_read_and_explicit_revision_checks_are_preserved() {
    let (_root, store, managed) = managed_fixture();
    let config = SkillsConfig::default();

    let listed = list_runner_skills(&config, &store).unwrap();
    assert_eq!(listed.skills.len(), 1);
    assert_eq!(listed.skills[0].source(), RunnerSkillSource::Managed);
    let resolved = resolve_runner_skill(&config, &store, managed.skill_id())
        .unwrap()
        .expect("managed target");
    assert_eq!(resolved.source(), RunnerSkillSource::Managed);

    let read = read_runner_skill(
        &config,
        &store,
        managed.skill_id(),
        RunnerSkillSource::Managed,
        "references/guide.md",
        1,
        20,
        None,
        None,
    )
    .unwrap();
    assert_eq!(read.text, "managed resource");

    assert_eq!(
        read_runner_skill(
            &config,
            &store,
            managed.skill_id(),
            RunnerSkillSource::Managed,
            "SKILL.md",
            1,
            20,
            Some(&"wc_skillpkg___________________________________________8".to_string()),
            None,
        )
        .unwrap_err(),
        "skill_package_changed"
    );
    assert_eq!(
        read_runner_skill(
            &config,
            &store,
            managed.skill_id(),
            RunnerSkillSource::Managed,
            "SKILL.md",
            1,
            20,
            None,
            Some(&"f".repeat(64)),
        )
        .unwrap_err(),
        "skill_definition_changed"
    );
}

#[test]
fn mixed_list_keeps_configured_and_managed_source_identity_explicit() {
    let (_configured_root, config, configured) = configured_fixture();
    let (_managed_root, store, managed) = managed_fixture();
    let listed = list_runner_skills(&config, &store).unwrap();
    assert_eq!(listed.skills.len(), 2);
    assert!(listed
        .skills
        .iter()
        .any(|skill| skill.skill_id() == configured.skill_id()
            && skill.source() == RunnerSkillSource::Configured));
    assert!(listed
        .skills
        .iter()
        .any(|skill| skill.skill_id() == managed.skill_id()
            && skill.source() == RunnerSkillSource::Managed));
    assert!(resolve_runner_skill(
        &config,
        &store,
        &"wc_skill_AAAAAAAAAAAAAAAAAAAAAA".to_string()
    )
    .unwrap()
    .is_none());
}

#[test]
fn duplicate_target_and_source_identity_change_fail_closed_without_priority() {
    let duplicate_id = "wc_skill_qqqqqqqqqqqqqqqqqqqqqg".to_string();
    let configured = RunnerSkillDescriptor::Configured {
        skill_id: duplicate_id.clone(),
        name: "configured".to_string(),
        description: "configured".to_string(),
        definition_revision: "b".repeat(64),
    };
    let managed = RunnerSkillDescriptor::Managed {
        skill_id: duplicate_id,
        skill_key: "managed".to_string(),
        name: "managed".to_string(),
        description: "managed".to_string(),
        package_revision: "wc_skillpkg_zMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMw".to_string(),
        definition_revision: "d".repeat(64),
    };

    assert_eq!(
        resolve_candidates(Some(configured.clone()), Some(managed.clone())).unwrap_err(),
        "skill_catalog_unavailable"
    );
    assert_eq!(
        ensure_unique_skill_ids(&[configured.clone(), managed.clone()]).unwrap_err(),
        "skill_catalog_unavailable"
    );
    assert_eq!(
        require_resolved_source(Some(&managed), RunnerSkillSource::Configured).unwrap_err(),
        "skill_source_changed"
    );
    assert_eq!(
        require_resolved_source(None, RunnerSkillSource::Configured).unwrap_err(),
        "skill_source_changed"
    );
}

#[test]
fn configured_and_managed_execution_targets_preserve_package_context_without_project_copying() {
    let (_configured_root, config, configured) = configured_fixture();
    let empty_store_root = tempfile::tempdir().unwrap();
    let empty_store = SkillStore::for_test(empty_store_root.path().join("store"), "runner-empty");
    let configured_request = execution_request(&configured, "scripts/probe.py", PYTHON_PROBE);
    let configured_prepared =
        prepare_skill_resource_execution_with_store(&config, &empty_store, &configured_request)
            .unwrap();
    assert_eq!(
        fs::read_to_string(&configured_prepared.target_path).unwrap(),
        PYTHON_PROBE
    );
    assert_eq!(configured_prepared.script, PYTHON_PROBE);
    assert_eq!(
        fs::read_to_string(configured_prepared.target_path.with_file_name("helper.py")).unwrap(),
        CONFIGURED_PYTHON_HELPER
    );
    assert!(configured_prepared._managed_snapshot.is_none());

    let (_managed_root, store, managed) = managed_fixture();
    let managed_request = execution_request(&managed, "scripts/probe.py", PYTHON_PROBE);
    let managed_prepared = prepare_skill_resource_execution_with_store(
        &SkillsConfig::default(),
        &store,
        &managed_request,
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(&managed_prepared.target_path).unwrap(),
        PYTHON_PROBE
    );
    assert_eq!(managed_prepared.script, PYTHON_PROBE);
    assert_eq!(
        fs::read_to_string(managed_prepared.target_path.with_file_name("helper.py")).unwrap(),
        MANAGED_PYTHON_HELPER
    );
    assert!(managed_prepared._managed_snapshot.is_some());
}

#[test]
fn interpreter_argv_binds_python_file_and_shell_zero_to_runner_owned_target() {
    let (_root, _config, configured) = configured_fixture();
    let request = execution_request(&configured, "scripts/probe.py", PYTHON_PROBE);
    let target = "/trusted/skill/scripts/probe.py";
    let candidates = skill_execution_candidates(&request, target).unwrap();
    assert_eq!(candidates[0].0, "python3");
    assert_eq!(
        candidates[0].1[0..4],
        ["-B", "-c", PYTHON_SKILL_WRAPPER, target]
    );
    assert!(PYTHON_SKILL_WRAPPER.contains("sys.path[0] = os.path.dirname(p)"));
    assert!(PYTHON_SKILL_WRAPPER.contains("\"__file__\": p"));
    let mut uppercase_request = request.clone();
    uppercase_request.path = "scripts/probe.PY".to_string();
    let uppercase_candidates = skill_execution_candidates(&uppercase_request, target).unwrap();
    assert_eq!(uppercase_candidates[0].0, "python3");

    let shell_script = "printf '%s\\n' \"$0\"\n";
    let shell_request = execution_request(&configured, "scripts/probe.sh", shell_script);
    let shell =
        skill_execution_candidates(&shell_request, "/trusted/skill/scripts/probe.sh").unwrap();
    assert_eq!(shell.len(), 1);
    assert_eq!(shell[0].0, "sh");
    assert_eq!(
        shell[0].1[0..3],
        ["-c", SHELL_SKILL_WRAPPER, "/trusted/skill/scripts/probe.sh"]
    );
}

#[cfg(feature = "runner-real-process-tests")]
#[test]
fn configured_python_skill_supports_sibling_import_file_identity_and_project_cwd() {
    let (_root, config, configured) = configured_fixture();
    let empty_store_root = tempfile::tempdir().unwrap();
    let store = SkillStore::for_test(empty_store_root.path().join("store"), "runner-empty");
    let request = execution_request(&configured, "scripts/probe.py", PYTHON_PROBE);
    assert_python_package_context(&config, &store, &request, "configured-helper");
}

#[cfg(feature = "runner-real-process-tests")]
#[test]
fn managed_python_skill_supports_sibling_import_file_identity_and_project_cwd() {
    let (_root, store, managed) = managed_fixture();
    let request = execution_request(&managed, "scripts/probe.py", PYTHON_PROBE);
    assert_python_package_context(&SkillsConfig::default(), &store, &request, "managed-helper");
}
