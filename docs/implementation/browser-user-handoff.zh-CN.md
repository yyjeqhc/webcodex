# 浏览器人工接管

[English](browser-user-handoff.md)

人工接管保护一个确切的受管理、可见、持久 Browser，让用户完成登录、MFA、授权等手动步骤，
不自动完成或绕过这些交互。Runner 必须声明 `browser_user_handoff` 并精确解析原生 surface；
不支持的平台或多窗口歧义在 hold 建立前拒绝。第一版不支持临时、无头或外部附加 Browser。

## 工作流

1. 在已授权的现有 Workflow Session 发布 open `question` 或 `todo`，说明需要用户完成的步骤，保留精确 message id。
2. 调用 `control_browser(action="begin_user_handoff", client_id, browser_id, session_id, message_id)`。
   独立检查 Session owner 与 Browser/Runner 权限，保留返回的 `handoff_fence`、精确 surface 和剩余期限。
   相同关联的重复 begin 不续期，也不创建第二次接管。
   begin 需要 `browser:control`、`computer:read` 和 `runtime:read`；当前 principal
   的 Session ref 先解析为确切 Session，再执行 owner 检查。
3. 请用户完成操作并明确回复。hold 最长保护十分钟的 idle lifetime，且不超过 Browser 剩余最大生命周期，
   不自动续期。观察、surface 解析及 `close_browser` 仍可用；包括 `close_page` 在内的页面效果均被拒绝。
   因焦点、指针和剪贴板共享桌面输入，存在任意接管时也暂停 Computer 原生效果；用户原生输入不受影响。
4. 用户明确回复后调用 `control_browser(action="finish_user_handoff", client_id, browser_id, session_id, handoff_fence)`，
   然后重新获取 Browser snapshot。首次成功恢复 snapshot（或显式关闭 Browser）之前 Computer 输入仍暂停。
   finish 时已存在的每个页面都必须分别取得新 snapshot，之后才能恢复该页面的效果。
   旧 Browser/Computer 元素和显示截图输入代次不可复用。
   必须观察实际结果；用户回复和 Session ACK 不证明登录成功，不会自动解决 todo，也不是权限授权。

## 到期与恢复

到期仅移除 idle 保护，不会静默恢复仍存活 Browser 的自动修改。普通 idle/max-lifetime
清理可关闭确切 Browser 并撤销其身份，同时释放原生输入保护；关闭、shutdown 和 Runner
替换同样撤销 hold。Server 重启不能根据保留的问题重新创建 hold。
回执丢失时先观察同一 Browser 的 handoff summary，再决定是否 finish；不要盲目重放，
也不要将旧 fence 用于替代 Browser。finish 不可用或陈旧时需要重新观察。
finish 后同一 Browser/Session/message 关联不能再次 begin。每个 Browser 最多保留
64 个已结束关联；新手动步骤使用新的待办消息，达到容量后需显式关闭并替换 Browser。

## 实现归属

Server 的 [Browser gateway](../../src/tool_runtime/browser_tools.rs) 独立校验
Session/message 关联与 Browser 权限；Runner 的
[Browser adapter](../../crates/webcodex-runner/src/webcodex_runner/browser.rs)
解析确切原生 surface，将
[Supervisor 接管状态](../../crates/webcodex-browser/src/supervisor/user_handoff.rs)
连接到 Computer 输入保护。
[接管测试](../../crates/webcodex-browser/src/supervisor/tests/user_handoff.rs) 覆盖
重复 begin、到期、逐页面恢复 snapshot 与原生保护释放；
[Server 测试](../../src/tool_runtime/tests/browser_user_handoff.rs) 覆盖独立授权边界。

`close_browser` 是明确的关闭出口。到期、Session ACK、本机恢复 Computer Control
或关闭单个页面，都不能代替精确 finish 与恢复观察。

## 验收与限制

在支持的原生 host 上使用隔离注册测试项目：启动 managed visible Browser，发布问题并 begin。
检查 Browser/Computer 修改被拒绝、观察仍可用；进行无害手动页面修改，明确继续、重新 snapshot，
使用新元素恢复。接管中关闭 Browser，再验证旧 fence 不能操作替代实例。

受控时钟/backend 与原生输入保护测试不执行真实登录。macOS/Windows 原生 surface 和 ChatGPT
人工交互需要独立手动验收；Linux capability 拒绝不证明支持原生 UI。
