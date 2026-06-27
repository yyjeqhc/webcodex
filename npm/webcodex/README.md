# @webcodex/webcodex

MVP npm wrapper for WebCodex native binaries. It provides:

- `webcodex`
- `webcodex-agent`
- `webcodex-cli`

Publishing is not ready until release artifacts and real SHA-256 checksums exist. The package does not include compiled binaries in git.

## Install Behavior

`install.js` detects platform/arch, reads a manifest entry, downloads a `.tar.gz` artifact, verifies SHA-256, and installs binaries into `vendor/bin`.

Supported targets:

- linux x64
- linux arm64
- darwin x64
- darwin arm64
- win32 x64

Development switches:

- `WEBCODEX_SKIP_DOWNLOAD=1` skips downloads.
- `WEBCODEX_BINARY_DIR=/path/to/bin` copies local binaries.
- `WEBCODEX_MANIFEST=/path/to/manifest.json` or `file:///.../manifest.json` uses a local manifest.

The wrappers preserve arguments and execute the package-local native binary. If a binary is missing, they print:

`Run npm install again or set WEBCODEX_BINARY_DIR=...`

## Enrollment Flow

Server init creates only `WEBCODEX_TOKEN`. Pairing creates a short-lived code, and client enroll creates/saves `wc_pat_*` and `wc_agent_*` tokens on the client side.
