# Private Drop

A self-hosted private long-text/file drop box. Replace QQ/WeChat's "File Transfer Assistant" with your own service.

## Features

- Token authentication
- 6 default channels: inbox, xline, thesis, packfix, omo, files
- Send long text (up to 2MB)
- Upload files (up to 100MB)
- Mobile-friendly web UI
- REST API with OpenAPI spec
- SQLite storage
- GPT Actions compatible

## Quick Start

### 1. Build

```bash
cargo build --release
```

### 2. Set Token

```bash
export DROP_TOKEN="your-secret-token"
```

### 3. Run

```bash
./target/release/private-drop
```

Or with custom settings:

```bash
DROP_ADDR=0.0.0.0:8080 DROP_DATA=./data DROP_TOKEN=your-secret ./target/release/private-drop
```

### 4. Access

- Web UI: http://localhost:8080
- API: http://localhost:8080/api
- OpenAPI: http://localhost:8080/openapi.json

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DROP_ADDR` | `0.0.0.0:8080` | Listen address |
| `DROP_DATA` | `./data` | Data directory |
| `DROP_TOKEN` | (none) | Auth token (required for production) |

## API Examples

### Health Check

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

### List Messages

```bash
curl -H "Authorization: Bearer your-secret-token" \
  http://localhost:8080/api/messages?channel=inbox&limit=10
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
  http://localhost:8080/api/files?channel=files
```

### Download File

```bash
curl -H "Authorization: Bearer your-secret-token" \
  http://localhost:8080/api/files/{id} -o downloaded.file
```

## GPT Actions Setup

1. Get your OpenAPI spec: `http://your-server:8080/openapi.json`
2. In ChatGPT, create a GPT with Actions
3. Import the OpenAPI schema URL
4. Set authentication to "API Key" with your `DROP_TOKEN`
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

## E2E Testing

Run the automated end-to-end smoke test:

```bash
bash scripts/e2e_test.sh
```

This script automatically:
- Builds the project (`cargo fmt`, `cargo test`, `cargo build --release`)
- Starts the server with a temporary data directory
- Runs 19 test cases covering:
  - Health check endpoint
  - Token authentication (401 on missing/wrong token)
  - Create text message
  - List messages by channel
  - Create 10K long text message
  - Create 100K long text message
  - Get single message detail
  - Delete message
  - Upload file
  - Download file with content verification
  - OpenAPI spec validation
  - Channel listing
- Cleans up the server and temporary data on exit

## Security Notes

- Always set `DROP_TOKEN` in production
- Use HTTPS in production (via nginx reverse proxy)
- Token is required for all API and file download operations
- File paths are randomized to prevent guessing
- Path traversal is prevented

## License

MIT
