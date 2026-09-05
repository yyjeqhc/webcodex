#[cfg(target_os = "windows")]
mod windows;

use webcodex_process::SpawnOptions;

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

pub fn current_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "desktop".to_string())
}
