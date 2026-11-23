# Windows Desktop 手动验收

[English](desktop-windows-acceptance.md) | [简体中文](desktop-windows-acceptance.zh-CN.md)

使用这个分支构建的完整安装包，在独立的 Windows 标准用户或虚拟机快照里测试。记录提交、Windows 版本、处理器架构、安装方式和观察结果。记录状态与错误码即可，截图时遮住个人目录与凭据。

## 从安装到第一次工具调用

1. 从一个没有 WebCodex、项目、计划任务或 cloudflared 的测试账号安装并启动 Desktop。观察启动、准备和重启全程，检查是否出现 CMD 或 PowerShell 黑框。
2. 点击一次 **开始使用**。准备过程中再点一次。应只建立一套配置，使用随包程序，无需选择目录、项目或申请管理员权限。就绪后应进入连接页。
3. 打开 **设置 → Runtime 与服务**，核对 Server 和 Runner 正在运行。进入任务计划程序，两个 WebCodex 用户任务应使用当前用户、登录触发器和普通权限。任务动作为安装目录里的 `WebCodex.exe --user-task …`。在任务管理器核对后台宿主和实际工作进程的父子关系。普通 CLI 在终端运行 `webcodex --version` 应仍有输出。
4. 在 **连接 ChatGPT** 中保存自己的 OpenAI Tunnel 凭据。按提示在 ChatGPT 添加连接并查询 WebCodex 状态。记录结果。
5. 点击 **连接 Gemini**。观察组件检查、下载、校验和连接启动。下载完成后应显示本次连接实际的 HTTPS MCP 地址。等待验证时不能复制。点击 **复制 MCP 地址**，核对其以 `/mcp` 结尾。打开 Gemini。
6. 在 Gemini 的 **Settings → Connected Apps → Custom apps** 添加该地址。先取消一次授权，确认 Desktop 没有报告成功调用。重新连接，在 WebCodex 登录页使用所有者的用户凭据并确认权限。需要该凭据时，可用 Desktop 的 Runtime 控制台复制用户凭据。Gemini 的配置表单仅需要 MCP 地址。
7. 在 Gemini 对话中选择该应用并询问“查看 WebCodex Runtime 状态”。记录实际工具响应。在 ChatGPT 也再执行一次查询。分别观察入口就绪、授权与工具调用。
8. 按当前后台策略关闭、重新打开 Desktop，然后重启 Windows 并使用同一用户登录。检查服务实例数、Runner 身份和连接恢复状态。自动启动只使用已经安装的组件。

在 Windows ARM64 上，另选一台系统支持 x64 用户程序的测试机验证组件下载。应安装同一份经过校验的固定版本 x64 cloudflared。不具备该能力的系统需要提供可信的兼容程序。[Windows 架构能力查询](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getmachinetypeattributes)

## 升级、异常和回滚

- 使用另一份测试快照，从已保存项目、Runner、OpenAI 连接和 Named 地址的旧版升级。核对项目清单、Runner ID、固定域名及连接可用性。Legacy 迁移按提示检查并确认。中断后应从同一个恢复记录继续。遇到归属不明时，确认界面提供检查入口且没有覆盖凭据。
- 在服务设置中依次停止、启动、重启已确认归属的 Server 和 Runner。停止后观察旧 PID 和子进程退出，启动后核对新 PID 和任务数量。已运行的旧控制台任务在下一次显式重启时切换到 GUI 宿主。故意改变测试任务的所有者或动作后，管理操作应拒绝接管。
- 在没有组件的测试快照断开网络，尝试连接 Gemini，查看下载诊断。恢复网络后重新连接。再对已运行的连接短暂断网，观察公网地址隐藏、健康检查恢复和原项目保留。重建 Quick 后应复制新地址并在 Gemini 重新授权。重启 Named 后核对固定地址。
- 在 Gemini 断开应用。在 Desktop 编辑该连接、关闭动态注册并应用，检查旧授权失效。ChatGPT 应仍可执行查询。
- 最可靠的测试回滚是还原整机快照。手工回退时，先用本分支的服务设置停止已确认归属的任务，再以同一用户用 `webcodex environment uninstall-service runner` 和 `webcodex environment uninstall-service server` 卸载对应任务。独立 Tunnel 可用 `uninstall-service tunnel --profile PROFILE`。保留 Environment 目录与凭据，在还原匹配的旧安装包后用其 `environment resume` 恢复服务。遇到不确定的进程归属或恢复记录时，先检查诊断。

## 自动化控制台回归

```powershell
cargo test --locked -p webcodex-environment --lib service::windows_user::tests
cargo test --locked -p webcodex-runner --bin webcodex-runner --features runner-real-process-tests runner_real_process_git_ -- --ignored --test-threads=1
```

原生项目清单回归先创建没有控制台的测试宿主，再走 Runner 启动时的有界 Git 与 SSH 能力探测路径，
检查这种 GUI/服务启动条件下没有创建控制台窗口，同时保留 stdin EOF、两个重定向输出流及退出码。
仅从终端运行测试可能因继承控制台而掩盖故障；`CREATE_NO_WINDOW` 创建的隐藏控制台属于正常行为，
不要求控制台成员数为零。Runtime 的后台子进程显式使用无窗口启动，普通交互 CLI 保留原有控制台行为。
这些检查不替代上面的登录与安装包视觉验收。

自动化测试覆盖协议和进程边界。黑框观察、登录恢复、真实 OAuth 同意及宿主工具调用均需记录实机结果。
