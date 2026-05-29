# TODO

## Done

- [x] Core API: create, list, get, delete messages
- [x] File upload/download with Content-Disposition
- [x] Token authentication (API + Web UI)
- [x] 6 default channels
- [x] SQLite storage (rusqlite bundled)
- [x] Web UI: home, channel, message detail, send
- [x] OpenAPI 3.0 spec for GPT Actions
- [x] E2E test script (31 tests)
- [x] Unit tests (8 tests)
- [x] Systemd service example
- [x] Nginx reverse proxy example
- [x] Long text support (10K, 100K tested)

## Done (Codex API)

- [x] Codex-like Project API: context, apply_patch, check, report
- [x] projects.toml whitelist configuration
- [x] Unified diff patch application via git apply
- [x] Pre-configured check suite execution
- [x] Report writing with channel message integration
- [x] Security: path traversal prevention, sensitive path blocking
- [x] OpenAPI spec updated with 4 new operationIds

## Future Enhancements

- [ ] Stronger command timeout control (per-command timeout from config)
- [ ] Audit logging for all Codex API operations
- [ ] Diff preview before applying patches
- [ ] GPT Actions real-world integration testing
- [ ] Private Drop report UI optimization
- [ ] Multi-project permission levels (read-only, patch, check, admin)
- [ ] Nginx HTTPS deployment guide (example exists in deploy/)
- [ ] ntfy notifications for new messages
- [ ] Message expiration cleanup task (expires_at field exists, no cron)
- [ ] Complete GPT Actions integration testing with real ChatGPT
- [ ] Better mobile UI with progressive enhancement
- [ ] Logging and audit trail
- [ ] Rate limiting
- [ ] Database backup utility
- [ ] Message search functionality
- [ ] Bulk delete operations
- [ ] Custom channel management (CRUD)
- [ ] Webhook support
- [ ] Dark mode for web UI
- [ ] Message pinning
- [ ] File preview (images, PDF)
- [ ] Multi-file upload
- [ ] Message editing
- [ ] API pagination cursor (currently uses before timestamp)
