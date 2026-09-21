# WebPi：WebCodex 能力资产取舍

审查日期：2026-09-19。本文记录当前 WebPi 工作树中已复用的相关能力，不代表对 WebCodex 所有历史分支、插件市场或未来版本完成了穷尽审计。

## 架构分工

网页 ChatGPT 是主要推理和编码主体。WebPi Server/Runner 负责鉴权、项目、文件编辑、进程、Job、Git 和审计；`plugins/pi-bridge` 承载固定版本 Pi 的扩展、工具、命令、技能、提示模板、资源加载和包管理。它不是另一个自动运行的 Pi 模型循环。

| 资产 | WebPi 的选择 | 约束与依据 |
| --- | --- | --- |
| Project / Runner / Workflow Session | 保留规范工具和显式身份，不另造会话控制层 | 项目和会话 ID 必须来自实际工具；不能用旧目录、传输连接或提示词推断授权。见 `docs/agent/session-model.md`、`docs/agent/permission-model.md`。 |
| 受保护文件读写与 revision/SHA 防护 | 普通修改仍用 canonical read/edit tools | Pi 自带 write/edit 不另开一条绕过现有写入栅栏的通道。Pi 辅助读取由 `plugins/pi-bridge/src/project-files.ts` 加上物理路径与有界读取校验。 |
| 进程和 durable Jobs | 保留结构化 argv、输出边界、同一次执行的 Job 续查 | 不用超时当成“未执行”，不因上下文丢失重复运行未知结果的命令。 |
| Git 审查、检查点与恢复 | 保留现有工具；每次修改分别审查 | 不自动 reset/clean/rebase；检查点必须实际创建成功才能作为回滚证据。 |
| 托管 PAT、bootstrap 和 Runner token | 复用规范认证和 scope 校验，但关闭公开部署的宽松模式 | `src/auth/shared_key.rs` 的任意非托管 Bearer 租户快速启动模式不适用于当前 WebPi 公网目标。关闭 shared-key、anonymous、OAuth shared-key bridge 和 query-token。 |
| OpenAPI / GPT Actions / MCP | 复用 canonical schema 和 `plugin_tool` 绑定机制 | WebPi 使用自己的公网 origin、凭据和 Runner。Schema 可公开，但受保护工具必须拒绝无效令牌。 |
| Rust Native Plugin 协议 | 保留 `crates/webcodex-core/src/plugin.rs` 的工具、Schema、内容和消息边界 | 图片与文本经过同一受限结果边界；不以自定义旁路绕过校验。 |
| TypeScript Plugin SDK | 复用 `npm/plugin-sdk` | `defineTool`、冻结目录、串行 stdio 调用、混合文本/图片和失败处理维持规范协议。 |
| 验证账本和 Session 交付 | 保留可追溯验证与失败记录 | 测试通过不等于已部署；旧源码上的成功不替代新源码验证；独立 fixture 不等于生产验收。 |
| 便携运行时与依赖固定 | 保留 WebPi 自己的 Node、锁文件和 bootstrap 工具 | 不复制 WebCodex 凭据、数据库或 Runner 身份；搜索工具使用已安装的绝对可执行文件，工具调用时不自动下载。 |
| Pi 原生资源和扩展生态 | 直接使用已固定的 Pi ResourceLoader / ExtensionRunner / PackageManager | 先发现和审查，再批准精确候选与哈希，再 reload、describe、call；包安装脚本属于代码执行。 |

## 不应照搬的部分

不要复制现有 WebCodex Desktop 的部署状态、bootstrap/PAT、Runner transport token、Tunnel 凭据或数据库。不要修改无关 WebCodex 服务，也不要扩大 Runner allowed roots 来绕过当前拒绝。

当前网页主模型架构不宣称实现 Pi TUI、模型切换、provider/model hook、原生 agent/turn/input 循环、完整终端会话树操作。依赖这些能力的扩展需要单独适配；不能返回虚假成功来宣称“完全持平”。

## 新增安全资产

`project-files.ts` 提供跨平台敏感路径校验、链接拒绝、有界文件快照和搜索结果重新读取。`approval-store.ts` 提供精确哈希审批、输入边界、排他更新锁和原子存储。`pi-runtime-host.ts` 在后续执行前重新检查已加载审批，并让失败 reload / shutdown 关闭旧调用权。

`scripts/webpi/security_smoke.py` 是公开暴露前后的 HTTP 负向与正向验收工具。`http_auth_fixture.py` 在新的临时 loopback 实例中验证真实认证，不接触生产服务。它们与单元测试、协议测试各自证明不同层次，不能互相替代。

## 受控自扩展流程

每次扩展只处理一个明确的用户目标：记录目标与验收标准 → 审查已有可复用包 → 固定来源和版本 → 审查安装脚本及权限 → 按用户授权生成/修改代码 → 运行测试并审查 diff → 审批精确 candidate/hash → reload → describe 实际工具 Schema → 调用并验证结果。

出现回归时停止继续扩展，撤销候选以阻止后续调用，再恢复已确认的代码/包版本并重新测试。撤销审批不会自动回滚文件、网络请求、数据库更改或扩展已启动的进程。禁止为了让测试通过或降低调用摩擦而关闭鉴权、跳过验证、扩大目录权限或删除失败证据。

## 操作系统安全边界

运行在同一 OS 用户下的扩展、安装脚本和已授权 shell 并不是彼此隔离的安全主体。候选文件/目录的哈希不覆盖任意动态导入闭包；运行中本地恶意进程的并发改写也不能由 JavaScript 路径检查完整解决。

需要运行不可信扩展时，应使用独立低权限账户或受限虚拟机/容器，限制出站网络和挂载目录；不要把模型拥有的 shell 权限误称为沙箱。当前代码加固降低已发现的路径、审批和配置风险，不构成“没有任何漏洞”的证明。
