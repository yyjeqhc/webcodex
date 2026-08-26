use salvo::conn::tcp::TcpAcceptor;
use salvo::conn::{Listener, TcpListener};
use std::net::{SocketAddr, ToSocketAddrs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListenerMode {
    Direct,
    SystemdActivated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivationMetadata {
    fd_name: Option<String>,
}

const SYSTEMD_LISTEN_FD_START: i32 = 3;
const HTTP_FD_NAME: &str = "webcodex-http";

#[cfg(target_os = "linux")]
static PREPARED_ACTIVATION_METADATA: std::sync::OnceLock<Option<ActivationMetadata>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "linux")]
const SYSTEMD_ACTIVATION_ENV_KEYS: [&str; 3] = ["LISTEN_PID", "LISTEN_FDS", "LISTEN_FDNAMES"];

fn activation_metadata_from_values(
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
    listen_fdnames: Option<&str>,
    current_pid: u32,
) -> Result<Option<ActivationMetadata>, String> {
    let any_present = listen_pid.is_some() || listen_fds.is_some() || listen_fdnames.is_some();
    if !any_present {
        return Ok(None);
    }
    let pid = listen_pid.ok_or_else(|| {
        "invalid systemd socket activation: LISTEN_PID is missing while activation metadata is present"
            .to_string()
    })?;
    let fds = listen_fds.ok_or_else(|| {
        "invalid systemd socket activation: LISTEN_FDS is missing while activation metadata is present"
            .to_string()
    })?;
    let pid = pid.parse::<u32>().map_err(|_| {
        format!("invalid systemd socket activation: LISTEN_PID is not a valid process id: {pid:?}")
    })?;
    if pid != current_pid {
        return Err(format!(
            "invalid systemd socket activation: LISTEN_PID={pid} does not match current process {current_pid}"
        ));
    }
    let fds = fds.parse::<u32>().map_err(|_| {
        format!("invalid systemd socket activation: LISTEN_FDS is not a valid descriptor count: {fds:?}")
    })?;
    if fds != 1 {
        return Err(format!(
            "unsupported systemd socket activation: expected exactly one HTTP listening socket, got LISTEN_FDS={fds}"
        ));
    }
    let fd_name = listen_fdnames.map(str::to_string);
    if let Some(name) = fd_name.as_deref() {
        if name != HTTP_FD_NAME {
            return Err(format!(
                "invalid systemd socket activation: expected LISTEN_FDNAMES={HTTP_FD_NAME:?}, got {name:?}"
            ));
        }
    }
    Ok(Some(ActivationMetadata { fd_name }))
}

#[cfg(target_os = "linux")]
fn read_activation_metadata_from_env() -> Result<Option<ActivationMetadata>, String> {
    let listen_pid = std::env::var("LISTEN_PID").ok();
    let listen_fds = std::env::var("LISTEN_FDS").ok();
    let listen_fdnames = std::env::var("LISTEN_FDNAMES").ok();
    activation_metadata_from_values(
        listen_pid.as_deref(),
        listen_fds.as_deref(),
        listen_fdnames.as_deref(),
        std::process::id(),
    )
}

#[cfg(target_os = "linux")]
fn clear_activation_environment() {
    for key in SYSTEMD_ACTIVATION_ENV_KEYS {
        std::env::remove_var(key);
    }
}

#[cfg(target_os = "linux")]
fn activation_metadata_from_env() -> Result<Option<ActivationMetadata>, String> {
    if let Some(metadata) = PREPARED_ACTIVATION_METADATA.get() {
        return Ok(metadata.clone());
    }
    read_activation_metadata_from_env()
}

/// Consume systemd's process-local socket-activation contract before the
/// Server creates its Tokio worker threads. `LISTEN_*` is one-shot metadata:
/// descendants must not mistake it for their own activation contract, and the
/// inherited listener must not leak through later `exec` calls.
#[cfg(target_os = "linux")]
pub(crate) fn prepare_activation_from_env() -> Result<(), String> {
    if PREPARED_ACTIVATION_METADATA.get().is_some() {
        return Ok(());
    }
    let metadata = read_activation_metadata_from_env()?;
    if metadata.is_some() {
        validate_listening_tcp_fd(SYSTEMD_LISTEN_FD_START)?;
        set_close_on_exec(SYSTEMD_LISTEN_FD_START)?;
    }
    PREPARED_ACTIVATION_METADATA
        .set(metadata)
        .map_err(|_| "systemd socket activation was prepared concurrently".to_string())?;
    clear_activation_environment();
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn prepare_activation_from_env() -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn activation_metadata_from_env() -> Result<Option<ActivationMetadata>, String> {
    Ok(None)
}

fn configured_addr_matches(configured: &str, actual: SocketAddr) -> Result<bool, String> {
    let configured_addrs = configured.to_socket_addrs().map_err(|error| {
        format!("invalid WEBCODEX_ADDR {configured:?}: cannot resolve configured listener address: {error}")
    })?;
    Ok(configured_addrs
        .into_iter()
        .any(|candidate| candidate == actual))
}

#[cfg(target_os = "linux")]
fn set_close_on_exec(fd: i32) -> Result<(), String> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags == -1 {
        return Err(format!(
            "invalid systemd socket activation: failed to inspect inherited fd {fd} flags: {}",
            std::io::Error::last_os_error()
        ));
    }
    if flags & libc::FD_CLOEXEC == 0
        && unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } == -1
    {
        return Err(format!(
            "invalid systemd socket activation: failed to mark inherited fd {fd} close-on-exec: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_listening_tcp_fd(fd: i32) -> Result<(), String> {
    let mut socket_type: libc::c_int = 0;
    let mut socket_type_len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    let type_result = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut socket_type as *mut libc::c_int).cast(),
            &mut socket_type_len,
        )
    };
    if type_result != 0 {
        return Err(format!(
            "invalid systemd socket activation: inherited fd {fd} is not a valid socket: {}",
            std::io::Error::last_os_error()
        ));
    }
    if socket_type != libc::SOCK_STREAM {
        return Err(format!(
            "invalid systemd socket activation: inherited fd {fd} is not a TCP stream listening socket"
        ));
    }

    let mut socket_addr: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let mut socket_addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
    let addr_result = unsafe {
        libc::getsockname(
            fd,
            (&mut socket_addr as *mut libc::sockaddr_storage).cast(),
            &mut socket_addr_len,
        )
    };
    if addr_result != 0 {
        return Err(format!(
            "invalid systemd socket activation: failed to inspect inherited fd {fd} address family: {}",
            std::io::Error::last_os_error()
        ));
    }
    if !matches!(
        socket_addr.ss_family as libc::c_int,
        libc::AF_INET | libc::AF_INET6
    ) {
        return Err(format!(
            "invalid systemd socket activation: inherited fd {fd} is not an IPv4/IPv6 TCP listening socket"
        ));
    }

    let mut accepting: libc::c_int = 0;
    let mut accepting_len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    let accept_result = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_ACCEPTCONN,
            (&mut accepting as *mut libc::c_int).cast(),
            &mut accepting_len,
        )
    };
    if accept_result != 0 || accepting != 1 {
        return Err(format!(
            "invalid systemd socket activation: inherited fd {fd} is not in listening state"
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn take_inherited_listener(
    fd: i32,
    configured_addr: &str,
) -> Result<std::net::TcpListener, String> {
    use std::os::fd::FromRawFd;

    validate_listening_tcp_fd(fd)?;
    // systemd deliberately passes activation descriptors without FD_CLOEXEC so
    // this process can receive them across exec. Once adopted, restore the
    // normal close-on-exec boundary before any Server child can be spawned.
    set_close_on_exec(fd)?;
    let listener = unsafe { std::net::TcpListener::from_raw_fd(fd) };
    listener.set_nonblocking(true).map_err(|error| {
        format!("failed to set inherited systemd HTTP listener nonblocking: {error}")
    })?;
    let actual = listener.local_addr().map_err(|error| {
        format!("failed to inspect inherited systemd HTTP listener address: {error}")
    })?;
    if !configured_addr_matches(configured_addr, actual)? {
        return Err(format!(
            "inherited systemd HTTP listener address {actual} does not match configured WEBCODEX_ADDR {configured_addr:?}"
        ));
    }
    Ok(listener)
}

pub(crate) async fn server_acceptor(
    configured_addr: &str,
) -> Result<(TcpAcceptor, ListenerMode, SocketAddr), String> {
    match activation_metadata_from_env()? {
        None => {
            let acceptor = TcpListener::new(configured_addr.to_string())
                .try_bind()
                .await
                .map_err(|error| {
                    format!("failed to bind HTTP listener {configured_addr}: {error}")
                })?;
            let actual = acceptor
                .local_addr()
                .map_err(|error| format!("failed to inspect bound HTTP listener: {error}"))?;
            Ok((acceptor, ListenerMode::Direct, actual))
        }
        Some(_metadata) => {
            #[cfg(target_os = "linux")]
            {
                let listener = take_inherited_listener(SYSTEMD_LISTEN_FD_START, configured_addr)?;
                let tokio_listener =
                    tokio::net::TcpListener::from_std(listener).map_err(|error| {
                        format!("failed to adopt inherited systemd HTTP listener: {error}")
                    })?;
                let acceptor = TcpAcceptor::try_from(tokio_listener).map_err(|error| {
                    format!("failed to adapt inherited systemd HTTP listener for Salvo: {error}")
                })?;
                let actual = acceptor.local_addr().map_err(|error| {
                    format!("failed to inspect adopted systemd HTTP listener: {error}")
                })?;
                Ok((acceptor, ListenerMode::SystemdActivated, actual))
            }
            #[cfg(not(target_os = "linux"))]
            {
                unreachable!("non-Linux activation metadata is ignored")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_metadata_absent_selects_direct_mode() {
        assert_eq!(
            activation_metadata_from_values(None, None, None, 42),
            Ok(None)
        );
    }

    #[test]
    fn activation_metadata_accepts_exact_current_pid_and_one_fd() {
        assert_eq!(
            activation_metadata_from_values(Some("42"), Some("1"), Some(HTTP_FD_NAME), 42),
            Ok(Some(ActivationMetadata {
                fd_name: Some(HTTP_FD_NAME.to_string())
            }))
        );
    }

    #[test]
    fn activation_metadata_rejects_partial_wrong_or_malformed_contracts() {
        for (pid, fds, names, current, needle) in [
            (None, Some("1"), None, 42, "LISTEN_PID is missing"),
            (Some("42"), None, None, 42, "LISTEN_FDS is missing"),
            (
                Some("41"),
                Some("1"),
                None,
                42,
                "does not match current process",
            ),
            (
                Some("wat"),
                Some("1"),
                None,
                42,
                "LISTEN_PID is not a valid",
            ),
            (
                Some("42"),
                Some("wat"),
                None,
                42,
                "LISTEN_FDS is not a valid",
            ),
            (Some("42"), Some("0"), None, 42, "exactly one"),
            (Some("42"), Some("2"), None, 42, "exactly one"),
            (Some("42"), Some("1"), Some("other"), 42, "LISTEN_FDNAMES"),
        ] {
            let error = activation_metadata_from_values(pid, fds, names, current).unwrap_err();
            assert!(
                error.contains(needle),
                "{error:?} did not contain {needle:?}"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn activation_environment_cleanup_removes_one_shot_systemd_metadata() {
        let mut env = crate::test_support::TestEnvGuard::new();
        for (key, value) in [
            ("LISTEN_PID", "123"),
            ("LISTEN_FDS", "1"),
            ("LISTEN_FDNAMES", HTTP_FD_NAME),
        ] {
            env.set(key, value);
        }
        clear_activation_environment();
        for key in SYSTEMD_ACTIVATION_ENV_KEYS {
            assert!(std::env::var_os(key).is_none(), "{key} was not cleared");
        }
        let inherited = std::process::Command::new("env")
            .output()
            .expect("spawn child environment probe");
        assert!(inherited.status.success());
        let inherited = String::from_utf8(inherited.stdout).expect("child environment is UTF-8");
        for key in SYSTEMD_ACTIVATION_ENV_KEYS {
            assert!(
                !inherited
                    .lines()
                    .any(|line| line.starts_with(&format!("{key}="))),
                "{key} leaked into a spawned child"
            );
        }
    }

    #[cfg(target_os = "linux")]
    fn move_to_isolated_test_fd(fd: i32) -> i32 {
        let isolated = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 1000) };
        assert!(
            isolated >= 1000,
            "failed to duplicate fd for isolated ownership test"
        );
        unsafe { libc::close(fd) };
        isolated
    }

    #[cfg(target_os = "linux")]
    fn move_to_isolated_inheritable_test_fd(fd: i32) -> i32 {
        let isolated = unsafe { libc::fcntl(fd, libc::F_DUPFD, 1000) };
        assert!(
            isolated >= 1000,
            "failed to duplicate fd for inheritable descriptor test"
        );
        unsafe { libc::close(fd) };
        let flags = unsafe { libc::fcntl(isolated, libc::F_GETFD) };
        assert_ne!(flags, -1);
        assert_eq!(flags & libc::FD_CLOEXEC, 0, "test fd was not inheritable");
        isolated
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inherited_fd_rejects_non_socket_without_taking_ownership() {
        use std::os::fd::RawFd;
        let mut fds: [RawFd; 2] = [-1, -1];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let error = take_inherited_listener(fds[0], "127.0.0.1:1").unwrap_err();
        assert!(error.contains("not a valid socket"), "{error}");
        assert_ne!(unsafe { libc::fcntl(fds[0], libc::F_GETFD) }, -1);
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inherited_fd_rejects_stream_socket_that_is_not_listening() {
        use std::os::fd::IntoRawFd;

        let stream = std::net::TcpStream::connect({
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || listener.accept().unwrap());
            addr
        })
        .unwrap();
        let raw_fd = stream.into_raw_fd();
        let error = take_inherited_listener(raw_fd, "127.0.0.1:1").unwrap_err();
        assert!(error.contains("not in listening state"), "{error}");
        assert_ne!(unsafe { libc::fcntl(raw_fd, libc::F_GETFD) }, -1);
        unsafe { libc::close(raw_fd) };
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inherited_fd_rejects_non_tcp_listening_stream_without_taking_ownership() {
        use std::os::fd::IntoRawFd;

        let tmp = tempfile::tempdir().unwrap();
        let listener =
            std::os::unix::net::UnixListener::bind(tmp.path().join("listener.sock")).unwrap();
        let raw_fd = listener.into_raw_fd();
        let error = take_inherited_listener(raw_fd, "127.0.0.1:1").unwrap_err();
        assert!(
            error.contains("not an IPv4/IPv6 TCP listening socket"),
            "{error}"
        );
        assert_ne!(unsafe { libc::fcntl(raw_fd, libc::F_GETFD) }, -1);
        unsafe { libc::close(raw_fd) };
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inherited_listener_is_marked_close_on_exec_when_adopted() {
        use std::os::fd::{AsRawFd, IntoRawFd};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let actual = listener.local_addr().unwrap();
        let raw_fd = move_to_isolated_inheritable_test_fd(listener.into_raw_fd());
        let listener = take_inherited_listener(raw_fd, &actual.to_string()).unwrap();
        let flags = unsafe { libc::fcntl(listener.as_raw_fd(), libc::F_GETFD) };
        assert_ne!(flags, -1);
        assert_ne!(
            flags & libc::FD_CLOEXEC,
            0,
            "adopted systemd listener remained inheritable"
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn inherited_fd_is_owned_once_and_adapts_to_salvo() {
        use std::os::fd::IntoRawFd;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let actual = listener.local_addr().unwrap();
        let raw_fd = move_to_isolated_test_fd(listener.into_raw_fd());
        let listener = take_inherited_listener(raw_fd, &actual.to_string()).unwrap();
        let tokio_listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let acceptor = TcpAcceptor::try_from(tokio_listener).unwrap();
        assert_eq!(acceptor.local_addr().unwrap(), actual);
        drop(acceptor);
        assert_eq!(unsafe { libc::fcntl(raw_fd, libc::F_GETFD) }, -1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inherited_fd_rejects_configured_address_mismatch() {
        use std::os::fd::IntoRawFd;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let raw_fd = move_to_isolated_test_fd(listener.into_raw_fd());
        let error = take_inherited_listener(raw_fd, "127.0.0.1:1").unwrap_err();
        assert!(
            error.contains("does not match configured WEBCODEX_ADDR"),
            "{error}"
        );
        assert_eq!(unsafe { libc::fcntl(raw_fd, libc::F_GETFD) }, -1);
    }
}
