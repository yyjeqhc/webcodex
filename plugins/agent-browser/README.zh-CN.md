# agent-browser Native Tool Plugin

`agent-browser` 是 WebCodex 的 first-party Native Tool Plugin，用来调用用户机器上**已经安装好的** [Agent Browser](https://agent-browser.dev/)。

它主要补一个 WebCodex 内建 Browser 有意不覆盖的场景：继承本机 Agent Browser 的配置/profile，或者连接用户已经打开并明确授权 remote debugging 的 Chrome。

它不会再安装一份 Agent Browser，不会自己复制 cookie/凭据，也不会绕过 Chrome 授权。ChatGPT/模型侧仍然只走 WebCodex 已有的 `plugin_tool describe -> call`。

## 构建与测试

需要 Node.js 18+、本机 `agent-browser 0.38.1`。当前版本对未验证的 Agent Browser 版本会 fail closed。

```bash
npm ci
npm run typecheck
npm test
```

可选真浏览器 smoke：

```bash
npm run test:live
npm run test:native
```

两套 smoke 都使用临时/合成浏览器数据，不会读取用户日常 profile。

## 推荐同时配置两个 provider

- `agent-browser`：`connection: "native"`，继承本机 Agent Browser 配置/profile，启动插件自己管理的浏览器；
- `agent-browser-current-chrome`：`connection: "auto"`，只连接用户已经授权 remote debugging 的当前 Chrome。

这样切换模式时不需要修改同一份配置再 reload，而且权限边界很清楚。

示例 Runner 配置：

```toml
[[plugins.providers]]
id = "agent-browser"
name = "Agent Browser"
command = "node"
args = ["/absolute/path/to/webcodex/plugins/agent-browser/dist/plugin.js", "--config", "/private/path/agent-browser-native.json"]
cwd = "/absolute/path/to/webcodex/plugins/agent-browser"
timeout_secs = 90

[[plugins.providers]]
id = "agent-browser-current-chrome"
name = "Agent Browser — Current Chrome"
command = "node"
args = ["/absolute/path/to/webcodex/plugins/agent-browser/dist/plugin.js", "--config", "/private/path/agent-browser-current-chrome.json"]
cwd = "/absolute/path/to/webcodex/plugins/agent-browser"
timeout_secs = 90
```

Native 模式可以读取 Agent Browser 自己的 user/project 配置和 `AGENT_BROWSER_*` 环境；profile 的复制/持久化语义由 Agent Browser 自己负责。Current Chrome 模式连接失败时会直接报错，不会偷偷启动替代浏览器。

## 16 个工具

插件提供：status、connect、tabs、open、navigate、reload、snapshot、click、fill、select_option、set_checked、press、scroll、screenshot、close_page、disconnect。

点击、输入、下拉选择、checkbox、按键都必须基于当前页面的新鲜 snapshot；一次操作后旧 snapshot 立即失效。导航、刷新和 provider reload 也会让旧 identity 失效。

snapshot 默认自适应：普通页面返回完整语义内容；大页面会自动压成只保留交互控件的结果。需要只找下一步控件时可显式 `interactive_only=true`；需要理解页面或确认操作结果时可显式 `interactive_only=false`。

截图是保存在本插件 checkout 下的 provider-local 私有文件。Native Plugin v1 目前只有文本/结构化内容协议，没有 WebCodex Project artifact 或 image handoff；因此返回的 provider 相对路径不能自动作为 `project_artifact` 路径使用，需要 operator 在本机查看或另行导出。

## 安全边界

- 只能关闭本插件创建的 tab，不能关闭用户原有 tab；
- 连接外部 Chrome 时 disconnect 只脱离，不关闭用户 Chrome；
- Native 模式只关闭本插件 session 自己启动的浏览器，从不使用 `close --all`；
- 不提供任意 shell、任意 JavaScript、任意 CDP、cookie/storage、凭据提取、任意 profile 路径或远程调试地址；
- 只允许 HTTP(S) 导航，拒绝本地文件、浏览器内部页、extension/data URL 和带账号密码的 URL；
- 不会把不确定的 effect 失败包装成“可安全重试”；这种情况交给 WebCodex 保留 `OutcomeUnknown`；
- 暂不提供文件上传，因为上传需要单独设计 WebCodex Project 的读取权限和路径边界，不能直接接受任意本地文件路径。

不需要真实本机 profile/current Chrome 时，仍优先使用 WebCodex 内建 Browser。
