pub(crate) fn usage() -> &'static str {
    "Usage: webcodex <COMMAND>\n\n\
Unified command-line interface for WebCodex.\n\n\
Project:\n\
  setup                         Configure the current Git project\n\
  doctor                        Diagnose project readiness\n\
  status                        Show concise project coding readiness\n\
  run                           Run the current project runtime and local Agent\n\
  share                         Temporarily share the local project over HTTPS\n\n\
Account (quick start):\n\
  connect                       Connect a local project to a hosted Server\n\
  disconnect                    Disconnect a local project from its hosted Server\n\
  login                         Log this device into a server (one-time pairing code)\n\
  logout                        Remove this device's credentials\n\
  auth status                   Show login status\n\n\
Server:\n\
  server init|install|run|start|stop|restart|status|logs|uninstall\n\
                                Configure and manage the Server service\n\n\
Agent:\n\
  agent init|install|run|start|stop|restart|status|logs|uninstall\n\
                                Configure and manage the standalone Agent service\n\n\
Operations:\n\
  task                          Review tasks and make host-local decisions\n\
  ops status|agents|projects|smoke-preflight\n\
                                Read-only operator workflow checks\n\n\
Advanced / Compatibility:\n\
  pairing create                Create a client enrollment code\n\
  client enroll                 Enroll this machine (advanced; prefer `webcodex login`)\n\
  users create|list             Manage users\n\
  tokens create|create-local|generate|register-hash|list|revoke\n\
                                Manage personal API tokens\n\
  agent-tokens create|create-local|register-hash|list|revoke\n\
                                Manage Agent tokens\n\
  setup single-user             Run the existing single-user bootstrap flow\n\n\
Options:\n\
  -h, --help                    Print help and exit\n\
  -V, --version                 Print version and exit\n"
}

pub(crate) fn connect_usage() -> &'static str {
    "Usage: webcodex connect <SERVER_URL> [OPTIONS]\n\n\
Connect a local project to a hosted WebCodex Server with one shared key.\n\
The command writes a reusable local profile, starts one background Runner,\n\
and waits until the Runner and project are visible through the Server.\n\n\
Options:\n\
  --proxy http://HOST:PORT   Override standard proxy environment for CLI Server requests\n\
  --no-system-proxy          Ignore proxy environment and connect directly\n\
  --key KEY                  Shared key (use --key-file to avoid shell history)\n\
  --key-file PATH            Read the shared key from a file\n\
  --project PATH             Local project directory [default: .]\n\
  --profile NAME             Override the derived local profile name\n\
  --client-id ID             Override the persistent Runner client id\n\
  --project-id ID            Override the derived project id\n\
  -h, --help                 Print help and exit\n\n\
When neither --key nor --key-file is supplied, a strong key is generated and\n\
printed once. Hosted shared keys must not start with wc_; managed credentials\n\
use `webcodex login` instead. Proxy flags apply only to this command's Server\n\
HTTP probes; Runner proxy configuration remains independent.\n"
}

pub(crate) fn disconnect_usage() -> &'static str {
    "Usage: webcodex disconnect [OPTIONS]\n\n\
Disconnect exactly one local repository registered by hosted `webcodex connect`.\n\
The source repository and .git are never removed or modified. A live Runner is\n\
unregistered through the Server first; an offline hosted Runner is updated locally.\n\n\
Options:\n\
  --project PATH             Local project directory [default: .]\n\
  --profile NAME             Select an exact hosted profile when more than one matches\n\
  -h, --help                 Print help and exit\n"
}

pub(crate) fn pairing_usage() -> &'static str {
    "Usage: webcodex pairing <COMMAND>\n\n\
     Commands:\n\
       create       Create a short-lived pairing code for client enrollment\n"
}

pub(crate) fn pairing_create_usage() -> &'static str {
    "Usage: webcodex pairing create --server-url URL --username USER [--client-id CLIENT_ID] [OPTIONS]\n\n\
     Options:\n\
       --server-url URL          WebCodex server URL\n\
       --proxy http://HOST:PORT Explicit proxy override for this CLI request\n\
       --no-system-proxy        Ignore proxy environment and connect directly\n\
       --env-file PATH           Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH         Read bootstrap/admin bearer token from file\n\
       --token TOKEN             Bootstrap/admin bearer token (discouraged in shell history)\n\
       --username USER           User to ensure/create for enrollment\n\
       --client-id CLIENT_ID     Optional device binding; omit to let the login device claim it.\n\
                                 When set, login must use the same --device value.\n\
       --display-name NAME       Optional display name for a newly created user\n\
       --ttl-secs SECS           Pairing code lifetime [default: 600; range: 60..3600]\n\
       --user-token-name NAME    Name for the user API token created during enroll\n\
       --agent-token-name NAME   Name for the agent token created during enroll\n\
       --json                    Print machine-readable output\n\
       -h, --help                Print help and exit\n\n\
     Server/admin-side command:\n\
       pairing create needs server bootstrap/admin auth. The default server\n\
       bootstrap env file lives on the server, not the client.\n\
       On the client, redeem the code with: webcodex login <server-url> --code <code>\n\
       If --client-id was specified, append the matching\n\
       --device <client-id> to that login command.\n\
       (advanced clients may use: webcodex client enroll)\n\n\
     Copy only the short-lived wc_pair_* code to the client. Do not copy\n\
     WEBCODEX_TOKEN, wc_pat_*, or wc_agent_* values from server to client.\n\
     This command does not create wc_pat_* or wc_agent_* token files on the\n\
     server.\n"
}

pub(crate) fn client_usage() -> &'static str {
    "Usage: webcodex client <COMMAND>\n\n\
     Commands:\n\
       enroll       Enroll this client using a temporary pairing code\n"
}

pub(crate) fn client_enroll_usage() -> &'static str {
    "Usage: webcodex client enroll --server-url URL --pairing-code CODE --client-id CLIENT_ID [OPTIONS]\n\n\
     Advanced / compatibility entry. Ordinary users should use\n\
     `webcodex login <server-url> --code <code>`, which derives the client id\n\
     and writes the same token files in one step.\n\n\
     Options:\n\
       --server-url URL              WebCodex server URL\n\
       --proxy http://HOST:PORT     Explicit proxy override for this CLI request\n\
       --no-system-proxy            Ignore proxy environment and connect directly\n\
       --pairing-code CODE           Temporary one-time pairing code\n\
       --client-id CLIENT_ID         Client id matching the pairing record\n\
       --display-name NAME           Optional agent display name\n\
       --transport websocket|polling|quic|auto Agent transport [default: websocket]\n\
       --profile NAME                Client config profile [default: client-id]\n\
       --output-dir DIR              Output dir [default: root /etc/webcodex/clients/<profile>; user ~/.config/webcodex/clients/<profile>]\n\
       --agent-config PATH           Agent config path [default: <output-dir>/agent.toml]\n\
       --projects-dir PATH           Projects registry dir [default: <output-dir>/projects.d]\n\
       --allowed-root PATH           Repeatable allowed project root\n\
       --allow-cwd-anywhere BOOL     Allow cwd outside allowed roots [default: false]\n\
       --overwrite                   Replace existing token/config files\n\
       --json                        Print machine-readable output without full tokens\n\
       -h, --help                    Print help and exit\n\n\
     Enroll receives wc_pat_* and wc_agent_* tokens over HTTPS and writes them\n\
     locally with 0600 permissions. Explicit --output-dir overrides the\n\
     profile-derived default. It never sends an Authorization header.\n"
}

pub(crate) fn ops_usage() -> &'static str {
    "Usage: webcodex ops <COMMAND>\n\n\
     Read-only operator workflow checks for WebCodex.\n\n\
     Commands:\n\
       status                  Summarize runtime, tools, jobs, agents, and projects\n\
       agents                  Show compact agent fleet status\n\
       projects                Show compact project inventory and smoke suitability\n\
       smoke-preflight         Check a project before deploy smoke validation\n\n\
     Common flags:\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --proxy http://HOST:PORT Explicit proxy override for Server requests\n\
       --no-system-proxy       Ignore proxy environment and connect directly\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       -h, --help              Print help and exit\n\n\
     These commands are read-only. They do not run jobs, start shell commands,\n\
     create sessions, write files, or print token/env values.\n"
}

pub(crate) fn ops_status_usage() -> &'static str {
    "Usage: webcodex ops status [OPTIONS]\n\n\
     Summarize runtime, tools, jobs, agents, and project health.\n\n\
     Options:\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --proxy http://HOST:PORT Explicit proxy override for Server requests\n\
       --no-system-proxy       Ignore proxy environment and connect directly\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       --strict                Exit 2 when the ops report status is FAIL\n\
       -h, --help              Print help and exit\n"
}

pub(crate) fn ops_agents_usage() -> &'static str {
    "Usage: webcodex ops agents [OPTIONS]\n\n\
     Show compact read-only agent fleet status.\n\n\
     Options:\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --proxy http://HOST:PORT Explicit proxy override for Server requests\n\
       --no-system-proxy       Ignore proxy environment and connect directly\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       --strict                Exit 2 when the ops report status is FAIL\n\
       -h, --help              Print help and exit\n"
}

pub(crate) fn ops_projects_usage() -> &'static str {
    "Usage: webcodex ops projects [OPTIONS]\n\n\
     Show compact read-only project inventory and smoke suitability.\n\n\
     Options:\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --proxy http://HOST:PORT Explicit proxy override for Server requests\n\
       --no-system-proxy       Ignore proxy environment and connect directly\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       --strict                Exit 2 when the ops report status is FAIL\n\
       -h, --help              Print help and exit\n"
}

pub(crate) fn ops_smoke_preflight_usage() -> &'static str {
    "Usage: webcodex ops smoke-preflight --project PROJECT_ID [OPTIONS]\n\n\
     Read-only deploy smoke preflight for one project.\n\n\
     Options:\n\
       --project PROJECT_ID    Runtime project id to check\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --proxy http://HOST:PORT Explicit proxy override for Server requests\n\
       --no-system-proxy       Ignore proxy environment and connect directly\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       --strict                Exit 2 when the ops report status is FAIL\n\
       -h, --help              Print help and exit\n\n\
     This command calls only read-only status/project/workspace inspection APIs.\n"
}

pub(crate) fn server_usage() -> &'static str {
    "Usage: webcodex server <COMMAND>\n\n\
Commands:\n\
  init        Initialize or update Server configuration\n\
  install     Install, enable, and start the systemd service\n\
  run         Run webcodex-server directly in the foreground\n\
  start       Start the installed service\n\
  stop        Stop the installed service\n\
  restart     Restart and verify the installed service\n\
  status      Check systemd, HTTP reachability, and build revisions\n\
  logs        Read bounded journal logs or explicitly follow them\n\
  uninstall   Remove only the systemd unit; requires --confirm\n"
}

pub(crate) fn server_init_usage() -> &'static str {
    "Usage: webcodex server init [OPTIONS]\n\n\
Options:\n\
  --listen ADDR          Listen address [default: 127.0.0.1:8080]\n\
  --data-dir PATH        Data directory\n\
  --env-file PATH        Server env file\n\
  --public-url URL       Optional public URL\n\
  --open                 Allow anonymous access for trusted demos\n\
  --overwrite            Update an existing env file while preserving its token\n\
  --json                 Print a summary without the full token\n\
  -h, --help             Print help and exit\n\n\
Shared-key mode is enabled. The full bootstrap token is saved only in the env file.\n"
}

pub(crate) fn server_install_service_usage() -> &'static str {
    "Usage: webcodex server install [OPTIONS]\n\n\
Options:\n\
  --env-file PATH             EnvironmentFile= path\n\
  --bin PATH                  webcodex-server path; sibling then absolute PATH by default\n\
  --service-file PATH         Unit path [default: /etc/systemd/system/webcodex.service]\n\
  --user USER                 Optional systemd User=\n\
  --group GROUP               Optional systemd Group=\n\
  --working-directory PATH    WorkingDirectory=\n\
  --overwrite                 Replace an existing unit\n\
  --no-start                  Enable without starting immediately\n\
  --dry-run                   Render only; never call systemctl\n\
  --output -                  Render only; never call systemctl\n\
  --json                      Print machine-readable output\n\
  -h, --help                  Print help and exit\n\n\
Normal execution runs daemon-reload, enable --now (or enable with --no-start), and verifies state. Tokens are never inlined.\n"
}

pub(crate) fn server_status_usage() -> &'static str {
    "Usage: webcodex server status [OPTIONS]\n\n\
     Options:\n\
       --url URL              Runtime URL [default: http://127.0.0.1:8080]\n\
       --proxy http://HOST:PORT Explicit proxy override for Server requests\n\
       --no-system-proxy      Ignore proxy environment and connect directly\n\
       --env-file PATH        Read WEBCODEX_TOKEN from env file [default: root /etc/webcodex/webcodex.env; user ~/.config/webcodex/webcodex.env]\n\
       --token-file PATH      Read bearer token from file\n\
       --json                 Print a machine-readable summary\n\
       -h, --help             Print help and exit\n\n\
     Token priority: --token-file, WEBCODEX_TOKEN from --env-file, process\n\
     WEBCODEX_TOKEN, then no token for auth-disabled servers.\n"
}

pub(crate) fn agent_usage() -> &'static str {
    "Usage: webcodex agent <COMMAND>\n\n\
Commands:\n\
  init        Generate an agent.toml config\n\
  install     Install, enable, and start the webcodex-runner service\n\
  run         Run webcodex-runner directly in the foreground\n\
  start       Start a hosted background Runner or installed profile service\n\
  stop        Stop a hosted background Runner or installed profile service\n\
  restart     Restart a hosted background Runner or installed profile service\n\
  status      Check Runner lifecycle, safe config metadata, and connectivity\n\
  logs        Read hosted Runner logs or the installed service journal\n\
  uninstall   Remove only the systemd unit; requires --confirm\n\n\
Service commands accept --scope user|system. Non-root users default to user; root defaults to system.\n\
Profiles created by `connect` keep their detached-process behavior when --scope is omitted.\n\n\
`webcodex run` is the current-project runtime coordinator. `webcodex agent run` directly executes the standalone Runner.\n"
}
pub(crate) fn agent_init_usage() -> &'static str {
    "Usage: webcodex agent init --server-url URL [--token TOKEN|--token-file PATH] --client-id ID --owner USER [OPTIONS]\n\n\
     Options:\n\
       --server-url URL           WebCodex server URL\n\
       --token TOKEN              Agent token for generated config\n\
       --token-file PATH          Read agent token from file\n\
       --client-id ID             Stable agent client id\n\
       --profile NAME             Client config profile [default: client-id when deriving defaults]\n\
       --owner USER               Owner username\n\
       --display-name NAME        Human-readable agent name\n\
       --transport NAME           websocket (default), polling, quic, or auto\n\
       --poll-interval-ms N       Polling interval, default 1000\n\
       --projects-dir PATH        Project config directory [default: profile projects.d]\n\
       --allowed-root PATH        Allowed project/root path; repeatable\n\
       --allow-cwd-anywhere BOOL  Allow cwd outside allowed_roots; default false\n\
       --output PATH|-            Output config path, or '-' for stdout [default: profile agent.toml]\n\
       --overwrite                Replace an existing output file\n\
       -h, --help                 Print help and exit\n\n\
     With --profile, missing output/projects-dir paths are derived under\n\
     /etc/webcodex/clients/<profile> for root or\n\
     ~/.config/webcodex/clients/<profile> for non-root users. Explicit path\n\
     flags override profile-derived defaults.\n"
}

pub(crate) fn agent_install_service_usage() -> &'static str {
    "Usage: webcodex agent install [--profile NAME] [--config PATH] [OPTIONS]\n\n\
Options:\n\
  --profile NAME             Profile for config and unit defaults\n\
  --scope user|system        Service manager scope [default: user for non-root; system for root]\n\
  --config PATH              Agent config path\n\
  --bin PATH                 webcodex-runner path; sibling then absolute PATH by default\n\
  --service-file PATH        Unit path [default: webcodex-runner[-<profile>].service]\n\
  --working-directory PATH   WorkingDirectory= [default: selected user's home]\n\
  --user USER                Required non-root system service user\n\
  --group GROUP              Optional system service Group=\n\
  --allow-root-runner        Explicitly allow project commands to run as root\n\
  --overwrite                Replace an existing unit\n\
  --no-start                 Enable without starting immediately\n\
  --dry-run                  Render only; never call systemctl\n\
  --output -                 Render only; never call systemctl\n\
  --json                     Print machine-readable output\n\
  -h, --help                 Print help and exit\n\n\
User scope uses systemctl --user, the XDG user unit directory, default.target,\n\
and never writes User= or Group=. System scope uses /etc/systemd/system,\n\
multi-user.target, and requires --user unless --allow-root-runner is explicit.\n\
The unit runs webcodex-runner --config <config>. Tokens are never inlined.\n"
}
pub(crate) fn agent_status_usage() -> &'static str {
    "Usage: webcodex agent status [OPTIONS]\n\n\
     Options:\n\
       --profile NAME             Client config profile for config/token defaults\n\
       --scope user|system        Service manager scope [default: user for non-root; system for root]\n\
       --config PATH              Agent config path [default: scope-specific agent.toml]\n\
       --service-file PATH        Override the scope-specific systemd unit path\n\
       --server-url URL           Override server URL for runtime checks\n\
       --proxy http://HOST:PORT  Explicit proxy override for Server checks\n\
       --no-system-proxy         Ignore proxy environment and connect directly\n\
       --user-token-file PATH     Read user API token for /api/runtime/status\n\
       --agent-token-file PATH    Read agent token for boundary check\n\
       --json                     Print a machine-readable summary\n\
       -h, --help                 Print help and exit\n\n\
     User scope derives config under $XDG_CONFIG_HOME/webcodex (or\n\
     $HOME/.config/webcodex) and units under $XDG_CONFIG_HOME/systemd/user\n\
     (or $HOME/.config/systemd/user). System scope uses /etc/webcodex and\n\
     /etc/systemd/system. Explicit path flags override profile-derived defaults.\n\
     Profiles created by `connect` report their detached process when --scope\n\
     is omitted; an explicit scope checks systemd instead. Status prints safe metadata only:\n\
     no tokens, Authorization headers, full agent.toml, env files, or secrets.\n"
}

pub(crate) fn login_usage() -> &'static str {
    "Usage: webcodex login <SERVER-URL> --code <PAIRING-CODE> [OPTIONS]\n\n\
     Log this device into a WebCodex server. Ask whoever runs the server for a\n\
     pairing code (`webcodex pairing create`), then run this. This is the\n\
     primary way to connect a machine.\n\n\
     Options:\n\
     \x20\x20--code CODE          Pairing code from the server (required)\n\
     \x20\x20--proxy http://HOST:PORT Explicit proxy override for this CLI request\n\
     \x20\x20--no-system-proxy   Ignore proxy environment and connect directly\n\
     \x20\x20--device NAME        Name for this device [default: hostname + local suffix]\n\
     \x20\x20--allowed-root PATH  Repeatable project root the agent may touch\n\
     \x20\x20--transport NAME     websocket|polling|quic|auto [default: websocket]\n\
     \x20\x20--dir PATH           Where connections are stored [default: root /etc/webcodex;\n\
     \x20\x20                       non-root ~/.config/webcodex]\n\
     \x20\x20--overwrite          Replace an existing login for this server and user\n\
     \x20\x20--json               Print machine-readable output (no credentials)\n\
     \x20\x20--print-mcp-config   Print a Bearer MCP connection block (includes a\n\
     \x20\x20                       credential); mutually exclusive with --json\n\
     \x20\x20-h, --help           Print help and exit\n\n\
     Credentials are written to <dir>/<server>/<user>/ with restrictive\n\
     permissions (0600 on Unix).\n\
     The same user can be logged in on several servers, and several users can\n\
     be logged in on one server; each is a separate directory.\n"
}

pub(crate) fn logout_usage() -> &'static str {
    "Usage: webcodex logout <SERVER-URL> [OPTIONS]\n\n\
     Remove this device's stored credentials for a server.\n\n\
     Options:\n\
     \x20\x20--user NAME    Only log out this user [default: every user on that server]\n\
     \x20\x20--dir PATH     Where connections are stored [default: ~/.config/webcodex]\n\
     \x20\x20-y, --yes      Confirm removal\n\
     \x20\x20--json         Print machine-readable output\n\
     \x20\x20-h, --help     Print help and exit\n\n\
     Without --yes this only reports what would be removed.\n"
}

pub(crate) fn status_usage() -> &'static str {
    "Usage: webcodex auth status [OPTIONS]\n\n\
     Show which servers this device is logged in to.\n\n\
     Options:\n\
     \x20\x20--dir PATH     Where connections are stored [default: ~/.config/webcodex]\n\
     \x20\x20--json         Print machine-readable output\n\
     \x20\x20-h, --help     Print help and exit\n"
}
