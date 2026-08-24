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
against a remote Linux Server. The `share` steps below apply to Linux/macOS.
For the default public flow, WebCodex reuses `WEBCODEX_CLOUDFLARED_BIN` or a
`cloudflared` on `PATH`, and otherwise downloads a pinned, verified managed copy.
The managed download reuses npm proxy/`noproxy`/CA/`strict-ssl` configuration when
launched through this npm wrapper, while standard proxy/system trust remains the
fallback when npm-specific settings are absent.

Try it without a global install:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

Or install it once and use the same bare first-run entry:

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex
```

If npm/npx did not retain postinstall output, the wrapper lazily runs the same
verified native installer before launching. Bare `webcodex` auto-dispatches to
`share` only in an interactive Linux/macOS Git checkout; use explicit
`webcodex share` for scripts or deterministic dispatch.

You do not need to run `setup`, `doctor`, or `run` first. `share` prepares the
tunnel dependency when needed, configures the project, starts a local Server +
Runner, opens a temporary Cloudflare Quick Tunnel, and prints the MCP URL and
temporary credential.

When it says **WebCodex ready**, public share best-effort copies the MCP URL but
never the credential. In an interactive terminal, press Enter to open ChatGPT
App settings. Then:

1. Enable Developer Mode and go to Settings -> Apps -> Create.
2. Paste the copied MCP URL, or copy the printed fallback URL.
3. Choose Access token / API key (Bearer token).
4. Paste the printed Credential.
5. Scan Tools.
6. Try: `Inspect this repository and summarize its structure. Do not make changes.`

Use `webcodex share --no-copy-url` to disable clipboard access.

The default share is temporary and ends when the command exits. For local-only
MCP debugging, `webcodex share --tunnel none` does not require `cloudflared`.
ChatGPT Developer Mode, custom MCP apps, and write/modify actions are controlled
by the ChatGPT plan, workspace, and admin settings; WebCodex cannot widen those
client-side permissions.

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
The artifact download honors npm lifecycle proxy, `noproxy`, `cafile`/`ca`, and
`strict-ssl` settings, so installations behind the same corporate proxy or CA
configuration used by npm do not need a separate WebCodex network setup.

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
Linux/macOS。默认公网流程会优先复用 `WEBCODEX_CLOUDFLARED_BIN` 或 `PATH` 中已有的
`cloudflared`，否则自动下载固定版本、校验后由 WebCodex 管理。
通过 npm wrapper 启动时，这次 managed 下载会复用 npm proxy、`noproxy`、CA 与
`strict-ssl` 配置；没有 npm-specific 配置时继续使用标准 proxy/系统信任路径。

无需全局安装即可试用：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

也可以全局安装后使用同样的裸 first-run 入口：

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex
```

如果 npm/npx 没有保留 postinstall 输出，wrapper 会在启动前 lazy 执行同一套经过校验的
native installer。裸 `webcodex` 只在 Linux/macOS 的交互式 Git checkout 中自动进入
`share`；脚本或需要确定性分发时继续显式使用 `webcodex share`。

第一次不需要先执行 `setup`、`doctor` 或 `run`。`share` 会在需要时先准备 tunnel 依赖，
然后配置项目、启动本地 Server + Runner、打开临时 Cloudflare Quick Tunnel，并输出 MCP URL
与临时 credential。

出现 **WebCodex ready** 后，公网 share 会 best-effort 复制 MCP URL，但绝不会自动复制
credential；交互式终端可以按 Enter 打开 ChatGPT App 设置。然后：

1. 开启 Developer Mode，进入 Settings -> Apps -> Create。
2. 粘贴已复制的 MCP URL；失败时使用终端打印的 fallback URL。
3. 认证选择 Access token / API key（Bearer token）。
4. 填入输出的 Credential。
5. Scan Tools。
6. 第一条可先说：`检查这个仓库并总结它的结构。先不要做任何修改。`

使用 `webcodex share --no-copy-url` 可以关闭剪贴板访问。

默认 share 会在命令退出时结束。仅做本地 MCP 调试可用
`webcodex share --tunnel none`，此时不需要 `cloudflared`。ChatGPT Developer Mode、custom
MCP app 与 write/modify action 受 ChatGPT 套餐、workspace 和管理员设置控制；WebCodex
不能扩大这些客户端侧权限。

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
artifact 下载会继承 npm lifecycle 中的 proxy、`noproxy`、`cafile`/`ca` 与
`strict-ssl` 配置，因此使用企业代理或私有 CA 时不需要再单独配置 WebCodex 网络层。

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
