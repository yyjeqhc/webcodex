# WebCodex Desktop Sessions

[English](DESKTOP_SESSIONS.md) | [简体中文](DESKTOP_SESSIONS.zh-CN.md)

本文档描述 WebCodex 的未来方向。它是战略设计说明，不是 release checklist，也不表示这些能力今天已经实现。

## 产品目标

WebCodex Desktop Sessions 不应表示无限制远程桌面控制。目标应该是可控、可审计、可回放的桌面工程会话：

```text
observe -> decide/propose -> authorize -> act -> verify -> record -> replay/report
```

这会把 WebCodex 从 AI coding runtime 扩展为 AI engineering workstation runtime。Runtime 应该能把代码仓库、文件产物、命令、Git diff、桌面观察、截图和最终报告放进同一个任务 session。

## 非目标

Desktop Sessions 不应该被设计成：

- generic RPA 平台；
- 无限制远程桌面控制；
- 公网 computer-use bot；
- 面向消费者的屏幕共享工具；
- 现有 project、Git、shell、MCP 或 GPT Action tools 的替代品；
- 让 agent 默认控制用户主力桌面的理由。

Computer use 应该是 WebCodex 的 visual/desktop execution backend，而不是整个产品。

## 为什么 desktop sessions 重要

MCP 和普通 runtime tools 很有价值，但很多工程任务仍然没有稳定 API、CLI 或 MCP surface：

```text
Windows 安装器测试
GUI 应用冒烟测试
依赖浏览器登录态的操作
IDE/编辑器辅助调试
OBS 或网页构建平台操作
Electron/Qt 应用测试
远程桌面里的发行版测试
游戏或 AI 游戏调试
控制面板、驱动、系统设置和安装向导
```

WebCodex 的差异化不应该是能点击坐标，而应该是每个视觉动作都有边界、意图、结果和证据链。

## Session 生命周期

Desktop Session 应该被建模为带视觉执行事件的 WebCodex 任务 session：

1. **Observe:** 捕获屏幕/窗口状态、窗口列表、进程 metadata 和相关 artifacts。
2. **Decide/propose:** 说明动作意图，包括目标窗口、UI 元素和预期结果。
3. **Authorize:** 对输入、破坏性、提交、支付或隐私敏感动作要求 policy 或用户授权。
4. **Act:** 执行 bounded desktop action，例如聚焦窗口、设置剪贴板、按键或点击窗口相对坐标。
5. **Verify:** 捕获 after-state，并与预期结果对比。
6. **Record:** 把 action metadata、截图、日志、artifacts 和 policy decisions 写入 session timeline。
7. **Replay/report:** 产出可审查报告和证据，而不只是文字声称任务成功。

这对应现有 WebCodex coding loop：inspect、edit、diff、validate、record 和 report。

## 权限模型

Desktop permissions 应该和 shell/job permissions 分开。一个有用模型需要两条轴。

Desktop capability levels：

```text
L0 artifact transfer
L1 screenshot, window list, process list, screen metadata
L2 clipboard get/set
L3 keyboard input and hotkeys
L4 mouse click, drag, scroll
L5 autonomous visual loop
```

Execution capability 单独保留：

```text
shell disabled
diagnostic-only shell
bounded shell
privileged shell prohibited by default
```

这种拆分很重要，因为 GUI action 和 shell command 的风险形态不同。一次点击即使没有运行 shell，也可能提交表单、发送消息、删除数据或暴露隐私信息。

## Artifact bus

Artifact flow 应该是一等 runtime capability，而不是 desktop-only 功能：

```text
ChatGPT upload
  -> WebCodex artifact
  -> agent workspace / desktop session
  -> generated logs, screenshots, builds, reports
  -> WebCodex artifact
  -> user download
```

这同时支持 coding 和 desktop 场景，也是 WebCodex 从“coding agent”扩展到“engineering workstation”的主要桥梁。上传/下载让用户可以提供数据、图片、文档、安装包、配置文件、截图或日志；agent 可以分析、转换、测试或打包这些输入；runtime 再把生成的报告、图表、修复文件、构建产物、日志和视觉证据返回给用户。

常见 flow 包括：

```text
上传实验 CSV 或结果压缩包 -> 分析数据 -> 下载图表和报告
上传截图或测试图片 -> 检查视觉证据 -> 下载标注后的输出
运行本地 Web UI -> 捕获截图 -> 调整布局 -> 保存 before/after 证据
上传安装包或样例文件 -> 运行冒烟测试 -> 下载日志和截图
上传文档或配置 -> 转换或修复 -> 下载修正后的 artifact
```

这正是 WebCodex 可以超过 ordinary coding agents 的地方。普通 coding agent 通常可以改文件、跑测试，但很难把真实任务输入和输出纳入 session，也很难检查生成图片，或把截图证据附到最终报告里。

初始 artifact categories 应包括：

```text
上传安装包
上传测试图片
上传 Excel/PDF/docx 文件
上传配置文件
下载日志
下载截图
下载构建产物
下载测试报告
下载修复后的文件
下载 before/after UI 证据
```

Artifacts 未来应携带稳定 metadata，例如 id、type、source、session id、project id、creator、SHA-256、size、retention policy、preview support 和 download routing。

## 证据链和回放

截图不应该只是模型输入，还应该成为证据。一个关键 desktop event 应该能形成如下记录：

```text
before.png
action.json
after.png
observation.md
```

概念性的 action record 可以像这样：

```json
{
  "action": "click",
  "target_window": "Chrome",
  "coordinate_space": "window",
  "x": 812,
  "y": 436,
  "intent": "Click the Login button",
  "timestamp": "..."
}
```

这只是 proposed record shape，不是已承诺 API。不变量更重要：用户应该能回答 agent 看到了什么、打算做什么、实际发送了什么输入、发生了什么变化、结果是否被验证。

## Windows MVP

Windows 是合适的第一 desktop provider，因为许多安装器、GUI 应用、企业软件和游戏测试工作流依赖它。架构不应绑定 Windows，但第一版实现可以优先聚焦 Windows。

第一批能力应保持保守：

```text
screenshot
window_list
focus_window
mouse_click, mouse_drag, scroll
keyboard_type, key_combo
clipboard_get, clipboard_set
artifact upload/download
screen_trace, action_trace
```

`window_list`、`focus_window` 和窗口相对坐标很重要。全屏绝对坐标在分辨率、DPI、窗口位置或布局变化时太脆弱。

Provider 抽象应保留未来扩展空间：

```text
windows-desktop-provider
linux-x11-provider
linux-wayland-limited-provider
macos-provider
browser-provider
vnc-provider
rdp-provider
```

## 安全策略

Desktop Sessions 必须从一开始就按高风险能力设计。默认 policy 应偏向观察和显式授权：

- 默认只允许 screenshot/window observation；
- keyboard、clipboard 或 mouse input 前要求 approval；
- 不自动输入密码；
- payment、send、delete、publish、submit、install 或 system-setting actions 必须确认；
- 支持 sensitive window 和 process deny lists；
- 支持 masked screenshot regions 和 retention limits；
- 默认阻止 sensitive paths 被 artifact upload；
- 提供 emergency stop；
- 对关键动作保留 before/after evidence；
- 推荐 virtual machines、test accounts、temporary desktops 和 dedicated OS users。

Desktop Session 不应默认控制用户的主力个人桌面。

## 路线图

实际路线图应该在早期保持低风险：

1. **Artifact bus and observation:** uploads/downloads、screenshots、window list、process list、screen metadata。
2. **Human-approved input actions:** focus、clipboard、keyboard、hotkeys、click、drag、scroll，在 session policy 下执行。
3. **Evidence and replay:** before/after screenshots、action trace、session timeline、downloadable report。
4. **Short GUI workflows:** 打开网页、上传文件、点击构建、下载结果、捕获证据。
5. **Vertical engineering workflows:** Windows GUI smoke tests、installer validation、OBS/web build triage、desktop app packaging validation、browser release workflows、game/AI game testing。

## 与现有 WebCodex sessions 的关系

Desktop Sessions 应复用现有 WebCodex 理念，而不是创建一套孤立自动化产品：

- project ids 和 agent identity 定义 work happens where；
- session ledgers 同时记录 desktop events、file、Git、shell 和 artifact events；
- task guards 和 risk metadata 定义 agent 能做什么；
- artifacts 承载 screenshots、logs、installers、reports 和 generated files；
- `show_changes` 和 final reports 可以同时包含 code diff evidence 和 desktop verification evidence。

长期定位是：WebCodex 不应该做 AI mouse controller。它应该是一个 engineering runtime，其中 desktop execution 是 authorized、scoped、auditable 和 replayable 的。
