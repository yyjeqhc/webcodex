# WebCodex v0.2.0 Release Checklist

This checklist is for preparing and executing the WebCodex v0.2.0 GitHub binary
release. It is not itself a release command log.

Release readiness audit tasks that update this document must not create tags,
push commits, publish npm packages, create GitHub Releases, upload artifacts, or
rewrite history.

## Scope

- Release WebCodex Rust binaries at version `0.2.0`.
- Publish v0.2.0 binaries through a GitHub Release.
- Validate public docs, release notes, artifact names, checksums, and post-release
  runtime health.
- Keep the npm wrapper documented as the current `0.1.0` package until an
  explicit later npm release changes that plan.

## Not in scope

- Do not run `npm publish`.
- Do not change the npm package version from `0.1.0` during v0.2.0 release
  execution unless a separate release decision explicitly changes scope.
- Do not publish Windows artifacts for v0.2.0.
- Do not publish `darwin-x64` artifacts for v0.2.0.
- Do not change OpenAPI schemas, MCP schemas, runtime behavior, tool
  implementation, or packaging code as part of release execution.

## Pre-tag repository checks

- [ ] Confirm the release branch/worktree is clean:

  ```bash
  git status --short
  git status --branch --short
  git log -8 --oneline
  ```

- [ ] Confirm the intended release commit is the current `HEAD`.
- [ ] Confirm no unrelated worktree files, local build artifacts, or generated
  release binaries are staged.
- [ ] Run formatting and build checks:

  ```bash
  cargo fmt --check
  cargo check --all-targets
  cargo test --bin webcodex
  git diff --check
  ```

## Private naming scan

- [ ] Run the targeted public-docs private naming scan from the release audit
  instructions.
- [ ] Confirm the only allowed hits are the removed-docs records in
  `docs/INDEX.md` and `docs/INDEX.zh-CN.md`.
- [ ] Do not treat `@yyjeqhc/webcodex`, `github.com/yyjeqhc/webcodex`, or Rust
  feature/version text such as `v4` as private naming.

## Version and packaging checks

- [ ] Confirm Rust package metadata is `0.2.0`:

  ```bash
  rg -n 'name = "webcodex"|version = "0\.2\.0"' Cargo.toml Cargo.lock
  ```

- [ ] Confirm npm package metadata remains `0.1.0`:

  ```bash
  rg -n '"name": "@yyjeqhc/webcodex"|"version": "0\.1\.0"' npm/webcodex/package.json npm/webcodex/manifest*.json
  ```

- [ ] Confirm public docs state that `npm install -g @yyjeqhc/webcodex`
  currently installs v0.1.0 binaries.
- [ ] Confirm public docs state that v0.2.0 users should download GitHub Release
  binaries directly instead of using npm install.
- [ ] Confirm `npm publish` remains out of scope for v0.2.0.
- [ ] Confirm docs use planned/expected wording before release publication and do
  not say v0.2.0 artifacts are already published before the GitHub Release exists.

## Artifact build matrix

Planned v0.2.0 GitHub Release artifacts:

- [ ] `linux-x64`
- [ ] `linux-arm64`
- [ ] `darwin-arm64`

Excluded from v0.2.0 artifacts:

- Windows
- `darwin-x64`

## Artifact validation

- [ ] Build release binaries from the final v0.2.0 commit.
- [ ] Package each artifact with the expected binary names:
  `webcodex`, `webcodex-cli`, and `webcodex-agent`.
- [ ] Confirm each archive extracts cleanly on a matching host or clean test
  environment.
- [ ] Run binary smoke checks for each artifact:

  ```bash
  ./webcodex --help
  ./webcodex-cli --help
  ./webcodex-agent --help
  ./webcodex-cli version
  ```

- [ ] Generate SHA-256 checksums for every artifact.
- [ ] Confirm checksum filenames and release asset names match the release notes.
- [ ] Keep generated artifacts and checksums out of git unless a separate policy
  explicitly says otherwise.

## GitHub Release steps

- [ ] Create the `v0.2.0` git tag only after all pre-tag checks pass.
- [ ] Push the release tag only during the release window.
- [ ] Create a GitHub Release for `v0.2.0`.
- [ ] Attach the planned artifacts:
  `linux-x64`, `linux-arm64`, and `darwin-arm64`.
- [ ] Attach or publish SHA-256 checksums.
- [ ] Use `docs/RELEASE_NOTES_v0.2.0.md` as the release-note source, adjusting
  only publication-state wording if needed.
- [ ] Do not attach Windows or `darwin-x64` binaries for v0.2.0.
- [ ] Do not run `npm publish`.

## Post-release smoke tests

- [ ] Download each GitHub Release artifact from the public release page and
  verify its SHA-256 checksum.
- [ ] Run CLI and agent help/version smoke checks from downloaded artifacts.
- [ ] Start a minimal server and agent from release binaries.
- [ ] Verify OpenAPI is reachable:

  ```bash
  curl -sS "$WEBCODEX_PUBLIC_URL/openapi.json" >/tmp/webcodex-openapi.json
  ```

- [ ] Verify MCP smoke with a user token:

  ```bash
  curl -sS --oauth2-bearer "$WEBCODEX_PAT" \
    -H 'Content-Type: application/json' \
    "$WEBCODEX_PUBLIC_URL/mcp" \
    -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
  ```

- [ ] Verify runtime status after deployment:

  ```bash
  curl -fsS -X POST "$WEBCODEX_PUBLIC_URL/api/runtime/status" \
    -H "Authorization: Bearer $WEBCODEX_PAT" \
    -H "Content-Type: application/json" \
    -d '{}'
  ```

- [ ] Confirm `runtime_status` reports expected service health, agent visibility,
  and build revision metadata.
- [ ] Confirm `list_projects` and one read-only project tool work against a
  registered project.

## Post-release docs sanity

- [ ] Re-run the version/artifact scan:

  ```bash
  rg -n '0.1.0|0.2.0|v0.1.0|v0.2.0|npm install|npm publish|GitHub release|linux-x64|linux-arm64|darwin-arm64|darwin-x64|Windows' README.md README.zh-CN.md docs Cargo.toml Cargo.lock npm package.json 2>/dev/null | sed -n '1,260p'
  ```

- [ ] Confirm docs do not imply `npm install` gets v0.2.0.
- [ ] Confirm docs still direct v0.2.0 users to GitHub Release binaries.
- [ ] Confirm docs do not list Windows or `darwin-x64` as v0.2.0 artifacts.
- [ ] Confirm any wording changed from "planned" to "published" only after the
  GitHub Release exists.

## Rollback / recovery notes

- If release artifacts are wrong before public announcement, delete or replace
  the GitHub Release assets and document the correction in the release body.
- If the tag points at the wrong commit, stop and coordinate a recovery plan
  before changing remote tags.
- If a binary fails post-release smoke, mark the GitHub Release as pre-release or
  add a visible warning while rebuilding replacement artifacts.
- If npm scope accidentally changes, unpublish/deprecate only according to npm
  policy and publish a correction note. Do not silently retarget the v0.2.0
  binary release plan.

## Known exclusions

- npm wrapper remains `0.1.0` for this release.
- `npm install -g @yyjeqhc/webcodex` currently installs v0.1.0 binaries.
- v0.2.0 users should download GitHub Release binaries directly.
- npm publish is out of scope for v0.2.0.
- Windows artifacts are excluded from v0.2.0.
- `darwin-x64` artifacts are excluded from v0.2.0.
