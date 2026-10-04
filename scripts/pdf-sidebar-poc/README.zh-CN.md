# PDF.js 侧边栏可行性实验

实验分支：`experiment/pdf-sidebar-poc`；基线：上游 main
`a45ac7f5b0c0bd175e2276fcbd814aa3b3bba78e`。本目录是独立 Node/MCP App
实验，没有改动 WebCodex 的生产工具、Rust 注册表或 Markdown 预览。

## 结论与证据范围

PDF.js 可以在受限的 MCP App iframe 内预览 PDF。这个原型在完全禁止外部
网络、禁止嵌套 iframe、禁止 unsafe-eval 的浏览器测试中通过；Blob Worker
被禁止时，内嵌解析器能回退到主线程。无需外部 PDF 阅读器 iframe 或 CDN。

**尚未在真实 ChatGPT 侧边栏运行。** 本地 Host 使用官方 MCP Apps SDK
`AppBridge`，CSP 是实验设定，不能据此宣称 ChatGPT、移动端、WKWebView 或所有
PDF 文件已经兼容。真实 Host 对 Worker、资源大小、字体和工具 `_meta` 的行为仍须实测。
本地父页和 iframe 同源，适合测试 CSP/协议/渲染，不是 ChatGPT 跨源安全隔离的验证。

## 构建与打开

需要 Node >= 22.13。从本目录执行：

```powershell
npm.cmd ci --registry=https://registry.npmjs.org --no-audit --no-fund
npm.cmd run build
npm.cmd run serve
```

控制台打印随机 loopback 地址和 `/mcp` endpoint；浏览器打开前者。
监听范围固定为 `127.0.0.1`，Host 与 Origin 均校验。无外部公开监听、部署或隧道。
本次先查询了清华 npm 地址，返回 404，随后使用 npm 官方源；未修改全局 npm 配置。
依赖版本和 integrity 锁在 `package-lock.json`。

真实 MCP 客户端可接同一地址的 `/mcp`，调用
`display_pdf_sample(sample="text"|"cjk"|"scan")`。`read_pdf_sample_bytes`
标记为 App-only。网页 harness 的 AppBridge 仅转发这个已知读取工具；协议测试
另行连接实际 SDK MCP server，覆盖 initialize、tools/list、resources/read、tools/call。

连接真实 ChatGPT 通常需要可达的开发 endpoint。本实验未公开本机端口、注册连接器、
修改现有 ChatGPT 配置或重启现有 WebCodex 服务。

## 原型内容

- 单个生成 HTML：PDF.js API、Worker、主线程回退解析器、182 个字体/CMap 资源和 CSS。
  Worker 和二进制资源使用 gzip + Base64 内嵌，以 `DecompressionStream` 解压。
- 直接传递 Worker port，避免跨域 Worker 包装器再次动态 import；版本固定为 `5.6.205`。
- Canvas 与 DOM TextLayer：翻页、适合宽度、缩放、选择文字、单页搜索、Host 主题及展开/收起。
  扫描件显示图片，不提供 OCR。
- MCP 样例分块：每次最多 256 KiB，文件最多 20 MiB，内容哈希冻结身份，校验偏移与总长度。
  Base64 只在 `_meta.pdfChunk` 中，模型 content 不包含完整二进制。
- 三个固定合成夹具；扫描样例 393,674 字节，实际经过两次分块读取。服务不接受项目路径或任意 URL。
- 可通过文件选择器打开本机 PDF；在 iframe 中读取，不上传、不通过 MCP 发送。
- 只渲染一页，Canvas backing store 最多 800 万像素，单张图片最大 1600 万像素。
  关闭文档和 SDK teardown 释放 loading task、Worker、Blob URL、Canvas 和 TextLayer。

## 验证结果（2026-10-04，Windows）

| 浏览器 | Blob Worker 允许 | Worker 禁止 | 首次加载三个样例 |
| --- | --- | --- | --- |
| Edge 154.0.4258.53 | 三种 PDF 通过，实际观察到 Worker | 三种 PDF 通过，主线程回退 | 100–180 ms |
| Chrome 154.0.8037.95 | 三种 PDF 通过，实际观察到 Worker | 三种 PDF 通过，主线程回退 | 103–205 ms |

时间包含解压、Worker/解析器初始化、文档加载及首次渲染，是本机小夹具的诊断数值，
不能用于估算真实大文件或低性能设备。主线程回退在小样例更快，也不能推断其大文件性能。

12 组矩阵覆盖渲染非空像素、中文提取与 CMap 读取、文本选择/搜索、翻页、缩放、
宽度变化、Host 模式和主题、关闭/重新打开、teardown、实际 Worker 关闭事件与零外部请求。
另外，两浏览器各通过本机文件不上传、坏 PDF 清理、超限文件拒绝和后台重绘按钮交互检查。
三个 Node contract 测试通过。全部五页夹具使用独立 PDFium 渲染并查看；本机 Poppler
包装器未能启动，未以它作为验证证据。

首次 Chrome smoke 暴露原型交互问题：主题/尺寸变化的后台重绘短暂禁用按钮，恰好落在
鼠标按下与抬起之间会丢失关闭点击。后台重绘现在保持按钮可用，用户动作仍串行。
回归检查恢复旧行为时稳定失败，修正后的两浏览器均通过；没有通过延长超时掩盖问题。

PDF.js 的 eval 能力探测产生一条被拒绝的 `script-src/eval` 事件，之后使用安全路径，
不影响这些样例。禁止 Worker 的策略另有预期 `worker-src/blob` 拒绝。没有放宽 CSP。

原始结果：[Edge](evidence/report-msedge.json)、[Chrome](evidence/report-chrome.json)。
截图：[中文 Worker](evidence/msedge-allow-cjk.png)、[扫描件主线程回退](evidence/chrome-deny-scan.png)。

```powershell
npm.cmd test
npm.cmd run smoke
npm.cmd run smoke:local
$env:PDF_POC_BROWSER = 'chrome'
npm.cmd run smoke
npm.cmd run smoke:local
```

smoke 使用已经安装的 Edge/Chrome，不下载 Playwright 浏览器。`PDF_POC_CASE=deny-scan`
可以只跑一个矩阵项。重生成夹具需要 ReportLab 4.4.9 和 Pillow；本次使用 Codex
预装 Python runtime。普通构建直接读取已提交的夹具，无 Python 依赖。

## 成本、限制与接入判断

生成 HTML 为 **4,856,343 字节（约 4.63 MiB）**，整体 gzip 测量为 2,523,453 字节。
MCP resources/read 返回 HTML 字符串，整体 gzip 数值不代表 Host 的实际传输大小；
当前 loopback 页面没有 HTTP gzip。双份解析器与全部 CMaps 是主要成本。
正式接入需先检查真实 Host 的资源限制，再决定保留自动回退或拆分模板/资源。

主线程回退使用固定 PDF.js 版本的 `globalThis.pdfjsWorker.WorkerMessageHandler`
入口；它不是稳定公开 API，升级必须重新验证。大文件主线程阻塞、复杂字体、嵌入字体、
跨平台未嵌入中文字体、加密 PDF、JPX/JBIG2、ICC 色彩、表单、XFA、注释和无障碍未完整测试。
原型关闭 WASM 和 XFA；不能据此承诺任意 PDF 的视觉保真或交互功能。

建议继续做真实 ChatGPT Host 冒烟测试。确认后，再把查看器接入现有 Work Result
文件预览：使用经过授权且不可重定向的冻结文件身份和 App-only 二进制读取适配器。
当前 `get_work_result_state` 的文本内容路径会拒绝 binary，不应放宽成无界二进制工具，
也不能把本实验的固定样例权限直接套到项目文件。

## 参考与许可

- [OpenAI MCP UI 文档](https://developers.openai.com/plugins/build/chatgpt-ui)
- [OpenAI CSP 与资源元数据](https://developers.openai.com/plugins/reference)
- [官方 MCP PDF.js 样例](https://github.com/modelcontextprotocol/ext-apps/tree/main/examples/pdf-server)
- [PDF.js API](https://mozilla.github.io/pdf.js/api/draft/module-pdfjsLib.html)

PDF.js 为 Apache-2.0，MCP SDK 为 MIT。生成 bundle 保留 JS legal comments，
`dist/THIRD_PARTY_NOTICES.txt` 另带 PDF.js、CMaps、Foxit/Liberation 字体通知；
复制生成 HTML 分发时应一并带上该文件。`dist/`、依赖、缓存及可再生成截图未全部入库；
只保留两张代表截图与最终 JSON 报告作为实验记录。
