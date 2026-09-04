#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use tokio::process::Command;

pub fn configure_child(command: &mut Command) {
    #[cfg(target_os = "windows")]
    windows::configure_child(command);
    #[cfg(target_os = "macos")]
    macos::configure_child(command);
}

pub async fn force_stop_owned_tree(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        return windows::force_stop_owned_tree(pid).await;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = pid;
        false
    }
}

pub fn current_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "desktop".to_string())
}
