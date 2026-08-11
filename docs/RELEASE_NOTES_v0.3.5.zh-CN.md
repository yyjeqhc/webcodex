# WebCodex 0.3.5

[English](RELEASE_NOTES_v0.3.5.md) | [简体中文](RELEASE_NOTES_v0.3.5.zh-CN.md)

WebCodex 0.3.5 是一次兼容性与维护发布。它发布 0.3.4 之后当前 mainline 的改动，把 native Linux x64 release 的 glibc 基线降低到 2.17，同时包含 Runner proxy transport hardening、Session persistence 内存压力优化和 Rust 维护清理。

## 主要更新

- **Linux x64 glibc 2.17 兼容。** Native `linux-x64` artifact 改为在 Special 的专用兼容 release builder 中构建，该 userspace 以 glibc 2.17 为基线。release validation 必须拒绝任何依赖高于 2.17 的 `GLIBC_*` symbol version 的 binary，从而解决 #20。
- **Runner proxy transport hardening。** 0.3.4 之后的 mainline 对 Runner polling / WebSocket proxy 的兼容性与失败路径进行了进一步加固。
- **降低 Session persistence 内存压力。** Closed Session history 与 persistence path 减少不必要的内存重复，同时保持 durable Session contract 不变。
- **Rust 维护清理。** 减少 dead-code 与 Clippy noise，不改变预期的公开 runtime contract。
- **Security 文档更新。** GitHub Security Policy 页面直接渲染的 `SECURITY.md` 已把受支持的 security-fix 版本线从过期的 0.2.x 更新为 0.3.x。

## 破坏性变更与兼容性

0.3.5 没有刻意引入 breaking protocol change。

从本版本开始，native Linux x64 tarball 以 glibc 2.17 或更新版本为兼容基线。这是对已发布 `linux-x64` native binary 的 ELF 兼容性承诺，不代表所有安装方式都能在每个 glibc 2.17 发行版上直接工作。特别是 npm wrapper 仍要求 Node.js 18 或更新版本，而且所使用的 Node.js build 本身也必须支持宿主系统。

Linux arm64 仍保留在发布矩阵中，但本版本暂时不对它做同样的 glibc 2.17 兼容性承诺；在单独迁移 arm64 release builder 之前，它的兼容性仍取决于 native arm64 release host。

## 升级说明

1. 从同一个不可变 v0.3.5 revision 一起升级 `webcodex`、`webcodex-server` 和 `webcodex-runner`。
2. 确认所有 binary 都报告 `0.3.5`、同一个具体 commit，并且 `dirty=false`。
3. 之前因为 glibc requirement 高于 2.17 而无法运行的 Linux x64 用户，可以使用 v0.3.5 native artifact；如果通过 npm 安装，宿主还需要可用且兼容的 Node.js 18+ runtime。
4. Linux arm64 用户不要把本版本的 x64 glibc 2.17 保证直接类推到 arm64。

## Binary packaging

计划发布的 v0.3.5 artifacts：

- `webcodex-v0.3.5-linux-x64.tar.gz`
- `webcodex-v0.3.5-linux-arm64.tar.gz`
- `webcodex-v0.3.5-darwin-arm64.tar.gz`
- `webcodex-v0.3.5-win32-x64.tar.gz`

所有 artifact 都必须从同一个不可变 `v0.3.5` tag 在对应 native host 上构建，并使用同一 release build timestamp。Linux x64 还必须使用专用 glibc 2.17 compatibility builder，并在打包前通过显式 `readelf` ABI gate。

## 已知限制

- glibc 2.17 release floor 当前只对 `linux-x64` 做保证；`linux-arm64` release builder 后续单独处理。
- Node.js compatibility 与 native ELF compatibility 是两件事；npm 安装路径仍要求 Node.js 18 或更新版本。
- macOS x64、Windows ARM64 和其他未发布 targets 仍不在 release artifact matrix 中。

## 发布验证

本次 release-prep 不新增 runtime feature code。验证重点包括 Cargo/npm version 一致性、npm self-test、Markdown link、clean release provenance、已有 source/CI gate，以及新的 Linux x64 `GLIBC_* <= 2.17` artifact 检查。

## 后续

在对 Linux arm64 做同样的 glibc floor 承诺之前，先把它的 release builder 迁移到等价的受控兼容性基线。
