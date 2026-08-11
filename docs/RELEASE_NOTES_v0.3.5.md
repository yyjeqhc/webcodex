# WebCodex 0.3.5

[English](RELEASE_NOTES_v0.3.5.md) | [简体中文](RELEASE_NOTES_v0.3.5.zh-CN.md)

WebCodex 0.3.5 is a compatibility and maintenance release. It publishes the current post-0.3.4 mainline, lowers the native Linux x64 release baseline to glibc 2.17, hardens Runner proxy transport behavior, reduces Session persistence memory pressure, and includes Rust maintenance cleanup.

## Highlights

- **Linux x64 glibc 2.17 compatibility.** The native `linux-x64` artifact is built on the dedicated Special compatibility release builder with a glibc 2.17 userspace baseline. Release validation must reject binaries that require any `GLIBC_*` symbol version newer than 2.17. This addresses #20.
- **Runner proxy transport hardening.** Polling and WebSocket proxy handling on the Runner has additional compatibility and failure-path hardening from the post-0.3.4 mainline.
- **Lower Session persistence memory pressure.** Closed Session history and persistence paths reduce unnecessary in-memory duplication while preserving the durable Session contract.
- **Rust maintenance cleanup.** Dead-code and Clippy noise was reduced without changing the intended public runtime contract.
- **Security documentation refresh.** `SECURITY.md`, which GitHub renders as the repository Security Policy, now identifies 0.3.x as the supported security-fix line instead of the stale 0.2.x text.

## Breaking changes and compatibility

There is no intentional breaking protocol change in 0.3.5.

Starting with this release, the native Linux x64 tarball targets glibc 2.17 or newer. This is an ELF compatibility guarantee for the published native `linux-x64` binaries, not a blanket promise that every installation path works on every glibc 2.17 distribution. In particular, the npm wrapper still requires Node.js 18 or newer, and the available Node.js build must itself support the host.

The Linux arm64 artifact remains in the published matrix, but it does not yet carry the same glibc 2.17 compatibility guarantee. Its compatibility continues to depend on the native arm64 release host until that builder is migrated separately.

## Upgrade notes

1. Upgrade `webcodex`, `webcodex-server`, and `webcodex-runner` together from the same immutable v0.3.5 revision.
2. Verify all installed binaries report `0.3.5`, the same concrete commit, and `dirty=false`.
3. Linux x64 operators that were blocked by a glibc requirement newer than 2.17 should use the v0.3.5 native artifact or the npm wrapper on a host with a compatible Node.js 18+ runtime.
4. Linux arm64 users should not infer the x64 glibc 2.17 guarantee for arm64 in this release.

## Binary packaging

The planned v0.3.5 release artifacts are:

- `webcodex-v0.3.5-linux-x64.tar.gz`
- `webcodex-v0.3.5-linux-arm64.tar.gz`
- `webcodex-v0.3.5-darwin-arm64.tar.gz`
- `webcodex-v0.3.5-win32-x64.tar.gz`

All artifacts must be built natively from the exact immutable `v0.3.5` tag and carry the same release build timestamp. The Linux x64 build additionally uses the dedicated glibc 2.17 compatibility builder and an explicit `readelf` ABI gate before packaging.

## Known limitations

- The glibc 2.17 release floor is currently guaranteed only for `linux-x64`; the `linux-arm64` release builder will be addressed separately.
- Node.js compatibility is separate from native ELF compatibility. The npm installation path still requires Node.js 18 or newer.
- macOS x64, Windows ARM64, and other unpublished targets remain outside the release artifact matrix.

## Release validation

This release-prep change does not add new runtime feature code. Validation should cover version consistency across Cargo/npm metadata, npm self-tests, Markdown links, clean release provenance, the existing source/CI gates, and the new Linux x64 `GLIBC_* <= 2.17` artifact check.

## Next steps

Migrate the Linux arm64 release builder to an equivalent controlled compatibility baseline before making the same glibc floor claim for that artifact.
