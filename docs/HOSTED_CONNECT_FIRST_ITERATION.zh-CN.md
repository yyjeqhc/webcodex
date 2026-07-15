# Hosted Connect：第一轮实现与验收边界

> 状态：第一轮可运行纵向切片，2026-07-15。本文只记录已经实现和已经验证的事实，不把缺少真实平台凭据的本地模拟写成公网验收通过。

## 1. 这一轮解决了什么

新的产品入口是：

    webcodex connect <target> --via <ingress>

命令会从当前目录发现 Git worktree root，并在一个前台 supervisor 下完成：

1. 创建或复用项目级私有 state；
2. 启动只监听 `127.0.0.1` 的 WebCodex runtime；
3. 创建彼此分离的 hosted-client PAT 和 agent token；
4. 生成并刷新 agent 配置、注册当前项目、等待 executor online；
5. 完成本地 MCP `initialize` 探测；
6. 对 provider 执行 preflight/doctor，启动官方 tunnel client并等待 readiness；
7. 任一受管子进程意外退出时停止整条连接；Ctrl-C 正常清理全部子进程。

重复运行同一个 project/profile 会复用 user、数据库和凭据，不再创建一套新身份。随机 loopback 端口变化时，agent 配置会被刷新，避免仍连接上次端口。

普通输出不打印 bootstrap、PAT、agent token 或 provider API key。OpenAI/Cloudflare provider 凭据不会继承给 WebCodex server、executor 或项目 shell。

## 2. 先运行无副作用检查

    webcodex connect chatgpt --via openai --dry-run

或：

    webcodex connect gpt-actions --via cloudflare --temporary --dry-run

`--dry-run` 会显示项目、target、ingress、loopback origin、state 位置和阻断项；不会创建目录、凭据或 provider 进程。缺 binary、tunnel id、runtime key 或 Named Tunnel config 时会在启动前明确列出。

## 3. OpenAI Secure MCP Tunnel

适用于 `chatgpt` 和标准 `mcp` target，不用于 GPT Actions：

    export CONTROL_PLANE_TUNNEL_ID=tunnel_0123456789abcdef0123456789abcdef
    # 通过本机 secret manager/安全环境注入 CONTROL_PLANE_API_KEY。
    webcodex connect chatgpt --via openai

要求：

- 官方 [`tunnel-client`](https://github.com/openai/tunnel-client) 在 `PATH`；
- 已在 OpenAI Platform 创建 tunnel id；
- runtime API key 具有该 tunnel 的 Read + Use 权限；
- key 通过 `CONTROL_PLANE_API_KEY` 注入，不作为 WebCodex CLI 参数。

WebCodex 使用 loopback HTTP MCP，并向 `tunnel-client` 配置 `Authorization: file:/...`。文件内容是独立的 hosted-client Bearer，路径可以进入 provider 配置，Bearer 本身不进入 argv。启动前会运行 `tunnel-client doctor --explain`，doctor 不通过就不会显示 Ready。

Ready 后，唯一剩余的平台侧动作是在 OpenAI/ChatGPT connector 设置中选择已经创建的 tunnel。WebCodex 不替用户创建平台账号、workspace 权限或 tunnel。

## 4. Cloudflare Quick Tunnel

第一轮只对 GPT Actions 开放 Quick Tunnel：

    webcodex connect gpt-actions --via cloudflare --temporary

要求官方 `cloudflared` 在 `PATH`。命令会取得随机 `trycloudflare.com` URL，验证公开的 `/openapi.json` 和带认证的只读 Action 请求，再显示 schema URL 和本地 PAT 文件路径。

这是显式的测试路径：URL 每次启动会变化；Cloudflare 官方说明 Quick Tunnel 不支持 SSE。因此当前实现拒绝 `mcp/chatgpt + --temporary`，不会猜测客户端恰好不需要流式传输。

## 5. Cloudflare Named Tunnel

Named Tunnel 由用户先在 Cloudflare 创建，配置必须把稳定 hostname 路由到 WebCodex loopback origin。未指定 `--port` 时固定使用 `127.0.0.1:8787`：

    tunnel: personal
    credentials-file: /home/me/.cloudflared/<tunnel-id>.json
    ingress:
      - hostname: code.example.com
        service: http://127.0.0.1:8787
      - service: http_status:404

连接标准 MCP：

    webcodex connect mcp \
      --via cloudflare \
      --profile personal \
      --public-url https://code.example.com

连接 GPT Actions：

    webcodex connect gpt-actions \
      --via cloudflare \
      --profile personal \
      --public-url https://code.example.com

默认读取 `~/.cloudflared/config.yml`，并把 `--profile` 同时作为 tunnel name。不同布局可显式传 `--cloudflare-config`、`--cloudflare-tunnel` 和 `--port`。WebCodex 不接受 Cloudflare token CLI 参数，也不修改用户的 Cloudflare 账号或 DNS。

MCP target 必须通过公开 hostname 的真实 `initialize` 探测；GPT Actions target 必须同时通过公开 schema 和带认证的只读 Action 探测，否则 fail closed。

## 6. 本地 state 与停止语义

默认 state：

    $XDG_STATE_HOME/webcodex/hosted/<profile>/<project-id>

没有 `XDG_STATE_HOME` 时使用：

    ~/.local/state/webcodex/hosted/<profile>/<project-id>

目录权限收紧为 `0700`，凭据、配置和日志为 `0600`。可以用 `--state-dir` 指定一个专用目录。不要把它放进仓库或同步盘。

`connect` 当前是前台命令，Ctrl-C 停止 runtime、agent 和 tunnel。在 Linux 上，父进程因 SSH/terminal 异常直接消失时，受管子进程还配置了 parent-death signal，避免遗留假在线进程。

## 7. 第一轮验收矩阵

| 路径 | 本轮证据 | 当前结论 |
|---|---|---|
| CLI preset/参数矩阵 | 单元测试覆盖 OpenAI/GPT Actions 冲突、Quick/SSE 冲突、Named HTTPS 要求 | 支持 |
| 私有 state、双凭据、runtime、agent 注册 | 完整本地进程 smoke；重复启动复用身份和凭据 | 支持 |
| 本地 MCP initialize | 真实 WebCodex HTTP MCP 探测 + MCP test lane | 支持 |
| OpenAI doctor/readiness/supervision contract | 本地受控 ingress harness 跑通 doctor、health、退出清理 | 实现完成；等待真实平台验收 |
| OpenAI hosted tools/list/read/write | 当前环境无 tunnel runtime key/tunnel id | 未验收，不能标记支持 |
| Cloudflare Quick 公网 Action 调用 | 当前环境无 `cloudflared` | 未验收 |
| Cloudflare Named 公网 MCP/Action | 当前环境无 Named Tunnel config/hostname | 未验收 |
| MCP/OpenAPI/metadata 一致性 | MCP 53、OpenAPI 51、metadata 118 项定向测试通过 | 支持 |
| 完整 server 回归 | 1607 passed，4 ignored，0 failed | 通过 |

真实 provider 验收应在具备账号权限的机器上完成，并把 tunnel-client/cloudflared 版本、首次连接耗时、tools/list、只读调用、有界写调用、断线重连和 provider 日志中的凭据脱敏结果补回此矩阵。

## 8. 明确没有在这一轮假装解决的事

- 本文提交时，MCP/OpenAPI 仍暴露旧的宽工具面；该边界已由[第二轮 Task Kernel 实现](HOSTED_TASK_KERNEL_SECOND_ITERATION.zh-CN.md)替换。Hosted profile 现在硬切为 8 项 canonical capability，普通 `serve` 的运维 surface 不受影响。
- 本地个人模式仍复用了现有 server + agent 内部实现，只是在一个产品入口下监督；尚未改成同进程 LocalExecutor。
- 没有实现多设备同步或多用户 shared control plane 新模型；本轮只保证凭据角色没有继续混用，为后续 User/Device/ConnectorGrant 分离留下边界。
- 没有内置 LLM、prompt loop、模型选择或推理能力；模型始终来自支持 Connector/MCP/GPT Actions 的线上平台。
