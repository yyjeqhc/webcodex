#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use tokio::process::Command;
use webcodex_process::SpawnOptions;

#[derive(Debug, Clone, Copy)]
pub struct OwnedProcessTree {
    root_pid: u32,
}

impl OwnedProcessTree {
    pub fn from_spawned_root(root_pid: u32) -> Option<Self> {
        if root_pid == 0 {
            return None;
        }
        #[cfg(target_os = "macos")]
        i32::try_from(root_pid).ok()?;
        Some(Self { root_pid })
    }
}

pub fn configure_child(command: &mut Command) {
    #[cfg(target_os = "windows")]
    windows::configure_child(command);
    #[cfg(target_os = "macos")]
    macos::configure_child(command);
}

pub fn managed_spawn_options() -> SpawnOptions {
    #[cfg(target_os = "windows")]
    {
        return windows::managed_spawn_options();
    }
    #[cfg(not(target_os = "windows"))]
    {
        SpawnOptions::new()
    }
}

pub async fn terminate_owned_tree(tree: OwnedProcessTree) -> bool {
    #[cfg(target_os = "windows")]
    {
        return windows::force_stop_owned_tree(tree.root_pid).await;
    }
    #[cfg(target_os = "macos")]
    {
        return macos::terminate_owned_tree(tree.root_pid);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = tree;
        false
    }
}

pub async fn force_stop_owned_tree(tree: OwnedProcessTree) -> bool {
    #[cfg(target_os = "windows")]
    {
        return windows::force_stop_owned_tree(tree.root_pid).await;
    }
    #[cfg(target_os = "macos")]
    {
        return macos::force_stop_owned_tree(tree.root_pid);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = tree;
        false
    }
}

#[cfg(target_os = "macos")]
pub fn owned_tree_is_running(tree: OwnedProcessTree) -> bool {
    macos::owned_tree_is_running(tree.root_pid)
}

pub fn current_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "desktop".to_string())
}
