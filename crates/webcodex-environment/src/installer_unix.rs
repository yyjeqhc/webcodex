//! One-process, inherited Unix privilege channel for a prepared package
//! upgrade. The root parent accepts only the frozen operation's service and
//! program-restore requests; the user child runs the ordinary Core journal.
use crate::installer_authorization::{
    clear_authorization_under_lock, load_authorized_upgrade, system_directory,
};
use crate::service::{Ownership, ServiceAccount, ServiceManager, ServiceSpec, ServiceStatus};
use crate::upgrade::{
    check_frozen_installer_transition, installed_cli_matches, FrozenInstallerUpgrade,
};
use crate::{
    EnvironmentStore, NativeEnvironment, ServiceOperation, SetupDiagnostic, SetupResultValue,
};
use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const CHILD_FD: RawFd = 3;
const MAX_FRAME: usize = 65536;
const CHILD_DEADLINE: std::time::Duration = std::time::Duration::from_secs(900);
const BROKER_SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(600);
static CHILD_BROKER: OnceLock<Mutex<UnixStream>> = OnceLock::new();

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Service {
        spec: ServiceSpec,
        operation: ServiceOperation,
    },
    Restore {
        operation_id: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Response {
    Service {
        result: Result<ServiceStatus, SetupDiagnostic>,
    },
    Restore {
        result: Result<(), SetupDiagnostic>,
    },
}

fn denied(message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new("installer_broker_denied", message, "Retain the prepared upgrade and its maintenance gate; inspect the original services before retrying")
}

fn write_frame(stream: &mut UnixStream, bytes: &[u8]) -> SetupResultValue<()> {
    if bytes.len() > MAX_FRAME {
        return Err(denied("The installer operation exceeds its bound"));
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(bytes))
        .map_err(|_| denied("The installer privilege channel closed"))
}

fn read_frame(stream: &mut UnixStream) -> SetupResultValue<Option<Vec<u8>>> {
    let mut length = [0u8; 4];
    let n = stream
        .read(&mut length[..1])
        .map_err(|_| denied("The installer privilege channel failed"))?;
    if n == 0 {
        return Ok(None);
    }
    stream
        .read_exact(&mut length[1..])
        .map_err(|_| denied("The installer privilege channel was truncated"))?;
    let size = u32::from_be_bytes(length) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err(denied("The installer operation has an invalid size"));
    }
    let mut bytes = vec![0u8; size];
    stream
        .read_exact(&mut bytes)
        .map_err(|_| denied("The installer privilege channel was truncated"))?;
    Ok(Some(bytes))
}

fn round_trip(request: Request) -> SetupResultValue<Response> {
    let stream = CHILD_BROKER
        .get()
        .ok_or_else(|| denied("The authorized installer channel is absent"))?;
    let mut stream = stream
        .lock()
        .map_err(|_| denied("The installer channel is unavailable"))?;
    write_frame(
        &mut stream,
        &serde_json::to_vec(&request).map_err(|_| denied("The installer request is invalid"))?,
    )?;
    let bytes = read_frame(&mut stream)?
        .ok_or_else(|| denied("The installer parent closed before confirming the operation"))?;
    serde_json::from_slice(&bytes).map_err(|_| denied("The installer response is invalid"))
}

pub(crate) fn child_channel_active() -> bool {
    CHILD_BROKER.get().is_some()
}

pub(crate) fn child_service(
    spec: ServiceSpec,
    operation: ServiceOperation,
) -> SetupResultValue<ServiceStatus> {
    match round_trip(Request::Service { spec, operation })? {
        Response::Service { result } => result,
        _ => Err(denied("The installer parent returned the wrong operation")),
    }
}

pub(crate) fn child_restore(operation_id: &str) -> SetupResultValue<()> {
    match round_trip(Request::Restore {
        operation_id: operation_id.into(),
    })? {
        Response::Restore { result } => result,
        _ => Err(denied("The installer parent returned the wrong operation")),
    }
}

fn authorized_response(frozen: &FrozenInstallerUpgrade, request: Request) -> Response {
    match request {
        Request::Service { spec, operation } => {
            let result = (|| {
                if !matches!(operation, ServiceOperation::Start | ServiceOperation::Stop)
                    || !frozen.services.contains(&spec)
                {
                    return Err(denied("The service action is outside the frozen upgrade"));
                }
                let phase = check_frozen_installer_transition(frozen)?;
                if !matches!(phase, "verifying" | "restoring") {
                    return Err(denied(
                        "The service action is outside the frozen upgrade phase",
                    ));
                }
                let ServiceAccount::SystemUser {
                    expected_identity, ..
                } = &spec.account
                else {
                    return Err(denied("The service account is not the original Unix user"));
                };
                if expected_identity != &frozen.owner.identity {
                    return Err(denied("The service account changed"));
                }
                let current =
                    ServiceManager::inspect(&spec).map_err(crate::native::service_error)?;
                if current.ownership != Ownership::Owned || current.running.is_none() {
                    return Err(denied(
                        "The original service owner or process state is uncertain",
                    ));
                }
                match operation {
                    ServiceOperation::Start if current.running == Some(true) => Ok(current),
                    ServiceOperation::Stop if current.running == Some(false) => Ok(current),
                    ServiceOperation::Start => {
                        ServiceManager::start(&spec).map_err(crate::native::service_error)
                    }
                    ServiceOperation::Stop => {
                        ServiceManager::stop(&spec).map_err(crate::native::service_error)
                    }
                    _ => unreachable!(),
                }
            })();
            Response::Service { result }
        }
        Request::Restore { operation_id } => {
            let result = (|| {
                if operation_id != frozen.operation_id
                    || check_frozen_installer_transition(frozen)? != "restoring"
                {
                    return Err(denied("The program restore is outside the frozen rollback"));
                }
                crate::upgrade::restore_programs_from_journal(
                    &frozen.root,
                    &frozen.operation_id,
                    &frozen.owner.identity,
                )
            })();
            Response::Restore { result }
        }
    }
}

fn serve(mut stream: UnixStream, frozen: FrozenInstallerUpgrade) -> SetupResultValue<()> {
    stream
        .set_read_timeout(Some(CHILD_DEADLINE))
        .map_err(|_| denied("The installer channel could not set its deadline"))?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(30)))
        .map_err(|_| denied("The installer channel could not set its deadline"))?;
    let mut operations = 0usize;
    while let Some(bytes) = read_frame(&mut stream)? {
        operations += 1;
        if operations > 32 {
            return Err(denied(
                "The installer requested too many privileged operations",
            ));
        }
        let request: Request = serde_json::from_slice(&bytes)
            .map_err(|_| denied("The installer request is invalid"))?;
        let response = authorized_response(&frozen, request);
        write_frame(
            &mut stream,
            &serde_json::to_vec(&response)
                .map_err(|_| denied("The installer response is invalid"))?,
        )?;
    }
    Ok(())
}

struct UnixOwner {
    uid: u32,
    gid: u32,
    groups: Vec<u32>,
    name: String,
    home: PathBuf,
}

fn lookup_owner(identity: &str) -> SetupResultValue<UnixOwner> {
    let uid: u32 = identity
        .parse()
        .map_err(|_| denied("The prepared owner UID is invalid"))?;
    if uid == 0 {
        return Err(denied("The installer child must not run as root"));
    }
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0i8; 16384];
    let status = unsafe {
        libc::getpwuid_r(
            uid,
            &mut pwd,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        )
    };
    if status != 0 || result.is_null() || pwd.pw_name.is_null() || pwd.pw_dir.is_null() {
        return Err(denied("The prepared user account no longer exists"));
    }
    let name = unsafe { CStr::from_ptr(pwd.pw_name) }
        .to_str()
        .map_err(|_| denied("The user name is invalid"))?
        .to_owned();
    let home = PathBuf::from(
        unsafe { CStr::from_ptr(pwd.pw_dir) }
            .to_str()
            .map_err(|_| denied("The user home is invalid"))?,
    );
    let c_name = CString::new(name.as_bytes()).map_err(|_| denied("The user name is invalid"))?;
    #[cfg(target_os = "linux")]
    let groups = {
        let mut groups = vec![0 as libc::gid_t; 128];
        let mut count = groups.len() as i32;
        if unsafe {
            libc::getgrouplist(c_name.as_ptr(), pwd.pw_gid, groups.as_mut_ptr(), &mut count)
        } < 0
            || count < 1
            || count as usize > groups.len()
        {
            return Err(denied(
                "The original user's groups could not be resolved within the bound",
            ));
        }
        groups.truncate(count as usize);
        groups
    };
    #[cfg(target_os = "macos")]
    let groups = {
        let mut groups = vec![0 as libc::c_int; 128];
        let mut count = groups.len() as libc::c_int;
        if unsafe {
            libc::getgrouplist(
                c_name.as_ptr(),
                pwd.pw_gid as libc::c_int,
                groups.as_mut_ptr(),
                &mut count,
            )
        } < 0
            || count < 1
            || count as usize > groups.len()
        {
            return Err(denied(
                "The original user's groups could not be resolved within the bound",
            ));
        }
        groups.truncate(count as usize);
        groups
            .into_iter()
            .map(|group| group as libc::gid_t)
            .collect()
    };
    Ok(UnixOwner {
        uid,
        gid: pwd.pw_gid,
        groups,
        name,
        home,
    })
}

#[cfg(target_os = "linux")]
fn verify_root_parent(stream: &UnixStream) -> SetupResultValue<()> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of_val(&credentials) as libc::socklen_t;
    let status = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    if status != 0
        || len as usize != std::mem::size_of_val(&credentials)
        || credentials.uid != 0
        || credentials.pid != unsafe { libc::getppid() }
    {
        return Err(denied(
            "The inherited installer channel is not owned by the root parent",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn verify_root_parent(stream: &UnixStream) -> SetupResultValue<()> {
    let mut uid = 0;
    let mut gid = 0;
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 || uid != 0 {
        return Err(denied(
            "The inherited installer channel is not owned by root",
        ));
    }
    // LOCAL_PEERPID is Darwin's process-bound counterpart to getpeereid.
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of_val(&pid) as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&mut pid as *mut libc::pid_t).cast(),
            &mut len,
        )
    } != 0
        || len as usize != std::mem::size_of_val(&pid)
        || pid != unsafe { libc::getppid() }
    {
        return Err(denied(
            "The inherited installer channel does not belong to its root parent",
        ));
    }
    Ok(())
}

/// CLI internal entrypoint. The only accepted inherited descriptor is fd 3.
/// This process has already been changed to the original real user by parent.
pub async fn run_installer_upgrade_child(
    fd: i32,
    action: &str,
    environment_dir: &Path,
) -> SetupResultValue<()> {
    if fd != CHILD_FD
        || !matches!(action, "finish" | "rollback")
        || !environment_dir.is_absolute()
        || unsafe { libc::geteuid() } == 0
        || unsafe { libc::geteuid() } != unsafe { libc::getuid() }
    {
        return Err(denied("The installer child invocation is invalid"));
    }
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    verify_root_parent(&stream)?;
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(denied(
            "The installer channel could not be confined to its Core child",
        ));
    }
    CHILD_BROKER
        .set(Mutex::new(stream))
        .map_err(|_| denied("The installer channel was already initialized"))?;
    let store = EnvironmentStore::open(environment_dir.to_path_buf())?;
    let backend = NativeEnvironment::new()?;
    if action == "finish" {
        let mut backend = backend;
        backend.upgrade_finish(&store).await
    } else {
        backend.upgrade_rollback(&store).await
    }
}

async fn spawn_owner_child(
    cli: &Path,
    owner: &UnixOwner,
    frozen: &FrozenInstallerUpgrade,
    action: &str,
) -> SetupResultValue<()> {
    use std::os::unix::process::CommandExt;
    let (parent, child) =
        UnixStream::pair().map_err(|_| denied("The installer channel could not be created"))?;
    let fd = child.as_raw_fd();
    let uid = owner.uid;
    let gid = owner.gid;
    let groups = owner.groups.clone();
    let mut command = tokio::process::Command::new(cli);
    command
        .args(["environment", "__installer-child", "3", action])
        .arg(&frozen.root)
        .env_clear()
        .env("HOME", &owner.home)
        .env("USER", &owner.name)
        .env("LOGNAME", &owner.name)
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    unsafe {
        command.as_std_mut().pre_exec(move || {
            #[cfg(target_os = "linux")]
            let groups_ok = libc::setgroups(groups.len(), groups.as_ptr()) == 0;
            #[cfg(target_os = "macos")]
            let groups_ok = libc::setgroups(groups.len() as libc::c_int, groups.as_ptr()) == 0;
            if libc::dup2(fd, CHILD_FD) < 0
                || libc::fcntl(CHILD_FD, libc::F_SETFD, 0) < 0
                || !groups_ok
                || libc::setgid(gid) != 0
                || libc::setuid(uid) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut process = command
        .spawn()
        .map_err(|_| denied("The original-user Core child could not start"))?;
    drop(child);
    let mut broker = tokio::task::spawn_blocking({
        let frozen = frozen.clone();
        move || serve(parent, frozen)
    });
    let status = match tokio::time::timeout(CHILD_DEADLINE, process.wait()).await {
        Ok(Ok(status)) => status,
        other => {
            let _ = process.kill().await;
            let _ = process.wait().await;
            // A single service action can comprise several individually
            // bounded 55-second systemctl/launchctl calls. Keep the root
            // authorization lock until that in-flight action settles.
            let _ = tokio::time::timeout(BROKER_SETTLE_DEADLINE, &mut broker).await;
            return Err(if other.is_err() {
                denied("The original-user Core child exceeded its deadline")
            } else {
                denied("The original-user Core child outcome is unknown")
            });
        }
    };
    let channel = tokio::time::timeout(BROKER_SETTLE_DEADLINE, &mut broker)
        .await
        .map_err(|_| denied("The installer broker did not settle after the child exited"))?
        .map_err(|_| denied("The installer broker stopped unexpectedly"))?;
    channel?;
    if status.success() {
        Ok(())
    } else {
        Err(denied(
            "The original-user Core child could not complete the upgrade",
        ))
    }
}

/// Root package postinstall entrypoint. Success means the user-owned Core
/// journal is durably Committed and its maintenance gate is released.
pub async fn finish_authorized_installation() -> SetupResultValue<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(denied("System installer finalization requires root"));
    }
    let installer_store = EnvironmentStore::open(system_directory()?)?;
    let _installer_lock = installer_store.lock()?;
    let (receipt, frozen) = load_authorized_upgrade(&installer_store)?;
    check_frozen_installer_transition(&frozen)?;
    let owner = lookup_owner(&receipt.owner_identity)?;
    if owner.name != frozen.owner.name || owner.home != frozen.owner.home {
        return Err(denied("The original account changed after preparation"));
    }
    let phase = check_frozen_installer_transition(&frozen)?;
    if phase == "invalid" {
        return Err(denied(
            "The prepared upgrade returned to an unauthorized phase",
        ));
    }
    if phase == "rolled_back" {
        return Err(denied(
            "The prepared installation was rolled back and cannot be committed",
        ));
    }
    if phase == "restoring" {
        if !installed_cli_matches(&frozen)
            || spawn_owner_child(&frozen.installed_cli, &owner, &frozen, "rollback")
                .await
                .is_err()
        {
            let _ = spawn_owner_child(&frozen.backup_cli, &owner, &frozen, "rollback").await;
        }
        return Err(denied(
            "The prepared installation is restoring its previous version",
        ));
    }
    if installed_cli_matches(&frozen) {
        if let Err(finish_error) =
            spawn_owner_child(&frozen.installed_cli, &owner, &frozen, "finish").await
        {
            if check_frozen_installer_transition(&frozen)? != "committed" {
                if spawn_owner_child(&frozen.installed_cli, &owner, &frozen, "rollback")
                    .await
                    .is_err()
                    && check_frozen_installer_transition(&frozen)? != "rolled_back"
                {
                    let _ =
                        spawn_owner_child(&frozen.backup_cli, &owner, &frozen, "rollback").await;
                }
            }
            return Err(finish_error);
        }
    } else {
        let _ = spawn_owner_child(&frozen.backup_cli, &owner, &frozen, "rollback").await;
        return Err(denied(
            "The installed CLI does not match the prepared published candidate",
        ));
    }
    if check_frozen_installer_transition(&frozen)? != "committed" {
        return Err(denied(
            "The original-user Core journal did not commit the upgrade",
        ));
    }
    clear_authorization_under_lock(&installer_store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn broker_rejects_nonfrozen_service_without_running_it() {
        let temp = tempfile::tempdir().unwrap();
        let frozen = FrozenInstallerUpgrade {
            root: temp.path().into(),
            operation_id: "op".into(),
            owner: crate::LocalAccount {
                name: "owner".into(),
                identity: "1000".into(),
                home: temp.path().into(),
            },
            environment_id: "env".into(),
            services: vec![],
            all_services: vec![],
            programs: vec![],
            desktop: None,
            manifest_sha256: "a".repeat(64),
            record_fingerprint: "b".repeat(64),
            candidate_fingerprint: "c".repeat(64),
            installed_cli: temp.path().join("webcodex"),
            candidate_cli_sha256: "b".repeat(64),
            backup_cli: temp.path().join("backup"),
        };
        let spec: ServiceSpec = serde_json::from_value(serde_json::json!({"id":"foreign","component":"Server","program":"/bin/false","args":[],"working_directory":"/tmp","account":{"SystemUser":{"name":"owner","group":null,"expected_identity":"1000","home":"/tmp"}},"config_identity":"foreign","env_file":null,"environment":{},"linux_socket":null})).unwrap();
        let response = authorized_response(
            &frozen,
            Request::Service {
                spec,
                operation: ServiceOperation::Start,
            },
        );
        assert!(matches!(response, Response::Service { result: Err(_) }));
        let response = authorized_response(
            &frozen,
            Request::Restore {
                operation_id: "other".into(),
            },
        );
        assert!(matches!(response, Response::Restore { result: Err(_) }));
    }

    #[test]
    fn ipc_frames_reject_oversized_payload_without_allocation() {
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        writer
            .write_all(&((MAX_FRAME + 1) as u32).to_be_bytes())
            .unwrap();
        assert_eq!(
            read_frame(&mut reader).unwrap_err().code,
            "installer_broker_denied"
        );
    }

    #[test]
    fn bounded_ipc_round_trip_and_fake_program_hash() {
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        let request = Request::Restore {
            operation_id: "fixture".into(),
        };
        write_frame(&mut writer, &serde_json::to_vec(&request).unwrap()).unwrap();
        let bytes = read_frame(&mut reader).unwrap().unwrap();
        assert!(
            matches!(serde_json::from_slice::<Request>(&bytes).unwrap(), Request::Restore { operation_id } if operation_id == "fixture")
        );

        let temp = tempfile::tempdir().unwrap();
        let cli = temp.path().join("webcodex");
        std::fs::write(&cli, b"fake executable fixture").unwrap();
        let frozen = FrozenInstallerUpgrade {
            root: temp.path().into(),
            operation_id: "op".into(),
            owner: crate::LocalAccount {
                name: "owner".into(),
                identity: "1000".into(),
                home: temp.path().into(),
            },
            environment_id: "env".into(),
            services: vec![],
            all_services: vec![],
            programs: vec![],
            desktop: None,
            manifest_sha256: "a".repeat(64),
            record_fingerprint: "b".repeat(64),
            candidate_fingerprint: "c".repeat(64),
            installed_cli: cli.clone(),
            candidate_cli_sha256: format!("{:x}", sha2::Sha256::digest(b"fake executable fixture")),
            backup_cli: temp.path().join("backup"),
        };
        assert!(installed_cli_matches(&frozen));
        std::fs::write(cli, b"changed").unwrap();
        assert!(!installed_cli_matches(&frozen));
    }

    #[test]
    fn broker_loop_replies_to_disallowed_request_and_closes_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let frozen = FrozenInstallerUpgrade {
            root: temp.path().into(),
            operation_id: "op".into(),
            owner: crate::LocalAccount {
                name: "owner".into(),
                identity: "1000".into(),
                home: temp.path().into(),
            },
            environment_id: "env".into(),
            services: vec![],
            all_services: vec![],
            programs: vec![],
            desktop: None,
            manifest_sha256: "a".repeat(64),
            record_fingerprint: "b".repeat(64),
            candidate_fingerprint: "c".repeat(64),
            installed_cli: temp.path().join("webcodex"),
            candidate_cli_sha256: "d".repeat(64),
            backup_cli: temp.path().join("backup"),
        };
        let (mut client, server) = UnixStream::pair().unwrap();
        let broker = std::thread::spawn(move || serve(server, frozen));
        let request = Request::Restore {
            operation_id: "foreign".into(),
        };
        write_frame(&mut client, &serde_json::to_vec(&request).unwrap()).unwrap();
        let response: Response =
            serde_json::from_slice(&read_frame(&mut client).unwrap().unwrap()).unwrap();
        assert!(matches!(response, Response::Restore { result: Err(_) }));
        drop(client);
        broker.join().unwrap().unwrap();
    }

    #[tokio::test]
    async fn nonroot_cannot_enter_system_installer_finalization() {
        if unsafe { libc::geteuid() } != 0 {
            assert_eq!(
                finish_authorized_installation().await.unwrap_err().code,
                "installer_broker_denied"
            );
        }
    }
}
