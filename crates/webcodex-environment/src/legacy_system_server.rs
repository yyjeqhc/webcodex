//! Frozen handoff of the old, root-owned default Linux CLI Server unit pair.
//! The privileged side accepts no unit, environment, data, or backup paths
//! from its caller. All mutable decisions are checked against a root-owned
//! receipt before systemd or the filesystem is changed.
use crate::migration::{
    migrate_legacy_environment, LegacyImport, LegacyOwner, LegacyOwnerSnapshot, LegacyProcess,
    MigrationJournal, MigrationPhase,
};
use crate::process::CommandOutputExt;
use crate::service::Component;
use crate::{
    EnvironmentMode, EnvironmentStore, LocalAccount, NativeEnvironment, SetupDiagnostic,
    SetupProgress, SetupRequest, SetupResult, SetupResultValue, SetupSecrets,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE: &str = "/etc/systemd/system/webcodex.service";
const SOCKET: &str = "/etc/systemd/system/webcodex.socket";
const ENV: &str = "/etc/webcodex/webcodex.env";
const DATA: &str = "/var/lib/webcodex";
const BACKUP: &str = "/etc/systemd/system/.webcodex-legacy-handoff";
const MAX_ENV: u64 = 128 * 1024;
const MAX_DATA_ENTRIES: usize = 100_000;
const MAX_DATA_BYTES: u64 = 64 * 1024 * 1024 * 1024;

fn conflict(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, message, "Inspect the frozen old system unit pair and the root-owned handoff receipt; do not start another Server")
}

#[derive(Clone, Debug)]
pub struct LegacyCliServerInput {
    pub username: String,
    pub user_token_file: PathBuf,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LegacyServerAction {
    Probe,
    Stop,
    VerifyData,
    Restore,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyServerRequest {
    pub kind: String,
    pub action: LegacyServerAction,
    pub requester: LocalAccount,
    pub listen: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LegacyServerObservation {
    pub owner: LegacyOwnerSnapshot,
    pub staged_env: PathBuf,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum RootPhase {
    Prepared,
    Stopped,
    DataReady,
    Released,
    Restoring,
    Restored,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootReceipt {
    schema: u16,
    requester_uid: u32,
    requester_home: PathBuf,
    store_root: PathBuf,
    listen: String,
    unit_sha256: String,
    socket_sha256: String,
    env_sha256: String,
    program: PathBuf,
    pid: u32,
    generation: u64,
    phase: RootPhase,
    data_sha256: Option<String>,
}

impl RootReceipt {
    fn fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        for value in [
            &self.unit_sha256,
            &self.socket_sha256,
            &self.env_sha256,
            &self.requester_uid.to_string(),
            &self.listen,
        ] {
            hash.update(value.as_bytes());
            hash.update([0]);
        }
        format!("{:x}", hash.finalize())
    }
    fn snapshot(&self) -> LegacyOwnerSnapshot {
        let processes = if self.phase == RootPhase::Released {
            Vec::new()
        } else {
            vec![LegacyProcess {
                kind: "server".into(),
                pid: self.pid,
                generation: self.generation,
            }]
        };
        LegacyOwnerSnapshot {
            owner_id: format!("systemd-system:webcodex:{}", self.requester_uid),
            configuration_fingerprint: self.fingerprint(),
            processes,
        }
    }
}

struct SystemServerOwner<'a> {
    store: &'a EnvironmentStore,
    request: &'a SetupRequest,
}

impl LegacyOwner for SystemServerOwner<'_> {
    async fn inspect(&mut self) -> SetupResultValue<LegacyOwnerSnapshot> {
        Ok(crate::privilege::legacy_server_operation(
            self.store,
            self.request,
            LegacyServerAction::Probe,
        )
        .await?
        .owner)
    }
    async fn stop(&mut self, expected: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
        let observed = crate::privilege::legacy_server_operation(
            self.store,
            self.request,
            LegacyServerAction::Stop,
        )
        .await?;
        if observed.owner.owner_id != expected.owner_id
            || observed.owner.configuration_fingerprint != expected.configuration_fingerprint
            || !observed.owner.processes.is_empty()
        {
            return Err(conflict(
                "legacy_stop_unconfirmed",
                "The old Server unit pair was not released",
            ));
        }
        Ok(())
    }
    async fn restore(&mut self, expected: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
        let observed = crate::privilege::legacy_server_operation(
            self.store,
            self.request,
            LegacyServerAction::Restore,
        )
        .await?;
        if observed.owner.owner_id != expected.owner_id
            || observed.owner.configuration_fingerprint != expected.configuration_fingerprint
            || observed.owner.processes.is_empty()
        {
            return Err(conflict(
                "legacy_restore_unconfirmed",
                "The old Server unit pair was not restored",
            ));
        }
        Ok(())
    }
    async fn verify_imported_data(&mut self) -> SetupResultValue<()> {
        crate::privilege::legacy_server_operation(
            self.store,
            self.request,
            LegacyServerAction::VerifyData,
        )
        .await?;
        Ok(())
    }
    fn replaces_native_service(&self, component: Component) -> bool {
        component == Component::Server
    }
}

/// Only a root-owned default CLI Server service/socket can be migrated. The
/// caller explicitly supplies a user credential; it is never inferred from
/// the root service environment or from a Runner shared key.
pub async fn migrate_legacy_cli_system_server(
    store: &EnvironmentStore,
    request: SetupRequest,
    input: LegacyCliServerInput,
    secrets: &SetupSecrets,
    progress: impl FnMut(SetupProgress),
) -> SetupResultValue<SetupResult> {
    let EnvironmentMode::Create { listen } = &request.mode else {
        return Err(conflict(
            "legacy_server_mode",
            "A system Server migration must create the local environment",
        ));
    };
    if !request.local_server() || request.local_runner() || request.project.is_some() {
        return Err(conflict(
            "legacy_server_binding",
            "Migrate the old Server alone before adding a Runner",
        ));
    }
    if input.username.is_empty() || !input.user_token_file.is_absolute() {
        return Err(conflict(
            "legacy_user_identity",
            "Select the original Server username and absolute user credential file",
        ));
    }
    let proof_url = bound_proof_url(listen)?;
    if request.server_url != proof_url {
        return Err(conflict(
            "legacy_server_binding",
            "The Server URL must reach the old local socket at its original address and port",
        ));
    }
    let token = crate::read_secret(&input.user_token_file)?;
    if token.expose().starts_with("wc_agent_")
        || token.expose().starts_with("wc_boot_")
        || token.expose().starts_with("wc_acct_")
    {
        return Err(conflict(
            "legacy_user_credential",
            "A user API credential is required",
        ));
    }
    crate::storage::ensure_private_directory(&store.root().join("server"))?;
    // Verify the root-owned unit and socket before sending a user credential
    // to the local endpoint. An unrelated listener must not receive it.
    let staged =
        crate::privilege::legacy_server_operation(store, &request, LegacyServerAction::Probe)
            .await?;
    let overview = NativeEnvironment::new()?
        .post(
            &proof_url,
            "/api/runtime-console/overview",
            Some(token.expose()),
            json!({}),
        )
        .await?;
    if overview.get("service").and_then(Value::as_str) != Some("webcodex")
        || overview.get("authenticated_user").and_then(Value::as_str)
            != Some(input.username.as_str())
    {
        return Err(conflict(
            "legacy_user_identity",
            "The old Server does not verify the selected user credential and username",
        ));
    }
    if crate::read_secret(&input.user_token_file)?.expose() != token.expose() {
        return Err(conflict(
            "legacy_user_credential_changed",
            "The saved user credential changed during Server verification",
        ));
    }
    let import = LegacyImport {
        server_env_file: Some(staged.staged_env),
        runner_config_file: None,
        user_token_file: input.user_token_file,
        username: input.username,
        runner_client_id: None,
        projects: vec![],
        tunnel_profiles: vec![],
    };
    let mut owner = SystemServerOwner {
        store,
        request: &request,
    };
    migrate_legacy_environment(
        store,
        request.clone(),
        import,
        &mut owner,
        secrets,
        progress,
    )
    .await
}

fn bound_proof_url(listen: &str) -> SetupResultValue<String> {
    let bound: SocketAddr = listen.parse().map_err(|_| {
        conflict(
            "legacy_listen",
            "The original Server listen address is invalid",
        )
    })?;
    if bound.port() == 0 || bound.to_string() != listen {
        return Err(conflict(
            "legacy_listen",
            "The original Server listen address is not canonical",
        ));
    }
    let proof_ip = if bound.ip().is_unspecified() {
        match bound.ip() {
            std::net::IpAddr::V4(_) => std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            std::net::IpAddr::V6(_) => std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        }
    } else {
        bound.ip()
    };
    crate::native::canonical_server_url(&format!(
        "http://{}",
        SocketAddr::new(proof_ip, bound.port())
    ))
}

// Privileged implementation below is called only through the exact CLI
// exchange. No path supplied by the JSON request selects a root resource.
pub(crate) fn privileged_apply(
    store_root: &Path,
    request: &LegacyServerRequest,
) -> SetupResultValue<LegacyServerObservation> {
    if unsafe { libc::geteuid() } != 0 || request.kind != "legacy_cli_system_server" {
        return Err(conflict(
            "legacy_authorization",
            "The old system Server handoff requires a privileged fixed-resource request",
        ));
    }
    let uid = verified_requester(store_root, &request.requester)?;
    let listen: SocketAddr = request
        .listen
        .parse()
        .map_err(|_| conflict("legacy_listen", "The original listen address is invalid"))?;
    if listen.port() == 0 || listen.to_string() != request.listen {
        return Err(conflict(
            "legacy_listen",
            "The original listen address is not canonical",
        ));
    }
    let root_dir = backup_dir()?;
    let root_lock = File::open(&root_dir).map_err(|_| SetupDiagnostic::io())?;
    if unsafe { libc::flock(root_lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(conflict(
            "legacy_backup_lock",
            "The root-protected handoff could not be locked",
        ));
    }
    let mut receipt = match load_receipt()? {
        Some(receipt) => {
            if receipt.schema != 1
                || receipt.requester_uid != uid
                || receipt.store_root != store_root
                || receipt.requester_home != request.requester.home
                || receipt.listen != request.listen
            {
                return Err(conflict(
                    "legacy_handoff_conflict",
                    "A different root-protected Server handoff already exists",
                ));
            }
            receipt
        }
        None => {
            if request.action != LegacyServerAction::Probe {
                return Err(conflict(
                    "legacy_handoff_missing",
                    "The root-protected Server handoff has not been prepared",
                ));
            }
            let receipt =
                probe_original(uid, store_root, &request.requester.home, &request.listen)?;
            save_receipt(&receipt)?;
            receipt
        }
    };
    match request.action {
        LegacyServerAction::Probe => {}
        LegacyServerAction::Stop => {
            verify_stop_intent(store_root, uid, &receipt)?;
            stop_and_release(&mut receipt)?;
        }
        LegacyServerAction::VerifyData => {
            if receipt.phase != RootPhase::Released {
                return Err(conflict(
                    "legacy_data_phase",
                    "The old Server units are not yet released",
                ));
            }
            verify_copied_data(&receipt)?;
        }
        LegacyServerAction::Restore => restore_original(&mut receipt)?,
    }
    verify_frozen_resources(&receipt)?;
    let staged_env = stage_env_file(store_root, uid, &receipt.env_sha256)?;
    Ok(LegacyServerObservation {
        owner: receipt.snapshot(),
        staged_env,
    })
}

fn root_file(path: &Path, max_len: u64, private: bool) -> SetupResultValue<Vec<u8>> {
    let meta = fs::symlink_metadata(path).map_err(|_| {
        conflict(
            "legacy_resource_missing",
            "An original root-owned Server resource is unavailable",
        )
    })?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.uid() != 0
        || meta.nlink() != 1
        || meta.mode() & (if private { 0o077 } else { 0o022 }) != 0
        || meta.len() > max_len
    {
        return Err(conflict(
            "legacy_resource_owner",
            "An original Server resource has unsafe ownership, mode or size",
        ));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| {
            conflict(
                "legacy_resource_unknown",
                "An original Server resource could not be opened safely",
            )
        })?;
    let opened = file.metadata().map_err(|_| SetupDiagnostic::io())?;
    if opened.ino() != meta.ino() || opened.dev() != meta.dev() {
        return Err(conflict(
            "legacy_resource_changed",
            "An original Server resource changed while being opened",
        ));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(max_len + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SetupDiagnostic::io())?;
    if bytes.len() as u64 > max_len {
        return Err(conflict(
            "legacy_resource_size",
            "An original Server resource exceeds the migration limit",
        ));
    }
    Ok(bytes)
}

fn verify_stop_intent(store_root: &Path, uid: u32, receipt: &RootReceipt) -> SetupResultValue<()> {
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(store_root)
        .map_err(|_| {
            conflict(
                "legacy_migration_intent",
                "The private migration directory is unavailable",
            )
        })?;
    if directory
        .metadata()
        .map_err(|_| SetupDiagnostic::io())?
        .uid()
        != uid
    {
        return Err(conflict(
            "legacy_migration_intent",
            "The private migration directory changed owner",
        ));
    }
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c"migration.json".as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(conflict(
            "legacy_migration_intent",
            "Core has not recorded an old Server stop intent",
        ));
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    let meta = file.metadata().map_err(|_| SetupDiagnostic::io())?;
    if !meta.is_file()
        || meta.uid() != uid
        || meta.mode() & 0o077 != 0
        || meta.nlink() != 1
        || meta.len() > 64 * 1024
    {
        return Err(conflict(
            "legacy_migration_intent",
            "The saved migration intent has unsafe ownership or size",
        ));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SetupDiagnostic::io())?;
    let journal: MigrationJournal = serde_json::from_slice(&bytes).map_err(|_| {
        conflict(
            "legacy_migration_intent",
            "The saved migration intent is malformed",
        )
    })?;
    if journal.phase != MigrationPhase::OldStopRequested
        || journal.request.account.identity != receipt.requester_uid.to_string()
        || !matches!(&journal.request.mode, EnvironmentMode::Create { listen } if listen == &receipt.listen)
        || journal.captured != receipt.snapshot()
        || journal.import.server_env_file.as_deref()
            != Some(store_root.join("legacy-cli-server.env").as_path())
    {
        return Err(conflict(
            "legacy_migration_intent",
            "The saved Core stop intent does not bind this root-protected old owner",
        ));
    }
    Ok(())
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn one_env(bytes: &[u8], key: &str) -> SetupResultValue<String> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        conflict(
            "legacy_server_env",
            "The original Server environment is not UTF-8",
        )
    })?;
    let mut found = None;
    for line in text.lines() {
        if let Some((name, value)) = line.split_once('=') {
            if name.trim() == key {
                if found.is_some() || value.trim().is_empty() {
                    return Err(conflict(
                        "legacy_server_env",
                        "An original Server environment key is duplicated or empty",
                    ));
                }
                found = Some(value.trim().to_owned());
            }
        }
    }
    found.ok_or_else(|| {
        conflict(
            "legacy_server_env",
            "An original Server environment key is missing",
        )
    })
}

fn simple_program(bytes: &[u8]) -> SetupResultValue<PathBuf> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| conflict("legacy_unit_format", "The old service unit is not UTF-8"))?;
    let line = text
        .lines()
        .find(|line| line.starts_with("ExecStart=\""))
        .ok_or_else(|| {
            conflict(
                "legacy_unit_format",
                "The old service unit has no expected executable",
            )
        })?;
    let value = line
        .strip_prefix("ExecStart=\"")
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| {
            conflict(
                "legacy_unit_format",
                "The old service executable is not a single quoted path",
            )
        })?;
    let program = PathBuf::from(value);
    if !program.is_absolute()
        || value.contains(['%', '\\', '"', '\n', '\r'])
        || program.file_name().and_then(|value| value.to_str()) != Some("webcodex-server")
    {
        return Err(conflict(
            "legacy_unit_program",
            "The old service executable is not a default WebCodex Server binary",
        ));
    }
    let meta = fs::symlink_metadata(&program).map_err(|_| {
        conflict(
            "legacy_unit_program",
            "The old Server executable is unavailable",
        )
    })?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.uid() != 0
        || meta.nlink() != 1
        || meta.mode() & 0o022 != 0
        || meta.mode() & 0o111 == 0
    {
        return Err(conflict(
            "legacy_unit_program",
            "The old Server executable is not a root-owned immutable program",
        ));
    }
    for ancestor in program.parent().into_iter().flat_map(Path::ancestors) {
        let meta = fs::symlink_metadata(ancestor).map_err(|_| SetupDiagnostic::io())?;
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != 0
            || meta.mode() & 0o022 != 0
        {
            return Err(conflict(
                "legacy_unit_program",
                "The old Server executable path is not root-controlled",
            ));
        }
    }
    Ok(program)
}

fn expected_unit(program: &Path) -> String {
    format!("[Unit]\nDescription=WebCodex Runtime\nRequires=webcodex.socket\nAfter=network-online.target webcodex.socket\nWants=network-online.target\n\n[Service]\nType=simple\nEnvironmentFile={ENV}\nExecStart=\"{}\"\nRestart=on-failure\nRestartSec=3\nTimeoutStopSec=330s\nWorkingDirectory={DATA}\n\n[Install]\nWantedBy=multi-user.target\n", program.display())
}

fn expected_socket(listen: &str) -> String {
    format!("[Unit]\nDescription=WebCodex HTTP Socket\n\n[Socket]\nListenStream={listen}\nService=webcodex.service\nFileDescriptorName=webcodex-http\n\n[Install]\nWantedBy=sockets.target\n")
}

fn systemctl(args: &[&str]) -> SetupResultValue<String> {
    let binary = ["/usr/bin/systemctl", "/bin/systemctl"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .ok_or_else(|| conflict("legacy_manager_missing", "systemctl is unavailable"))?;
    let deadline = if args.first() == Some(&"disable") && args.contains(&"--now") {
        std::time::Duration::from_secs(350)
    } else if args.first() == Some(&"enable") && args.contains(&"--now") {
        std::time::Duration::from_secs(120)
    } else {
        std::time::Duration::from_secs(55)
    };
    let output = Command::new(binary)
        .args(args)
        .output_with_limits(deadline, 128 * 1024)
        .map_err(|_| {
            conflict(
                "legacy_manager_unknown",
                "The system service manager did not produce a bounded result",
            )
        })?;
    if !output.status.success() {
        return Err(conflict(
            "legacy_manager_failed",
            "The system service manager rejected the exact old unit operation",
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| {
        conflict(
            "legacy_manager_unknown",
            "The system service manager returned invalid text",
        )
    })
}

fn show(unit: &str) -> SetupResultValue<std::collections::BTreeMap<String, String>> {
    let output = systemctl(&["show", unit, "--no-pager", "--property=LoadState,FragmentPath,ActiveState,UnitFileState,MainPID,DropInPaths,NeedDaemonReload"])?;
    let mut fields = std::collections::BTreeMap::new();
    for line in output.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| conflict("legacy_manager_unknown", "The old unit state is malformed"))?;
        if fields.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(conflict(
                "legacy_manager_unknown",
                "The old unit state repeats a field",
            ));
        }
    }
    Ok(fields)
}

fn field<'a>(
    fields: &'a std::collections::BTreeMap<String, String>,
    key: &str,
) -> SetupResultValue<&'a str> {
    fields.get(key).map(String::as_str).ok_or_else(|| {
        conflict(
            "legacy_manager_unknown",
            "The old unit state omits a required field",
        )
    })
}

fn process_generation(pid: u32, program: &Path) -> SetupResultValue<u64> {
    if pid == 0 {
        return Err(conflict(
            "legacy_process_unknown",
            "The old Server has no running PID",
        ));
    }
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let meta = fs::symlink_metadata(&proc)
        .map_err(|_| conflict("legacy_process_unknown", "The old Server PID disappeared"))?;
    if meta.uid() != 0 {
        return Err(conflict(
            "legacy_process_owner",
            "The old Server PID is not root-owned",
        ));
    }
    let exe = fs::read_link(proc.join("exe")).map_err(|_| {
        conflict(
            "legacy_process_unknown",
            "The old Server executable cannot be inspected",
        )
    })?;
    if exe != program.canonicalize().map_err(|_| SetupDiagnostic::io())? {
        return Err(conflict(
            "legacy_process_program",
            "The old Server PID is running another binary",
        ));
    }
    let stat = fs::read_to_string(proc.join("stat")).map_err(|_| {
        conflict(
            "legacy_process_unknown",
            "The old Server generation cannot be inspected",
        )
    })?;
    stat.rsplit_once(") ")
        .and_then(|(_, tail)| tail.split_whitespace().nth(19))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| {
            conflict(
                "legacy_process_unknown",
                "The old Server generation is invalid",
            )
        })
}

fn verified_requester(store_root: &Path, account: &LocalAccount) -> SetupResultValue<u32> {
    let uid = account
        .identity
        .parse::<u32>()
        .ok()
        .filter(|uid| *uid != 0)
        .ok_or_else(|| {
            conflict(
                "legacy_requester",
                "A real non-root destination account is required",
            )
        })?;
    let id = Command::new("/usr/bin/id")
        .args(["-u", &account.name])
        .bounded_output()
        .map_err(|_| {
            conflict(
                "legacy_requester",
                "The destination account cannot be resolved",
            )
        })?;
    if !id.status.success() || String::from_utf8_lossy(&id.stdout).trim() != account.identity {
        return Err(conflict(
            "legacy_requester",
            "The destination account name and UID differ",
        ));
    }
    let meta = fs::symlink_metadata(store_root).map_err(|_| {
        conflict(
            "legacy_destination",
            "The destination environment directory is unavailable",
        )
    })?;
    if !store_root.is_absolute()
        || !meta.is_dir()
        || meta.file_type().is_symlink()
        || meta.uid() != uid
        || meta.mode() & 0o077 != 0
    {
        return Err(conflict(
            "legacy_destination",
            "The destination environment directory is not private to the selected user",
        ));
    }
    Ok(uid)
}

fn probe_original(
    uid: u32,
    store_root: &Path,
    home: &Path,
    listen: &str,
) -> SetupResultValue<RootReceipt> {
    let unit = root_file(Path::new(SERVICE), 16 * 1024, false)?;
    let socket = root_file(Path::new(SOCKET), 16 * 1024, false)?;
    let env = root_file(Path::new(ENV), MAX_ENV, true)?;
    let program = simple_program(&unit)?;
    if unit != expected_unit(&program).as_bytes()
        || socket != expected_socket(listen).as_bytes()
        || one_env(&env, "WEBCODEX_ADDR")? != listen
        || one_env(&env, "WEBCODEX_DATA")? != DATA
        || one_env(&env, "WEBCODEX_TOKEN")?.is_empty()
    {
        return Err(conflict(
            "legacy_unit_format",
            "The old unit/socket/environment differ from the default CLI root Server template",
        ));
    }
    let data = fs::symlink_metadata(DATA).map_err(|_| {
        conflict(
            "legacy_data_missing",
            "The old root Server data directory is unavailable",
        )
    })?;
    if !data.is_dir()
        || data.file_type().is_symlink()
        || data.uid() != 0
        || data.mode() & 0o022 != 0
    {
        return Err(conflict(
            "legacy_data_owner",
            "The old root Server data directory has unsafe ownership",
        ));
    }
    scan_tree_limits(Path::new(DATA), 0)?;
    let service = show("webcodex.service")?;
    let socket_state = show("webcodex.socket")?;
    for (state, path) in [(&service, SERVICE), (&socket_state, SOCKET)] {
        if field(state, "LoadState")? != "loaded"
            || field(state, "FragmentPath")? != path
            || field(state, "ActiveState")? != "active"
            || field(state, "UnitFileState")? != "enabled"
            || !field(state, "DropInPaths")?.is_empty()
            || field(state, "NeedDaemonReload")? != "no"
        {
            return Err(conflict(
                "legacy_unit_state",
                "Only an active, enabled default unit pair without overrides can be migrated",
            ));
        }
    }
    let pid = field(&service, "MainPID")?
        .parse::<u32>()
        .map_err(|_| conflict("legacy_process_unknown", "The old Server PID is invalid"))?;
    let generation = process_generation(pid, &program)?;
    Ok(RootReceipt {
        schema: 1,
        requester_uid: uid,
        requester_home: home.to_path_buf(),
        store_root: store_root.to_path_buf(),
        listen: listen.into(),
        unit_sha256: sha(&unit),
        socket_sha256: sha(&socket),
        env_sha256: sha(&env),
        program,
        pid,
        generation,
        phase: RootPhase::Prepared,
        data_sha256: None,
    })
}

fn backup_dir() -> SetupResultValue<PathBuf> {
    let path = PathBuf::from(BACKUP);
    if !path.exists() {
        fs::create_dir(&path).map_err(|_| {
            conflict(
                "legacy_backup",
                "The root-protected handoff directory could not be created",
            )
        })?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|_| SetupDiagnostic::io())?;
    }
    let meta = fs::symlink_metadata(&path).map_err(|_| SetupDiagnostic::io())?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || meta.uid() != 0
        || meta.mode() & 0o077 != 0
    {
        return Err(conflict(
            "legacy_backup",
            "The root-protected handoff directory has unsafe ownership",
        ));
    }
    Ok(path)
}

fn load_receipt() -> SetupResultValue<Option<RootReceipt>> {
    let path = Path::new(BACKUP).join("receipt.json");
    if !path.exists() {
        if Path::new(BACKUP).exists()
            && fs::read_dir(BACKUP)
                .map_err(|_| SetupDiagnostic::io())?
                .next()
                .is_some()
        {
            return Err(conflict(
                "legacy_backup_conflict",
                "A handoff backup exists without its root-protected receipt",
            ));
        }
        return Ok(None);
    }
    let bytes = root_file(&path, 16 * 1024, true)?;
    serde_json::from_slice(&bytes).map(Some).map_err(|_| {
        conflict(
            "legacy_backup_conflict",
            "The root-protected handoff receipt is invalid",
        )
    })
}

fn save_receipt(receipt: &RootReceipt) -> SetupResultValue<()> {
    let dir = backup_dir()?;
    let path = dir.join("receipt.json");
    crate::storage::atomic_private_write(
        &path,
        &serde_json::to_vec(receipt).map_err(|_| SetupDiagnostic::io())?,
    )
}

fn stage_env_file(store_root: &Path, uid: u32, expected_hash: &str) -> SetupResultValue<PathBuf> {
    let source = root_file(Path::new(ENV), MAX_ENV, true)?;
    if sha(&source) != expected_hash {
        return Err(conflict(
            "legacy_server_env_changed",
            "The original Server environment changed during handoff",
        ));
    }
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(store_root)
        .map_err(|_| {
            conflict(
                "legacy_destination",
                "The private environment directory could not be opened safely",
            )
        })?;
    let directory_meta = dir.metadata().map_err(|_| SetupDiagnostic::io())?;
    if !directory_meta.is_dir() || directory_meta.uid() != uid || directory_meta.mode() & 0o077 != 0
    {
        return Err(conflict(
            "legacy_destination",
            "The private environment directory changed owner",
        ));
    }
    let path = store_root.join("legacy-cli-server.env");
    let name = c"legacy-cli-server.env";
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd >= 0 {
        let mut file = unsafe { File::from_raw_fd(fd) };
        if unsafe { libc::fchown(file.as_raw_fd(), uid, !0) } != 0 {
            return Err(SetupDiagnostic::io());
        }
        file.write_all(&source)
            .and_then(|_| file.sync_all())
            .map_err(|_| SetupDiagnostic::io())?;
    } else if std::io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST) {
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(conflict(
                "legacy_server_env_changed",
                "The staged Server environment is not a safe regular file",
            ));
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        let meta = file.metadata().map_err(|_| SetupDiagnostic::io())?;
        if !meta.is_file() || meta.uid() != uid || meta.mode() & 0o077 != 0 || meta.nlink() != 1 {
            return Err(conflict(
                "legacy_server_env_changed",
                "The staged Server environment changed owner or type",
            ));
        }
        let mut staged = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_ENV + 1)
            .read_to_end(&mut staged)
            .map_err(|_| SetupDiagnostic::io())?;
        if staged != source {
            return Err(conflict(
                "legacy_server_env_changed",
                "The staged Server environment no longer matches the root-protected source",
            ));
        }
    } else {
        return Err(conflict(
            "legacy_destination",
            "The private Server environment could not be staged safely",
        ));
    }
    Ok(path)
}

fn lchown(path: &Path, uid: u32) -> SetupResultValue<()> {
    use std::os::unix::ffi::OsStrExt;
    let name =
        std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| SetupDiagnostic::io())?;
    if unsafe { libc::lchown(name.as_ptr(), uid, !0) } != 0 {
        return Err(SetupDiagnostic::io());
    }
    Ok(())
}

fn verify_frozen_resources(receipt: &RootReceipt) -> SetupResultValue<()> {
    let env = root_file(Path::new(ENV), MAX_ENV, true)?;
    if sha(&env) != receipt.env_sha256
        || one_env(&env, "WEBCODEX_ADDR")? != receipt.listen
        || one_env(&env, "WEBCODEX_DATA")? != DATA
    {
        return Err(conflict(
            "legacy_server_env_changed",
            "The root-owned Server environment changed after capture",
        ));
    }
    for (original, backup, hash) in [
        (SERVICE, "webcodex.service", &receipt.unit_sha256),
        (SOCKET, "webcodex.socket", &receipt.socket_sha256),
    ] {
        let saved = Path::new(BACKUP).join(backup);
        match receipt.phase {
            RootPhase::Prepared | RootPhase::Restored => {
                if sha(&root_file(Path::new(original), 16 * 1024, false)?) != *hash
                    || saved.exists()
                {
                    return Err(conflict(
                        "legacy_unit_changed",
                        "The original Server unit changed after capture",
                    ));
                }
            }
            RootPhase::Stopped | RootPhase::DataReady | RootPhase::Restoring => {
                let path = if saved.exists() {
                    &saved
                } else {
                    Path::new(original)
                };
                if sha(&root_file(path, 16 * 1024, false)?) != *hash {
                    return Err(conflict(
                        "legacy_unit_changed",
                        "The old Server unit changed while its name was being released",
                    ));
                }
                if saved.exists() && Path::new(original).exists() {
                    return Err(conflict(
                        "legacy_unit_changed",
                        "Both original and backup Server units occupy the handoff",
                    ));
                }
            }
            RootPhase::Released => {
                if sha(&root_file(&saved, 16 * 1024, false)?) != *hash {
                    return Err(conflict(
                        "legacy_unit_changed",
                        "The root-protected Server unit backup changed",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn stopped_exact_pair() -> SetupResultValue<()> {
    for (unit, path) in [("webcodex.service", SERVICE), ("webcodex.socket", SOCKET)] {
        let state = show(unit)?;
        if field(&state, "ActiveState")? != "inactive"
            || (Path::new(path).exists() && field(&state, "UnitFileState")? != "disabled")
            || field(&state, "MainPID")? != "0"
        {
            return Err(conflict(
                "legacy_stop_unconfirmed",
                "The old Server unit pair did not confirm disabled and stopped",
            ));
        }
    }
    Ok(())
}

fn stop_and_release(receipt: &mut RootReceipt) -> SetupResultValue<()> {
    if matches!(receipt.phase, RootPhase::Restoring | RootPhase::Restored) {
        return Err(conflict(
            "legacy_handoff_restored",
            "A restoring old Server cannot be cut over again automatically",
        ));
    }
    verify_frozen_resources(receipt)?;
    if receipt.phase == RootPhase::Prepared {
        let service = show("webcodex.service")?;
        let socket = show("webcodex.socket")?;
        if field(&service, "FragmentPath")? != SERVICE
            || field(&socket, "FragmentPath")? != SOCKET
            || !field(&service, "DropInPaths")?.is_empty()
            || !field(&socket, "DropInPaths")?.is_empty()
        {
            return Err(conflict(
                "legacy_process_changed",
                "The old Server owner changed before shutdown",
            ));
        }
        let current_pid = field(&service, "MainPID")?
            .parse::<u32>()
            .map_err(|_| conflict("legacy_process_unknown", "The old Server PID is invalid"))?;
        if current_pid != 0
            && (current_pid != receipt.pid
                || process_generation(current_pid, &receipt.program)? != receipt.generation)
        {
            return Err(conflict(
                "legacy_process_changed",
                "A different old Server generation appeared before shutdown",
            ));
        }
        // A previous invocation may have reached systemctl but crashed before
        // saving Stopped. Repeating disable/stop is safe only after verifying
        // no replacement process generation appeared.
        systemctl(&["disable", "--now", "webcodex.service", "webcodex.socket"])?;
        stopped_exact_pair()?;
        receipt.phase = RootPhase::Stopped;
        save_receipt(receipt)?;
    }
    if receipt.phase == RootPhase::Stopped {
        stopped_exact_pair()?;
        copy_root_data(receipt)?;
        receipt.phase = RootPhase::DataReady;
        save_receipt(receipt)?;
    }
    if receipt.phase == RootPhase::DataReady {
        stopped_exact_pair()?;
        verify_copied_data(receipt)?;
        for (original, name, hash) in [
            (SERVICE, "webcodex.service", &receipt.unit_sha256),
            (SOCKET, "webcodex.socket", &receipt.socket_sha256),
        ] {
            let backup = Path::new(BACKUP).join(name);
            if backup.exists() {
                if Path::new(original).exists()
                    || sha(&root_file(&backup, 16 * 1024, false)?) != *hash
                {
                    return Err(conflict(
                        "legacy_unit_changed",
                        "The old Server unit backup is ambiguous",
                    ));
                }
            } else {
                if sha(&root_file(Path::new(original), 16 * 1024, false)?) != *hash {
                    return Err(conflict(
                        "legacy_unit_changed",
                        "The old Server unit changed before release",
                    ));
                }
                fs::rename(original, &backup).map_err(|_| {
                    conflict(
                        "legacy_release_unknown",
                        "The old unit move outcome is unknown",
                    )
                })?;
            }
        }
        systemctl(&["daemon-reload"])?;
        receipt.phase = RootPhase::Released;
        save_receipt(receipt)?;
    }
    Ok(())
}

fn restore_original(receipt: &mut RootReceipt) -> SetupResultValue<()> {
    verify_frozen_resources(receipt)?;
    if receipt.phase == RootPhase::Restored {
        return Ok(());
    }
    if receipt.phase != RootPhase::Restoring {
        receipt.phase = RootPhase::Restoring;
        save_receipt(receipt)?;
    }
    for (original, name, hash) in [
        (SERVICE, "webcodex.service", &receipt.unit_sha256),
        (SOCKET, "webcodex.socket", &receipt.socket_sha256),
    ] {
        let backup = Path::new(BACKUP).join(name);
        if Path::new(original).exists() {
            if backup.exists() || sha(&root_file(Path::new(original), 16 * 1024, false)?) != *hash {
                return Err(conflict(
                    "legacy_restore_conflict",
                    "Another Server unit occupies the original systemd name",
                ));
            }
        } else if backup.exists() {
            if sha(&root_file(&backup, 16 * 1024, false)?) != *hash {
                return Err(conflict(
                    "legacy_unit_changed",
                    "The old Server backup changed",
                ));
            }
            fs::rename(&backup, original).map_err(|_| {
                conflict(
                    "legacy_restore_unknown",
                    "The old Server unit restore outcome is unknown",
                )
            })?;
        } else {
            return Err(conflict(
                "legacy_restore_missing",
                "An original Server unit is missing",
            ));
        }
    }
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", "webcodex.socket", "webcodex.service"])?;
    let service = show("webcodex.service")?;
    let socket = show("webcodex.socket")?;
    for (state, path) in [(&service, SERVICE), (&socket, SOCKET)] {
        if field(state, "FragmentPath")? != path
            || field(state, "ActiveState")? != "active"
            || field(state, "UnitFileState")? != "enabled"
        {
            return Err(conflict(
                "legacy_restore_unconfirmed",
                "The original Server unit pair did not confirm restart",
            ));
        }
    }
    receipt.pid = field(&service, "MainPID")?.parse().map_err(|_| {
        conflict(
            "legacy_restore_unconfirmed",
            "The restored Server PID is unknown",
        )
    })?;
    receipt.generation = process_generation(receipt.pid, &receipt.program)?;
    receipt.phase = RootPhase::Restored;
    save_receipt(receipt)?;
    Ok(())
}

fn scan_tree_limits(root: &Path, owner: u32) -> SetupResultValue<()> {
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut entries = 0usize;
    let mut bytes = 0u64;
    while let Some((path, depth)) = pending.pop() {
        if depth > 32 {
            return Err(conflict(
                "legacy_data_depth",
                "The old Server data tree exceeds the migration depth limit",
            ));
        }
        let meta = fs::symlink_metadata(&path).map_err(|_| SetupDiagnostic::io())?;
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != owner
            || meta.mode() & 0o022 != 0
        {
            return Err(conflict(
                "legacy_data_owner",
                "The old Server data tree contains an unsafe directory",
            ));
        }
        for entry in fs::read_dir(&path).map_err(|_| SetupDiagnostic::io())? {
            let entry = entry.map_err(|_| SetupDiagnostic::io())?;
            entries += 1;
            if entries > MAX_DATA_ENTRIES {
                return Err(conflict(
                    "legacy_data_limit",
                    "The old Server data tree exceeds the entry limit",
                ));
            }
            let child = entry.path();
            let meta = fs::symlink_metadata(&child).map_err(|_| SetupDiagnostic::io())?;
            if meta.file_type().is_symlink() || meta.uid() != owner || meta.mode() & 0o022 != 0 {
                return Err(conflict(
                    "legacy_data_owner",
                    "The old Server data tree contains an unsafe entry",
                ));
            }
            if meta.is_dir() {
                pending.push((child, depth + 1));
            } else if meta.is_file() && meta.nlink() == 1 {
                bytes = bytes
                    .checked_add(meta.len())
                    .filter(|size| *size <= MAX_DATA_BYTES)
                    .ok_or_else(|| {
                        conflict(
                            "legacy_data_size",
                            "The old Server data exceeds the bounded automatic migration limit",
                        )
                    })?;
            } else {
                return Err(conflict(
                    "legacy_data_type",
                    "The old Server data tree contains an unsupported entry",
                ));
            }
        }
    }
    Ok(())
}

fn walk_tree(
    root: &Path,
    owner: u32,
    mut file_action: impl FnMut(&Path, &Path, u64) -> SetupResultValue<()>,
    mut dir_action: impl FnMut(&Path, &Path) -> SetupResultValue<()>,
) -> SetupResultValue<String> {
    let mut hash = Sha256::new();
    let mut pending = vec![(root.to_path_buf(), PathBuf::new(), 0usize)];
    let mut count = 0usize;
    let mut total_bytes = 0u64;
    while let Some((dir, relative, depth)) = pending.pop() {
        if depth > 32 {
            return Err(conflict(
                "legacy_data_depth",
                "The old Server data tree exceeds the migration depth limit",
            ));
        }
        let metadata = fs::symlink_metadata(&dir).map_err(|_| SetupDiagnostic::io())?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.uid() != owner
            || metadata.mode() & (if owner == 0 { 0o022 } else { 0o077 }) != 0
        {
            return Err(conflict(
                "legacy_data_owner",
                "The old Server data tree contains an unsafe directory",
            ));
        }
        dir_action(&dir, &relative)?;
        hash.update(b"D");
        hash.update(relative.as_os_str().as_encoded_bytes());
        let mut entries = Vec::new();
        for entry in fs::read_dir(&dir).map_err(|_| SetupDiagnostic::io())? {
            if entries.len() + count >= MAX_DATA_ENTRIES {
                return Err(conflict(
                    "legacy_data_limit",
                    "The old Server data tree exceeds the entry limit",
                ));
            }
            entries.push(entry.map_err(|_| SetupDiagnostic::io())?);
        }
        entries.sort_by_key(|entry| entry.file_name());
        let mut subdirs = Vec::new();
        for entry in entries {
            count += 1;
            if count > MAX_DATA_ENTRIES {
                return Err(conflict(
                    "legacy_data_limit",
                    "The old Server data tree exceeds the entry limit",
                ));
            }
            let name = entry.file_name();
            let relative_child = relative.join(&name);
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).map_err(|_| SetupDiagnostic::io())?;
            if meta.file_type().is_symlink()
                || meta.uid() != owner
                || meta.mode() & (if owner == 0 { 0o022 } else { 0o077 }) != 0
            {
                return Err(conflict(
                    "legacy_data_owner",
                    "The old Server data tree contains an unsafe entry",
                ));
            }
            if meta.is_dir() {
                subdirs.push((path, relative_child, depth + 1));
            } else if meta.is_file() && meta.nlink() == 1 {
                total_bytes = total_bytes
                    .checked_add(meta.len())
                    .filter(|size| *size <= MAX_DATA_BYTES)
                    .ok_or_else(|| {
                        conflict(
                            "legacy_data_size",
                            "The old Server data exceeds the bounded automatic migration limit",
                        )
                    })?;
                hash.update(b"F");
                hash.update(relative_child.as_os_str().as_encoded_bytes());
                hash.update(meta.len().to_le_bytes());
                let file = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(&path)
                    .map_err(|_| SetupDiagnostic::io())?;
                let opened = file.metadata().map_err(|_| SetupDiagnostic::io())?;
                if opened.ino() != meta.ino()
                    || opened.dev() != meta.dev()
                    || opened.len() != meta.len()
                {
                    return Err(conflict(
                        "legacy_data_changed",
                        "An old Server data file changed during copy",
                    ));
                }
                let mut buffer = [0u8; 64 * 1024];
                let mut limited = file.take(meta.len() + 1);
                let mut read_total = 0u64;
                loop {
                    let read = limited
                        .read(&mut buffer)
                        .map_err(|_| SetupDiagnostic::io())?;
                    if read == 0 {
                        break;
                    }
                    read_total += read as u64;
                    hash.update(&buffer[..read]);
                }
                if read_total != meta.len() {
                    return Err(conflict(
                        "legacy_data_changed",
                        "An old Server data file changed while being hashed",
                    ));
                }
                file_action(&path, &relative_child, meta.len())?;
            } else {
                return Err(conflict(
                    "legacy_data_type",
                    "The old Server data tree contains an unsupported entry",
                ));
            }
        }
        pending.extend(subdirs.into_iter().rev());
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn digest_tree(root: &Path, owner: u32) -> SetupResultValue<String> {
    walk_tree(root, owner, |_, _, _| Ok(()), |_, _| Ok(()))
}

fn copy_root_tree(source: &Path, target: &Path) -> SetupResultValue<String> {
    fs::create_dir(target).map_err(|_| {
        conflict(
            "legacy_data_stage",
            "The root-protected data stage cannot be created",
        )
    })?;
    fs::set_permissions(target, fs::Permissions::from_mode(0o700))
        .map_err(|_| SetupDiagnostic::io())?;
    let copied = walk_tree(
        source,
        0,
        |file, relative, expected_len| {
            let destination = target.join(relative);
            let from = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(file)
                .map_err(|_| SetupDiagnostic::io())?;
            if from.metadata().map_err(|_| SetupDiagnostic::io())?.len() != expected_len {
                return Err(conflict(
                    "legacy_data_changed",
                    "The old Server data file changed before copy",
                ));
            }
            let mut to = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(destination)
                .map_err(|_| {
                    conflict(
                        "legacy_data_stage",
                        "The Server data stage contains an unexpected file",
                    )
                })?;
            let copied = std::io::copy(&mut from.take(expected_len + 1), &mut to)
                .map_err(|_| SetupDiagnostic::io())?;
            if copied != expected_len {
                return Err(conflict(
                    "legacy_data_changed",
                    "The old Server data file changed during copy",
                ));
            }
            to.sync_all().map_err(|_| SetupDiagnostic::io())?;
            Ok(())
        },
        |_, relative| {
            if !relative.as_os_str().is_empty() {
                let path = target.join(relative);
                fs::create_dir(&path).map_err(|_| {
                    conflict(
                        "legacy_data_stage",
                        "The Server data stage contains an unexpected directory",
                    )
                })?;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                    .map_err(|_| SetupDiagnostic::io())?;
            }
            Ok(())
        },
    )?;
    if digest_tree(target, 0)? != copied {
        return Err(conflict(
            "legacy_data_changed",
            "The copied Server data differs from the original",
        ));
    }
    Ok(copied)
}

fn chown_tree(root: &Path, uid: u32) -> SetupResultValue<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let meta = fs::symlink_metadata(&path).map_err(|_| SetupDiagnostic::io())?;
        if meta.file_type().is_symlink() || meta.uid() != 0 {
            return Err(conflict(
                "legacy_data_stage",
                "The staged Server data changed ownership unexpectedly",
            ));
        }
        if meta.is_dir() {
            for entry in fs::read_dir(&path).map_err(|_| SetupDiagnostic::io())? {
                pending.push(entry.map_err(|_| SetupDiagnostic::io())?.path());
            }
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .map_err(|_| SetupDiagnostic::io())?;
        } else if meta.is_file() {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|_| SetupDiagnostic::io())?;
        } else {
            return Err(conflict(
                "legacy_data_stage",
                "The staged Server data contains an unsupported entry",
            ));
        }
        lchown(&path, uid)?;
    }
    Ok(())
}

fn user_server_dir(receipt: &RootReceipt) -> SetupResultValue<PathBuf> {
    let dir = receipt.store_root.join("server");
    let handle = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&dir)
        .map_err(|_| {
            conflict(
                "legacy_destination",
                "The destination Server directory is unavailable",
            )
        })?;
    let meta = handle.metadata().map_err(|_| SetupDiagnostic::io())?;
    if !meta.is_dir() || meta.uid() != receipt.requester_uid || meta.mode() & 0o077 != 0 {
        return Err(conflict(
            "legacy_destination",
            "The destination Server directory is not private to the selected user",
        ));
    }
    Ok(dir)
}

fn rename_data_noreplace(
    source_parent: &Path,
    destination_parent: &Path,
    uid: u32,
) -> SetupResultValue<()> {
    let src = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(source_parent)
        .map_err(|_| SetupDiagnostic::io())?;
    let dst = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(destination_parent)
        .map_err(|_| SetupDiagnostic::io())?;
    let source_meta = src.metadata().map_err(|_| SetupDiagnostic::io())?;
    let destination_meta = dst.metadata().map_err(|_| SetupDiagnostic::io())?;
    if source_meta.uid() != 0
        || destination_meta.uid() != uid
        || destination_meta.mode() & 0o077 != 0
    {
        return Err(conflict(
            "legacy_destination",
            "The data publish directories changed ownership",
        ));
    }
    let source_name = c"data-stage";
    let target_name = c"data";
    let result = unsafe {
        libc::renameat2(
            src.as_raw_fd(),
            source_name.as_ptr(),
            dst.as_raw_fd(),
            target_name.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result != 0 {
        return Err(conflict(
            "legacy_data_publish_unknown",
            "The private Server data publish outcome is unknown",
        ));
    }
    Ok(())
}

fn copy_root_data(receipt: &mut RootReceipt) -> SetupResultValue<()> {
    let server_dir = user_server_dir(receipt)?;
    let target = server_dir.join("data");
    let original_hash = digest_tree(Path::new(DATA), 0)?;
    if let Some(expected) = &receipt.data_sha256 {
        if *expected != original_hash {
            return Err(conflict(
                "legacy_data_changed",
                "The stopped Server data changed during handoff",
            ));
        }
    } else {
        receipt.data_sha256 = Some(original_hash.clone());
        save_receipt(receipt)?;
    }
    if target.exists() {
        if digest_tree(&target, receipt.requester_uid)? != original_hash {
            return Err(conflict(
                "legacy_data_conflict",
                "The destination Server data differs from the frozen old data",
            ));
        }
        return Ok(());
    }
    let stage = Path::new(BACKUP).join("data-stage");
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|_| {
            conflict(
                "legacy_data_stage",
                "The incomplete root-protected data stage cannot be reset",
            )
        })?;
    }
    if copy_root_tree(Path::new(DATA), &stage)? != original_hash {
        return Err(conflict(
            "legacy_data_changed",
            "The original Server data changed while being copied",
        ));
    }
    chown_tree(&stage, receipt.requester_uid)?;
    rename_data_noreplace(Path::new(BACKUP), &server_dir, receipt.requester_uid)?;
    verify_copied_data(receipt)
}

fn verify_copied_data(receipt: &RootReceipt) -> SetupResultValue<()> {
    let expected = receipt.data_sha256.as_deref().ok_or_else(|| {
        conflict(
            "legacy_data_missing",
            "The root-protected data digest is missing",
        )
    })?;
    let target = receipt.store_root.join("server/data");
    if digest_tree(&target, receipt.requester_uid)? != expected {
        return Err(conflict(
            "legacy_data_changed",
            "The staged Server data no longer matches the frozen old data",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_default_root_unit_pair_has_expected_bytes() {
        let unit = expected_unit(Path::new("/usr/local/bin/webcodex-server"));
        assert!(unit.contains("EnvironmentFile=/etc/webcodex/webcodex.env\n"));
        assert!(unit.contains("WorkingDirectory=/var/lib/webcodex\n"));
        assert!(!unit.contains("User="));
        let socket = expected_socket("127.0.0.1:8080");
        assert!(socket.contains("ListenStream=127.0.0.1:8080\n"));
        assert!(socket.contains("Service=webcodex.service\n"));
        assert_ne!(
            unit.as_bytes(),
            unit.replace("RestartSec=3", "RestartSec=1").as_bytes()
        );
    }
    #[test]
    fn auth_proof_uses_only_the_local_bound_socket() {
        assert_eq!(
            bound_proof_url("0.0.0.0:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            bound_proof_url("192.0.2.5:8080").unwrap(),
            "http://192.0.2.5:8080"
        );
        assert_eq!(bound_proof_url("[::]:8080").unwrap(), "http://[::1]:8080");
        assert!(bound_proof_url("0.0.0.0:0").is_err());
    }
    #[test]
    fn root_receipt_keeps_owner_binding_across_handoff_phase() {
        let mut receipt = RootReceipt {
            schema: 1,
            requester_uid: 1001,
            requester_home: "/home/alice".into(),
            store_root: "/home/alice/.local/share/webcodex/environment".into(),
            listen: "127.0.0.1:8080".into(),
            unit_sha256: "unit".into(),
            socket_sha256: "socket".into(),
            env_sha256: "env".into(),
            program: "/usr/bin/webcodex-server".into(),
            pid: 42,
            generation: 9,
            phase: RootPhase::Prepared,
            data_sha256: None,
        };
        let captured = receipt.snapshot();
        assert_eq!(captured.processes.len(), 1);
        receipt.phase = RootPhase::Released;
        let stopped = receipt.snapshot();
        assert_eq!(
            captured.configuration_fingerprint,
            stopped.configuration_fingerprint
        );
        assert!(stopped.processes.is_empty());
        receipt.requester_uid += 1;
        assert_ne!(
            captured.configuration_fingerprint,
            receipt.snapshot().configuration_fingerprint
        );
    }
    #[test]
    fn data_digest_rejects_symlink_without_following_it() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = temp.path().join("data");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let owner = unsafe { libc::geteuid() };
        let before = digest_tree(&path, owner).unwrap();
        fs::write(path.join("state"), b"identity").unwrap();
        fs::set_permissions(path.join("state"), fs::Permissions::from_mode(0o600)).unwrap();
        assert_ne!(before, digest_tree(&path, owner).unwrap());
        symlink("/etc/passwd", path.join("link")).unwrap();
        assert!(digest_tree(&path, owner).is_err());
    }
}
