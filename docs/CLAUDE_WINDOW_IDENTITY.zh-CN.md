# Claude Code 窗口标识

[English](CLAUDE_WINDOW_IDENTITY.md)

可选的 [`claude_window_headers.py`](../scripts/claude_window_headers.py)
辅助脚本只为 Claude Code HTTP MCP 的 `headersHelper` 提供
`X-WebCodex-Client-Window`。它不认证请求、不选择 Project 或 Workflow Session，
也不修改 Claude 配置。需要使用支持显式客户端窗口请求头的 WebCodex Server 版本；
仅安装辅助脚本，不会让旧版 Server 自动具备 Window 标识能力。
相关边界见 [Session 模型](agent/session-model.md)。

## 配置辅助脚本

Claude Code 的 [MCP 文档](https://code.claude.com/docs/en/mcp)将
`headersHelper` 定义为输出 HTTP 请求头 JSON 对象的 shell 命令。
Claude 在连接或重新连接时运行它，超时为 10 秒。
使用解释器和脚本的绝对路径，避免 shell 工作目录变化影响命令。
需要 Python 3.9 或更高版本。

在你自己的 Claude 配置中，为目标服务器条目添加辅助脚本，并沿用现有服务器
URL 与认证配置。例如：

```json
{
  "mcpServers": {
    "webcodex": {
      "type": "http",
      "url": "https://YOUR-WEBCODEX-SERVER/mcp",
      "headersHelper": "/usr/bin/python3 /absolute/path/webcodex/scripts/claude_window_headers.py"
    }
  }
}
```

路径包含空格时，按 Claude 所用 shell 的规则引用绝对路径。
示例只增加自定义窗口请求头。脚本绝不读取或输出 `Authorization` 请求头。
Claude 的辅助脚本请求头会覆盖同名静态请求头；辅助脚本提供
`Authorization` 会关闭 Claude 的 OAuth 回退，因此不要在此脚本中加入它。

## 所有平台均支持显式标识

启动目标 Claude 进程前设置 `WEBCODEX_CLIENT_WINDOW_ID`。
显式值必须精确匹配 ASCII `[A-Za-z0-9._:-]{1,256}`；不接受空格、
自动去除首尾空白、Unicode、空值或超长值。变量存在但无效时，脚本输出
`{}`，不会回退到自动检测。

为每个需要独立关联的窗口生成新的高熵值。在 POSIX shell 中：

```sh
export WEBCODEX_CLIENT_WINDOW_ID="$(python3 -c 'import secrets; print(secrets.token_hex(32))')"
claude
```

在 PowerShell 中：

```powershell
$env:WEBCODEX_CLIENT_WINDOW_ID = [guid]::NewGuid().ToString("N")
claude
```

重新连接期间保持该值不变。启动独立窗口前生成新值。同一 shell 启动的进程会
继承同一个值，直到该值被更改；全局固定值会合并这些进程的窗口关联。
该值只是一个不透明关联标签，不应包含凭据、账户名、工作目录或对话标题。

Windows 和 macOS 只使用显式标识。未设置时脚本输出 `{}`；
普通工具仍可在没有 Window 关联的情况下运行。

## Linux 自动进程标识

未设置显式变量时，脚本通过有界的 `/proc/<pid>/stat` 读取及候选可执行文件
符号链接检查，最多遍历 32 层祖先进程。它只接受可识别的 Claude 原生进程，
包括原生安装器使用的带版本号二进制布局，并主动拒绝 Node/Bun 包装器和
无法确定的祖先关系。它读取 Linux boot ID 和 machine ID，再对包含域分隔符、
machine ID、boot ID、Claude PID 和进程启动时钟计数的元组进行哈希。
原始 machine ID、boot ID 和进程信息不会输出。

返回标识前，脚本会重新检查所观察到的祖先链、启动时间、可执行文件、
boot ID 和 machine ID。输入缺失、不可读、过大、格式错误或发生变化时，
输出 `{}`。这是有界的尽力而为进程观察，不是操作系统的原子快照。

同一存活 Claude 进程重新连接时，即使辅助脚本或 shell 进程变化，标识仍保持
一致。新的 Claude 进程、PID 复用、重启系统或更换机器都会改变标识。
**它标识 Claude 进程，不标识对话。** 一个进程处理的多个聊天共享标识；
在新进程中恢复的对话会得到新的自动标识。需要不同边界时请使用显式 ID。

脚本从不读取命令行、其他进程环境、Claude 私有文件、对话记录或认证状态。
失败时只输出一个 JSON 对象，不向 stderr 输出诊断。Python 本身无法启动、
或 Claude 无法调用命令的情况，发生在辅助脚本之外。

## 验证范围

运行不使用凭据的测试夹具：

```sh
python3 -m unittest scripts.tests.test_claude_window_headers -v
```

测试模拟 Linux 进程文件和可执行文件链接，覆盖重新连接、进程替换、
畸形数据、权限失败及竞争变化。命令输出测试通过显式测试 ID 实际调用脚本。
这些夹具不能证明真实 Claude/Linux 集成已经通过；该结论需要另外观察到
集成运行。测试不读取 Claude 配置或凭据，安装脚本也不会部署或重启 WebCodex。
