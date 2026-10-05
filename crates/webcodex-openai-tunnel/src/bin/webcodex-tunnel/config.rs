use std::{collections::BTreeMap, io::Read, net::SocketAddr, path::PathBuf, time::Duration};
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    ControlPlaneIdentity, Credential, FixedMcpTarget, TunnelClient,
};

pub const HELP: &str = "webcodex-tunnel: standalone Secure MCP Tunnel client

Usage: webcodex-tunnel [run|doctor|status] [OPTIONS]
  run       Forward one main channel until Ctrl-C / SIGTERM (default)
  doctor    Validate configuration without network requests or starting a Tunnel
  status    Read the local health endpoint; exits 0 only when polling is ready

Options (flags override environment):
  --control-plane.base-url URL       CONTROL_PLANE_BASE_URL (https://api.openai.com)
  --control-plane.tunnel-id ID       CONTROL_PLANE_TUNNEL_ID (required for run/doctor)
  --control-plane.api-key REF        env:NAME or file:PATH; otherwise CONTROL_PLANE_API_KEY
  --mcp.server-url URL               MCP_SERVER_URL (required for run/doctor)
  --mcp.authorization REF            env:NAME or file:PATH; otherwise MCP_AUTHORIZATION
  --health.listen-addr IP:PORT        HEALTH_LISTEN_ADDR (127.0.0.1:8080; run permits port 0)
  --state-dir PATH                   WEBCODEX_TUNNEL_STATE_DIR (per-user tunnel-runs directory)
  --max-response-seconds N           Optional finite cap, 1..86400; default honors wire deadlines
  --concurrency N                    1..256 (default 8)
  --json                            JSON lifecycle/status output, without credentials
  --stop-on-stdin-eof                Parent-owned run exits when its stdin closes
  --help / --version

No local MCP credential is required unless the target requires one. A supplied
MCP_AUTHORIZATION is a complete header such as Bearer <local-key>. It stays local.
Secrets in flags must use env:/file: references. No OPENAI_API_KEY fallback.
No admin, OAuth rewriting, stdio MCP, YAML profiles or arbitrary header forwarding.
Health GETs never execute MCP. /healthz is liveness; /readyz is poll readiness.
";

pub enum Action {
    Help,
    Version,
    BuildInfo,
    Run,
    Doctor,
    Status,
}
pub struct Config {
    pub action: Action,
    pub listen: SocketAddr,
    pub json: bool,
    pub stdin_eof: bool,
    pub state_dir: Option<PathBuf>,
    pub base: String,
    pub tunnel: String,
    options: BTreeMap<String, String>,
}

impl Config {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, &'static str> {
        let mut action = Action::Run;
        let mut named_action = false;
        let mut options = BTreeMap::new();
        let mut json = false;
        let mut stdin_eof = false;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    action = Action::Help;
                    break;
                }
                "--build-info-json" => {
                    action = Action::BuildInfo;
                    break;
                }
                "--version" | "-V" => {
                    action = Action::Version;
                    break;
                }
                "--json" => {
                    json = true;
                    continue;
                }
                "--stop-on-stdin-eof" => {
                    stdin_eof = true;
                    continue;
                }
                "run" | "doctor" | "status" if !named_action => {
                    named_action = true;
                    action = match arg.as_str() {
                        "doctor" => Action::Doctor,
                        "status" => Action::Status,
                        _ => Action::Run,
                    };
                    continue;
                }
                _ => {}
            }
            let (name, inline) = arg
                .split_once('=')
                .map_or((arg.as_str(), None), |(k, v)| (k, Some(v)));
            if !matches!(
                name,
                "--control-plane.base-url"
                    | "--control-plane.tunnel-id"
                    | "--control-plane.api-key"
                    | "--mcp.server-url"
                    | "--mcp.authorization"
                    | "--health.listen-addr"
                    | "--state-dir"
                    | "--max-response-seconds"
                    | "--concurrency"
            ) {
                return Err("unsupported option; see --help (argument values are not logged)");
            }
            let value = inline
                .map(str::to_string)
                .or_else(|| args.next())
                .ok_or("missing option value")?;
            if value.is_empty()
                || value.len() > 8192
                || options.insert(name.to_string(), value).is_some()
            {
                return Err("empty, repeated or oversized option");
            }
        }
        // Help and version must work without credentials, state or a valid environment.
        let informational = matches!(action, Action::Help | Action::Version | Action::BuildInfo);
        let listen = if informational {
            "127.0.0.1:8080".into()
        } else {
            setting(&options, "--health.listen-addr", "HEALTH_LISTEN_ADDR")?
                .unwrap_or_else(|| "127.0.0.1:8080".into())
        };
        let listen: SocketAddr = listen
            .parse()
            .map_err(|_| "health listener must be a numeric loopback IP:port")?;
        if !listen.ip().is_loopback() {
            return Err("health listener must be loopback");
        }
        if matches!(action, Action::Status) && listen.port() == 0 {
            return Err("status requires the actual health port, not port zero");
        }
        let base = if informational {
            "https://api.openai.com".into()
        } else {
            setting(
                &options,
                "--control-plane.base-url",
                "CONTROL_PLANE_BASE_URL",
            )?
            .unwrap_or_else(|| "https://api.openai.com".into())
        };
        let tunnel = if informational {
            String::new()
        } else {
            setting(
                &options,
                "--control-plane.tunnel-id",
                "CONTROL_PLANE_TUNNEL_ID",
            )?
            .unwrap_or_default()
        };
        let state_dir = if informational {
            None
        } else {
            setting(&options, "--state-dir", "WEBCODEX_TUNNEL_STATE_DIR")?.map(PathBuf::from)
        };
        if state_dir.as_ref().is_some_and(|p| !p.is_absolute()) {
            return Err("state directory must be absolute");
        }
        Ok(Self {
            action,
            listen,
            json,
            stdin_eof,
            state_dir,
            base,
            tunnel,
            options,
        })
    }

    pub fn client(&self) -> Result<TunnelClient, &'static str> {
        let key = secret(
            &self.options,
            "--control-plane.api-key",
            "CONTROL_PLANE_API_KEY",
        )?
        .ok_or("CONTROL_PLANE_API_KEY is required")?;
        let url = setting(&self.options, "--mcp.server-url", "MCP_SERVER_URL")?
            .ok_or("MCP_SERVER_URL is required")?;
        let cp = ControlPlaneIdentity::new(
            &self.base,
            &self.tunnel,
            Credential::bearer(&key).map_err(|_| "invalid runtime key")?,
        )
        .map_err(|_| "invalid control-plane origin or Tunnel ID")?;
        let target = match secret(&self.options, "--mcp.authorization", "MCP_AUTHORIZATION")? {
            Some(value) => FixedMcpTarget::new(
                &url,
                Credential::authorization(&value).map_err(|_| "invalid local authorization")?,
            ),
            None => FixedMcpTarget::unauthenticated(&url),
        }
        .map_err(|_| "invalid fixed MCP target URL")?;
        let policy = match self.options.get("--max-response-seconds") {
            Some(s) => DeadlinePolicy::RequireFinite {
                max_duration: Duration::from_secs(number(s, 86400)?),
            },
            None => DeadlinePolicy::ProtocolCompatible,
        };
        let mut limits = Limits::default();
        if let Some(s) = self.options.get("--concurrency") {
            limits.concurrency = number(s, 256)? as usize;
        }
        TunnelClient::new(cp, target, policy, limits).map_err(|_| "invalid Tunnel configuration")
    }
}
fn setting(
    options: &BTreeMap<String, String>,
    flag: &str,
    env: &str,
) -> Result<Option<String>, &'static str> {
    if let Some(v) = options.get(flag) {
        return Ok(Some(v.clone()));
    }
    match std::env::var(env) {
        Ok(v) if v.len() <= 8192 => Ok(Some(v)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        _ => Err("invalid environment setting"),
    }
}
fn secret(
    options: &BTreeMap<String, String>,
    flag: &str,
    env: &str,
) -> Result<Option<String>, &'static str> {
    let Some(reference) = options.get(flag) else {
        return setting(options, flag, env);
    };
    if let Some(name) = reference.strip_prefix("env:") {
        return std::env::var(name)
            .map(Some)
            .map_err(|_| "secret environment reference unavailable");
    }
    if let Some(path) = reference.strip_prefix("file:") {
        let mut value = String::new();
        std::fs::File::open(path)
            .map_err(|_| "secret file unavailable")?
            .take(8193)
            .read_to_string(&mut value)
            .map_err(|_| "secret file unreadable")?;
        if value.len() > 8192 {
            return Err("secret file too large");
        }
        return Ok(Some(value.trim_end_matches(['\r', '\n']).to_string()));
    }
    Err("secret options require env:NAME or file:PATH; use environment variables instead of literal command-line credentials")
}
fn number(value: &str, max: u64) -> Result<u64, &'static str> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= max)
        .ok_or("numeric option outside supported range")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn help_needs_no_runtime_configuration() {
        assert!(matches!(
            Config::parse(args(&["--help"])).unwrap().action,
            Action::Help
        ));
    }
    #[test]
    fn health_is_loopback_and_status_port_is_concrete() {
        for a in [
            ["run", "--health.listen-addr", "0.0.0.0:9"],
            ["status", "--health.listen-addr", "127.0.0.1:0"],
        ] {
            assert!(Config::parse(args(&a)).is_err());
        }
    }
    #[test]
    fn options_never_echo_unknown_secret_material() {
        assert!(!Config::parse(args(&["--password=private-key"]))
            .err()
            .unwrap()
            .contains("private-key"));
        assert!(Config::parse(args(&["--concurrency", "1", "--concurrency", "2"])).is_err());
        assert!(number("0", 256).is_err());
    }
}
