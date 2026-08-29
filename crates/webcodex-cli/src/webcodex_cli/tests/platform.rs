//! Windows platform-boundary tests: unsupported operations fail with a clear
//! message before any server/service logic runs, and help still renders.
//!
//! The guard is Windows-only by design; on Unix these commands remain valid.

#[cfg(windows)]
mod windows_guard {
    use crate::windows_unsupported_platform_action;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn managed_server_lifecycle_fails_closed_on_windows() {
        for command in [
            vec!["server"],
            vec!["server", "install"],
            vec!["server", "start"],
            vec!["server", "stop"],
            vec!["server", "restart"],
            vec!["server", "logs"],
            vec!["server", "uninstall", "--confirm"],
        ] {
            let message = windows_unsupported_platform_action(&args(&command))
                .expect("managed server lifecycle must be blocked on Windows");
            assert!(
                message.contains("service-managed Server lifecycle"),
                "{command:?}: {message}"
            );
            assert!(
                message.contains("webcodex server run"),
                "{command:?}: {message}"
            );
            assert!(
                !message.contains("Server runtime is not supported"),
                "{command:?}: {message}"
            );
        }
    }

    #[test]
    fn runner_install_fails_closed_on_windows() {
        for command in [
            vec!["runner", "install"],
            vec!["runner", "install", "--scope", "user"],
        ] {
            let message = windows_unsupported_platform_action(&args(&command))
                .expect("runner install must be blocked on Windows");
            assert!(
                message.contains("Automatic Windows Runner startup is not supported"),
                "{command:?}: {message}"
            );
            assert!(
                message.contains("webcodex runner start --profile"),
                "{command:?}: {message}"
            );
        }
    }

    #[test]
    fn help_still_renders_for_blocked_commands() {
        for command in [
            vec!["server", "--help"],
            vec!["server", "run", "-h"],
            vec!["share", "--help"],
            vec!["runner", "install", "--help"],
        ] {
            assert!(
                windows_unsupported_platform_action(&args(&command)).is_none(),
                "{command:?} must still render help"
            );
        }
    }

    #[test]
    fn supported_windows_commands_are_not_blocked() {
        for command in [
            vec!["server", "init"],
            vec!["server", "run"],
            vec!["server", "run", "--env-file", "C:\\temp\\webcodex.env"],
            vec!["server", "status"],
            vec!["share"],
            vec!["share", "--tunnel", "cloudflare"],
            vec!["share", "--tunnel", "openai"],
            vec!["share", "--tunnel", "none"],
            vec!["connect", "https://server.example.com"],
            vec!["login", "https://server.example.com", "--code", "wc_pair_x"],
            vec!["runner", "status"],
            vec!["runner", "start", "--profile", "demo"],
            vec!["runner", "stop", "--profile", "demo"],
            vec!["runner", "restart", "--profile", "demo"],
            vec!["runner", "logs", "--profile", "demo"],
            vec!["agent-tokens"],
            vec!["status"],
            vec!["doctor"],
        ] {
            assert!(
                windows_unsupported_platform_action(&args(&command)).is_none(),
                "{command:?} is part of the supported Windows surface"
            );
        }
    }
}
