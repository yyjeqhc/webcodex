use super::*;

#[test]
fn mcp_url_is_derived_only_from_public_base() {
    assert_eq!(
        mcp_url("https://demo.trycloudflare.com"),
        "https://demo.trycloudflare.com/mcp"
    );
    assert_eq!(
        mcp_url("https://share.example/"),
        "https://share.example/mcp"
    );
    assert!(!mcp_url("https://share.example").contains("credential"));
}

#[test]
fn clipboard_backends_are_platform_bounded() {
    assert_eq!(
        clipboard_commands("macos"),
        vec![HelperCommand {
            program: "pbcopy",
            args: &[]
        }]
    );
    assert_eq!(
        clipboard_commands("linux"),
        vec![
            HelperCommand {
                program: "wl-copy",
                args: &[]
            },
            HelperCommand {
                program: "xclip",
                args: &["-selection", "clipboard"]
            },
            HelperCommand {
                program: "xsel",
                args: &["--clipboard", "--input"]
            }
        ]
    );
    assert_eq!(
        clipboard_commands("windows"),
        vec![HelperCommand {
            program: "clip.exe",
            args: &[]
        }]
    );
}

#[test]
fn browser_backends_open_only_the_fixed_chatgpt_settings_target() {
    assert_eq!(
        CHATGPT_APP_SETTINGS_URL,
        "https://chatgpt.com/#settings/Connectors"
    );
    assert_eq!(
        browser_commands("macos"),
        vec![HelperCommand {
            program: "open",
            args: &[]
        }]
    );
    assert_eq!(
        browser_commands("linux"),
        vec![
            HelperCommand {
                program: "xdg-open",
                args: &[]
            },
            HelperCommand {
                program: "gio",
                args: &["open"]
            }
        ]
    );
    assert!(browser_commands("windows").is_empty());
}

#[test]
fn helper_children_remove_npm_wrapper_network_environment() {
    let command = helper_command("webcodex-helper");
    let envs: Vec<_> = command.as_std().get_envs().collect();
    for key in [
        "npm_config_https_proxy",
        "npm_config_proxy",
        "npm_config_noproxy",
        "npm_config_no_proxy",
        "npm_config_cafile",
        "npm_config_ca",
        "npm_config_strict_ssl",
        "WEBCODEX_NPM_WRAPPER",
    ] {
        assert!(
            envs.iter()
                .any(|(candidate, value)| candidate.to_str() == Some(key) && value.is_none()),
            "helper child did not remove wrapper-only environment key {key}"
        );
    }
}

#[test]
fn clipboard_status_never_claims_the_credential_was_copied() {
    assert_eq!(
        render_clipboard_status(ClipboardCopyOutcome::Copied),
        Some("MCP URL copied to clipboard. The credential was not copied.")
    );
    assert_eq!(
        render_clipboard_status(ClipboardCopyOutcome::Unavailable),
        Some("Clipboard copy unavailable; copy the MCP URL above manually.")
    );
    assert_eq!(
        render_clipboard_status(ClipboardCopyOutcome::Disabled),
        None
    );
}

#[tokio::test]
async fn disabled_clipboard_copy_has_no_helper_side_effect() {
    assert_eq!(
        copy_mcp_url("https://demo.trycloudflare.com/mcp", false).await,
        ClipboardCopyOutcome::Disabled
    );
}

#[cfg(unix)]
#[tokio::test]
async fn clipboard_helper_receives_exact_mcp_url_and_nothing_else() {
    let command = HelperCommand {
        program: "sh",
        args: &[
            "-c",
            "payload=$(cat); [ \"$payload\" = \"https://demo.trycloudflare.com/mcp\" ]",
        ],
    };
    assert!(
        run_clipboard_command(&command, "https://demo.trycloudflare.com/mcp").await,
        "the helper should accept exactly the MCP URL"
    );
    assert!(
        !run_clipboard_command(
            &command,
            "https://demo.trycloudflare.com/mcp\nwebcodex_temporary-secret"
        )
        .await,
        "adding any credential-like payload must no longer satisfy the exact helper contract"
    );
}
