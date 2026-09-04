use super::remove_npm_wrapper_network_environment;
use std::io::IsTerminal;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::task::JoinHandle;

const CLIPBOARD_HELPER_TIMEOUT: Duration = Duration::from_secs(2);
const BROWSER_HELPER_TIMEOUT: Duration = Duration::from_secs(5);
// This deep link is convenience-only. The CLI also prints the current manual
// Settings -> Apps -> Create path because ChatGPT UI routes may change.
const CHATGPT_APP_SETTINGS_URL: &str = "https://chatgpt.com/#settings/Connectors";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClipboardCopyOutcome {
    Copied,
    Unavailable,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HelperCommand {
    program: &'static str,
    args: &'static [&'static str],
}

pub(super) fn mcp_url(public_url: &str) -> String {
    format!("{}/mcp", public_url.trim_end_matches('/'))
}

pub(super) async fn copy_text_to_clipboard(text: &str, enabled: bool) -> ClipboardCopyOutcome {
    if !enabled {
        return ClipboardCopyOutcome::Disabled;
    }
    let commands = clipboard_commands(std::env::consts::OS);
    for command in commands {
        if run_clipboard_command(&command, text).await {
            return ClipboardCopyOutcome::Copied;
        }
    }
    ClipboardCopyOutcome::Unavailable
}

pub(super) async fn copy_mcp_url(url: &str, enabled: bool) -> ClipboardCopyOutcome {
    copy_text_to_clipboard(url, enabled).await
}

pub(super) fn render_clipboard_status(outcome: ClipboardCopyOutcome) -> Option<&'static str> {
    match outcome {
        ClipboardCopyOutcome::Copied => {
            Some("MCP URL copied to clipboard. The credential was not copied.")
        }
        ClipboardCopyOutcome::Unavailable => {
            Some("Clipboard copy unavailable; copy the MCP URL above manually.")
        }
        ClipboardCopyOutcome::Disabled => None,
    }
}

pub(super) fn maybe_spawn_chatgpt_open_prompt() -> Option<JoinHandle<()>> {
    if !interactive_terminal() || !cfg!(unix) {
        return None;
    }
    println!(
        "Press Enter to open ChatGPT App settings (Settings -> Apps -> Create). Ctrl-C stops sharing."
    );
    Some(tokio::spawn(async {
        if wait_for_empty_terminal_line().await {
            if open_chatgpt_settings().await {
                println!("Opened ChatGPT App settings. Paste the MCP URL there.");
            } else {
                println!(
                    "Could not open ChatGPT automatically; open ChatGPT and go to Settings -> Apps -> Create."
                );
            }
        }
    }))
}

fn interactive_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn clipboard_commands(os: &str) -> Vec<HelperCommand> {
    match os {
        "macos" => vec![HelperCommand {
            program: "pbcopy",
            args: &[],
        }],
        "linux" => vec![
            HelperCommand {
                program: "wl-copy",
                args: &[],
            },
            HelperCommand {
                program: "xclip",
                args: &["-selection", "clipboard"],
            },
            HelperCommand {
                program: "xsel",
                args: &["--clipboard", "--input"],
            },
        ],
        "windows" => vec![HelperCommand {
            program: "clip.exe",
            args: &[],
        }],
        _ => Vec::new(),
    }
}

fn helper_command(program: &str) -> Command {
    let mut command = Command::new(program);
    remove_npm_wrapper_network_environment(&mut command);
    command
}

async fn run_clipboard_command(command: &HelperCommand, text: &str) -> bool {
    let mut child = match helper_command(command.program)
        .args(command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return false;
    };
    if stdin.write_all(text.as_bytes()).await.is_err() || stdin.shutdown().await.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return false;
    }
    drop(stdin);

    matches!(
        tokio::time::timeout(CLIPBOARD_HELPER_TIMEOUT, child.wait()).await,
        Ok(Ok(status)) if status.success()
    )
}

#[cfg(unix)]
async fn wait_for_empty_terminal_line() -> bool {
    let mut child = match helper_command("sh")
        .arg("-c")
        .arg("IFS= read -r line; [ -z \"$line\" ]")
        .stdin(Stdio::inherit())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    matches!(child.wait().await, Ok(status) if status.success())
}

#[cfg(not(unix))]
async fn wait_for_empty_terminal_line() -> bool {
    false
}

async fn open_chatgpt_settings() -> bool {
    for command in browser_commands(std::env::consts::OS) {
        let mut child = match helper_command(command.program)
            .args(command.args)
            .arg(CHATGPT_APP_SETTINGS_URL)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(_) => continue,
        };
        if matches!(
            tokio::time::timeout(BROWSER_HELPER_TIMEOUT, child.wait()).await,
            Ok(Ok(status)) if status.success()
        ) {
            return true;
        }
    }
    false
}

fn browser_commands(os: &str) -> Vec<HelperCommand> {
    match os {
        "macos" => vec![HelperCommand {
            program: "open",
            args: &[],
        }],
        "linux" => vec![
            HelperCommand {
                program: "xdg-open",
                args: &[],
            },
            HelperCommand {
                program: "gio",
                args: &["open"],
            },
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[path = "project_entry_client_handoff_tests.rs"]
mod tests;
