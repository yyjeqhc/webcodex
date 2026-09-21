# WebPi 产品身份、配置隔离与升级

本页是 2026-09-19 命名空间修复后的操作契约。早期审查和部署进度文件是历史记录，不代表当前实例已切换。当前支持目标是 Windows 本机 WebPi Server + Runner + Pi bridge + 网页控制台；新二进制构建不等于生产部署。

## 明确分开的运行身份

| 层次 | 新 WebPi 行为 |
| --- | --- |
| 原生程序 | `target/dogfood/webpi.exe`、`webpi-server.exe`、`webpi-runner.exe` |
| 配置键 | `WEBPI_*`；原生入口在创建线程前移除继承的 `WEBCODEX_*` |
| 专用启动器 | `webpi.cmd` 只使用当前根目录下的新程序；Server/Runner/admin 子进程不继承两种产品的配置变量，从明确的 WebPi 文件配置 |
| 默认本机 HTTP | `127.0.0.1:56542`，不会默认绑定公网 |
| 独立安装状态 | `.webpi-state`；不复制另一套 WebCodex 的数据库、PAT、Runner 凭据或 Tunnel 凭据 |
| CLI 默认用户目录 | Windows `%APPDATA%/webpi`、`%LOCALAPPDATA%/webpi`；Unix 对应 `/etc/webpi`、`.config/webpi`、`.local/state/webpi` 等 |
| 程序发现 | 内部 Server/Runner 只从同安装目录发现；缺失即失败，不自动搜索 PATH 中另一套程序。支持 `--bin` 的管理命令仍保留显式操作员选定路径；项目入口的 `WEBPI_AGENT_BIN` 仅接受显式绝对路径，设置无效时也不会回退 |
| 对外 API | OpenAPI 标题 `WebPi GPT Actions`；运行状态 `service=webpi`；MCP `serverInfo.name=webpi` |
| 网页存储 | `webpi.runtime.*` / `webpi.admin.*`；不迁移 WebCodex 登录凭据，原网页需要重新输入自己的 WebPi 凭据 |
| Linux 服务 | `webpi.service`、`webpi.socket`、`webpi-runner.service`，不与上游同名服务共用 |

独立命名不是操作系统沙箱。同一用户运行的可信扩展和已授权进程仍拥有该用户权限；不可信扩展需要单独低权限账户或容器/虚拟机隔离。

## 保留的兼容资产

内部 Rust Cargo 包和模块仍保留 `webcodex*` 名称，因此构建使用 `-p webcodex` 等，而不是不存在的 `-p webpi`。TypeScript SDK 仍以本地依赖 `@yyjeqhc/webcodex-plugin-sdk` 导入，已设为 private 防止误发布。

以下不是产品运行身份，不能做机械替换：插件握手 `webcodex-plugin-v1`、现有 Runner wire/ALPN 标识、`wc_pat_*` / `wc_agent_*` / Session 和 binding ID、哈希域、验证身份、已有数据库文件名 `webcodex.db`。保留这些不会使独立实例凭据通用；凭据仍由自己的数据库验证。旧文件名和新文件名的敏感路径防护均保留。

## 新程序的启动安全条件

Server 缺少非空 `WEBPI_TOKEN`，或启用了 anonymous、direct shared-key、OAuth shared-key bridge、MCP query-token 模式时，在建立监听器前拒绝启动。Runner 必须有自己的非空 transport token。`server init` 不再生成宽松认证模式，拒绝 `--open`。

原生程序不自动读取旧 `WEBCODEX_*` 配置文件。`init --overwrite` 遇到旧键也拒绝，不会为了“初始化”悄悄更换旧 bootstrap。配置迁移必须由独立命令明确执行。

`webpi.cmd verify` 检查无效凭据应为 401、有效 PAT 实际调用，以及 WebPi 产品身份；静态 HTML、全路径 403、530、重定向，或者另一产品的正常接口都不算通过。

## 已有独立 WebPi 安装的升级顺序

在实际根目录 `E:\WebPi\webpi-core` 操作。新旧程序可同时存放，但不得同时争用 56542。不要按相同进程名批量杀掉 WebCodex Desktop；核对原实例的路径和进程树。

先停止旧 WebPi Server/Runner，暂不开放 Cloudflare 转发。然后：

```powershell
Set-Location 'E:\WebPi\webpi-core'
.\webpi.cmd migrate-config --check
.\webpi.cmd migrate-config
.\webpi.cmd cloudflare-config https://webpi.piforme.vip
.\webpi.cmd run
```

`migrate-config --check` 只报告是否需要迁移。实际迁移只操作本安装 `.webpi-state/server/webpi.env`：改配置键、不改凭据值和数据目录；保留精确旧文件备份并在写入秘密前复制原 Windows DACL；拒绝冲突别名、重复键、空 bootstrap、逃逸/链接路径和超大文件。旧实例仍在线时命令拒绝实际迁移。不存在 env 文件的全新安装不走该命令，应先按正常 init/enroll 配对流程创建自己的状态。

启动后另开普通用户终端：

```powershell
.\webpi.cmd verify --with-action-token
```

本机通过后，由本地操作员启动自己的 cloudflared，保持其 origin 为 `http://127.0.0.1:56542`，再执行：

```powershell
.\webpi.cmd verify --base-url https://webpi.piforme.vip --expect-public-origin https://webpi.piforme.vip --with-action-token
```

在网页 GPT 重新导入当前 `/openapi.json`，使用独立 WebPi Action PAT 的 Bearer 认证。不要发送 PAT 到聊天、命令参数或其他域名。Cloudflare Actions 路线不需要 OpenAI Secure MCP Tunnel ID，也不要混用 `run-web`。

如需回滚，先停止新实例并保持公网入口关闭，再由本地操作员恢复备份和与其匹配的旧程序/启动脚本；旧宽松认证配置绝不可直接重新公开。审批撤销不能撤回已经发生的文件、网络或进程副作用。

## 构建与验证

2026-09-19 的 Windows 接续构建结果、已修复 CLI 失败、二进制摘要和验证范围见 [本机构建接续验收](WEBPI_BUILD_ACCEPTANCE_2026-09-19.md)。该报告不代替本节后续生产迁移与公网验收。

先准备项目自己的 Node、SDK 和 Pi bridge 依赖，生命周期脚本禁用。原生构建命令：

```powershell
cargo build --profile dogfood --locked -p webcodex -p webcodex-cli -p webcodex-runner --bin webpi --bin webpi-server --bin webpi-runner
python -m unittest discover -s scripts/webpi/tests
python scripts/webpi/http_auth_fixture.py
```

HTTP fixture 只创建全新临时 loopback Server/Runner 和托管 PAT，验证产品名、配置隔离、无效凭据拒绝、插件协议和实际 Pi 文件读取，再停止并清理自己的进程；它不会接管生产服务，也不替代公网验收。

## 未启用的上游交付入口

Desktop 目前不是 WebPi 安装方式：独立 identifier 已准备，打包开关关闭，构建脚本明确拒绝；不能把未迁移的上游桌面功能伪称为已验证的新产品。旧 npm 下载器和命令包装器禁止执行/发布，不会下载上游 WebCodex 来冒充新 WebPi。

旧发布设计仍保留作迁移参考，但当前公开 `release-build.yml` 与 `release-image.yml` 的根 job 被硬编码为 fail-closed，不会构建或发布候选；`release-image` 还保留仓库级全局串行与发布前重新确认 latest stable 的保护，供未来完成独立发布验收后启用。`webpi-checks.yml` 仅做 Windows 校验，无发布/部署写权限。Docker bootstrap 已迁为 WebPi 契约并有合成事务测试，但这不等于远程 Linux/Docker 发布链已完成真实宿主验收。

Dockerfile/Compose 已改为 WebPi 程序、用户、服务和数据卷。Compose 强制要求显式 `WEBPI_SERVER_IMAGE`，不再默认取上游 latest；Docker source build、Linux 服务和其他平台仍需在相应宿主上验收，Windows 测试不证明它们已部署成功。

许可证、上游版权和源代码出处保留。清理产品碰撞不等于抹去来源，也不等于消除所有未知漏洞。
