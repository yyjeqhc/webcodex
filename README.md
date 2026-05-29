# Private Drop

A self-hosted private long-text/file drop box. Replace QQ/WeChat's "File Transfer Assistant" with your own service.

## Features

- Token authentication (Bearer token or query param)
- 6 default channels: inbox, xline, thesis, packfix, omo, files
- Send long text (up to 2MB per message)
- Upload files (up to 100MB)
- Mobile-friendly web UI (server-rendered HTML)
- REST API with OpenAPI 3.0 spec
- SQLite storage (bundled, no system dependency)
- GPT Actions compatible
- File download with Content-Disposition header
- Path traversal protection

## Quick Start

### 1. Build

```bash
cargo build --release
```

### 2. Run

```bash
DROP_TOKEN="your-secret-token" ./target/release/private-drop
```

With custom settings:

```bash
DROP_ADDR=0.0.0.0:8080 DROP_DATA=./data DROP_TOKEN=your-secret ./target/release/private-drop
```

### 3. Access

- Web UI: http://localhost:8080
- API: http://localhost:8080/api
- OpenAPI: http://localhost:8080/openapi.json

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DROP_ADDR` | `0.0.0.0:8080` | Listen address |
| `DROP_DATA` | `./data` | Data directory (SQLite DB + uploads) |
| `DROP_TOKEN` | (none) | Auth token. **Required for production.** If unset, runs in dev mode with warning. |

## API Examples

### Health Check (no auth)

```bash
curl http://localhost:8080/api/health
```

### Send Text Message

```bash
curl -X POST http://localhost:8080/api/messages \
  -H "Authorization: Bearer your-secret-token" \
  -H "Content-Type: application/json" \
  -d '{"channel": "inbox", "title": "My Note", "text": "Hello, this is a long text..."}'
```

### Send 10K Text

```bash
python3 -c "print('A' * 10240)" | \
curl -X POST http://localhost:8080/api/messages \
  -H "Authorization: Bearer your-secret-token" \
  -H "Content-Type: application/json" \
  -d @- --json '{"channel":"inbox","title":"Long","text":"PLACEHOLDER"}'
```

Or inline:

```bash
LONG=$(python3 -c "print('A' * 10240)")
curl -X POST http://localhost:8080/api/messages \
  -H "Authorization: Bearer your-secret-token" \
  -H "Content-Type: application/json" \
  -d "{\"channel\":\"inbox\",\"title\":\"10K Text\",\"text\":\"$LONG\"}"
```

### List Messages

```bash
curl -H "Authorization: Bearer your-secret-token" \
  "http://localhost:8080/api/messages?channel=inbox&limit=10"
```

### Get Message

```bash
curl -H "Authorization: Bearer your-secret-token" \
  http://localhost:8080/api/messages/{id}
```

### Delete Message

```bash
curl -X DELETE -H "Authorization: Bearer your-secret-token" \
  http://localhost:8080/api/messages/{id}
```

### Upload File

```bash
curl -X POST \
  -H "Authorization: Bearer your-secret-token" \
  -F "file=@myfile.pdf" \
  "http://localhost:8080/api/files?channel=files"
```

### Download File

```bash
curl -H "Authorization: Bearer your-secret-token" \
  http://localhost:8080/api/files/{id} -o downloaded.file
```

## Web UI

The web UI is server-rendered HTML, mobile-friendly, no JavaScript framework required.

- `GET /` - Home page with channel list and recent messages
- `GET /c/{channel}` - Channel messages (e.g., `/c/inbox`)
- `GET /m/{id}` - Message detail
- `GET /send` - Send text / upload file form

Web UI auth: pass `?token=your-secret-token` in the URL, or use the login page which stores the token in localStorage.

## GPT Actions Setup

1. Deploy Private Drop with HTTPS (see Deployment section below)
2. In ChatGPT, create a GPT with Actions
3. Import your OpenAPI spec URL: `https://your-server/openapi.json`
4. Set authentication to "API Key" / "Bearer" with your `DROP_TOKEN`
5. Key operations for GPT:
   - `createMessage` - Send text to any channel
   - `listMessages` - Read messages from channels
   - `getMessage` - Get specific message details
   - `deleteMessage` - Delete a message

## Channels

Default channels for organizing messages:

- **inbox** - General inbox
- **xline** - Xline related
- **thesis** - Thesis work
- **packfix** - Packfix tasks
- **omo** - OMO items
- **files** - File uploads

## Deployment

### Systemd Service

Example service file: `deploy/private-drop.service.example`

```bash
# Create user
sudo useradd -r -s /usr/sbin/nologin private-drop

# Copy binary
sudo mkdir -p /opt/private-drop
sudo cp target/release/private-drop /opt/private-drop/
sudo chown -R private-drop:private-drop /opt/private-drop

# Install service
sudo cp deploy/private-drop.service.example /etc/systemd/system/private-drop.service
# Edit DROP_TOKEN in the service file
sudo systemctl daemon-reload
sudo systemctl enable --now private-drop
```

### Nginx Reverse Proxy

Example config: `deploy/nginx.example.conf`

```bash
# Install nginx and certbot
sudo apt install nginx certbot python3-certbot-nginx

# Copy config
sudo cp deploy/nginx.example.conf /etc/nginx/sites-available/private-drop
# Edit server_name
sudo ln -s /etc/nginx/sites-available/private-drop /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx

# Get SSL cert
sudo certbot --nginx -d drop.example.com
```

## E2E Testing

Run the automated end-to-end smoke test:

```bash
bash scripts/e2e_test.sh
```

This script:
- Builds the project (`cargo fmt`, `cargo test`, `cargo build --release`)
- Starts the server on a random free port with a temporary data directory
- Runs 31 test cases covering:
  - Health check endpoint
  - Token authentication (401 on missing/wrong token)
  - Create, list, get, delete text messages
  - 10K and 100K long text messages
  - File upload and download with content verification
  - Content-Disposition header on file download
  - OpenAPI spec validation
  - Channel listing
  - Web UI pages: home, channel, message detail, send
- Cleans up the server and temporary data on exit

## Unit Tests

```bash
cargo test
```

8 unit tests covering UUID generation, config defaults, token validation, filename sanitization, and message serialization.

## Codex-like GPT Actions API

Private Drop includes a set of coarse-grained APIs designed for ChatGPT GPT Actions to operate on whitelisted projects. These APIs enable GPT to act as a "Codex brain" — observing projects, generating patches, running checks, and writing reports.

### Why 4 Coarse Interfaces?

Instead of exposing fine-grained dangerous APIs (readFile, writeFile, runShell, deleteFile), we expose 4 coarse operations:

1. **getProjectContext** — Read-only observation (overview, tree, search, read_file, git_status, git_diff)
2. **applyProjectPatch** — Apply a unified diff to a whitelisted project
3. **runProjectCheck** — Run pre-configured check commands (fmt, test, build, e2e, full)
4. **writeProjectReport** — Write operation reports and post messages to channels

This design prevents arbitrary shell access, arbitrary file I/O, and git push while still giving GPT enough capability to complete code review and fix workflows.

### Configuration

Create `projects.toml` (or set `PROJECTS_CONFIG` env var to its path):

```toml
[projects.private-drop]
path = "/root/git/private-drop"
allow_patch = true
allowed_checks = ["fmt", "test", "build", "e2e", "full"]

[projects.private-drop.checks]
fmt = "cargo fmt --check"
test = "cargo test"
build = "cargo build --release"
e2e = "bash scripts/e2e_test.sh"
full = "cargo fmt --check && cargo test && cargo build --release && bash scripts/e2e_test.sh"
```

If `projects.toml` is missing, the Codex API returns clear errors but the original message/file APIs remain fully functional.

### SSH Executor (Remote Projects)

When private-drop is deployed on a gateway machine (e.g., SG4 as a public HTTPS entry point) but the actual project lives on a different machine (reachable via Tailscale), use the SSH executor:

```toml
[projects.private-drop]
executor = "ssh"
host = "msi"
user = "root"
path = "/root/git/private-drop"
allow_patch = true
allowed_checks = ["fmt", "test", "build", "e2e"]

[projects.private-drop.checks]
fmt = "cargo fmt --check"
test = "cargo test"
build = "cargo build --release"
e2e = "bash scripts/e2e_test.sh"
```

**Architecture:**
- SG4 runs private-drop with HTTPS (public entry point)
- SSH commands go to the remote machine (e.g., `msi` via Tailscale)
- All Codex API operations (context, apply_patch, check) execute on the remote machine
- Reports are written locally on SG4

**How it works:**
- `executor = "ssh"`: Commands run via `ssh <host> -- <remote command>`
- `host`: SSH hostname (from `projects.toml`, not user input)
- `user`: SSH user (optional, defaults to current user)
- `path`: Project root on the remote machine
- All path safety checks (no `..`, no absolute paths, no sensitive files) are enforced before SSH commands are constructed
- Patches are piped via SSH stdin to a temp file on the remote machine

**Before using SSH executor, verify connectivity:**
```bash
ssh msi 'cd /root/git/private-drop && cargo test'
```

### API Examples

```bash
# Get project overview
curl -X POST http://localhost:8080/api/codex/context \
  -H "Authorization: Bearer your-secret-token" \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","mode":"overview"}'

# Search for code
curl -X POST http://localhost:8080/api/codex/context \
  -H "Authorization: Bearer your-secret-token" \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","mode":"search","query":"fn main"}'

# Apply a patch
curl -X POST http://localhost:8080/api/codex/apply_patch \
  -H "Authorization: Bearer your-secret-token" \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","patch":"diff --git a/...","reason":"fix bug"}'

# Run checks
curl -X POST http://localhost:8080/api/codex/check \
  -H "Authorization: Bearer your-secret-token" \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","suite":"test"}'

# Write a report
curl -X POST http://localhost:8080/api/codex/report \
  -H "Authorization: Bearer your-secret-token" \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","status":"completed","title":"Fix X","summary":"Changed Y, tests pass","channel":"omo"}'
```

### GPT Actions Setup

1. Import `https://your-server/openapi.json` into your GPT Actions
2. Set authentication to API Key / Bearer with your `DROP_TOKEN`
3. GPT can use the 4 `codex` operationIds: `getProjectContext`, `applyProjectPatch`, `runProjectCheck`, `writeProjectReport`

### Security Boundaries

- **No arbitrary shell**: Only pre-configured check commands from `projects.toml`
- **No git push**: Only `git apply` for patches, no commit or push
- **No arbitrary path access**: All paths are canonicalized and verified to be within project root
- **Sensitive files blocked**: `.git/`, `.env`, `*.pem`, `*.key`, `id_rsa`, `target/`, `node_modules/` cannot be modified
- **Output truncated**: All outputs capped at 50K characters to avoid overwhelming GPT Actions
- **Project whitelist only**: Only projects listed in `projects.toml` are accessible

## Security Notes

- Always set `DROP_TOKEN` in production
- Use HTTPS in production (via nginx reverse proxy)
- Token is required for all API and file download operations
- File paths are randomized (UUID) to prevent guessing
- Path traversal is prevented (canonicalized paths checked against uploads dir)
- Filename in Content-Disposition is sanitized (no path separators)
- Request body size limited to 2MB for text, 100MB for files

## License

MIT
