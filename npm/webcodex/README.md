# @yyjeqhc/webcodex

Thin npm installer/wrapper for WebCodex native binaries. It installs command wrappers for:

- `webcodex`
- `webcodex-agent`
- `webcodex-cli`

## Install

```bash
npm install -g @yyjeqhc/webcodex
```

The package does not commit compiled binaries to git. During installation, `install.js` detects the current platform/architecture, reads `manifest.json`, downloads the matching `.tar.gz` artifact from the GitHub Release, verifies its SHA-256 checksum, and installs the native binaries into `vendor/bin`.

## Supported platforms in v0.2.0

Current release artifacts include:

- `linux-x64`

`linux-arm64`, `darwin-arm64`, `darwin-x64`, Windows, and other platforms are not included in v0.2.0. They are future targets unless a later release adds matching artifacts.

Do not publish the npm package until `manifest.json` contains the real SHA-256 checksums for the v0.2.0 GitHub Release artifacts. The package includes a `prepublishOnly` check that rejects placeholder checksums.

## Development switches

- `WEBCODEX_SKIP_DOWNLOAD=1` skips downloads.
- `WEBCODEX_BINARY_DIR=/path/to/bin` copies local binaries.
- `WEBCODEX_MANIFEST=/path/to/manifest.json` or `file:///.../manifest.json` uses a local manifest.

The wrappers preserve arguments and execute the package-local native binary. If a binary is missing, they print:

```text
Run npm install again or set WEBCODEX_BINARY_DIR=...
```

## Local package smoke

From the repository root, build local binaries, pack this npm package, install the tarball into a temporary prefix, and verify the wrapper commands without publishing:

```bash
bash scripts/npm_package_smoke.sh
```

For a faster debug-build smoke:

```bash
WEBCODEX_NPM_SMOKE_PROFILE=debug bash scripts/npm_package_smoke.sh
```

## Enrollment flow

Server init creates only the server bootstrap token. Pairing creates a short-lived `wc_pair_*` code, and client enroll creates/saves the user API token and agent token on the client side. GPT Actions should use the client-side user token, not the server bootstrap token.

## License

Apache-2.0. See the repository `LICENSE` file.
