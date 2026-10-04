# PDF.js 侧边栏可行性实验

实验分支：`experiment/pdf-sidebar-poc`；基线：上游 main
`a45ac7f5b0c0bd175e2276fcbd814aa3b3bba78e`。本目录是独立 Node/MCP App
实验，没有改动 WebCodex 的生产工具、Rust 注册表或 Markdown 预览。

## 结论与证据范围

**PDF.js 5.6.205 已在真实 ChatGPT 网页右侧栏运行。** 合成文本、中文、扫描件和
630 × 496 pt 比例校验 PDF 均通过 MCP 打开，诊断实际观察到 Blob module Worker。
四种样例的真实 Host 诊断均无 CSP violation；中文使用内嵌 CMaps，未请求外部字体。
比例校验页以 630 × 496 CSS px、100% 缩放显示，阅读区和外层页面均无溢出。

本地官方 SDK `AppBridge` harness 另外验证了禁止外部网络、嵌套 iframe、unsafe-eval
和 Blob Worker 的情况：Worker 禁止时内嵌解析器回退主线程。这个回退仅在本地
验证，没有在真实 ChatGPT 人为修改 CSP。移动端、WKWebView 和复杂 PDF 尚未验证。
本地 harness 父页和 iframe 同源，其结果不能代替真实 Host 的跨源测试。

## 构建与打开

需要 Node >= 22.13。从本目录执行：

```powershell
npm.cmd ci --registry=https://registry.npmjs.org --no-audit --no-fund
npm.cmd run build
npm.cmd run serve
```

控制台打印随机 loopback 地址和 `/mcp` endpoint；浏览器打开前者。
监听范围固定为 `127.0.0.1`，Host 与 Origin 均校验。此命令只启动本机预览，
真实 ChatGPT 接入使用下节的独立实验入口。
本次先查询了清华 npm 地址，返回 404，随后使用 npm 官方源；未修改全局 npm 配置。
依赖版本和 integrity 锁在 `package-lock.json`。

真实 MCP 客户端可接同一地址的 `/mcp`，调用
`display_pdf_sample(sample="text"|"cjk"|"scan"|"ratio")`。`read_pdf_sample_bytes`
标记为 App-only。网页 harness 的 AppBridge 仅转发这个已知读取工具；协议测试
另行连接实际 SDK MCP server，覆盖 initialize、tools/list、resources/read、tools/call。

### 真实 ChatGPT 接入（2026-10-04）

实验工具增加 `_meta["openai/ui"].entrypoints` 的 `global` / `thread` 入口，
资源声明 inline / fullscreen，首选 fullscreen，复用当前 WebCodex 原生入口的元数据形状。
这里 SDK 的 fullscreen 模式实际进入 ChatGPT 右侧栏；Host 仍另有占满窗口的按钮。
模型工具调用已验证能直接打开右侧栏，原型的展开按钮也实际打开过右侧栏。
默认空参数仍只打开合成 text.pdf，不获取线程文件或项目身份。

`node chatgpt-server.mjs` 启动独立 loopback 入口，只接受 `/mcp` POST；
请求体限制 16 KiB，拒绝浏览器 Origin，其他路径不提供本地 harness、文件或资源。
真实 MCP 客户端的握手、空参数打开、资源读取、路径/Origin/大小限制已通过测试。
2026-10-04 获得用户授权后，启动临时 Cloudflare HTTPS 转发；现有 WebCodex
Tunnel 保持原状。Cloudflare 官方 2026.9.3 Windows 客户端下载后按官方 release
digest 核验 SHA-256 一致。独立空配置、HTTP/2、禁止自动更新，转发时重写 Host
为 loopback；未使用现有 Cloudflare 配置或凭据，也未改变入口的 Host 校验。
实际通过公网 HTTPS 的 MCP 客户端完成 initialize、tools/list、resources/read
（约 4.63 MiB HTML）和空参数打开 text.pdf。

真实 Chrome 中已确认 ChatGPT 自定义 MCP 表单支持 Server URL / Tunnel，当前
两个既有 Tunnel 属于现有 WebCodex 连接。依据
[OpenAI 接入文档](https://developers.openai.com/plugins/deploy/connect-chatgpt)，
ChatGPT 需要公共 HTTPS 或 Secure MCP Tunnel，不能直接连接本机 loopback。
临时 Cloudflare 方案只转发这个夹具专用入口，测试后停止进程。
用户在 Chrome 中创建 `PDF.js Sidebar Experiment` 测试连接后，四种样例均完成真实
模型工具调用及侧栏渲染。扩大 sample 枚举后需刷新插件工具并重新加载测试对话，
否则对话仍持有旧枚举；本次先遇到旧 schema 拒绝 ratio，刷新后正常。
正式功能不依赖 Cloudflare：复用 WebCodex 已有的可达 MCP endpoint 或 Secure MCP
Tunnel 即可；这次临时隧道用于独立实验入口，不是 PDF.js 的运行依赖。

`generate-ratio-fixture.py` 可在测得真实 PDF 阅读区域后，生成同尺寸/同比例的
单页校验 PDF；圆形、网格、TL/TR/BL/BR 标记用于检查拉伸及裁切。
本次 PDF 的 MediaBox 为 `[0, 0, 630, 496]`，3417 字节，SHA-256 为
`3c7fd3f0979b1f6e2dea2f6d6119385045255d911e90d8e7f2d0f3502bc39b80`。
阅读区 client size 为 662 × 528，减去四边各 16 px 内边距，正好匹配 630 × 496。
PDF 单位 pt 与 CSS px 的数值在 PDF.js 的 scale=1 下对应，并非物理屏幕尺寸。
四项 MCP/传输测试已通过。

首次真实 Host 测试发现阅读区固定高度导致内外两层滚动；fullscreen 现在使用
全高 flex 布局，并保留稳定滚动条空间。修正后中文长页只在阅读区纵向滚动，
630 × 496 匹配页的阅读区 scroll/client size 均为 662 × 528，外层 scroll/client
size 均为 674 × 728。三页文本翻到第二、第三页和单页英文/中文搜索通过；
关闭文档后 activeWorkers / activeBlobUrls 均为 0。

真实侧栏证据：[脱敏诊断和尺寸报告](evidence/report-chatgpt.json)、
[中文侧栏截图](evidence/chatgpt-cjk-sidebar.jpg)。截图只包含本实验侧栏。
**匹配页截图未取得**：截图接口多次超时，重新打开实验对话及延长截图期限也未恢复，
原因未确认。匹配页已通过真实 Host 的几何检查，但其实际画面没有截图验收；
独立 PDFium 参考渲染图不作为真实 ChatGPT 截图。扫描件也仅保留真实加载诊断。

测试结束后临时 Cloudflare 进程、夹具 MCP 入口和本机 harness 均已关闭。
用户创建的测试插件保留，现有 WebCodex 服务和 Tunnel 未改动；未推送或发布。
测试对话保留当前已加载的比例校验页，刷新后需重新启动开发入口。

## 原型内容

- 单个生成 HTML：PDF.js API、Worker、主线程回退解析器、182 个字体/CMap 资源和 CSS。
  Worker 和二进制资源使用 gzip + Base64 内嵌，以 `DecompressionStream` 解压。
- 直接传递 Worker port，避免跨域 Worker 包装器再次动态 import；版本固定为 `5.6.205`。
- Canvas 与 DOM TextLayer：翻页、适合宽度、缩放、选择文字、单页搜索、Host 主题及展开/收起。
  扫描件显示图片，不提供 OCR。
- MCP 样例分块：每次最多 256 KiB，文件最多 20 MiB，内容哈希冻结身份，校验偏移与总长度。
  Base64 只在 `_meta.pdfChunk` 中，模型 content 不包含完整二进制。
- 四个固定合成夹具；扫描样例 393,674 字节，需要两次分块读取。服务不接受项目路径或任意 URL。
- 可通过文件选择器打开本机 PDF；在 iframe 中读取，不上传、不通过 MCP 发送。
  本地两浏览器验证通过；真实 ChatGPT 的浏览器自动上传接口受扩展权限限制，未完成这条路径。
- 只渲染一页，Canvas backing store 最多 800 万像素，单张图片最大 1600 万像素。
  关闭文档和 SDK teardown 释放 loading task、Worker、Blob URL、Canvas 和 TextLayer。

## 验证结果（2026-10-04，Windows）

以下本地矩阵来自初始实验提交 `f7c8d4e1`，渲染器与离线资源保持相同。
本轮侧栏模式、布局修正及新增夹具的真实 Host 证据见上节。

| 浏览器 | Blob Worker 允许 | Worker 禁止 | 首次加载三个样例 |
| --- | --- | --- | --- |
| Edge 154.0.4258.53 | 三种 PDF 通过，实际观察到 Worker | 三种 PDF 通过，主线程回退 | 100–180 ms |
| Chrome 154.0.8037.95 | 三种 PDF 通过，实际观察到 Worker | 三种 PDF 通过，主线程回退 | 103–205 ms |

时间包含解压、Worker/解析器初始化、文档加载及首次渲染，是本机小夹具的诊断数值，
不能用于估算真实大文件或低性能设备。主线程回退在小样例更快，也不能推断其大文件性能。

12 组矩阵覆盖渲染非空像素、中文提取与 CMap 读取、文本选择/搜索、翻页、缩放、
宽度变化、Host 模式和主题、关闭/重新打开、teardown、实际 Worker 关闭事件与零外部请求。
另外，两浏览器各通过本机文件不上传、坏 PDF 清理、超限文件拒绝和后台重绘按钮交互检查。
四个 Node contract 测试通过。前三个夹具的五页及新增比例页使用独立 PDFium 渲染并查看；本机 Poppler
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

生成 HTML 为 **4,856,772 字节（约 4.63 MiB）**，整体 gzip 测量为 2,523,584 字节。
MCP resources/read 返回 HTML 字符串，整体 gzip 数值不代表 Host 的实际传输大小；
当前 loopback 页面没有 HTTP gzip。双份解析器与全部 CMaps 是主要成本。
当前大小已在真实 ChatGPT 成功读取和运行，但不能推导 Host 的最大资源限制。
正式接入仍需权衡是否保留双份解析器和全部 CMaps。

主线程回退使用固定 PDF.js 版本的 `globalThis.pdfjsWorker.WorkerMessageHandler`
入口；它不是稳定公开 API，升级必须重新验证。大文件主线程阻塞、复杂字体、嵌入字体、
跨平台未嵌入中文字体、加密 PDF、JPX/JBIG2、ICC 色彩、表单、XFA、注释和无障碍未完整测试。
原型关闭 WASM 和 XFA；不能据此承诺任意 PDF 的视觉保真或交互功能。

本次真实 ChatGPT 冒烟确认了渲染与侧栏模式的可行性。下一步可把查看器接入现有 Work Result
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
保留代表截图与按测试环境区分的 JSON 报告作为实验记录。
