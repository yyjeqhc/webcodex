# WebPi

**WebPi 是面向网页 AI Agent 的本地优先编码运行时。** 它让 ChatGPT / Custom GPT 能在项目真实所在的机器上使用仓库、Git、测试、进程、浏览器/桌面只读观察、artifact，以及 Pi 原生扩展生态，而不是把代码搬到另一套托管工作区。

WebPi 是独立产品和独立部署。对外支持的原生程序是 `webpi`、`webpi-server`、`webpi-runner`；已验证的 Windows Standalone 安装使用 `webpi.cmd`。

> 为兼容已有实现，部分 `webcodex-*` Rust crate、wire/resource 标识、数据库文件名、credential prefix，以及已发布的 `@yyjeqhc/webcodex-plugin-sdk` 仍会保留旧兼容名称。面向用户的命令、配置、运行时消息和产品文档统一使用 WebPi。

## 为什么用 WebPi

- **代码留在自己的机器上** —— 直接使用现有仓库和工具链。
- **受保护的修改** —— 项目根、敏感路径、stale-write fence、精确文件编辑、scope 检查和 Git 审查都是一等能力。
- **真实开发工具** —— 可以运行测试、编译器、格式化器、语言服务、耐久 Job 和项目自己的命令。
- **长任务可持续** —— Workflow Session、Job、验证证据和可恢复 observation 不受单次模型回复寿命限制。
- **桌面/浏览器默认只读** —— Computer / Browser 可以只开放观察能力，不自动获得鼠标、键盘、剪贴板或应用启动权限。
- **高清 artifact 传输** —— 截图等二进制结果可保存为项目 artifact，并通过短时一次性 HTTPS capability 下载，避免把大图 base64 塞进 Action JSON。
- **Pi 原生生态，但不启动第二个模型循环** —— 第一方 `pi-bridge` 承载 Pi resource/package/extension，执行权限仍由 WebPi 管理。

## 快速开始——已验证 Windows Standalone

在 WebPi checkout 中：

```powershell
.\webpi.cmd status
.\webpi.cmd doctor
.\webpi.cmd run
```

遇到问题先跑 `doctor`。健康且源码/二进制一致的部署应显示 Server、Runner 在线，配置了 Pi Bridge 时状态为 ready，并且 source alignment 为 `aligned`。

首次本地配置：

```powershell
.\webpi.cmd init
.\webpi.cmd enroll
.\webpi.cmd doctor
```

私有运行状态保存在 `.webpi-state/`，便携运行时资源放在 `.webpi-runtime/`。不要把 PAT、bootstrap credential、Tunnel credential 或私钥写进提示词、日志或 Git。

### 接入 ChatGPT Actions

如果已有 HTTPS 域名转发到本机 WebPi Server，显式登记这个 public origin：

```powershell
.\webpi.cmd cloudflare-config https://webpi.example.com
.\webpi.cmd doctor
.\webpi.cmd verify --base-url https://webpi.example.com --expect-public-origin https://webpi.example.com --with-action-token
```

然后导入：

```text
https://webpi.example.com/openapi.json
```

在客户端把 WebPi Action PAT 配置为 Bearer 认证，**不要把 PAT 发到聊天里**。Public origin 使用 `WEBPI_PUBLIC_URL`；公网可达不代表受保护执行接口变成匿名，Bearer/scope 检查仍然存在。

## 推荐 Agent 工作流

1. **先确定精确权限边界** —— 用 `work_on_project` 获取真实 Project 和 Workflow Session，绝不猜 id。
2. **修改前先观察** —— 先看 runtime、Git/workspace 状态、相关源码和工具/扩展 schema。
3. **优先最窄的结构化工具** —— 精确文件/Git/process/validation 工具优先于 shell；直接 Action 优先于泛化 gateway。
4. **一次只改一个有界问题** —— 不碰无关工作；如果 mutation 返回 `outcome_unknown`，先检查真实状态，再决定是否重试。
5. **证明结果** —— 先跑最小定向测试，再跑相关完整 gate；HTTP 200、进程启动或“重试成功”都不等于功能正确。
6. **独立看 diff** —— 在宣称完成前检查 changed paths 和验证证据。
7. **清楚收尾** —— 说明改了什么、验证了什么、剩余风险、用户是否还需要操作。

详见 [WebPi Agent 指令](docs/WEBPI_GPT_INSTRUCTIONS.md) 和 [WebPi 提示词手册](docs/WEBPI_PROMPTS.md)。

## 核心能力

| 领域 | WebPi 能力 |
| --- | --- |
| 项目文件 | 有界 read/search、精确受保护 edit、敏感路径策略 |
| Git | status、diff/review、references；只有明确授权时才 commit |
| 进程 | 结构化命令、必要时 shell、耐久 Job、terminal attention |
| 代码智能 | symbols、definition、references、diagnostics、验证证据 |
| Session | Workflow Session provenance、消息、goal/task、handoff 恢复 |
| Computer | display/window 发现、只读 snapshot、artifact-backed 高清桌面截图 |
| Browser | target/DOM 观察；launch/control 是独立权限 |
| Artifact | metadata/inspect、受限项目 artifact、一次性 HTTPS 下载 |
| Plugin | inspect/describe/invoke 与 management 分权 |
| Pi 生态 | resource/skill/prompt/package、精确 candidate fingerprint、approve/reload/revoke |

## 安全默认值

WebPi 有意把“观察”和“控制”、“发现”和“执行”分开：

- `computer:display_read` 不会自动获得鼠标/键盘/launch 或 clipboard。
- `browser:read` 不会自动获得 browser control 或 launch。
- Plugin inspect/invoke 不等于 plugin management。
- Pi extension discovery 不会自动 import 未批准可执行代码。
- package install/update/remove 可能运行 lifecycle script，属于 consequential 操作，需要显式确认/信任。
- mutation 出现 `outcome_unknown` 时绝不盲目重复执行。
- 删除、credential rotation、release/push、trust elevation、大范围覆盖仍由用户显式决定。

详见 [认证与凭据边界](docs/WEBPI_AUTH_BOUNDARIES.md) 和 [Pi parity](docs/WEBPI_PI_PARITY.md)。

## Pi 与 Native Tool Plugin

第一方 `pi-bridge` 在不启动第二个推理 Agent 的情况下暴露 Pi 资源。推荐扩展流程：

```text
发现 candidate
→ 审查源码/依赖/权限
→ 核对 exact candidate id + SHA-256
→ 批准该 fingerprint
→ reload resource
→ list/describe 精确工具
→ invoke
→ 验证
```

扩展内容一旦改变，fingerprint 也改变，必须重新审查和批准。

编写 Native Tool Plugin 的常用流程：

```text
webpi plugin init ...
webpi plugin check ...
webpi plugin reload ...
webpi plugin list ...
webpi plugin describe ...
```

已发布 SDK 当前仍保留兼容包名：`@yyjeqhc/webcodex-plugin-sdk`。

## 文档

- [WebPi 运行指南](docs/WEBPI.md)
- [品牌、配置隔离和迁移](docs/WEBPI_IDENTITY.md)
- [认证与凭据边界](docs/WEBPI_AUTH_BOUNDARIES.md)
- [WebPi ↔ Pi 能力契约](docs/WEBPI_PI_PARITY.md)
- [WebPi + Pi 生态策略与候选包清单](docs/WEBPI_PI_ECOSYSTEM.md)
- [WebPi Agent / System 指令](docs/WEBPI_GPT_INSTRUCTIONS.md)
- [研究、Feature、Bugfix、Review、Release 提示词](docs/WEBPI_PROMPTS.md)
- [上游与兼容策略](docs/WEBPI_UPSTREAM.md)

`docs/` 里的历史审计、部署和切换记录只证明对应时间点的事实，不是当前安装说明。

## 开发与验证

修改时先跑定向测试，再跑相关完整 gate。常用 Rust 验证：

```bash
cargo fmt --all -- --check
cargo test -p webcodex --lib
cargo test -p webcodex-tool-contracts --lib
cargo test -p webcodex-cli
```

Windows Standalone 层：

```powershell
C:\Python314\python.exe -m unittest discover -s scripts\webpi\tests -p "test_*.py" -v
.\webpi.cmd doctor
```

生产构建应来自 clean Git revision，让 Server、Runner 与 source-alignment 证据一致。

## 上游与兼容

WebPi 复用了 WebCodex 中经过验证的 hardened 实现资产，但拥有自己的产品身份、运行状态、Server/Runner 部署、公开 schema 和 Pi 集成。内部兼容标识不应机械重命名；边界见 [WEBPI_UPSTREAM.md](docs/WEBPI_UPSTREAM.md) 与 [WEBPI_IDENTITY.md](docs/WEBPI_IDENTITY.md)。

## 许可证

使用 Apache License 2.0，见 [LICENSE](LICENSE)。
