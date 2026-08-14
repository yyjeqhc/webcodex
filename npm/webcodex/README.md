# @yyjeqhc/webcodex

[English](#english) | [简体中文](#简体中文)

## English

WebCodex turns ChatGPT, Claude, and other MCP clients into assistants connected
to your own repositories and machines. The npm package installs the native
`webcodex`, `webcodex-server`, and `webcodex-runner` binaries for supported
platforms.

### Install and connect

Supported platforms are Linux x64, Linux arm64, macOS arm64, Windows x64, and native Windows
arm64. Node.js 18 or newer is required by the installer wrapper.

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex connect https://your-server.example
```

`connect` uses the current directory, creates a local profile, starts a detached
Runner, and prints the MCP URL and generated key. Add those values to ChatGPT or
Claude, then ask it to inspect code, edit files, run tests, or work with Git.

The generated key is printed in full only when first created. Keep it and the
generated `agent.toml` out of Git. After a machine reboot, rerun `connect` or
start the profile reported by the first run:

```bash
webcodex agent start --profile <profile>
```

Advanced users can provide `--key-file`, `--key`, or `--project`.

### Package integrity

The npm package exposes one public command: `webcodex`. During installation it
downloads the matching release artifact, verifies its SHA-256 checksum, checks
that all three binaries share one version and build identity, and atomically
replaces the previous complete binary set. A failed download, extraction,
checksum, or identity check leaves the previous installation intact.

`webcodex-server` and `webcodex-runner` are package-local binaries rather than
separate npm `bin` entries. The public CLI discovers them for `webcodex server`
and `webcodex agent` commands.

### Disclaimer

WebCodex can read and modify files and execute commands inside configured
project boundaries. Use version control and recoverable backups.

## 简体中文

WebCodex 让 ChatGPT、Claude 和其他 MCP client 成为连接到你自己仓库和机器的
专属助手。npm package 会为支持的平台安装原生 `webcodex`、
`webcodex-server` 和 `webcodex-runner` binaries。

### 安装与接入

支持 Linux x64、Linux arm64、macOS arm64、Windows x64 和原生 Windows arm64；installer
wrapper 需要 Node.js 18 或更新版本。

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex connect https://your-server.example
```

`connect` 默认使用当前目录，创建本地 profile，启动 detached Runner，并输出 MCP
URL 与生成的 key。把这些信息添加到 ChatGPT 或 Claude 后，就可以让它查看和修改
代码、运行测试、执行命令或操作 Git。

生成的 key 只在首次创建时完整显示。不要把它或生成的 `agent.toml` 提交进 Git。
机器重启后重新执行 `connect`，或者启动首次输出的 profile：

```bash
webcodex agent start --profile <profile>
```

高级用户可以使用 `--key-file`、`--key` 或 `--project`。

### Package 完整性

npm package 只暴露一个公共命令：`webcodex`。安装时会下载匹配的 release
artifact，校验 SHA-256，确认三个 binary 的版本和 build identity 一致，再原子替换
旧的完整 binary set。下载、解压、checksum 或 identity 校验失败时，旧安装保持不变。

`webcodex-server` 和 `webcodex-runner` 是 package-local binaries，不单独作为 npm
`bin` 暴露；公共 CLI 会在 `webcodex server` 和 `webcodex agent` 命令中发现它们。

### 免责声明

WebCodex 能够在配置的项目边界内读取、修改文件并执行命令，请使用版本控制和可恢复
备份。

## Development verification / 开发验证

Source-level npm tests do not require publish checksums:

```bash
npm --prefix npm/webcodex test
```

Release smoke runs against the release-control-host-staged package that contains the generated publish-ready manifest. Reuse the exact extracted `linux-x64` CI candidate so the smoke does not rebuild release bytes:

```bash
bash scripts/npm_package_smoke.sh \
  --package-dir <STAGE_DIR>/npm-package \
  --binary-dir <EXTRACTED_LINUX_X64_DIR>
```

Omitting `--binary-dir` keeps the development behavior and builds the selected release/debug profile locally.

## License

Apache-2.0. See the repository `LICENSE` file.
