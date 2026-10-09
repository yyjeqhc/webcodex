use super::config::Config;
use std::path::PathBuf;

pub struct Guard(webcodex_openai_tunnel::run_fence::RunFence);
impl Guard {
    pub fn acquire(config: &Config) -> Result<Self, &'static str> {
        let root = config
            .state_dir
            .clone()
            .map(Ok)
            .unwrap_or_else(default_root)?;
        let origin =
            reqwest::Url::parse(&config.base).map_err(|_| "invalid control-plane origin")?;
        webcodex_openai_tunnel::run_fence::RunFence::acquire(&root, origin.as_str(), &config.tunnel)
            .map(Self)
            .map_err(|code| match code {
                "tunnel_owner_active" => "Tunnel is already running; stop its exact owner before recovery",
                "tunnel_restart_uncertain" => "Tunnel is already running or its previous shutdown is unconfirmed; diagnose its exact restart fence before recovery",
                _ => "Tunnel run ownership could not be verified; preserve its restart fence and inspect the private state directory",
            })
    }
    pub fn finish(self, unsettled: bool) -> Result<(), &'static str> {
        if unsettled {
            return Err("Tunnel stopped with unconfirmed work; run marker retained");
        }
        self.0.finish()
    }
}
fn default_root() -> Result<PathBuf, &'static str> {
    webcodex_openai_tunnel::run_fence::default_root()
}
