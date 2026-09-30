//! Command wrappers, tokens, and synchronized output framing.

use super::*;

#[cfg(any(unix, windows))]
pub(super) fn drain_sync_receiver(
    receiver: &Mutex<mpsc::Receiver<String>>,
    token: &str,
    synced: &mut bool,
) -> bool {
    let receiver = lock_unpoison(receiver);
    loop {
        match receiver.try_recv() {
            Ok(received) if received == token => *synced = true,
            Ok(_) => {}
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => return true,
        }
    }
}

#[cfg(all(test, windows))]
thread_local! {
    static TEST_COMMAND_TOKEN: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(all(test, windows))]
pub(super) fn set_test_command_token(token: Option<&str>) {
    TEST_COMMAND_TOKEN.with(|slot| {
        *slot.borrow_mut() = token.map(str::to_string);
    });
}

pub(super) fn command_token() -> String {
    #[cfg(all(test, windows))]
    if let Some(token) = TEST_COMMAND_TOKEN.with(|slot| slot.borrow().clone()) {
        return token;
    }
    Uuid::new_v4().simple().to_string()
}

/// Build the command wrapper for a remote shell that has reserved FD 7 (a dup
/// of the SSH channel's original stdout) and FD 8 (a dup of the original
/// stderr) at startup. Because an SSH exec channel has no extra control FD,
/// the control frame travels inline on the reserved stderr (FD 8) right after
/// the stderr sync marker.
///
/// Frame layout written after the user command (same magic + NUL framing as the
/// local shell, so the manager's marker parsing is shared):
///   - `WCPSO1\0{token}\0`            -> FD 7   (stdout sync boundary)
///   - `WCPSE1\0{token}\0`            -> FD 8   (stderr sync boundary)
///   - `WCPS1\0{token}\0{status}\0`   -> FD 8   (control frame: exit status)
///   - `pwd -P` output                -> FD 8   (absolute cwd)
///   - `\0`                           -> FD 8   (control frame terminator)
///
/// The remote shell's own `printf`/`pwd` builtins are used. User redirects
/// (`exec 2>&1`, etc.) cannot move the protocol targets because FD 7/8 are
/// reserved at startup, not bound to the current stdout/stderr.
pub fn remote_command_wrapper(command: &str, token: &str) -> String {
    let status_variable = format!("__wc_ps_status_{token}");
    let framed = format!(
        "\\eval {}\n\
         {status_variable}=$?\n\
         printf 'WCPSO1\\000{}\\000' >&{}\n\
         printf 'WCPSE1\\000{}\\000' >&{}\n\
         printf 'WCPS1\\000{}\\000%s\\000' \"${status_variable}\" >&{}\n\
         pwd -P >&{}\n\
         printf '\\000' >&{}\n",
        shell_quote(command),
        token,
        STDOUT_SYNC_FD,
        token,
        STDERR_SYNC_FD,
        token,
        STDERR_SYNC_FD,
        STDERR_SYNC_FD,
        STDERR_SYNC_FD,
    );
    // Same eval/alias-suppression hardening as the local command_wrapper.
    format!("\\eval {}\n", shell_quote(&framed))
}

/// Single-quote a value for a POSIX shell, escaping embedded quotes. Reused by
/// remote transports so the trusted postamble stays in one resolved builtin.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
pub(super) fn command_wrapper(command: &str, token: &str, printf: &str, pwd: &str) -> String {
    let status_variable = format!("__wc_ps_status_{token}");
    let framed = format!(
        "\\eval {}\n\
         {status_variable}=$?\n\
         {} 'WCPSO1\\000{}\\000' >&{}\n\
         {} 'WCPSE1\\000{}\\000' >&{}\n\
         {} 'WCPS1\\000{}\\000%s\\000' \"${status_variable}\" >&{}\n\
         {} -P >&{}\n\
         {} '\\000' >&{}\n",
        shell_quote(command),
        shell_quote(printf),
        token,
        STDOUT_SYNC_FD,
        shell_quote(printf),
        token,
        STDERR_SYNC_FD,
        shell_quote(printf),
        token,
        CONTROL_FD,
        shell_quote(pwd),
        CONTROL_FD,
        shell_quote(printf),
        CONTROL_FD,
    );
    // The outer eval keeps the trusted postamble in the same already-resolved
    // builtin invocation as the user command. The inner eval gives malformed
    // command text a normal non-zero completion instead of corrupting the
    // shell parser. A leading backslash suppresses user aliases.
    format!("\\eval {}\n", shell_quote(&framed))
}

#[cfg(any(unix, windows))]
pub(super) fn process_output_pending(
    pending: &mut Vec<u8>,
    buffer: &Arc<Mutex<BoundedBuffer>>,
    expected_token: &Arc<Mutex<Option<String>>>,
    sync_sender: &mpsc::SyncSender<String>,
    sync_magic: &[u8],
    last_synced_token: &mut Option<String>,
) {
    let expected = lock_unpoison(expected_token).clone();
    let Some(token) = expected else {
        lock_unpoison(buffer).append(pending);
        pending.clear();
        return;
    };
    if last_synced_token.as_deref() == Some(token.as_str()) {
        lock_unpoison(buffer).append(pending);
        pending.clear();
        return;
    }
    let marker = output_sync_marker(sync_magic, &token);
    if let Some(position) = find_bytes(pending, &marker) {
        lock_unpoison(buffer).append(&pending[..position]);
        pending.drain(..position + marker.len());
        let _ = sync_sender.send(token.clone());
        *last_synced_token = Some(token);
        lock_unpoison(buffer).append(pending);
        pending.clear();
        return;
    }
    let retained = longest_suffix_prefix(pending, &marker);
    let emit = pending.len().saturating_sub(retained);
    if emit > 0 {
        lock_unpoison(buffer).append(&pending[..emit]);
        pending.drain(..emit);
    }
}

pub fn output_sync_marker(magic: &[u8], token: &str) -> Vec<u8> {
    let mut marker = Vec::with_capacity(magic.len() + token.len() + 2);
    marker.extend_from_slice(magic);
    marker.push(0);
    marker.extend_from_slice(token.as_bytes());
    marker.push(0);
    marker
}

pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    (!needle.is_empty() && haystack.len() >= needle.len())
        .then(|| {
            haystack
                .windows(needle.len())
                .position(|window| window == needle)
        })
        .flatten()
}

pub fn longest_suffix_prefix(value: &[u8], marker: &[u8]) -> usize {
    let max = value.len().min(marker.len().saturating_sub(1));
    (1..=max)
        .rev()
        .find(|length| value[value.len() - length..] == marker[..*length])
        .unwrap_or(0)
}

pub fn canonical_dialect(program: &str) -> Option<&'static str> {
    let basename = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase();
    match basename.as_str() {
        "sh" => Some("sh"),
        "bash" => Some("bash"),
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => Some("powershell"),
        _ => None,
    }
}
