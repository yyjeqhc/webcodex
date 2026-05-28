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
