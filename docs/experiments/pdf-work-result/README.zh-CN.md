# 正式 Work Result PDF 预览验证

实验分支：`experiment/pdf-sidebar-poc`。本次将 PDF.js **6.4.299** 接入正式
`src/mcp_work_result_app.html`，资源地址升级为 `ui://webcodex/work-result/v25`。
生产文件来自既有 App-only 工具的不可变 Git 快照，PDF 二进制只进入私有 MCP
metadata；渲染器、Worker、CMaps 和字体随 HTML 打包。正式使用沿用 WebCodex
现有 MCP / Secure Tunnel，不需要 Cloudflare 或独立 Node 服务。

## UI 与浏览器验证

工具栏按翻页、缩放、搜索分组，窄屏自动换行。PDF 默认使用适宽阅读，隐藏无用
的全文按钮；打开时保留文件名和模式按钮，粘性导航不遮挡文件标题。中文支持
离线 CMaps，扫描件可阅读但不做 OCR。搜索仅针对当前页的文本。

以下截图来自 **正式 HTML 在本地 Edge 的真实浏览器渲染**，不代表 ChatGPT
Host 测试。合成测试适配器不读取真实 Project，也不能访问任意文件。

| 场景 | 浏览器视口 | 结果 | 截图 |
| --- | --- | --- | --- |
| 文本，浅色 | 360×900 | 翻页、搜索选中、缩放、适宽通过 | [文本](evidence/text-360-light.png) |
| 中文，浅色 | 420×900 | 中文文字可见且能提取 | [中文](evidence/cjk-420-light.png) |
| 扫描件，深色 | 420×900 | 图像正常，无文本层 | [扫描件](evidence/scan-420-dark.png) |
| 630×496 校准页 | 860×1100 | 保持原比例，圆形和四角标记完整 | [比例](evidence/ratio-860-light.png) |

Edge 版本：154.0.4258.53。四个场景均无水平溢出、无 JS 错误、无外部网络请求，
文件标题不被粘性导航遮住；关闭后 Worker 从 1 个降为 0 个。Worker 被 CSP 禁止
时显示错误与 Retry。详细像素、文本和几何测量见 [report.json](evidence/report.json)。

校准 PDF 为 [sidebar-match-630x496.pdf](../../../scripts/pdf-sidebar-poc/output/pdf/sidebar-match-630x496.pdf)，
MediaBox 为 `[0, 0, 630, 496]`，SHA-256：
`3c7fd3f0979b1f6e2dea2f6d6119385045255d911e90d8e7f2d0f3502bc39b80`。

## 自动检查与复现

- Work Result 控制器：159 项通过，包括关闭、模式切换、刷新、teardown 的迟到响应隔离。
- 前端 admin 定向测试：19 项通过，其中 PDF 分块读取 4 项。
- 后端 PDF 不可变读取与权限测试通过；原 UTF-8 分页回归测试通过。
- MCP Work Result 定向测试：14 项通过，包括 PDF 不进入 structured/text content。
- Markdown、sections、PDF 三个生成产物检查通过。
- `cargo check -p webcodex` 通过；开发构建使用 `dogfood` profile。

全量 `check:dist` 在既有 admin.html/js/css 的逐字节产物检查处失败，尚未执行到
Work Result 检查。重新构建后，三个文件统一 CRLF/LF 的内容均完全一致，确认是
Windows checkout 换行差异。本次没有修改 admin 源码或这些产物；上述三个
Work Result 产物检查已独立运行通过。原命令不能记录为通过。

从 worktree 根目录执行：

```powershell
npm --prefix frontend run build:work-result
node --test src/mcp_tests/work_result_app.test.mjs
npm --prefix frontend run test:admin
node scripts/pdf-sidebar-poc/work-result-smoke.mjs
cargo test -p webcodex --lib work_result_pdf
cargo test -p webcodex --lib work_result_content_pages
cargo test -p webcodex --lib mcp::tests::work_result_app
cargo build -p webcodex --profile dogfood --bin webcodex-server
```

浏览器测试需先安装 `scripts/pdf-sidebar-poc` 的依赖。其历史独立 POC 所用
PDF.js 5.6.205 不进入正式 HTML；正式版本从 `frontend` 依赖构建。

真实 ChatGPT 复测使用 `PDF_POC_WORK_RESULT=1` 的只读合成文件入口与临时测试
隧道，不替代上述后端权限测试，也不部署或重启现有 WebCodex 服务。
