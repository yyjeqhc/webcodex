# @yyjeqhc/webcodex

[English](#english) | [简体中文](#简体中文)

## English

WebCodex connects ChatGPT, Claude, and other MCP clients to repositories and
development tools on your own machines. This npm package installs the native
`webcodex`, `webcodex-server`, and `webcodex-runner` binaries for supported
platforms.

### Fastest first connection

Supported platforms are Linux x64/arm64, macOS arm64, Windows x64, and native
Windows arm64. The installer wrapper requires Node.js 18 or newer. Windows is a
CLI + Runner client platform in this release: local Server runtime and
`webcodex share` are unsupported there, so use `webcodex connect <server-url>`
against a remote Linux Server. The `share` steps below apply to Linux/macOS and
the default public flow also requires
[`cloudflared`](https://developers.cloudflare.com/tunnel/downloads/) on `PATH`.

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

You do not need to run `setup`, `doctor`, or `run` first. `share` configures the
project, starts a local Server + Runner, opens a temporary Cloudflare Quick
Tunnel, and prints the MCP URL and temporary credential.

When it says **WebCodex ready**:

1. In ChatGPT Developer Mode, create a custom MCP app.
2. Paste the printed MCP URL.
3. Choose Access token / API key (Bearer token).
4. Paste the printed Credential.
5. Scan Tools.
6. Try: `Inspect this repository and summarize its structure. Do not make changes.`

The default share is temporary and ends when the command exits. For local-only
MCP debugging, `webcodex share --tunnel none` does not require `cloudflared`.

If you already operate a WebCodex Server, use the long-lived path instead:

```bash
webcodex connect https://webcodex.example
```

`connect` creates a reusable profile and prints the corresponding MCP setup
values. A generated hosted shared key is disclosed in full only on its permitted
first output; status/log commands do not reveal it.

### Package integrity

The package exposes one public command, `webcodex`. During installation it
downloads the matching release artifact, verifies its SHA-256 checksum, checks
that all three binaries share one version/build identity, and atomically
replaces the previous complete binary set. A failed download, extraction,
checksum, or identity check leaves the previous installation intact.

`webcodex-server` and `webcodex-runner` are package-local binaries rather than
separate npm `bin` entries. The public CLI discovers them for `webcodex server`
and the compatibility `webcodex agent` Runner-management namespace.

### Disclaimer

WebCodex can read and modify files and execute commands inside configured
project boundaries. Use version control and recoverable backups.

## 简体中文

WebCodex 把 ChatGPT、Claude 和其他 MCP client 连接到你自己机器上的仓库与开发工具。
npm package 会为支持的平台安装原生 `webcodex`、`webcodex-server`、
`webcodex-runner` binaries。

### 最快的第一次接入

支持 Linux x64/arm64、macOS arm64、Windows x64 与原生 Windows arm64；installer
wrapper 需要 Node.js 18 或更新版本。本版本的 Windows 是 CLI + Runner 客户端平台，
不支持本地 Server runtime 或 `webcodex share`；Windows 请使用
`webcodex connect <server-url>` 连接远程 Linux Server。下面的 `share` 步骤适用于
Linux/macOS，默认公网流程还需要把
[`cloudflared`](https://developers.cloudflare.com/tunnel/downloads/) 安装到 `PATH`。

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

第一次不需要先执行 `setup`、`doctor` 或 `run`。`share` 会配置项目、启动本地 Server +
Runner、打开临时 Cloudflare Quick Tunnel，并输出 MCP URL 与临时 credential。

出现 **WebCodex ready** 后：

1. 在 ChatGPT Developer Mode 创建 MCP custom app。
2. 填入输出的 MCP URL。
3. 认证选择 Access token / API key（Bearer token）。
4. 填入输出的 Credential。
5. Scan Tools。
6. 第一条可先说：`检查这个仓库并总结它的结构。先不要做任何修改。`

默认 share 会在命令退出时结束。仅做本地 MCP 调试可用
`webcodex share --tunnel none`，此时不需要 `cloudflared`。

如果你已经运营一个 WebCodex Server，再使用长期路径：

```bash
webcodex connect https://webcodex.example
```

`connect` 会创建可复用 profile 并输出对应 MCP 配置。自动生成的 hosted shared key 只会在
允许的首次输出中完整显示；status/log 不会泄露它。

### Package 完整性

package 只暴露一个公共命令 `webcodex`。安装时会下载匹配的 release artifact、校验
SHA-256、确认三个 binary 版本/build identity 一致，再原子替换旧 binary set。下载、
解压、checksum 或 identity 校验失败时，旧安装保持不变。

`webcodex-server` 与 `webcodex-runner` 是 package-local binaries，不单独作为 npm `bin`
暴露。公共 CLI 会通过 `webcodex server` 和兼容保留的 `webcodex agent` Runner 管理
命名空间发现它们。

### 免责声明

WebCodex 能够在配置的项目边界内读取、修改文件并执行命令，请使用版本控制和可恢复备份。

## Development verification / 开发验证

```bash
npm --prefix npm/webcodex test
```

Release smoke uses the release-control-host-staged package and exact extracted
binary candidate; see the repository release documentation for the full command.

## License

Apache-2.0. See the repository `LICENSE` file.
