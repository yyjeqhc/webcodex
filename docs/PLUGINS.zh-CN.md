# Native Tool Plugins

[English](PLUGINS.md) | [简体中文](PLUGINS.zh-CN.md)

Native Tool Plugin 允许 WebCodex Runner 把任意本地可执行程序变成工具。Plugin
可以使用 Node.js、Bun、Deno、Python、`uv`、Ruby，也可以是 Rust、Go、C/C++
编译后的二进制。它**不需要**实现 MCP Server，也不需要依赖 MCP SDK。

Plugin 进程只运行在 Runner 所在机器。Server 不会收到它的 command path、argv、
cwd、prepared environment、PID、stderr 或本地凭据。

Native Plugin 是**受信任的本地 executable**。WebCodex 不会 sandbox、隔离、签名或把
不可信 executable 自动变安全；启动 Plugin 与在 prepared Runner environment 下直接运行
这个本地程序具有同等级别的本机信任含义。

## 配置 Plugin

Plugin 使用独立的 `runner.toml` 配置，不复用 MCP provider：

```toml
[shell]
default_profile = "node"

[shell.profiles.node]
program = "bash"
args = ["-lc"]
init_script = """
source ~/.nvm/nvm.sh
nvm use 22
"""

[plugins]
request_timeout_secs = 30

[[plugins.providers]]
id = "repo-tools"
name = "Repo Tools"
command = "node"
args = ["tools/plugin.mjs"]
cwd = "/root/git/example"
profile = "node"
timeout_secs = 30
```

`id`、`name`、`command` 必填；`args`、`cwd`、`profile`、`timeout_secs`
可选。配置 `cwd` 时必须使用绝对路径。

Plugin 不再发明一套 runtime/PATH/env 机制，而是真正复用 Runner 的 shell profile：

1. 优先使用 `plugins.providers[].profile`；
2. 否则使用 `shell.default_profile`；
3. 都没有时使用 Runner base shell environment。

选中的 profile 会先得到 prepared environment snapshot，包括
`shell.path_prepend`、`shell.env`、profile env 和 `init_script`。随后 Plugin
本身按 **native argv process** 启动，不再额外套一层 shell。`node`、`python`、
`uv`、`bun` 这样的 bare command 会从 prepared snapshot 的 `PATH` 解析，而不是
只看 Runner 父进程的 PATH。敏感 WebCodex 进程凭据会被过滤。

Windows 继续使用 Runner 已有的 `PATH` / `PATHEXT` native executable 规则；
`.cmd` / `.bat` 需要 shell 语义，因此 Native Plugin ABI 会明确拒绝它们。

## Startup 与 Dynamic 两个平面

Plugin 只有两个状态平面，没有额外 draft/stable/published 状态。

### Startup

Runner 启动时读取 `[plugins]`，为每个 provider 准备环境并启动进程，依次完成
`initialize`、**一次** `tools/list` 和 bounded schema validation，然后 canonicalize
catalog，并把它冻结到 exact `provider_instance_id`。成功 admission 的 provider 进程会
保持 persistent，但普通 list/describe/call 不会再要求同一个 provider instance 重新
执行 `tools/list`。

冻结后的 provider catalog 在这个 provider-instance 生命周期内 immutable。canonical
validated catalog 会生成稳定 SHA-256 digest，并保留 exact Tool count，供 stale detection、
audit 和后续 surface pinning 使用。catalog 只能通过 reload/restart 产生新的 provider
instance 后改变；startup plane 本身在整个 Runner process lifetime 中 immutable。修改 Plugin 源码、
`runner.toml` 或执行 `plugin_tool reload` 都不会改变一级工具。只有 Runner restart
才会创建新的 Runner/provider instance 并重新做 startup admission。

完整 provider catalog 与 startup direct-eligible subset 是两个不同 contract。
`PLUGIN_STARTUP_MAX_DIRECT_TOOLS`（当前为 64）只是 Runner 对 direct subset 的安全上限，
不是“provider 最多只能有 64 个 Tool”，也不是每个 model/surface 的 routing budget。只要
完整 provider catalog 仍在自身有界 contract 内，即使 direct subset 没有被 admission，
它仍可通过 `plugin_tool` 使用；Server-side surface budget 属于独立 governance 层。

如果 startup tool 名称合法、schema 在边界内、在当前 caller-visible startup
inventory 中唯一，并且没有和 WebCodex reserved tool 冲突，它会直接进入 MCP
`tools/list`。模型可以直接调用：

```text
search_symbol({"query":"RunnerRegistry"})
```

重名或 reserved-name 冲突不会做一级暴露，但仍然可以在 `plugin_tool` dynamic
流程中，于 describe 阶段明确指定 Runner + Plugin 后获得 binding 使用。

单个 startup provider 启动失败不会拖死整个 Runner registration。已经 admission 的
startup provider 如果之后失效，WebCodex 会 retire 这个 exact provider instance，
direct call fail closed；不会在同一个 identity 下静默重启。

### Dynamic

已提交 Plugin 的 canonical discovery 使用三级 ladder：

```text
plugin_tool(action="list")
    -> 当前 caller 可见且支持 Plugin 的 Runners
plugin_tool(action="list", runner="my-runner")
    -> 该 exact Runner 当前 effective providers
plugin_tool(action="list", runner="my-runner", plugin="repo-tools")
    -> 该 exact provider 当前有界 tool name/title
plugin_tool(action="describe", runner="my-runner", plugin="repo-tools", tool="search_symbol")
    -> { ..., "binding": "wc_pbind_..." }
plugin_tool(action="call", binding="wc_pbind_...", arguments={"query":"foo"})
```

`list(runner, plugin)` 只观察已经 committed/effective 的 runtime provider。它先解析
caller-visible exact Runner 和 provider instance，然后直接读取这个 instance 的 frozen
catalog；**不会**再向 provider 发出 `tools/list` round trip。它也不会重新读取
`runner.toml`、启动 disposable candidate、调用 `check`、reload provider、修改 dynamic
overlay、创建 binding 或改变 `firstClassRestartRequired`。如果 exact Runner/provider 已被
replacement，discovery 会 fail closed 并要求重新 list；WebCodex 不会把同名 logical
provider 静默重解析到 replacement，也不会 replay。完整 schema 仍然只能由 `describe`
观察；list 只返回 tool name、可选 title 等有界 discovery metadata。

provider list 中的 `status` 只表示当前 effective runtime health。logical provider 如果也存在于
frozen startup catalog，则 `startupAdmission` 单独表示冻结的 startup admission（`direct`、
`secondary` 或 `failed`），必要时带 `startupAdmissionCode`。dynamic reload 不会重写这份
startup admission。即使 `startupAdmission=direct`，也**不保证**所有 caller 最终都能在 MCP
`tools/list` 看见一级工具：Server 仍会因为 reserved name 或 caller-visible duplicate Plugin tool
name 抑制 direct exposure。

`check` 是开发时 `reload` 之前推荐使用的预检。Runner 会重新读取当前
`runner.toml`，只定位 requested provider，使用正常 Plugin runtime 的同一套
shell/profile environment preparation 和 native executable resolution，**真实启动**配置的
executable，完成 `initialize`、`tools/list` 与普通 Plugin protocol/bounds validation，随后
终止整个 disposable candidate process tree。WebCodex 在 check 中绝不会调用 provider
`tools/call`，也不会把 candidate 提交到 dynamic overlay。由于 Native Plugin 本身就是任意
本地 executable，其 startup/initialize/list 自身仍可能产生外部副作用，因此 `check` 不是
纯静态配置 lint。

成功预检返回 `ready=true`，tool summary 只包含 name 和可选 title。坏 candidate 通常仍然
表示“check 操作成功完成了诊断”，因此返回 `ready=false`、结构化 `phase`、稳定 `code`
以及有界、由 WebCodex 生成的安全 `detail`，而不是把所有坏 Plugin 都变成
`plugin_tool isError=true`。Tool definition validation failure 还会带一个很小的
`diagnostic`：使用有限、稳定的 WebCodex-defined code，并在安全时携带 validated tool name
以及 `inputSchema` 等有限 field label。diagnostic 只来自 WebCodex protocol parser/validator；
不会透传 raw serde error、protocol line、schema fragment、Plugin stdout/stderr、executable
配置、environment 或 process identity。Plugin stderr 始终留在 Runner 本机。Runner 只为
live provider 与最近一次 disposable `check` candidate 保留 bounded、control-sanitized 的
本机 stderr ring；这个 local projection 不属于 Plugin gateway response。

`startupToolShape.eligible=true` 只表示这个 checked provider 自己的 Tool definitions 满足
更严格的 provider-local startup Tool bounds；它**不保证**最终成为一级 MCP tool。实际
startup admission 还受 whole-catalog bounds 影响，而 Server 一级暴露还取决于 reserved
name 与 caller-visible name uniqueness。

`call` 的 dispatch identity 只有这个 opaque `binding`；call 阶段不再接受
`runner`、`plugin`、`tool` 作为路由字段。每次 describe 都会创建独立 binding，精确
记录当时观察到的 Runner instance、provider instance、tool name 和 schema；handle
本身不会编码或暴露这些内部 identity。

`reload` 是 Windows、macOS、Linux 都可用的 authoritative reload 入口。Runner
自己重新读取自己的 `runner.toml`；Server 不会上传 executable、env 或 raw Plugin
config。candidate management 在 `check` 与 `reload` 之间共同串行：第二个 check 返回
`plugin_check_busy`，reload 保持已有 `plugin_reload_busy`；都以 `NotStarted` 明确拒绝，
不排队等待。candidate 准备期间不会长期持有 current dynamic/provider session，因此
list/describe/call 与 startup direct tools 仍可继续使用当前已提交 provider。

changed provider 会先准备 candidate，只有 initialize/list 成功后才替换旧 dynamic
instance；candidate 失败会保留已有可工作的 dynamic instance。被删除的 provider 会从
dynamic view 中消失。这些操作都不会修改 frozen startup catalog。

标准开发闭环是：

```text
修改 Plugin code/config
    -> plugin_tool check
    -> 按 bounded diagnostic 修复直到 disposable candidate ready
    -> plugin_tool reload
    -> plugin_tool list(runner, plugin)
    -> plugin_tool describe
    -> 收到 opaque binding
    -> plugin_tool call(binding, arguments)
    -> 调试完成
    -> restart Runner
    -> 新 startup admission / 一级工具 promotion
```

如果 startup provider A 已经提供一级 `search_symbol`，reload 后产生 dynamic provider
B，那么直接 `search_symbol(...)` 仍然调用 A；新的 dynamic `describe` 会观察 B，返回的
binding 也只会调用那个 exact B instance。只有 Runner restart 才能替换一级 startup
绑定。

在 reload 前运行 `check` 不会替换 A 或当前 dynamic provider，不会改变
`firstClassRestartRequired`，不会创建 binding，也不会改变 direct MCP tool inventory。
成功的 check candidate 会被销毁，不会偷偷复用成下一次 reload provider。

## WebCodex Plugin Protocol v1

Native protocol 使用 newline-delimited JSON-RPC 2.0 framing，protocol version 是
`webcodex-plugin-v1`。每个 request/response 各占一行 JSON。它是 WebCodex Plugin
Protocol，不是 MCP。

初始化：

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"webcodex-plugin-v1"}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"webcodex-plugin-v1"}}
```

发现工具：

```json
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

调用工具：

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add","arguments":{"a":2,"b":5}}}
```

```json
{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"7"}],"structuredContent":{"sum":7},"isError":false}}
```

Tool definition 支持 `name`、可选 `title`、`description`、`inputSchema`、可选
`outputSchema` 和 `annotations`。v1 result 只支持 text content、可选 object
`structuredContent` 和 `isError`。暂不支持 image/resource、sampling、callback、
streaming、pagination 或 arbitrary JSON-RPC tunneling。

message、schema、arguments、JSON depth/node/string、tool count 和 result 都有明确边界；
非法或超限数据 fail closed，不会通过截断把它变成另一份 Tool contract。

### Native Plugin Schema Profile v1

`inputSchema` / `outputSchema` 使用 WebCodex 明确定义的小型 profile，**不宣称**支持完整
JSON Schema 2020-12。每个 schema node 都必须有单一字符串 `type`，支持：`object`、
`array`、`string`、`number`、`integer`、`boolean`、`null`。支持的 keyword 只有：

- 所有 node：`type`，以及可选 `title`、`description`、`enum`、`const`；
- object：`properties`、`required`、boolean `additionalProperties`；
- string：`minLength`、`maxLength`；
- array：`minItems`、`maxItems`、`items`。

input/output schema root 必须是 `type: "object"`。property 数量、required 数量、enum
长度、schema bytes/depth/nodes/string 以及声明的 length/item bounds 都有有限上限。未知
keyword 会在 provider admission 时明确拒绝，不会 silently ignore。v1 特别**不支持**
`$ref` / remote ref、`$defs`、recursive ref、`pattern`、`format`、numeric
`minimum` / `maximum`、schema 形式的 `additionalProperties`、union `type`、`anyOf`、
`oneOf`、`allOf`、`not` 或任意 draft-specific keyword。

完整无依赖 Node 示例见
[`examples/native-tool-plugin.mjs`](../examples/native-tool-plugin.mjs)。

## 调用与失败语义

每个 provider 同一时间只处理一个 request。并发请求会得到 provider-busy，而不是无限
排队。

`tools/call` 进入 provider connection 之前，WebCodex 会先从 exact frozen catalog
解析 Tool、核对 caller 的 exact schema observation，再使用 frozen input schema 校验
`arguments`。这里任何失败都是 `NotStarted`，provider executable 会看到 **0 个**
`tools/call` byte；call path 也不会执行 `tools/list`。provider response 返回后仍执行已有
text/result bounds；如果 Tool 声明了 `outputSchema`，则 `structuredContent` 必须存在并
匹配 exact frozen output schema。违约属于已完成 dispatch 后的 provider contract failure，
WebCodex 会 fail closed 并 retire 该 provider instance。

`plugin_tool call` 必须使用前一次 `describe` 返回的 opaque binding。binding 是
Server 端有界保存的一次 exact observation，不是 bearer authorization token：每次 call
仍然重新要求当前 credential 具有 `plugin:invoke`，并且当前 caller 仍有权访问对应 logical
Runner。binding 也可能因为容量上限被 eviction。Runner/provider instance 被替换、tool 被
删除或 schema 改变时，旧 binding 以 `NotStarted` fail closed，必须重新 describe；
WebCodex 不会把它 re-resolve 到新的同名 provider/tool，不会自动生成新 binding，也不会
replay。内部 Runner/provider instance id 和 schema revision 不会编码到 handle 里或暴露给
模型。

local frozen-schema preflight 成功后，provider timeout 才作为一次 RPC 的总预算开始：
它从 provider request encode 前开始，同时覆盖 bounded stdin queue admission、完整
frame 的 write + flush confirmation，以及 response wait。因此 Plugin 即使停止读取 stdin，
`write_all` 也不能逃逸 provider timeout。对于
effectful `tools/call`，只要 frame 已经可能开始写入，write timeout/failure、connection
loss、process death 或 response timeout 都返回 `OutcomeUnknown`，不会自动 retry/replay；
只有能证明任何 byte 都不可能发送的失败才是 `NotStarted`。

stdout 只用于 protocol。stderr 由独立 worker 持续 drain，因此 stderr flood 不会反向
阻塞 stdout protocol。local ring 最多保留 64 行、每行 1 KiB、aggregate 32 KiB；control /
非 UTF-8 byte 会做安全 projection，超长行会标记 truncated。stderr 不会被当作 protocol，
不会进入 model ToolResult 或 Workflow Session ledger，也不会自动发送到 Server / startup
registration catalog。

## OAuth

Native Plugin authority 按 operation 拆分：

- `plugin:inspect` 允许 list、describe 等纯 metadata observation；
- `plugin:invoke` 允许 `plugin_tool call` 和 startup Plugin 一级工具；
- `plugin:manage` 允许会启动或改变本地 Plugin process 的开发/管理操作，目前是 check 和
  reload；它本身不会隐式授予 `plugin:invoke`；
- 以上 scope 都不属于 direct shared-key model baseline；
- 使用 shared-key OAuth bridge 时，需要显式
  `webcodex connect ... --auth oauth --oauth-local-plugins`；该 opt-in 只授予
  `plugin:inspect` + `plugin:invoke`，绝不会授予 `plugin:manage`。

`mcp:local` 不授予 Plugin 权限；Plugin scopes 也不授予 Runner-owned MCP provider 权限。
effectful Plugin operation 如果携带显式 `recording_session_id`，还必须通过与其他
consequential WebCodex execution 相同的 Workflow Session guard 和 authority-mode
permission policy；WebCodex 不会从 MCP transport identity 推断 Workflow Session。

## 排障

Plugin 无法启动时，优先检查 profile、prepared `PATH`、绝对 `cwd`、runtime executable，
以及 stdout 是否只包含 protocol JSON line；本地诊断写 stderr。

如果 dynamic 修改通过 `plugin_tool` 已经生效，但一级工具仍然是旧行为，这是预期语义：
restart Runner 后才会生成新的 startup catalog 并完成 promotion。

如果 tool 能通过 `plugin_tool` 使用，却没有直接出现在 MCP `tools/list`，检查当前
caller-visible inventory 是否存在同名 Plugin tool，或是否与 WebCodex reserved name
冲突。
