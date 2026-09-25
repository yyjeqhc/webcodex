# 0.4.2 Windows Desktop hardening 验证记录

基线：`44ac49e`，分支 `fix—ui`，仓库 `C:\Users\17724\Desktop\webcodex`。起始工作区干净。验证环境：Microsoft Windows 10.0.19045。

## 根因与路径审计

- WorkspaceContext 的合并和 Dashboard 的默认项目查找只比较路径；旧 sameProjectPath 没有消除 Windows extended-length namespace，导致 Runner canonical path 与 saved display path 成为两行。
- 新合并优先比较双方 runtime ID；任一侧缺 ID 才使用 Windows-aware 路径 fallback。双方 ID 不同时不因路径相同而合并。Runner row 对象不被 saved 数据覆盖。
- adapter.rs 的 same_path 以及 same_existing_file 的失败回退曾使用 display_path 加 ASCII case 比较；现复用 runner-config 的 paths_equal。原 display_path 保持展示用途，现有盘符与 UNC 转换测试通过。
- state.rs 的保存项目去重已使用 paths_equal，无需新建 Rust identity 规则。
- Runner registration.rs 的 bounded_project_name 对盘符或 UNC share root 的 file_name 空值使用通用名称；catalog 与 fingerprint 保留 canonical path。此次仅在展示 helper 修正泛化根目录名称，不改注册内容、ID 或 fingerprint。
- 共享 displayProjectPath / projectPresentationName 覆盖 Desktop Projects、Dashboard、Activity/Extensions picker、FirstRun，以及 Runtime ProjectsView、GoalWorkbench、SessionInspector 与 Admin。Session detail 的相对 Git 文件路径不转换。
- ID、canonical path、协议、allowed roots 和文件操作输入均不经展示 helper 改写。

## 产品行为

Projects 现在是“此 Runner 的项目”清单；移除行切换与独占 Current 标记，首页也不再以按钮激活已有项目。保留 Add Project 的目录选择和现有注册流程。

新增“取消注册”：先观察 exact Runner target、runtime Project ID 和 revision，确认后在 native operation 中重新验证 target 与 revision，调用 canonical unregister_project。服务端继续负责权限、活动 Job 冲突与 registry-only 删除。确认成功后清理同一 Runner 的 Desktop 保存记录；默认项目被移除时清空默认展示，inventory 不依赖默认项目继续读取。不改目录、Git 文件、allowed roots，也不停止 Runner。取消默认项目后添加目录可进入已有设置流程，不自动激活其他项目。

失败或结果不确定不自动重发，也不乐观移除 UI 行；需重新观察。当前取消注册入口需要在线 Runner，未引入离线直接编辑 registry 的路径。

重复行和错误的“尚未观察到活动”由错误 identity merge 引起，与 Git branch 无关。回归验证保留 Runner 的 connected、sessions、latest_updated_at、name 和 ID，并用该 ID 查询 Git；另覆盖 Git 查询失败时活动信息仍存在。

## 可读性

- 项目表 header：10 → 12px；project title：13 → 14px；路径和最近活动：11 → 12px。
- Desktop 主界面需要阅读的 10/11px label、状态、时间与辅助正文收敛至 12px；小字号必要信息由 tertiary 改为 secondary。品牌装饰与键盘提示仍可保留 10px。
- 主界面的非标准权重收敛至 500/600/700；正文 400 保留。
- foundation 与 Desktop Mantine font stack 保留 Apple 前序，并添加 Segoe UI Variable、Segoe UI、Microsoft YaHei UI；保留通用 Linux fallback。不附加字体文件，不改变色板、整体布局尺寸或全局页面缩放。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| Desktop Vitest：Workspace、App、presentation、locale | 80 passed |
| Runtime/Admin Vitest：project-presentation、navigation、goal-workbench、relations、admin_app | 21 passed |
| runner-config paths::tests | 23 passed |
| Tauri adapter Windows path tests | 3 passed |
| Tauri --lib project_inventory | 7 passed，含现有 saved inventory 回归 |
| Desktop typecheck / build | passed |
| Runtime/Admin typecheck / build / check:dist | passed |
| Desktop form-control CSS contract | passed |
| Tauri cargo check --locked | passed |
| 根 Cargo workspace 与 Desktop 独立 workspace 的 cargo fmt --check | passed |
| git diff --check / conflict review | passed，无冲突 |

Windows 测试显式覆盖 C:\、D:\、\\?\C:\、\\?\D:\、UNC share/root、不同大小写和 trailing separator；Unix case sensitivity 与空路径仍保留。取消注册测试使用隔离临时 fixture 和 loopback fake endpoint，覆盖确认/target/revision 拒绝、未知结果不重试、保存记录清理与文件保留，没有对真实 Runner 执行取消注册。

初次验证遇到的环境问题：npm 缓存缺少依赖，按原 lockfile 下载后解决；Vitest 的 Windows 子进程被沙箱以 spawn EPERM 阻止，经允许的执行方式通过。Tauri 非 --lib 测试命令尝试生成额外 archive 时出现 C 盘空间不足（error 112）；改用所需 --lib 测试目标后通过，未删除构建目录或用户数据。原有 Rust unused/dead-code warnings 和 Desktop bundle >500kB 提示保留，未放宽任何检查。

未运行已安装 Desktop 的 WebView2 交互 smoke，未制作安装包。后续可在磁盘空间充足时补 100%/125% DPI、中英文与明暗主题的人工视觉检查。取消注册后的不确定结果仍要求人工重新观察；不以自动重放换取表面成功。

无 push、PR、发布、部署、服务重启或真实用户配置修改。

## 修改文件

- `apps/desktop/src-tauri/src/commands/mod.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src-tauri/src/models.rs`
- `apps/desktop/src-tauri/src/project_inventory.rs`
- `apps/desktop/src-tauri/src/project_inventory/tests.rs`
- `apps/desktop/src-tauri/src/state/workspace.rs`
- `apps/desktop/src-tauri/src/webcodex/adapter.rs`
- `apps/desktop/src-tauri/src/workspace.rs`
- `apps/desktop/src/App.test.tsx`
- `apps/desktop/src/App.tsx`
- `apps/desktop/src/components/DesktopMantineProvider.tsx`
- `apps/desktop/src/features/activity/ActivityPanel.tsx`
- `apps/desktop/src/features/dashboard/Dashboard.tsx`
- `apps/desktop/src/features/extensions/ExtensionsPanel.tsx`
- `apps/desktop/src/features/onboarding/FirstRun.tsx`
- `apps/desktop/src/features/projects/ProjectRows.tsx`
- `apps/desktop/src/features/projects/ProjectsPanel.tsx`
- `apps/desktop/src/features/workspace/Workspace.test.tsx`
- `apps/desktop/src/features/workspace/WorkspaceContext.tsx`
- `apps/desktop/src/i18n/messages/de-DE.json`
- `apps/desktop/src/i18n/messages/en-US.json`
- `apps/desktop/src/i18n/messages/fr-FR.json`
- `apps/desktop/src/i18n/messages/ja-JP.json`
- `apps/desktop/src/i18n/messages/ko-KR.json`
- `apps/desktop/src/i18n/messages/zh-CN.json`
- `apps/desktop/src/i18n/presentation.ts`
- `apps/desktop/src/i18n/product.ts`
- `apps/desktop/src/i18n/runtime-shell.ts`
- `apps/desktop/src/lib/desktop-api.ts`
- `apps/desktop/src/models/topology.ts`
- `apps/desktop/src/models/workspace.ts`
- `apps/desktop/src/styles/app.css`
- `apps/desktop/src/styles/product.css`
- `apps/desktop/src/styles/workspace.css`
- `docs/desktop-guide.md`
- `docs/desktop-guide.zh-CN.md`
- `docs/desktop-windows-hardening-0.4.2.md`
- `frontend/dist/admin.css`
- `frontend/dist/admin.js`
- `frontend/dist/app.js`
- `frontend/dist/styles.css`
- `frontend/src/admin-react/AdminApp.tsx`
- `frontend/src/runtime-v2/components/GoalWorkbench.tsx`
- `frontend/src/runtime-v2/components/SessionInspector.tsx`
- `frontend/src/runtime-v2/model/format.ts`
- `frontend/src/runtime-v2/views/ProjectsView.tsx`
- `frontend/src/ui/foundation.css`
- `frontend/src/ui/projectPresentation.ts`
- `frontend/test-v2/project-presentation.test.ts`
