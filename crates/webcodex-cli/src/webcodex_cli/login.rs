//! `login` / `logout` / `status` — the everyday device commands.
//!
//! `client enroll` needs the server URL, a pairing code, a client id, and a
//! profile name, and the client id has to match what the server put in the
//! pairing record — so the device name is typed twice, once on each side.
//! `login` keeps the two values a person actually has (which server, which
//! code) and derives the rest: the device name from the hostname, and the
//! destination from the username the server returns.
//!
//! # Why publishing is transactional
//!
//! Redeeming a pairing code is destructive and one-shot: the code is spent and
//! fresh tokens are minted the moment the server answers. Everything after that
//! point has to avoid two failure modes — losing the new credentials, and
//! damaging a working connection that was already there. So the whole result is
//! built in a staging directory first and only then moved into place; see
//! [`publish_connection`].

use std::path::{Path, PathBuf};
use webcodex_admin::ServerHttpOptions;

use super::connections::{
    canonical_server_url, connections_for_server, default_base_dir, descriptor_toml,
    list_connections, resolve_connection_parent, user_slug, Connection, ConnectionPaths,
    INTERNAL_DIR_PREFIX,
};
use super::{is_effective_root, shell_command, validate_user_api_token};

/// Device name reported to the server. The hostname is what a person would call
/// this machine; `--device` overrides it.
pub(crate) fn default_device_name() -> String {
    default_hostname()
        .map(|value| sanitize_device_name(&value))
        .unwrap_or_else(|| "device".to_string())
}

/// The machine-name source for the default device name.
///
/// - Windows: `COMPUTERNAME` — the OS-owned machine name set at logon — then
///   `HOSTNAME` as a fallback for shells that export it. The default never
///   depends on `HOSTNAME` alone: on a plain Windows machine `HOSTNAME` is
///   absent and `COMPUTERNAME` is the reliable source.
/// - Unix: `HOSTNAME`, then `/etc/hostname` (historical behavior preserved).
#[cfg(windows)]
fn default_hostname() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("HOSTNAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
}

#[cfg(not(windows))]
fn default_hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn sanitize_device_name(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() {
        "device".to_string()
    } else {
        cleaned.chars().take(80).collect()
    }
}

/// Name of the per-machine identity file holding the local random suffix.
const DEVICE_ID_FILE: &str = ".device-id";
/// The suffix is 16 lowercase hex characters (>= 12, the required minimum).
const DEVICE_SUFFIX_HEX_LEN: usize = 16;
/// A bounded read cap for the identity file so a corrupt file cannot force an
/// unbounded allocation. 16 hex + one newline, with headroom.
const DEVICE_ID_MAX_BYTES: usize = 64;
/// Bounded wait for the narrow window where another login has created the
/// identity file but has not finished writing its 16-byte value yet.
const DEVICE_ID_CREATE_RETRY_ATTEMPTS: usize = 20;
const DEVICE_ID_CREATE_RETRY_DELAY_MS: u64 = 5;

/// Validate a `client_id` with the same rules the server enforces
/// (`validate_allowed_client_id` in `src/auth/pat.rs`): non-empty, at most 80
/// characters, and only ASCII letters, digits, `-`, `_`, and `.`. The CLI
/// keeps its own copy so a bad explicit `--device` fails locally before the
/// one-time pairing code is spent, instead of round-tripping to the server.
pub(crate) fn validate_client_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("client_id cannot be empty".to_string());
    }
    if value.chars().count() > 80 {
        return Err("client_id is too long; maximum is 80 characters".to_string());
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(
            "client_id may only contain ASCII letters, digits, '-', '_', and '.'".to_string(),
        );
    }
    Ok(value.to_string())
}

/// Read the persistent local device suffix, or create it first.
///
/// The suffix is what makes two machines with the same hostname distinct: it is
/// generated once per machine, kept under `base`, and reused on every login, so
/// an overwrite on the same machine mints the same `client_id` and the bound
/// agent token stays usable.
///
/// `base` must already be a verified real directory tree (login resolves it
/// through `resolve_connection_parent` before calling this). The file is
/// created atomically with `create_new` so concurrent logins race cleanly:
/// whoever wins, the loser re-reads the winner's value. Every failure mode —
/// a symlink, a non-regular file, loose permissions, empty or malformed
/// content — fails before the pairing code is spent.
fn device_suffix(base: &Path) -> Result<String, String> {
    let path = base.join(DEVICE_ID_FILE);
    // Already present: read-and-validate it, surfacing the concrete reason if
    // it is a symlink, non-regular file, loose, empty, or malformed.
    match std::fs::symlink_metadata(&path) {
        Ok(_) => return read_device_suffix_after_concurrent_create(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("failed to inspect {}: {error}", path.display())),
    }

    // Absent: create it atomically. `create_new` makes concurrent logins race
    // cleanly — whoever wins, the loser re-reads the winner's value.
    let suffix = generate_device_suffix();
    match write_device_suffix_new(&path, &suffix) {
        Ok(()) => Ok(suffix),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_device_suffix_after_concurrent_create(&path)
        }
        Err(error) => Err(format!("failed to create {}: {error}", path.display())),
    }
}

fn generate_device_suffix() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..DEVICE_SUFFIX_HEX_LEN].to_string()
}

/// Atomically create `path` with `content`; fails if `path` already exists.
fn write_device_suffix_new(path: &Path, content: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    std::io::Write::write_all(&mut file, content.as_bytes())?;
    std::io::Write::write_all(&mut file, b"\n")?;
    Ok(())
}

/// Read a file that may have just won a concurrent `create_new` race.
///
/// `create_new` publishes the directory entry before the winner writes the
/// contents. A loser can therefore observe a valid 0600 regular file at length
/// zero. Retry only while that file is shorter than the complete 16-byte value;
/// planted symlinks, non-regular files, loose permissions, and complete but
/// malformed values still fail immediately.
fn read_device_suffix_after_concurrent_create(path: &Path) -> Result<String, String> {
    for attempt in 0..DEVICE_ID_CREATE_RETRY_ATTEMPTS {
        match read_device_suffix(path) {
            Ok(suffix) => return Ok(suffix),
            Err(error) => {
                let incomplete_regular_file = std::fs::symlink_metadata(path)
                    .is_ok_and(|meta| meta.is_file() && meta.len() < DEVICE_SUFFIX_HEX_LEN as u64);
                if !incomplete_regular_file || attempt + 1 == DEVICE_ID_CREATE_RETRY_ATTEMPTS {
                    return Err(error);
                }
                std::thread::sleep(std::time::Duration::from_millis(
                    DEVICE_ID_CREATE_RETRY_DELAY_MS,
                ));
            }
        }
    }
    unreachable!("bounded device identity retry loop must return")
}

/// Validate and read the device suffix file. Enforces everything an attacker
/// could otherwise plant before we trust it: the path must be a regular file,
/// not a symlink, with no group/other permissions on Unix, bounded size, and
/// exactly 16 lowercase hex characters.
fn read_device_suffix(path: &Path) -> Result<String, String> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if meta.is_symlink() {
        return Err(format!(
            "{} is a symlink; refusing to read a device identity through it",
            path.display()
        ));
    }
    if !meta.is_file() {
        return Err(format!(
            "{} is not a regular file; refusing to use it as a device identity",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!(
                "{} has group/other permissions ({mode:o}); refusing to read a device identity from it",
                path.display()
            ));
        }
    }
    let file = std::fs::File::open(path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    use std::io::Read;
    file.take(DEVICE_ID_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if bytes.len() > DEVICE_ID_MAX_BYTES {
        return Err(format!(
            "{} is unexpectedly large; refusing to use it as a device identity",
            path.display()
        ));
    }
    let content = String::from_utf8_lossy(&bytes);
    let suffix = content.trim().to_string();
    let valid = suffix.len() == DEVICE_SUFFIX_HEX_LEN
        && suffix
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !valid {
        return Err(format!(
            "{} does not contain exactly {DEVICE_SUFFIX_HEX_LEN} lowercase hex characters; refusing to use it as a device identity",
            path.display()
        ));
    }
    Ok(suffix)
}

/// Resolve the device name a login will report to the server.
///
/// An explicit `--device` wins verbatim (validated against the server's
/// `client_id` rules). Otherwise the readable hostname is combined with the
/// persistent local suffix, truncating the hostname side so the total stays
/// within the server's 80-character cap — the suffix is what guarantees
/// uniqueness, so it is never the part that gets cut.
pub(crate) fn resolve_device_name(base: &Path, opts: &LoginOptions) -> Result<String, String> {
    if opts.device_explicit {
        return validate_client_id(&opts.device);
    }
    let suffix = device_suffix(base)?;
    let hostname = sanitize_device_name(&opts.device);
    // Reserve room for the separator and the suffix.
    let budget = 80usize.saturating_sub(suffix.len() + 1);
    let head: String = hostname.chars().take(budget).collect();
    let head = if head.is_empty() {
        "device".to_string()
    } else {
        head
    };
    let combined = format!("{head}-{suffix}");
    validate_client_id(&combined)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoginOptions {
    pub(crate) server_url: String,
    pub(crate) server_http: ServerHttpOptions,
    pub(crate) code: String,
    pub(crate) device: String,
    pub(crate) device_explicit: bool,
    pub(crate) base_dir: PathBuf,
    pub(crate) transport: String,
    pub(crate) allowed_roots: Vec<PathBuf>,
    pub(crate) overwrite: bool,
    pub(crate) json: bool,
    pub(crate) print_mcp_config: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogoutOptions {
    pub(crate) server_url: String,
    pub(crate) username: Option<String>,
    pub(crate) base_dir: PathBuf,
    pub(crate) yes: bool,
    pub(crate) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusOptions {
    pub(crate) base_dir: PathBuf,
    pub(crate) json: bool,
}

/// The credentials a redeemed pairing code produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnrolledIdentity {
    pub(crate) username: String,
    pub(crate) user_token: String,
    pub(crate) agent_token: String,
}

/// Where a finished login ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PublishOutcome {
    /// The connection is live at its final path.
    Published,
    /// The final path was already taken and `--overwrite` was not given, so the
    /// new credentials were parked here instead of being thrown away.
    SavedForRecovery { path: PathBuf },
    /// The new connection is live, but the replaced one could not be deleted
    /// and its credentials are still on disk.
    PublishedWithBackupResidue { path: PathBuf },
}

fn unique_internal_dir(parent: &Path, kind: &str) -> PathBuf {
    let token = uuid::Uuid::new_v4().simple().to_string();
    parent.join(format!("{INTERNAL_DIR_PREFIX}{kind}-{token}"))
}

#[cfg(unix)]
fn harden_dir(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("failed to secure {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn harden_dir(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn harden_secret_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to secure {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn harden_secret_file(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Create a private staging directory next to where the connection will land.
///
/// It has to share a parent with the destination so that publishing is a
/// same-filesystem rename rather than a copy that could half-succeed.
pub(crate) fn create_staging_dir(parent: &Path) -> Result<PathBuf, String> {
    let staging = unique_internal_dir(parent, "staging");
    std::fs::create_dir(&staging)
        .map_err(|error| format!("failed to create staging directory: {error}"))?;
    harden_dir(&staging)?;
    Ok(staging)
}

/// Removal of an internal (staging / backup / recovery) directory.
///
/// Wrapped so tests can force the failure path: these directories hold live
/// tokens, so "could not delete" has to reach the user rather than being
/// swallowed.
fn remove_internal_dir(path: &Path) -> Result<(), String> {
    #[cfg(test)]
    if tests::removal_is_forced_to_fail() {
        return Err("injected removal failure".to_string());
    }
    std::fs::remove_dir_all(path).map_err(|error| error.to_string())
}

/// Delete an internal directory, reporting the path if anything is left.
///
/// A leftover staging or backup directory still contains a usable agent token
/// and user token. `status` will not show it, which is exactly why silence here
/// would be wrong: nothing else would ever mention it again.
#[must_use = "leftover internal directories hold live credentials"]
fn discard_internal_dir(path: &Path) -> Option<PathBuf> {
    match remove_internal_dir(path) {
        Ok(()) => None,
        Err(_) => Some(path.to_path_buf()),
    }
}

/// Append a residue note to an error, naming the path but never its contents.
fn note_residue(error: String, residue: Option<PathBuf>) -> String {
    match residue {
        None => error,
        Some(path) => format!(
            "{error}; credentials were also left behind at {} and should be deleted",
            path.display()
        ),
    }
}

/// Move a fully-built staging directory into its final place.
///
/// * destination free — one rename, nothing else to undo.
/// * destination taken with `overwrite` — the old connection is moved aside
///   first and only deleted once the new one is in place; if the second rename
///   fails the old one goes back.
/// * destination taken without `overwrite` — the code has already been spent, so
///   the staged connection is kept under a recovery directory rather than
///   deleted, and the existing connection is left untouched.
pub(crate) fn publish_connection(
    staging: &Path,
    final_dir: &Path,
    overwrite: bool,
) -> Result<PublishOutcome, String> {
    let parent = final_dir
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", final_dir.display()))?;

    // `symlink_metadata` rather than `exists`, so a dangling symlink at the
    // destination is seen as occupied instead of silently renamed through.
    let destination_taken = std::fs::symlink_metadata(final_dir).is_ok();

    if !destination_taken {
        return match std::fs::rename(staging, final_dir) {
            Ok(()) => Ok(PublishOutcome::Published),
            Err(error) => {
                let residue = discard_internal_dir(staging);
                Err(note_residue(
                    format!(
                        "failed to publish the connection to {}: {error}",
                        final_dir.display()
                    ),
                    residue,
                ))
            }
        };
    }

    if !overwrite {
        let recovery = unique_internal_dir(parent, "recovery");
        return match std::fs::rename(staging, &recovery) {
            Ok(()) => Ok(PublishOutcome::SavedForRecovery { path: recovery }),
            Err(error) => {
                let residue = discard_internal_dir(staging);
                Err(note_residue(
                    format!("failed to save the new credentials: {error}"),
                    residue,
                ))
            }
        };
    }

    let backup = unique_internal_dir(parent, "backup");
    if let Err(error) = std::fs::rename(final_dir, &backup) {
        let residue = discard_internal_dir(staging);
        return Err(note_residue(
            format!(
                "failed to move the existing connection aside: {error}; {} is unchanged",
                final_dir.display()
            ),
            residue,
        ));
    }

    if let Err(error) = std::fs::rename(staging, final_dir) {
        // Put the old connection back before reporting; a failed login must not
        // cost the user a working one.
        let restored = std::fs::rename(&backup, final_dir).is_ok();
        let residue = discard_internal_dir(staging);
        let base = if restored {
            format!(
                "failed to publish the connection: {error}; the previous connection at {} was restored",
                final_dir.display()
            )
        } else {
            format!(
                "failed to publish the connection: {error}; the previous connection is at {}",
                backup.display()
            )
        };
        return Err(note_residue(base, residue));
    }

    match discard_internal_dir(&backup) {
        None => Ok(PublishOutcome::Published),
        Some(path) => Ok(PublishOutcome::PublishedWithBackupResidue { path }),
    }
}

/// Unchecked destination for tests; production resolves through a verified
/// parent directory instead.
#[cfg(test)]
pub(crate) fn resolve_destination(
    base: &Path,
    server_url: &str,
    username: &str,
) -> Result<ConnectionPaths, String> {
    ConnectionPaths::resolve(base, server_url, username)
}

pub(crate) fn write_descriptor(
    paths: &ConnectionPaths,
    server_url: &str,
    username: &str,
    device: &str,
    now: &str,
) -> Result<(), String> {
    std::fs::write(
        &paths.descriptor,
        descriptor_toml(server_url, username, device, now),
    )
    .map_err(|error| format!("failed to write {}: {error}", paths.descriptor.display()))
}

/// Build the whole connection inside `staging`.
///
/// The agent token is written only into `agent.toml`; there is deliberately no
/// second copy on disk for it to drift from.
pub(crate) fn stage_connection(
    staging: &Path,
    published_projects_dir: &Path,
    opts: &LoginOptions,
    server_url: &str,
    identity: &EnrolledIdentity,
    device: &str,
    now: &str,
) -> Result<(), String> {
    let paths = ConnectionPaths::new(staging.to_path_buf());
    std::fs::create_dir_all(&paths.projects_dir)
        .map_err(|error| format!("failed to create {}: {error}", paths.projects_dir.display()))?;

    super::system::write_text_file(
        &paths.user_token,
        &format!("{}\n", identity.user_token),
        true,
        true,
    )?;

    crate::agent_init::run_agent_init(crate::agent_init::AgentInitOptions {
        server_url: server_url.to_string(),
        token: Some(identity.agent_token.clone()),
        token_file: None,
        client_id: device.to_string(),
        owner: identity.username.clone(),
        display_name: None,
        transport: opts.transport.clone(),
        poll_interval_ms: crate::agent_init::DEFAULT_POLL_INTERVAL_MS,
        // The directory is created inside staging above so it is published
        // atomically with the rest of the connection, but agent.toml must
        // point at its final path after that staging directory is renamed.
        projects_dir: published_projects_dir.to_path_buf(),
        output: paths.agent_config.clone(),
        allowed_roots: opts.allowed_roots.clone(),
        allow_cwd_anywhere: false,
        overwrite: true,
    })?;
    harden_secret_file(&paths.agent_config)?;

    write_descriptor(&paths, server_url, &identity.username, device, now)
}

const ROOT_AGENT_INSTALL_REASON: &str = "login ran as root; no safe systemd installation argv can be generated without explicitly selecting a non-root Runner user and validating access to the agent config, working directory, projects directory, and allowed roots";
const ROOT_FOREGROUND_REASON: &str = "login ran as root; no foreground Runner argv is emitted because it would execute project commands as root";
const WINDOWS_AGENT_INSTALL_REASON: &str = "automatic Windows Runner service installation is not supported in this release; start the foreground Runner shown above instead";

pub(crate) fn render_login_result(
    paths: &ConnectionPaths,
    server_url: &str,
    username: &str,
    device: &str,
    effective_root: bool,
    json: bool,
    print_mcp_config: bool,
) -> Result<String, String> {
    let foreground_argv = (!effective_root).then(|| {
        vec![
            "webcodex-runner".to_string(),
            "--config".to_string(),
            paths.agent_config.to_string_lossy().into_owned(),
        ]
    });
    let agent_install_argv = if effective_root || cfg!(windows) {
        None
    } else {
        Some(vec![
            "webcodex".to_string(),
            "agent".to_string(),
            "install".to_string(),
            "--scope".to_string(),
            "user".to_string(),
            "--config".to_string(),
            paths.agent_config.to_string_lossy().into_owned(),
        ])
    };
    let agent_install_reason = if effective_root {
        Some(ROOT_AGENT_INSTALL_REASON)
    } else if cfg!(windows) {
        Some(WINDOWS_AGENT_INSTALL_REASON)
    } else {
        None
    };
    let foreground_command = foreground_argv.as_ref().map(|argv| shell_command(argv));
    let agent_install_command = agent_install_argv.as_ref().map(|argv| shell_command(argv));

    // JSON output carries only safe metadata; never a full token. The
    // `--print-mcp-config` path is text-only and mutually exclusive with `--json`
    // (enforced at parse time), so the two cannot both apply here.
    if json {
        let mut next_steps = Vec::new();
        if let Some(command) = &foreground_command {
            next_steps.push(command.clone());
        }
        if let Some(command) = &agent_install_command {
            next_steps.push(command.clone());
        }
        let summary = serde_json::json!({
            "server_url": server_url,
            "username": username,
            "device": device,
            "mcp_url": format!("{server_url}/mcp"),
            "dir": paths.dir.to_string_lossy(),
            "user_token_file": paths.user_token.to_string_lossy(),
            "agent_config": paths.agent_config.to_string_lossy(),
            "credential_usage": {
                "webcodex-user-token": "GPT Actions, MCP, and REST/project APIs",
                "agent_config_token": "Runner/Agent transport only",
            },
            "foreground_available": foreground_argv.is_some(),
            "foreground_argv": &foreground_argv,
            "foreground_reason": effective_root.then_some(ROOT_FOREGROUND_REASON),
            "agent_install_available": agent_install_argv.is_some(),
            "agent_install_argv": &agent_install_argv,
            "agent_install_reason": agent_install_reason,
            "next_steps": next_steps,
        });
        return serde_json::to_string_pretty(&summary).map_err(|error| error.to_string());
    }

    if print_mcp_config {
        let token = std::fs::read_to_string(&paths.user_token)
            .map_err(|error| {
                format!(
                    "cannot read user token {} for MCP config: {error}",
                    paths.user_token.display()
                )
            })?
            .trim()
            .to_string();
        validate_user_api_token(&token)?;
        return Ok(format!(
            "Sensitive HTTP MCP connection details\n\
             ======================================\n\
             These connection details include a credential. Store them privately.\n\n\
             MCP URL: {server_url}/mcp\n\
             Authorization: Bearer {token}\n"
        ));
    }

    let next_step_guidance = match (foreground_command, agent_install_command) {
        (Some(foreground_command), Some(command)) => format!(
            "Start the agent in the foreground:\n  {foreground_command}\n\n\
             Or install it as a non-root user service (run as the same ordinary user):\n  {command}\n"
        ),
        (Some(foreground_command), None) => format!(
            "Start the agent in the foreground:\n  {foreground_command}\n\n{}\n",
            agent_install_reason.unwrap_or("automatic Runner service installation is unavailable")
        ),
        (None, None) => "Login ran as root, so no command to start a root Runner is recommended.\n\
                        Recommended: have the ordinary local user who will run the Runner use a fresh pairing code to execute `webcodex login`, then install the user service as that same user.\n\
                        Advanced system service deployment requires an administrator to explicitly select a non-root account with `--scope system --user`, verify that account can read the agent config and access the working directory, and ensure the config, projects directory, and allowed roots have suitable permissions.\n"
            .to_string(),
        _ => return Err("inconsistent login Runner guidance state".to_string()),
    };

    Ok(format!(
        "Logged in to {server_url} as {username} ({device}).\n\n  \
         MCP endpoint: {server_url}/mcp\n  \
         user token file: {} (GPT Actions, MCP, and REST/project APIs)\n  \
         agent config:   {} (contains Runner/Agent transport credentials only)\n\n\
         {}",
        paths.user_token.display(),
        paths.agent_config.display(),
        next_step_guidance,
    ))
}

/// Message for the case where the code was spent but the destination was taken.
pub(crate) fn render_recovery_error(
    final_dir: &Path,
    recovery: &Path,
    server_url: &str,
    username: &str,
) -> String {
    format!(
        "Already logged in to {server_url} as {username}.\n\n\
         The pairing code was redeemed, so the new credentials are real and have\n\
         been saved rather than discarded. Nothing about the existing connection\n\
         was changed.\n\n  \
         existing:        {}\n  \
         new credentials: {}\n\n\
         To keep the new credentials, remove the existing connection and move the\n\
         saved directory into its place, or run `login` again with --overwrite\n\
         using a fresh code. Delete the saved directory once you are done.\n",
        final_dir.display(),
        recovery.display(),
    )
}

pub(crate) fn render_status(connections: &[Connection], json: bool) -> Result<String, String> {
    if json {
        let rows: Vec<_> = connections
            .iter()
            .map(|connection| {
                serde_json::json!({
                    "server_url": connection.server_url,
                    "username": connection.username,
                    "device": connection.device,
                    "logged_in_at": connection.logged_in_at,
                    "dir": connection.paths.dir.to_string_lossy(),
                })
            })
            .collect();
        return serde_json::to_string_pretty(&serde_json::json!({ "connections": rows }))
            .map_err(|error| error.to_string());
    }
    if connections.is_empty() {
        return Ok(
            "Not logged in to any server.\n\nRun: webcodex login <server-url> --code <pairing-code>\n"
                .to_string(),
        );
    }
    let width = connections
        .iter()
        .map(|connection| connection.server_url.len())
        .max()
        .unwrap_or(0)
        .max(6);
    let mut out = String::from("Logged in:\n\n");
    for connection in connections {
        out.push_str(&format!(
            "  {:width$}  {}  ({})\n",
            connection.server_url,
            connection.username,
            connection.device,
            width = width
        ));
    }
    Ok(out)
}

/// Connections a logout would remove.
pub(crate) fn logout_targets(opts: &LogoutOptions) -> Vec<Connection> {
    connections_for_server(&opts.base_dir, &opts.server_url)
        .into_iter()
        .filter(|connection| {
            opts.username
                .as_deref()
                .is_none_or(|username| connection.username.eq_ignore_ascii_case(username))
        })
        .collect()
}

/// Remove one connection directory.
///
/// The path is re-checked against the base directory immediately before the
/// delete, and the directory itself must still be a real directory rather than
/// a symlink, so a link swapped in after listing cannot redirect the removal.
pub(crate) fn remove_connection(base: &Path, connection: &Connection) -> Result<(), String> {
    let dir = &connection.paths.dir;
    let base = base
        .canonicalize()
        .map_err(|error| format!("failed to resolve {}: {error}", base.display()))?;

    let meta = std::fs::symlink_metadata(dir)
        .map_err(|error| format!("failed to inspect {}: {error}", dir.display()))?;
    if !meta.is_dir() {
        return Err(format!(
            "refusing to remove {}: not a directory",
            dir.display()
        ));
    }

    let resolved = dir
        .canonicalize()
        .map_err(|error| format!("failed to resolve {}: {error}", dir.display()))?;
    if !resolved.starts_with(&base) {
        return Err(format!(
            "refusing to remove {}: outside {}",
            resolved.display(),
            base.display()
        ));
    }
    // Two levels down from the base and no deeper: <base>/<server>/<user>.
    if resolved
        .strip_prefix(&base)
        .map(|rest| rest.components().count())
        != Ok(2)
    {
        return Err(format!(
            "refusing to remove {}: not a connection directory",
            resolved.display()
        ));
    }

    std::fs::remove_dir_all(&resolved)
        .map_err(|error| format!("failed to remove {}: {error}", resolved.display()))?;

    // Drop the server directory once its last user is gone, so `status` and
    // `ls` do not show an empty shell.
    if let Some(server_dir) = resolved.parent() {
        if std::fs::read_dir(server_dir).is_ok_and(|mut entries| entries.next().is_none()) {
            let _ = std::fs::remove_dir(server_dir);
        }
    }
    Ok(())
}

pub(crate) fn all_connections(base: &Path) -> Vec<Connection> {
    list_connections(base)
}

pub(crate) fn base_dir_or_default(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    explicit.map(Ok).unwrap_or_else(default_base_dir)
}

/// Redeem a pairing code. Network only — writes nothing.
///
/// Errors deliberately do not carry the response body or the code: the body can
/// contain freshly minted tokens, and the code is a live credential until it is
/// spent.
pub(crate) async fn redeem_pairing_code(
    server_url: &str,
    opts: &LoginOptions,
    device: &str,
) -> Result<EnrolledIdentity, String> {
    let mut body = serde_json::json!({
        "pairing_code": opts.code,
        "client_id": device,
        "transport": opts.transport,
        "allow_cwd_anywhere": false,
    });
    if !opts.allowed_roots.is_empty() {
        body["allowed_roots"] = serde_json::json!(opts
            .allowed_roots
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>());
    }

    let value =
        super::http::post_json_unauthed(server_url, &opts.server_http, "/api/pairing/enroll", body)
            .await?;
    let field = |name: &str| -> Result<String, String> {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("the server response did not include {name}"))
    };
    Ok(EnrolledIdentity {
        username: field("username")?,
        user_token: field("user_token")?,
        agent_token: field("agent_token")?,
    })
}

/// Log this device into a server.
pub(crate) async fn run_login(opts: LoginOptions) -> Result<String, String> {
    let effective_root = is_effective_root();
    // Reject a URL we could not turn into an identity *before* spending the
    // one-time code on it, and settle the directory it would be stored under
    // for the same reason: a symlinked base is better found now than after the
    // code is gone.
    let canonical = canonical_server_url(&opts.server_url)?;
    let server_url = canonical.url.clone();
    let parent = resolve_connection_parent(&opts.base_dir, &canonical)?;

    // The device name is settled here, after the target directory has been
    // verified but before the one-time code is spent: `resolve_device_name`
    // generates and validates the client_id, and creates `.device-id` in the
    // verified base (the server directory's parent). Every local problem —
    // an invalid explicit `--device`, a planted `.device-id` — fails now.
    let base = parent
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", parent.display()))?;
    let device = resolve_device_name(base, &opts)?;

    let identity = redeem_pairing_code(&server_url, &opts, &device).await?;
    let user = user_slug(&identity.username)?;
    let paths = ConnectionPaths::new(parent.join(user));

    let staging = create_staging_dir(&parent)?;
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(error) = stage_connection(
        &staging,
        &paths.projects_dir,
        &opts,
        &server_url,
        &identity,
        &device,
        &now,
    ) {
        let residue = discard_internal_dir(&staging);
        return Err(note_residue(error, residue));
    }

    match publish_connection(&staging, &paths.dir, opts.overwrite)? {
        PublishOutcome::Published => render_login_result(
            &paths,
            &server_url,
            &identity.username,
            &device,
            effective_root,
            opts.json,
            opts.print_mcp_config,
        ),
        // The pairing code was spent but the destination was taken; the fresh
        // credentials are real and are parked for recovery. Never emit a token
        // here, even with `--print-mcp-config` — there is no published
        // connection to print for.
        PublishOutcome::SavedForRecovery { path } => Err(render_recovery_error(
            &paths.dir,
            &path,
            &server_url,
            &identity.username,
        )),
        // The login worked, but the credentials it replaced are still on disk.
        // Reported as a failure so it cannot be mistaken for a clean run.
        PublishOutcome::PublishedWithBackupResidue { path } => Err(format!(
            "Logged in to {server_url} as {}, but the credentials this replaced could not be\n\
             deleted and are still readable at {}.\n\n\
             The new connection at {} is in place and usable. Delete the leftover\n\
             directory once you have confirmed nothing else needs it.\n",
            identity.username,
            path.display(),
            paths.dir.display(),
        )),
    }
}

pub(crate) fn run_logout(opts: LogoutOptions) -> Result<String, String> {
    canonical_server_url(&opts.server_url)?;
    let targets = logout_targets(&opts);
    if targets.is_empty() {
        return Err(format!("not logged in to {}", opts.server_url));
    }
    if !opts.yes {
        let names: Vec<String> = targets
            .iter()
            .map(|connection| format!("{} as {}", connection.server_url, connection.username))
            .collect();
        return Err(format!(
            "this removes {} connection(s):\n  {}\n\nRe-run with --yes to confirm.",
            targets.len(),
            names.join("\n  ")
        ));
    }
    for connection in &targets {
        remove_connection(&opts.base_dir, connection)?;
    }
    if opts.json {
        let rows: Vec<_> = targets
            .iter()
            .map(|connection| {
                serde_json::json!({
                    "server_url": connection.server_url,
                    "username": connection.username,
                })
            })
            .collect();
        return serde_json::to_string_pretty(&serde_json::json!({ "removed": rows }))
            .map_err(|error| error.to_string());
    }
    Ok(format!("Removed {} connection(s).\n", targets.len()))
}

pub(crate) fn run_status(opts: StatusOptions) -> Result<String, String> {
    render_status(&all_connections(&opts.base_dir), opts.json)
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        /// Forces `remove_internal_dir` to fail, so the residue-reporting paths
        /// can be exercised. Deleting a directory otherwise always succeeds
        /// here, and the process runs as root, so permissions cannot be used.
        static FORCE_REMOVAL_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    pub(super) fn removal_is_forced_to_fail() -> bool {
        FORCE_REMOVAL_FAILURE.with(std::cell::Cell::get)
    }

    /// Run `body` with internal-directory removal forced to fail.
    fn with_failing_removal<T>(body: impl FnOnce() -> T) -> T {
        FORCE_REMOVAL_FAILURE.with(|flag| flag.set(true));
        let result = body();
        FORCE_REMOVAL_FAILURE.with(|flag| flag.set(false));
        result
    }

    const CODE: &str = "wc_pair_supersecretcode";
    const USER_TOKEN: &str = "wc_pat_usersecret";
    const AGENT_TOKEN: &str = "wc_agent_agentsecret";

    fn login_opts(base: &Path, server_url: &str, overwrite: bool) -> LoginOptions {
        LoginOptions {
            server_url: server_url.to_string(),
            server_http: ServerHttpOptions::default(),
            code: CODE.to_string(),
            device: "laptop".to_string(),
            device_explicit: false,
            base_dir: base.to_path_buf(),
            transport: "websocket".to_string(),
            allowed_roots: Vec::new(),
            overwrite,
            json: false,
            print_mcp_config: false,
        }
    }

    fn identity() -> EnrolledIdentity {
        EnrolledIdentity {
            username: "alice".to_string(),
            user_token: USER_TOKEN.to_string(),
            agent_token: AGENT_TOKEN.to_string(),
        }
    }

    /// The local half of a login, with the network exchange already done.
    fn publish_login(
        base: &Path,
        server_url: &str,
        overwrite: bool,
    ) -> Result<PublishOutcome, String> {
        let opts = login_opts(base, server_url, overwrite);
        let canonical = canonical_server_url(server_url).unwrap();
        let identity = identity();
        let parent = resolve_connection_parent(base, &canonical)?;
        let paths = ConnectionPaths::new(parent.join(user_slug(&identity.username).unwrap()));
        let staging = create_staging_dir(&parent)?;
        if let Err(error) = stage_connection(
            &staging,
            &paths.projects_dir,
            &opts,
            &canonical.url,
            &identity,
            &opts.device,
            "t",
        ) {
            let _ = discard_internal_dir(&staging);
            return Err(error);
        }
        publish_connection(&staging, &paths.dir, overwrite)
    }

    fn seed_connection(base: &Path, server_url: &str, username: &str) -> ConnectionPaths {
        let canonical = canonical_server_url(server_url).unwrap();
        let paths = resolve_destination(base, &canonical.url, username).unwrap();
        std::fs::create_dir_all(&paths.dir).unwrap();
        write_descriptor(&paths, &canonical.url, username, "laptop", "t").unwrap();
        paths
    }

    fn assert_no_internal_residue(dir: &Path) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let name = entry.unwrap().file_name();
            assert!(
                !name.to_string_lossy().starts_with(INTERNAL_DIR_PREFIX),
                "leftover internal directory in {}: {name:?}",
                dir.display()
            );
        }
    }

    #[test]
    fn device_name_is_derived_and_path_safe() {
        assert_eq!(sanitize_device_name("Alice-Laptop"), "alice-laptop");
        assert_eq!(sanitize_device_name("  host.local  "), "host.local");
        assert_eq!(sanitize_device_name("weird/name:here"), "weird-name-here");
        assert_eq!(sanitize_device_name("---"), "device");
        assert_eq!(sanitize_device_name(""), "device");
        assert!(sanitize_device_name(&"x".repeat(200)).len() <= 80);
    }

    /// `default_device_name` must be derivable from controlled environment
    /// variables alone — tests must never depend on the real machine name.
    #[test]
    fn default_device_name_uses_the_platform_hostname_source() {
        let _guard = crate::webcodex_cli::test_support::env_test_guard();
        #[cfg(windows)]
        let env = crate::webcodex_cli::test_support::EnvGuard::new()
            .remove("COMPUTERNAME")
            .remove("HOSTNAME");
        #[cfg(not(windows))]
        let env = crate::webcodex_cli::test_support::EnvGuard::new().remove("HOSTNAME");
        let _env = env;

        #[cfg(windows)]
        {
            // COMPUTERNAME is the OS-owned source and wins over HOSTNAME.
            let _c = crate::webcodex_cli::test_support::EnvGuard::new()
                .set("COMPUTERNAME", "DESKTOP-ABC123")
                .set("HOSTNAME", "msys-host");
            assert_eq!(default_device_name(), "desktop-abc123");
            // Without COMPUTERNAME, HOSTNAME is the fallback.
            let _c2 = crate::webcodex_cli::test_support::EnvGuard::new()
                .remove("COMPUTERNAME")
                .set("HOSTNAME", "Msys-Host");
            assert_eq!(default_device_name(), "msys-host");
            // Neither: the stable fallback.
            let _c3 = crate::webcodex_cli::test_support::EnvGuard::new()
                .remove("COMPUTERNAME")
                .remove("HOSTNAME");
            assert_eq!(default_device_name(), "device");
            // An empty COMPUTERNAME is treated as missing.
            let _c4 = crate::webcodex_cli::test_support::EnvGuard::new()
                .set("COMPUTERNAME", "  ")
                .remove("HOSTNAME");
            assert_eq!(default_device_name(), "device");
        }
        #[cfg(not(windows))]
        {
            // Unix keeps the historical HOSTNAME-first behavior.
            let _h = crate::webcodex_cli::test_support::EnvGuard::new().set("HOSTNAME", "web-1");
            assert_eq!(default_device_name(), "web-1");
            // COMPUTERNAME is a foreign variable on Unix and must not win.
            let _h2 = crate::webcodex_cli::test_support::EnvGuard::new()
                .set("HOSTNAME", "web-2")
                .set("COMPUTERNAME", "desktop-x");
            assert_eq!(default_device_name(), "web-2");
        }
    }

    #[test]
    fn destination_is_server_then_user() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = resolve_destination(temp.path(), "https://api.example.com", "alice").unwrap();
        assert_eq!(
            paths.dir,
            temp.path().join("https_api.example.com").join("alice")
        );
        assert_eq!(paths.agent_config, paths.dir.join("agent.toml"));
    }

    #[test]
    fn a_fresh_login_publishes_through_staging_and_leaves_nothing_behind() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        assert_eq!(
            publish_login(base, "https://api.example.com", false).unwrap(),
            PublishOutcome::Published
        );

        let listed = all_connections(base);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].username, "alice");
        assert_eq!(listed[0].server_url, "https://api.example.com");

        let paths = &listed[0].paths;
        assert!(paths.agent_config.is_file());
        assert!(paths.user_token.is_file());
        assert!(paths.projects_dir.is_dir());
        // The agent token has exactly one home.
        assert!(!paths.dir.join("webcodex-runner-token").exists());
        let agent_config = std::fs::read_to_string(&paths.agent_config).unwrap();
        assert!(agent_config.contains(AGENT_TOKEN));
        let parsed: toml::Value = toml::from_str(&agent_config).unwrap();
        let configured_projects_dir = PathBuf::from(
            parsed
                .get("projects_dir")
                .and_then(toml::Value::as_str)
                .expect("projects_dir must be present"),
        );
        assert_eq!(
            configured_projects_dir.canonicalize().unwrap(),
            paths.projects_dir.canonicalize().unwrap(),
            "published agent.toml must reference the published projects.d directory"
        );
        // Canonical equality above proves this is the published projects.d,
        // not the differently named staging directory that existed before the
        // atomic rename.

        assert_no_internal_residue(base);
        assert_no_internal_residue(paths.dir.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn published_secrets_are_not_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::TempDir::new().unwrap();
        publish_login(temp.path(), "https://api.example.com", false).unwrap();
        let paths = all_connections(temp.path())[0].paths.clone();
        for secret in [&paths.agent_config, &paths.user_token] {
            let mode = std::fs::metadata(secret).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{} has mode {mode:o}", secret.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_staging_directory_is_private_while_it_exists() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::TempDir::new().unwrap();
        let paths = resolve_destination(temp.path(), "https://api.example.com", "alice").unwrap();
        std::fs::create_dir_all(paths.dir.parent().unwrap()).unwrap();
        let staging = create_staging_dir(paths.dir.parent().unwrap()).unwrap();
        let mode = std::fs::metadata(&staging).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "staging has mode {mode:o}");
        let _ = discard_internal_dir(&staging);
        assert!(!staging.exists());
    }

    #[test]
    fn a_failure_while_staging_leaves_no_connection_and_no_residue() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        let opts = login_opts(base, "https://api.example.com", false);
        let identity = identity();
        let paths = resolve_destination(base, "https://api.example.com", "alice").unwrap();
        std::fs::create_dir_all(paths.dir.parent().unwrap()).unwrap();
        let staging = create_staging_dir(paths.dir.parent().unwrap()).unwrap();

        // Make agent.toml impossible to create by putting a directory there.
        std::fs::create_dir_all(staging.join("agent.toml")).unwrap();
        let result = stage_connection(
            &staging,
            &paths.projects_dir,
            &opts,
            "https://api.example.com",
            &identity,
            &opts.device,
            "t",
        );
        assert!(result.is_err(), "staging should have failed");
        let _ = discard_internal_dir(&staging);

        assert!(all_connections(base).is_empty());
        assert!(!paths.dir.exists(), "no connection may have been published");
        assert!(!staging.exists(), "staging must be cleaned up");
    }

    #[test]
    fn overwrite_replaces_the_connection_and_removes_the_backup() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();
        let paths = all_connections(base)[0].paths.clone();
        std::fs::write(paths.dir.join("marker"), "old").unwrap();

        assert_eq!(
            publish_login(base, "https://api.example.com", true).unwrap(),
            PublishOutcome::Published
        );
        assert!(
            !paths.dir.join("marker").exists(),
            "the old connection should have been replaced"
        );
        assert_eq!(all_connections(base).len(), 1);
        assert_no_internal_residue(paths.dir.parent().unwrap());
    }

    #[test]
    fn a_failed_overwrite_restores_the_previous_connection() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();
        let paths = all_connections(base)[0].paths.clone();
        std::fs::write(paths.dir.join("marker"), "old").unwrap();

        // A staging path that cannot be renamed into place.
        let missing = paths.dir.parent().unwrap().join(".staging-does-not-exist");
        let error = publish_connection(&missing, &paths.dir, true).unwrap_err();
        assert!(error.contains("restored"), "{error}");

        let listed = all_connections(base);
        assert_eq!(listed.len(), 1, "the old connection must survive");
        assert_eq!(
            std::fs::read_to_string(paths.dir.join("marker")).unwrap(),
            "old"
        );
    }

    #[test]
    fn without_overwrite_the_old_connection_stays_and_new_credentials_are_kept() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();
        let paths = all_connections(base)[0].paths.clone();
        std::fs::write(paths.dir.join("marker"), "old").unwrap();

        let outcome = publish_login(base, "https://api.example.com", false).unwrap();
        let PublishOutcome::SavedForRecovery { path } = outcome else {
            panic!("expected the staged connection to be saved, got {outcome:?}");
        };

        // The existing connection is untouched...
        assert_eq!(
            std::fs::read_to_string(paths.dir.join("marker")).unwrap(),
            "old"
        );
        // ...the redeemed credentials still exist...
        assert!(path.join("agent.toml").is_file());
        assert!(std::fs::read_to_string(path.join("agent.toml"))
            .unwrap()
            .contains(AGENT_TOKEN));
        // ...and `status` shows one connection, not two.
        assert_eq!(all_connections(base).len(), 1);

        let message = render_recovery_error(&paths.dir, &path, "https://api.example.com", "alice");
        assert!(!message.contains(CODE), "message leaked the pairing code");
        assert!(!message.contains(AGENT_TOKEN), "message leaked a token");
        assert!(!message.contains(USER_TOKEN), "message leaked a token");
    }

    #[test]
    fn status_ignores_staging_backup_and_recovery_directories() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();
        // A second login without --overwrite parks a full recovery directory.
        publish_login(base, "https://api.example.com", false).unwrap();

        let server_dir = base.join("https_api.example.com");
        let staging = create_staging_dir(&server_dir).unwrap();
        std::fs::write(
            staging.join("server.toml"),
            descriptor_toml("https://api.example.com", "alice", "laptop", "t"),
        )
        .unwrap();

        let listed = all_connections(base);
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0].paths.dir, server_dir.join("alice"));
    }

    #[test]
    fn logout_without_username_targets_every_user_on_that_server() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        seed_connection(base, "https://api.example.com", "alice");
        seed_connection(base, "https://api.example.com", "bob");
        seed_connection(base, "https://other.example.com", "alice");

        let opts = LogoutOptions {
            server_url: "https://api.example.com".to_string(),
            username: None,
            base_dir: base.to_path_buf(),
            yes: true,
            json: false,
        };
        assert_eq!(logout_targets(&opts).len(), 2);

        let scoped = LogoutOptions {
            username: Some("bob".to_string()),
            ..opts.clone()
        };
        let targets = logout_targets(&scoped);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].username, "bob");

        run_logout(opts).unwrap();
        let left = all_connections(base);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].server_url, "https://other.example.com");
    }

    #[test]
    fn logout_over_https_does_not_touch_the_http_connection() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        seed_connection(base, "http://api.example.com", "alice");
        seed_connection(base, "https://api.example.com", "alice");
        assert_eq!(all_connections(base).len(), 2);

        run_logout(LogoutOptions {
            server_url: "https://api.example.com".to_string(),
            username: None,
            base_dir: base.to_path_buf(),
            yes: true,
            json: false,
        })
        .unwrap();

        let left = all_connections(base);
        assert_eq!(left.len(), 1, "{left:?}");
        assert_eq!(left[0].server_url, "http://api.example.com");
    }

    #[test]
    fn logout_on_one_port_does_not_touch_another() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        seed_connection(base, "https://api.example.com", "alice");
        seed_connection(base, "https://api.example.com:8443", "alice");

        run_logout(LogoutOptions {
            server_url: "https://api.example.com:8443".to_string(),
            username: None,
            base_dir: base.to_path_buf(),
            yes: true,
            json: false,
        })
        .unwrap();

        let left = all_connections(base);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].server_url, "https://api.example.com");
    }

    #[cfg(unix)]
    #[test]
    fn logout_never_follows_a_symlinked_connection_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path().join("config");
        let outside = temp.path().join("precious");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keepme"), "important").unwrap();

        let server_dir = base.join("https_api.example.com");
        std::fs::create_dir_all(&server_dir).unwrap();
        std::os::unix::fs::symlink(&outside, server_dir.join("alice")).unwrap();

        // A symlinked user directory is not a connection to begin with.
        assert!(all_connections(&base).is_empty());

        // Even handed a hand-built Connection pointing at it, removal refuses.
        let connection = Connection {
            server_url: "https://api.example.com".to_string(),
            username: "alice".to_string(),
            device: "laptop".to_string(),
            logged_in_at: None,
            paths: ConnectionPaths::new(server_dir.join("alice")),
        };
        let error = remove_connection(&base, &connection).unwrap_err();
        assert!(error.contains("refusing to remove"), "{error}");
        assert!(
            outside.join("keepme").exists(),
            "the symlink target was followed and deleted"
        );
    }

    #[test]
    fn removal_refuses_a_path_outside_the_base_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path().join("config");
        std::fs::create_dir_all(&base).unwrap();
        let outside = temp.path().join("elsewhere/https_api.example.com/alice");
        std::fs::create_dir_all(&outside).unwrap();

        let connection = Connection {
            server_url: "https://api.example.com".to_string(),
            username: "alice".to_string(),
            device: "laptop".to_string(),
            logged_in_at: None,
            paths: ConnectionPaths::new(outside.clone()),
        };
        let error = remove_connection(&base, &connection).unwrap_err();
        assert!(error.contains("outside"), "{error}");
        assert!(outside.exists());
    }

    #[test]
    fn status_lists_every_connection_and_guides_when_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        let empty = render_status(&all_connections(base), false).unwrap();
        assert!(empty.contains("Not logged in"), "{empty}");
        assert!(empty.contains("login"), "{empty}");

        publish_login(base, "https://api.example.com", false).unwrap();
        publish_login(base, "https://other.example.com", false).unwrap();
        let listed = render_status(&all_connections(base), false).unwrap();
        assert!(listed.contains("https://api.example.com"), "{listed}");
        assert!(listed.contains("https://other.example.com"), "{listed}");
        assert!(!listed.contains(AGENT_TOKEN), "status leaked a token");
        assert!(!listed.contains(USER_TOKEN), "status leaked a token");

        let json = render_status(&all_connections(base), true).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["connections"].as_array().unwrap().len(), 2);
        assert!(!json.contains(AGENT_TOKEN), "status json leaked a token");
    }

    #[test]
    fn one_device_can_hold_the_same_user_on_several_servers() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://s1.example.com", false).unwrap();
        publish_login(base, "https://s2.example.com", false).unwrap();
        assert_eq!(all_connections(base).len(), 2);
        assert_eq!(
            connections_for_server(base, "https://s1.example.com").len(),
            1
        );
    }

    // --- verified parent directory -------------------------------------------

    /// Nothing a login writes may appear in a directory outside the base.
    #[cfg(unix)]
    fn assert_no_credentials_in(dir: &Path) {
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().to_string();
            if matches!(
                name.as_str(),
                "server.toml" | "agent.toml" | "webcodex-user-token"
            ) || name.starts_with(INTERNAL_DIR_PREFIX)
            {
                offenders.push(name);
            }
        }
        assert!(
            offenders.is_empty(),
            "credentials leaked into {}: {offenders:?}",
            dir.display()
        );
    }

    #[cfg(unix)]
    #[test]
    fn login_refuses_a_symlinked_base_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let base = temp.path().join("config");
        std::os::unix::fs::symlink(&outside, &base).unwrap();

        let canonical = canonical_server_url("https://api.example.com").unwrap();
        let error = resolve_connection_parent(&base, &canonical).unwrap_err();
        assert!(!error.contains(CODE), "{error}");
        assert_no_credentials_in(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn login_refuses_a_symlinked_server_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let base = temp.path().join("config");
        std::fs::create_dir_all(&base).unwrap();

        let canonical = canonical_server_url("https://api.example.com").unwrap();
        std::os::unix::fs::symlink(&outside, base.join(&canonical.slug)).unwrap();

        let error = resolve_connection_parent(&base, &canonical).unwrap_err();
        assert!(error.contains("not a directory"), "{error}");
        assert!(!error.contains(CODE), "{error}");
        assert_no_credentials_in(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_server_symlink_is_rejected() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path().join("config");
        std::fs::create_dir_all(&base).unwrap();
        let canonical = canonical_server_url("https://api.example.com").unwrap();
        // `Path::exists` reports false for this, which is exactly why the
        // resolution uses `symlink_metadata`.
        let missing = temp.path().join("nowhere");
        std::os::unix::fs::symlink(&missing, base.join(&canonical.slug)).unwrap();
        assert!(!base.join(&canonical.slug).exists());

        let error = resolve_connection_parent(&base, &canonical).unwrap_err();
        assert!(error.contains("not a directory"), "{error}");
        assert!(!missing.exists(), "the dangling target was created");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn login_does_not_create_staging_outside_base() {
        let temp = tempfile::TempDir::new().unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let base = temp.path().join("config");
        std::fs::create_dir_all(&base).unwrap();

        let canonical = canonical_server_url("https://api.example.com").unwrap();
        std::os::unix::fs::symlink(&outside, base.join(&canonical.slug)).unwrap();

        // A full login attempt must stop at parent resolution.
        let opts = login_opts(&base, "https://api.example.com", false);
        let error = run_login(opts).await.unwrap_err();
        assert!(!error.contains(CODE), "{error}");
        assert_no_credentials_in(&outside);
        assert!(all_connections(&base).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn login_refuses_a_symlinked_base_ancestor() {
        // safe/link -> outside, base = safe/link/config. `create_dir_all` walks
        // straight through `link` and the canonicalize afterwards then reports
        // the relocated path as if it were fine.
        let temp = tempfile::TempDir::new().unwrap();
        let outside = temp.path().join("outside");
        let safe = temp.path().join("safe");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&safe).unwrap();
        std::os::unix::fs::symlink(&outside, safe.join("link")).unwrap();
        let base = safe.join("link").join("config");

        let canonical = canonical_server_url("https://api.example.com").unwrap();
        let error = resolve_connection_parent(&base, &canonical).unwrap_err();
        assert!(error.contains("symlink"), "{error}");
        assert!(!error.contains(CODE), "{error}");
        assert!(
            !outside.join("config").exists(),
            "a directory was created through the symlinked ancestor"
        );
        assert_no_credentials_in(&outside);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn login_refuses_a_symlinked_base_ancestor_before_redeeming() {
        let temp = tempfile::TempDir::new().unwrap();
        let outside = temp.path().join("outside");
        let safe = temp.path().join("safe");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&safe).unwrap();
        std::os::unix::fs::symlink(&outside, safe.join("link")).unwrap();
        let base = safe.join("link").join("config");

        // No network is reachable in tests; reaching redemption at all would
        // surface as a connection error rather than this one.
        let error = run_login(login_opts(&base, "https://api.example.com", false))
            .await
            .unwrap_err();
        assert!(error.contains("symlink"), "{error}");
        for secret in [CODE, AGENT_TOKEN, USER_TOKEN] {
            assert!(!error.contains(secret), "{error}");
        }
        assert!(!outside.join("config").exists());
        assert_no_credentials_in(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn login_refuses_a_dangling_symlinked_base_ancestor() {
        let temp = tempfile::TempDir::new().unwrap();
        let safe = temp.path().join("safe");
        std::fs::create_dir_all(&safe).unwrap();
        let nowhere = temp.path().join("nowhere");
        std::os::unix::fs::symlink(&nowhere, safe.join("link")).unwrap();
        let base = safe.join("link").join("config");

        let canonical = canonical_server_url("https://api.example.com").unwrap();
        let error = resolve_connection_parent(&base, &canonical).unwrap_err();
        assert!(error.contains("symlink"), "{error}");
        assert!(
            !nowhere.exists(),
            "the dangling ancestor target was created"
        );
    }

    #[cfg(unix)]
    #[test]
    fn login_refuses_a_file_in_the_base_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let safe = temp.path().join("safe");
        std::fs::create_dir_all(&safe).unwrap();
        std::fs::write(safe.join("blocker"), "not a directory").unwrap();
        let base = safe.join("blocker").join("config");

        let canonical = canonical_server_url("https://api.example.com").unwrap();
        let error = resolve_connection_parent(&base, &canonical).unwrap_err();
        assert!(error.contains("not a directory"), "{error}");
        assert_eq!(
            std::fs::read_to_string(safe.join("blocker")).unwrap(),
            "not a directory",
            "the blocking file was modified"
        );
    }

    #[cfg(unix)]
    #[test]
    fn missing_base_components_are_created_without_following_symlinks() {
        let temp = tempfile::TempDir::new().unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        // A symlink that shares a name with a component that will be created
        // further along a *different* branch must not be consulted.
        let base = temp.path().join("a/b/c/config");

        let canonical = canonical_server_url("https://api.example.com").unwrap();
        let parent = resolve_connection_parent(&base, &canonical).unwrap();

        // Every level exists as a real directory, none of them a symlink.
        let mut walk = temp.path().to_path_buf();
        for part in ["a", "b", "c", "config"] {
            walk.push(part);
            let meta = std::fs::symlink_metadata(&walk).unwrap();
            assert!(meta.is_dir() && !meta.is_symlink(), "{}", walk.display());
        }
        assert_eq!(parent, walk.join(&canonical.slug));
        assert_no_credentials_in(&outside);
    }

    #[test]
    fn base_paths_may_be_relative_and_contain_dot_components() {
        let temp = tempfile::TempDir::new().unwrap();
        let anchor = temp.path().canonicalize().unwrap();
        let canonical = canonical_server_url("https://api.example.com").unwrap();

        // `.` components are skipped and `..` is resolved lexically against
        // components already verified to be real directories.
        let base = anchor.join("./nested/./deeper/../deeper/config");
        let parent = resolve_connection_parent(&base, &canonical).unwrap();
        assert_eq!(
            parent,
            anchor
                .join("nested")
                .join("deeper")
                .join("config")
                .join(&canonical.slug)
        );
        assert!(parent.is_dir());
    }

    #[test]
    fn resolve_connection_parent_creates_a_missing_base_and_server_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path().join("nested/config");
        let canonical = canonical_server_url("https://api.example.com").unwrap();
        let parent = resolve_connection_parent(&base, &canonical).unwrap();
        assert_eq!(parent, base.canonicalize().unwrap().join(&canonical.slug));
        assert!(parent.is_dir());
        // Re-resolving an existing directory is fine.
        assert_eq!(
            resolve_connection_parent(&base, &canonical).unwrap(),
            parent
        );
    }

    // --- cleanup failures ----------------------------------------------------

    #[test]
    fn successful_overwrite_does_not_silently_leave_backup() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();
        let outcome = publish_login(base, "https://api.example.com", true).unwrap();
        assert_eq!(
            outcome,
            PublishOutcome::Published,
            "a clean overwrite must not report residue"
        );
        assert_no_internal_residue(base.join("https_api.example.com").as_path());
    }

    #[test]
    fn backup_cleanup_failure_is_reported() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();

        let outcome =
            with_failing_removal(|| publish_login(base, "https://api.example.com", true).unwrap());
        let PublishOutcome::PublishedWithBackupResidue { path } = outcome else {
            panic!("a backup that could not be deleted must be reported, got {outcome:?}");
        };
        assert!(path.exists(), "the reported residue should still be there");
        // The new connection is nonetheless live and is the only one listed.
        let listed = all_connections(base);
        assert_eq!(listed.len(), 1, "{listed:?}");
    }

    #[test]
    fn staging_cleanup_failure_is_reported() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        let canonical = canonical_server_url("https://api.example.com").unwrap();
        let parent = resolve_connection_parent(base, &canonical).unwrap();
        let final_dir = parent.join("alice");
        std::fs::create_dir_all(&final_dir).unwrap();

        // Renaming a staging directory that is not there fails; with removal
        // also failing, both facts have to appear in the message.
        let staging = parent.join(".staging-missing");
        let error =
            with_failing_removal(|| publish_connection(&staging, &final_dir, true).unwrap_err());
        assert!(error.contains("left behind"), "{error}");
        assert!(error.contains(".staging-missing"), "{error}");
    }

    #[test]
    fn cleanup_errors_do_not_contain_credentials() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        let canonical = canonical_server_url("https://api.example.com").unwrap();
        let parent = resolve_connection_parent(base, &canonical).unwrap();
        let final_dir = parent.join("alice");

        // Build a real staging directory holding real tokens, then force every
        // cleanup path to fail and check none of the messages spill them.
        let mut messages = Vec::new();
        for overwrite in [false, true] {
            std::fs::create_dir_all(&final_dir).unwrap();
            let missing = parent.join(".staging-gone");
            messages.push(with_failing_removal(|| {
                publish_connection(&missing, &final_dir, overwrite).unwrap_err()
            }));
        }

        let opts = login_opts(base, "https://api.example.com", false);
        let staging = create_staging_dir(&parent).unwrap();
        stage_connection(
            &staging,
            &final_dir.join("projects.d"),
            &opts,
            &canonical.url,
            &identity(),
            &opts.device,
            "t",
        )
        .unwrap();
        std::fs::create_dir_all(&final_dir).unwrap();
        let residue = with_failing_removal(|| discard_internal_dir(&staging));
        messages.push(note_residue("staging failed".to_string(), residue));

        for message in &messages {
            for secret in [CODE, AGENT_TOKEN, USER_TOKEN] {
                assert!(
                    !message.contains(secret),
                    "cleanup message leaked a credential: {message}"
                );
            }
        }
    }

    #[tokio::test]
    async fn login_rejects_an_unusable_server_url_before_spending_the_code() {
        let temp = tempfile::TempDir::new().unwrap();
        for bad in [
            "https://api.example.com/path",
            "ftp://api.example.com",
            "https://user:pw@api.example.com",
            "https://api.example.com/?a=b",
        ] {
            let error = run_login(login_opts(temp.path(), bad, false))
                .await
                .unwrap_err();
            assert!(!error.contains(CODE), "error leaked the pairing code");
            assert!(all_connections(temp.path()).is_empty());
        }
    }

    // --- device identity -----------------------------------------------------

    fn explicit_device_opts(base: &Path, server_url: &str, device: &str) -> LoginOptions {
        LoginOptions {
            device: device.to_string(),
            device_explicit: true,
            ..login_opts(base, server_url, false)
        }
    }

    #[test]
    fn explicit_device_is_validated_locally_with_the_server_rules() {
        for bad in [
            "",              // empty
            "bad/client",    // slash
            "bad client",    // whitespace
            &"x".repeat(81), // too long
        ] {
            assert!(validate_client_id(bad).is_err(), "{bad:?} accepted");
        }
        for good in ["alice-laptop", "alice_macbook", "ci.runner-1", "UPPER"] {
            assert_eq!(validate_client_id(good).unwrap(), good);
        }
    }

    #[test]
    fn resolved_default_device_combines_hostname_and_persistent_suffix() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        let opts = login_opts(base, "https://api.example.com", false);
        let first = resolve_device_name(base, &opts).unwrap();
        // hostname + "-" + 16 hex.
        assert_eq!(first.len(), opts.device.len() + 1 + 16, "{first}");
        assert!(first.starts_with(&opts.device));
        let (head, suffix) = first.split_once('-').unwrap();
        assert_eq!(head, opts.device);
        assert_eq!(suffix.len(), 16, "{suffix}");
        assert!(suffix
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        assert!(first
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')));

        // The suffix is stable across calls.
        let second = resolve_device_name(base, &opts).unwrap();
        assert_eq!(first, second);
        // Two different bases get different suffixes.
        let other = tempfile::TempDir::new().unwrap();
        let third = resolve_device_name(other.path(), &opts).unwrap();
        assert_ne!(first, third);
    }

    #[tokio::test]
    async fn default_login_device_redeems_an_unbound_code() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 16384];
            let length = stream.read(&mut request).unwrap();
            request_tx
                .send(String::from_utf8_lossy(&request[..length]).to_string())
                .unwrap();
            let body = serde_json::json!({
                "username": "alice",
                "user_token": USER_TOKEN,
                "agent_token": AGENT_TOKEN,
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path().join("config root");
        let opts = LoginOptions {
            server_url: format!("http://{address}"),
            server_http: ServerHttpOptions {
                proxy: None,
                no_system_proxy: true,
            },
            code: CODE.to_string(),
            device: "shared-host".to_string(),
            device_explicit: false,
            base_dir: base.clone(),
            transport: "websocket".to_string(),
            allowed_roots: Vec::new(),
            overwrite: false,
            json: false,
            print_mcp_config: false,
        };
        let output = run_login(opts).await.unwrap();
        server.join().unwrap();

        let request = request_rx.recv().unwrap();
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        let device = value["client_id"].as_str().unwrap();
        assert!(device.starts_with("shared-host-"), "{device}");
        assert_eq!(device.len(), "shared-host-".len() + DEVICE_SUFFIX_HEX_LEN);
        assert_eq!(value["pairing_code"], CODE);
        assert!(output.contains(device), "{output}");
        assert!(base.join(DEVICE_ID_FILE).is_file());
    }

    #[test]
    fn explicit_device_wins_verbatim_without_a_suffix() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        let opts = explicit_device_opts(base, "https://api.example.com", "my-rig");
        assert_eq!(resolve_device_name(base, &opts).unwrap(), "my-rig");
        // No `.device-id` is created when the device is explicit.
        assert!(!base.join(DEVICE_ID_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn device_suffix_file_is_created_with_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        std::fs::create_dir_all(base).unwrap();
        let opts = login_opts(base, "https://api.example.com", false);
        resolve_device_name(base, &opts).unwrap();

        let mode = std::fs::metadata(base.join(DEVICE_ID_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn device_suffix_is_not_listed_as_a_connection() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();
        std::fs::write(base.join(DEVICE_ID_FILE), "aabbccddeeff0011\n").unwrap();
        let listed = all_connections(base);
        assert_eq!(listed.len(), 1, "{listed:?}");
    }

    #[test]
    fn device_suffix_race_reuses_the_winner() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        let path = base.join(DEVICE_ID_FILE);
        std::fs::create_dir_all(base).unwrap();
        // Plant a winner using `create_new` semantics.
        write_device_suffix_new(&path, "feedfacecafe0001").unwrap();
        assert_eq!(device_suffix(base).unwrap(), "feedfacecafe0001");
    }

    #[test]
    fn device_suffix_waits_for_a_concurrent_creator_to_finish() {
        use std::io::Write;

        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join(DEVICE_ID_FILE);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).unwrap();
        let writer = std::thread::spawn(move || {
            // Short sleep so the reader normally observes the empty file
            // before the write lands.
            std::thread::sleep(std::time::Duration::from_millis(2));
            file.write_all(b"feedfacecafe0001\n").unwrap();
        });

        // Each call has a fixed 100ms wait budget; under full-suite parallel
        // load the scheduler can stall the writer past one budget, so retry
        // the wait until the writer (which always eventually writes) lands.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let suffix = loop {
            match read_device_suffix_after_concurrent_create(&path) {
                Ok(suffix) => break suffix,
                Err(_) if std::time::Instant::now() < deadline => continue,
                Err(error) => panic!("concurrent device suffix create never landed: {error}"),
            }
        };
        assert_eq!(suffix, "feedfacecafe0001");
        writer.join().unwrap();
    }

    #[test]
    fn device_suffix_rejects_malformed_or_planted_files_before_redeem() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        std::fs::create_dir_all(base).unwrap();

        // Empty.
        std::fs::write(base.join(DEVICE_ID_FILE), "").unwrap();
        assert!(device_suffix(base).is_err());

        // Not hex / wrong length.
        std::fs::write(base.join(DEVICE_ID_FILE), "not-hex-at-all\n").unwrap();
        assert!(device_suffix(base).is_err());
        std::fs::write(base.join(DEVICE_ID_FILE), "aabb\n").unwrap();
        assert!(device_suffix(base).is_err());

        // Uppercase hex is rejected (must be lowercase).
        std::fs::write(base.join(DEVICE_ID_FILE), "AABBCCDDEEFF0011\n").unwrap();
        assert!(device_suffix(base).is_err());

        // A directory is not a regular file.
        std::fs::remove_file(base.join(DEVICE_ID_FILE)).unwrap();
        std::fs::create_dir(base.join(DEVICE_ID_FILE)).unwrap();
        assert!(device_suffix(base).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn device_suffix_rejects_a_symlink() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(base).unwrap();
        std::fs::write(&outside, "feedfacecafe0001\n").unwrap();
        std::os::unix::fs::symlink(&outside, base.join(DEVICE_ID_FILE)).unwrap();
        let error = device_suffix(base).unwrap_err();
        assert!(error.contains("symlink"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn device_suffix_rejects_group_or_other_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        std::fs::create_dir_all(base).unwrap();
        let path = base.join(DEVICE_ID_FILE);
        std::fs::write(&path, "feedfacecafe0001\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let error = device_suffix(base).unwrap_err();
        assert!(error.contains("permissions"), "{error}");
    }

    #[test]
    fn a_hostname_near_the_cap_is_truncated_so_the_suffix_survives() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        // 80-char hostname plus suffix would exceed the server's 80 cap; the
        // head must be truncated to make room for `-` + 16 hex.
        let opts = LoginOptions {
            device: "x".repeat(80),
            ..login_opts(base, "https://api.example.com", false)
        };
        let resolved = resolve_device_name(base, &opts).unwrap();
        assert!(resolved.len() <= 80, "{resolved}");
        let suffix = resolved.rsplit_once('-').unwrap().1;
        assert_eq!(suffix.len(), 16, "{suffix}");
        assert!(resolved
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')));
    }

    #[test]
    fn render_login_result_includes_safe_metadata_and_no_tokens_by_default() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();
        let paths = all_connections(base)[0].paths.clone();
        let text = render_login_result(
            &paths,
            "https://api.example.com",
            "alice",
            "laptop",
            false,
            false,
            false,
        )
        .unwrap();
        assert!(text.contains("https://api.example.com/mcp"), "{text}");
        assert!(
            text.contains(&paths.user_token.display().to_string()),
            "{text}"
        );
        assert!(
            text.contains(&paths.agent_config.display().to_string()),
            "{text}"
        );
        assert!(text.contains("webcodex-runner --config"), "{text}");
        assert!(
            text.contains("GPT Actions, MCP, and REST/project APIs"),
            "{text}"
        );
        assert!(
            text.contains("Runner/Agent transport credentials only"),
            "{text}"
        );
        assert!(
            (!cfg!(windows) && text.contains("webcodex agent install --scope user --config"))
                || (cfg!(windows) && !text.contains("webcodex agent install")),
            "{text}"
        );
        assert!(
            cfg!(windows) || text.contains("same ordinary user"),
            "non-root guidance was not explicit: {text}"
        );
        assert!(
            !cfg!(windows) || text.contains(WINDOWS_AGENT_INSTALL_REASON),
            "Windows login did not explain the service-install boundary: {text}"
        );
        assert!(!text.contains(USER_TOKEN), "default text leaked a token");
        assert!(!text.contains(AGENT_TOKEN), "default text leaked a token");

        let json = render_login_result(
            &paths,
            "https://api.example.com",
            "alice",
            "laptop",
            false,
            true,
            false,
        )
        .unwrap();
        let json_value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            json_value
                .get("mcp_url")
                .and_then(serde_json::Value::as_str),
            Some("https://api.example.com/mcp")
        );
        assert!(json_value.get("credential_usage").is_some(), "{json}");
        assert_eq!(
            json_value
                .get("user_token_file")
                .and_then(serde_json::Value::as_str),
            paths.user_token.to_str()
        );
        if cfg!(windows) {
            assert_eq!(
                json_value["agent_install_available"],
                serde_json::json!(false)
            );
            assert!(json_value["agent_install_argv"].is_null());
            assert_eq!(
                json_value["agent_install_reason"],
                serde_json::json!(WINDOWS_AGENT_INSTALL_REASON)
            );
            assert_eq!(json_value["next_steps"].as_array().unwrap().len(), 1);
        }
        assert!(!json.contains(USER_TOKEN), "json leaked a token");
        assert!(!json.contains(AGENT_TOKEN), "json leaked a token");
    }

    #[test]
    fn render_login_result_quotes_config_paths_and_exposes_argv() {
        let paths = ConnectionPaths::new(PathBuf::from("/tmp/path with 'quote/$value/`tick`;semi"));
        let foreground_argv = vec![
            "webcodex-runner".to_string(),
            "--config".to_string(),
            paths.agent_config.to_string_lossy().into_owned(),
        ];
        let install_argv = vec![
            "webcodex".to_string(),
            "agent".to_string(),
            "install".to_string(),
            "--scope".to_string(),
            "user".to_string(),
            "--config".to_string(),
            paths.agent_config.to_string_lossy().into_owned(),
        ];

        let text = render_login_result(
            &paths,
            "https://api.example.com",
            "alice",
            "laptop",
            false,
            false,
            false,
        )
        .unwrap();
        assert!(text.contains(&shell_command(&foreground_argv)), "{text}");
        assert_eq!(
            text.contains(&shell_command(&install_argv)),
            !cfg!(windows),
            "{text}"
        );
        assert!(
            !cfg!(windows) || text.contains(WINDOWS_AGENT_INSTALL_REASON),
            "{text}"
        );

        let json_text = render_login_result(
            &paths,
            "https://api.example.com",
            "alice",
            "laptop",
            false,
            true,
            false,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_text).unwrap();
        assert_eq!(value["foreground_available"], serde_json::json!(true));
        assert_eq!(value["foreground_argv"], serde_json::json!(foreground_argv));
        assert!(value["foreground_reason"].is_null());
        assert_eq!(
            value["agent_install_argv"],
            if cfg!(windows) {
                serde_json::Value::Null
            } else {
                serde_json::json!(install_argv)
            }
        );
        assert_eq!(
            value["agent_install_available"],
            serde_json::json!(!cfg!(windows))
        );
        if cfg!(windows) {
            assert_eq!(
                value["agent_install_reason"],
                serde_json::json!(WINDOWS_AGENT_INSTALL_REASON)
            );
        } else {
            assert!(value["agent_install_reason"].is_null());
        }
        assert_eq!(
            value["next_steps"][0].as_str().unwrap(),
            shell_command(&foreground_argv)
        );
        assert_eq!(
            value["next_steps"]
                .get(1)
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
            if cfg!(windows) {
                String::new()
            } else {
                shell_command(&install_argv)
            }
        );
        assert!(!json_text.contains(USER_TOKEN));
        assert!(!json_text.contains(AGENT_TOKEN));

        let recommended_argv: Vec<String> = if cfg!(windows) {
            install_argv.clone()
        } else {
            serde_json::from_value(value["agent_install_argv"].clone()).unwrap()
        };
        let parser_env = tempfile::TempDir::new().unwrap();
        std::fs::write(parser_env.path().join("webcodex-runner"), "").unwrap();
        #[cfg(windows)]
        std::fs::write(parser_env.path().join("webcodex-runner.exe"), "").unwrap();
        let _guard = crate::webcodex_cli::test_support::env_test_guard();
        let _env = crate::webcodex_cli::test_support::EnvGuard::new()
            .set_os("HOME", parser_env.path().as_os_str().to_owned())
            .set_os("XDG_CONFIG_HOME", parser_env.path().as_os_str().to_owned())
            .set_os("PATH", parser_env.path().as_os_str().to_owned());
        let parsed =
            crate::parse_agent_install_service_with_identity(&recommended_argv[3..], false);
        assert!(
            parsed.is_ok(),
            "non-root recommendation was rejected by the install parser: {parsed:?}"
        );
    }

    #[test]
    fn render_login_result_omits_invalid_root_service_guidance() {
        let paths = ConnectionPaths::new(PathBuf::from("/tmp/root-login"));
        let foreground_argv = vec![
            "webcodex-runner".to_string(),
            "--config".to_string(),
            paths.agent_config.to_string_lossy().into_owned(),
        ];

        let text = render_login_result(
            &paths,
            "https://api.example.com",
            "alice",
            "laptop",
            true,
            false,
            false,
        )
        .unwrap();
        assert!(
            !text.contains("webcodex agent install --scope user"),
            "{text}"
        );
        assert!(!text.contains("--allow-root-runner"), "{text}");
        assert!(
            !text.contains("Start the agent in the foreground"),
            "{text}"
        );
        assert!(!text.contains(&shell_command(&foreground_argv)), "{text}");
        assert!(text.contains("Login ran as root"), "{text}");
        assert!(
            text.contains("ordinary local user who will run the Runner"),
            "{text}"
        );
        assert!(text.contains("fresh pairing code"), "{text}");
        assert!(
            text.contains("install the user service as that same user"),
            "{text}"
        );
        assert!(
            text.contains("Advanced system service deployment"),
            "{text}"
        );
        assert!(text.contains("--scope system --user"), "{text}");
        assert!(text.contains("read the agent config"), "{text}");
        assert!(text.contains("working directory"), "{text}");
        assert!(text.contains("projects directory"), "{text}");
        assert!(text.contains("allowed roots"), "{text}");
        assert!(!text.contains(USER_TOKEN), "root text leaked a token");
        assert!(!text.contains(AGENT_TOKEN), "root text leaked a token");

        let json_text = render_login_result(
            &paths,
            "https://api.example.com",
            "alice",
            "laptop",
            true,
            true,
            false,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_text).unwrap();
        assert_eq!(value["agent_install_available"], serde_json::json!(false));
        assert!(value["agent_install_argv"].is_null());
        let reason = value["agent_install_reason"].as_str().unwrap();
        assert!(reason.contains("login ran as root"), "{reason}");
        assert!(reason.contains("non-root Runner user"), "{reason}");
        assert!(!reason.contains("--allow-root-runner"), "{reason}");
        assert_eq!(value["foreground_available"], serde_json::json!(false));
        assert!(value["foreground_argv"].is_null());
        let foreground_reason = value["foreground_reason"].as_str().unwrap();
        assert!(foreground_reason.contains("login ran as root"));
        assert!(foreground_reason.contains("project commands as root"));
        assert_eq!(value["next_steps"], serde_json::json!([]));
        assert!(!json_text.contains("--allow-root-runner"));
        assert!(!json_text.contains("webcodex agent install --scope user"));
        assert!(!json_text.contains(USER_TOKEN), "root json leaked a token");
        assert!(!json_text.contains(AGENT_TOKEN), "root json leaked a token");
    }

    #[test]
    fn print_mcp_config_emits_the_bearer_block_and_marks_it_sensitive() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();
        let paths = all_connections(base)[0].paths.clone();
        let text = render_login_result(
            &paths,
            "https://api.example.com",
            "alice",
            "laptop",
            false,
            false,
            true,
        )
        .unwrap();
        assert!(
            text.contains("Sensitive HTTP MCP connection details"),
            "{text}"
        );
        assert!(text.contains("https://api.example.com/mcp"), "{text}");
        assert!(
            text.contains(&format!("Authorization: Bearer {USER_TOKEN}")),
            "{text}"
        );
    }

    #[test]
    fn print_mcp_config_rejects_an_agent_token_in_the_user_token_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConnectionPaths::new(temp.path().join("connection"));
        std::fs::create_dir_all(&paths.dir).unwrap();
        let secret = "wc_agent_do_not_echo_login_0123456789";
        std::fs::write(&paths.user_token, format!("{secret}\n")).unwrap();
        let error = render_login_result(
            &paths,
            "https://api.example.com",
            "alice",
            "laptop",
            false,
            false,
            true,
        )
        .unwrap_err();
        assert!(error.contains("Agent transport token"), "{error}");
        assert!(error.contains("webcodex-user-token"), "{error}");
        assert!(!error.contains(secret));
    }

    #[test]
    fn print_mcp_config_never_touches_the_recovery_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path();
        publish_login(base, "https://api.example.com", false).unwrap();
        let paths = all_connections(base)[0].paths.clone();
        std::fs::write(paths.dir.join("marker"), "old").unwrap();
        // A second login without --overwrite parks a recovery directory; the
        // recovery error must not contain a token even when print_mcp_config
        // was requested (the connection was not published cleanly).
        let opts = LoginOptions {
            overwrite: false,
            print_mcp_config: true,
            ..login_opts(base, "https://api.example.com", false)
        };
        let canonical = canonical_server_url("https://api.example.com").unwrap();
        let identity = identity();
        let parent = resolve_connection_parent(base, &canonical).unwrap();
        let paths = ConnectionPaths::new(parent.join(user_slug(&identity.username).unwrap()));
        let staging = create_staging_dir(&parent).unwrap();
        stage_connection(
            &staging,
            &paths.projects_dir,
            &opts,
            &canonical.url,
            &identity,
            &opts.device,
            "t",
        )
        .unwrap();
        match publish_connection(&staging, &paths.dir, false).unwrap() {
            PublishOutcome::SavedForRecovery { path } => {
                let message =
                    render_recovery_error(&paths.dir, &path, "https://api.example.com", "alice");
                assert!(!message.contains(USER_TOKEN), "{message}");
                assert!(!message.contains(AGENT_TOKEN), "{message}");
            }
            other => panic!("expected recovery, got {other:?}"),
        }
    }
}
