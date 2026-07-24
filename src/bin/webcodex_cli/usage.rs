pub(crate) fn usage() -> &'static str {
    "Usage: webcodex-cli <COMMAND>\n\n\
     Management/setup commands for WebCodex.\n\n\
     Commands:\n\
       server up                                      Bootstrap server env with auto-generated admin key\n\
       server init                                      Create server env bootstrap file\n\
       server install-service                           Generate/install a systemd unit\n\
       server status                                    Check service and runtime status\n\
       pairing create                                   Create a temporary client pairing code\n\
       client enroll                                    Enroll a client from a pairing code\n\
       ops status|agents|projects|smoke-preflight       Read-only operator workflow checks\n\
       user/users create/list                             Manage users\n\
       token generate                                   Generate a local wc_pat_* value and hash\n\
       token create-local                               Locally create and register a wc_pat_* with an account credential\n\
       token register-hash                              Register a precomputed wc_pat_* hash\n\
       token list/revoke                                Manage personal API tokens\n\
       tokens create-local/register-hash/list/revoke    Manage personal API tokens\n\
       agent-token create-local                         Locally create and register a wc_agent_* with an account credential\n\
       agent-token register-hash                        Register a precomputed wc_agent_* hash\n\
       agent-tokens create-local/register-hash/list/revoke Manage agent tokens\n\
       agent init/install-service/status                  Manage client-side agent config/service\n\
       setup single-user                                  Create a user + GPT + agent token set\n\n\
     Options:\n\
       -h, --help       Print help and exit\n\
       -V, --version    Print version and exit\n\n\
     Common flags (users/tokens/agent-tokens/setup):\n\
       --server-url URL    WebCodex server URL (required)\n\
       --token TOKEN       Bootstrap/admin/self bearer token\n\
       --token-file PATH   Read bearer token from file\n\
       Token fallback: WEBCODEX_TOKEN\n\
     Output: JSON unless noted otherwise.\n"
}

pub(crate) fn pairing_usage() -> &'static str {
    "Usage: webcodex-cli pairing <COMMAND>\n\n\
     Commands:\n\
       create       Create a short-lived pairing code for client enrollment\n"
}

pub(crate) fn pairing_create_usage() -> &'static str {
    "Usage: webcodex-cli pairing create --server-url URL --username USER --client-id CLIENT_ID [OPTIONS]\n\n\
     Options:\n\
       --server-url URL          WebCodex server URL\n\
       --env-file PATH           Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH         Read bootstrap/admin bearer token from file\n\
       --token TOKEN             Bootstrap/admin bearer token (discouraged in shell history)\n\
       --username USER           User to ensure/create for enrollment\n\
       --client-id CLIENT_ID     Client id the code is bound to\n\
       --display-name NAME       Optional display name for a newly created user\n\
       --ttl-secs SECS           Pairing code lifetime [default: 600; range: 60..3600]\n\
       --user-token-name NAME    Name for the user API token created during enroll\n\
       --agent-token-name NAME   Name for the agent token created during enroll\n\
       --json                    Print machine-readable output\n\
       -h, --help                Print help and exit\n\n\
     Server/admin-side command:\n\
       pairing create needs server bootstrap/admin auth. The default server\n\
       bootstrap env file lives on the server, not the client.\n\
       On the client, use: webcodex-cli client enroll\n\n\
     Copy only the short-lived wc_pair_* code to the client. Do not copy\n\
     WEBCODEX_TOKEN, wc_pat_*, or wc_agent_* values from server to client.\n\
     This command does not create wc_pat_* or wc_agent_* token files on the\n\
     server.\n"
}

pub(crate) fn client_usage() -> &'static str {
    "Usage: webcodex-cli client <COMMAND>\n\n\
     Commands:\n\
       enroll       Enroll this client using a temporary pairing code\n"
}

pub(crate) fn server_up_usage() -> &'static str {
    "Usage: webcodex-cli server up [OPTIONS]\n\n\
     Quick-start server bootstrap. Generates a local bootstrap/admin key when no\n\
     WEBCODEX_TOKEN is configured, writes a user-writable env file, and prints\n\
     next steps. Anonymous access is rejected by default; pass --open only for\n\
     local/trusted-network demos.\n\n\
     Options:\n\
       --public-url URL     Public URL reported to clients\n\
       --listen ADDR        Listen address [default: 0.0.0.0:8080]\n\
       --open               Allow anonymous GPT/MCP and client access\n\
       --data-dir DIR       Data directory [default: user/state dependent]\n\
       --env-file PATH      Env file path [default: user/config dependent]\n\
       --json               Print machine-readable output\n\
       -h, --help           Print help and exit\n\n\
     Default (no --open): anonymous denied, shared-key clients allowed,\n\
     managed tokens allowed, bootstrap/admin key enabled.\n\
     With --open: anonymous GPT/MCP and clients allowed (demo/test only).\n"
}

pub(crate) fn client_enroll_usage() -> &'static str {
    "Usage: webcodex-cli client enroll --server-url URL --pairing-code CODE --client-id CLIENT_ID [OPTIONS]\n\n\
     Options:\n\
       --server-url URL              WebCodex server URL\n\
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
    "Usage: webcodex-cli ops <COMMAND>\n\n\
     Read-only operator workflow checks for WebCodex.\n\n\
     Commands:\n\
       status                  Summarize runtime, tools, jobs, agents, and projects\n\
       agents                  Show compact agent fleet status\n\
       projects                Show compact project inventory and smoke suitability\n\
       smoke-preflight         Check a project before deploy smoke validation\n\n\
     Common flags:\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       -h, --help              Print help and exit\n\n\
     These commands are read-only. They do not run jobs, start shell commands,\n\
     create sessions, write files, or print token/env values.\n"
}

pub(crate) fn ops_status_usage() -> &'static str {
    "Usage: webcodex-cli ops status [OPTIONS]\n\n\
     Summarize runtime, tools, jobs, agents, and project health.\n\n\
     Options:\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       --strict                Exit 2 when the ops report status is FAIL\n\
       -h, --help              Print help and exit\n"
}

pub(crate) fn ops_agents_usage() -> &'static str {
    "Usage: webcodex-cli ops agents [OPTIONS]\n\n\
     Show compact read-only agent fleet status.\n\n\
     Options:\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       --strict                Exit 2 when the ops report status is FAIL\n\
       -h, --help              Print help and exit\n"
}

pub(crate) fn ops_projects_usage() -> &'static str {
    "Usage: webcodex-cli ops projects [OPTIONS]\n\n\
     Show compact read-only project inventory and smoke suitability.\n\n\
     Options:\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       --strict                Exit 2 when the ops report status is FAIL\n\
       -h, --help              Print help and exit\n"
}

pub(crate) fn ops_smoke_preflight_usage() -> &'static str {
    "Usage: webcodex-cli ops smoke-preflight --project PROJECT_ID [OPTIONS]\n\n\
     Read-only deploy smoke preflight for one project.\n\n\
     Options:\n\
       --project PROJECT_ID    Runtime project id to check\n\
       --server-url URL        WebCodex server URL [default: http://127.0.0.1:8080]\n\
       --url URL               Alias for --server-url\n\
       --env-file PATH         Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH       Read bearer token from file\n\
       --token TOKEN           Bearer token input; never printed\n\
       --json                  Print machine-readable output\n\
       --strict                Exit 2 when the ops report status is FAIL\n\
       -h, --help              Print help and exit\n\n\
     This command calls only read-only status/project/workspace inspection APIs.\n"
}

pub(crate) fn server_usage() -> &'static str {
    "Usage: webcodex-cli server <COMMAND>\n\n\
     Server bootstrap commands.\n\n\
     Commands:\n\
       up                   Quick-start: auto-generate admin key + env, optional --open\n\
       init                 Create WEBCODEX_TOKEN env bootstrap file\n\
       install-service      Generate/install a systemd unit\n\
       status               Check systemd and /api/runtime/status\n\n\
     Notes:\n\
       server init only creates the server bootstrap/admin WEBCODEX_TOKEN.\n\
       It does not create user API tokens or agent tokens.\n"
}

pub(crate) fn server_init_usage() -> &'static str {
    "Usage: webcodex-cli server init [OPTIONS]\n\n\
     Options:\n\
       --listen ADDR          Listen address [default: 127.0.0.1:8080]\n\
       --data-dir PATH        Data directory [root: /var/lib/webcodex; user: ~/.local/share/webcodex]\n\
       --env-file PATH        Env file [root: /etc/webcodex/webcodex.env; user: ~/.config/webcodex/webcodex.env]\n\
       --public-url URL       Optional public URL to report from runtime status\n\
       --overwrite            Replace an existing env file\n\
       --output -             Also print env contents to stdout, including the full WEBCODEX_TOKEN\n\
       --json                 Print a machine-readable summary without the full token\n\
       -h, --help             Print help and exit\n\n\
     server init writes only WEBCODEX_TOKEN. It does not create wc_pat_* user\n\
     tokens or wc_agent_* agent tokens.\n"
}

pub(crate) fn server_install_service_usage() -> &'static str {
    "Usage: webcodex-cli server install-service [OPTIONS]\n\n\
     Options:\n\
       --env-file PATH             Env file [default: /etc/webcodex/webcodex.env]\n\
       --bin PATH                  webcodex server binary path; defaults to webcodex from PATH when safely discoverable\n\
       --service-file PATH         systemd unit path [default: /etc/systemd/system/webcodex.service]\n\
       --user USER                 Optional systemd User=\n\
       --group GROUP               Optional systemd Group=\n\
       --working-directory PATH    WorkingDirectory= [default: /var/lib/webcodex]\n\
       --overwrite                 Replace an existing service file\n\
       --dry-run                   Print the unit instead of writing it\n\
       --output -                  Print the unit instead of writing it\n\
       --json                      Print a machine-readable summary\n\
       -h, --help                  Print help and exit\n\n\
     Tokens are never inlined in the unit; it uses EnvironmentFile=.\n"
}

pub(crate) fn server_status_usage() -> &'static str {
    "Usage: webcodex-cli server status [OPTIONS]\n\n\
     Options:\n\
       --url URL              Runtime URL [default: http://127.0.0.1:8080]\n\
       --env-file PATH        Read WEBCODEX_TOKEN from env file [default: root /etc/webcodex/webcodex.env; user ~/.config/webcodex/webcodex.env]\n\
       --token-file PATH      Read bearer token from file\n\
       --json                 Print a machine-readable summary\n\
       -h, --help             Print help and exit\n\n\
     Token priority: --token-file, WEBCODEX_TOKEN from --env-file, process\n\
     WEBCODEX_TOKEN, then no token for auth-disabled servers.\n"
}

pub(crate) fn agent_usage() -> &'static str {
    "Usage: webcodex-cli agent <COMMAND>\n\n\
     Client-side agent commands.\n\n\
     Commands:\n\
       init                 Generate an agent.toml config\n\
       install-service      Generate/install a webcodex-agent systemd unit\n\
       status               Check systemd status and safe agent metadata\n"
}

pub(crate) fn agent_init_usage() -> &'static str {
    "Usage: webcodex-cli agent init --server-url URL [--token TOKEN|--token-file PATH] --client-id ID --owner USER [OPTIONS]\n\n\
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
    "Usage: webcodex-cli agent install-service [--config PATH] [--bin PATH] [OPTIONS]\n\n\
     Options:\n\
       --profile NAME             Client config profile for config/service defaults\n\
       --config PATH              Agent config path [default: /etc/webcodex/agent.toml, or profile agent.toml]\n\
       --bin PATH                 webcodex-agent binary path; defaults to webcodex-agent from PATH when safely discoverable\n\
       --service-file PATH        systemd unit path [default: /etc/systemd/system/webcodex-agent.service, or webcodex-agent-<profile>.service]\n\
       --working-directory PATH   WorkingDirectory= [default: /root]\n\
       --user USER                Optional systemd User=\n\
       --group GROUP              Optional systemd Group=\n\
       --overwrite                Replace an existing service file\n\
       --dry-run                  Print the unit instead of writing it\n\
       --output -                 Print the unit instead of writing it\n\
       --json                     Print a machine-readable summary\n\
       -h, --help                 Print help and exit\n\n\
     With --profile, missing config/service paths are derived under\n\
     /etc/webcodex/clients/<profile> for root or\n\
     ~/.config/webcodex/clients/<profile> for non-root users. Explicit path\n\
     flags override profile-derived defaults. The unit runs:\n\
     webcodex-agent --config <config>. Tokens are never inlined.\n"
}

pub(crate) fn agent_status_usage() -> &'static str {
    "Usage: webcodex-cli agent status [OPTIONS]\n\n\
     Options:\n\
       --profile NAME             Client config profile for config/token defaults\n\
       --config PATH              Agent config path [default: /etc/webcodex/agent.toml, or profile agent.toml]\n\
       --server-url URL           Override server URL for runtime checks\n\
       --user-token-file PATH     Read user API token for /api/runtime/status\n\
       --agent-token-file PATH    Read agent token for boundary check\n\
       --json                     Print a machine-readable summary\n\
       -h, --help                 Print help and exit\n\n\
     With --profile, missing config and token paths are derived under\n\
     /etc/webcodex/clients/<profile> for root or\n\
     ~/.config/webcodex/clients/<profile> for non-root users. Explicit path\n\
     flags override profile-derived defaults. Status prints safe metadata only:\n\
     no tokens, Authorization headers, full agent.toml, env files, or secrets.\n"
}
