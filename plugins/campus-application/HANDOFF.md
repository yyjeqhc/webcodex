# campus-application 中国电信真实表单 dogfood 交接

## 当前目标

用中国电信 2027 校招在线简历编辑页作为真实 dogfood 页面，持续优化 `plugins/campus-application`，目标不是一次性“硬填完”，而是把真实 ATS 页面中的困难沉淀成可复用的 Plugin 能力。

当前原则：

- Plugin 负责：简历结构化、表单分析、稳定字段映射、填表计划。
- Browser Use 负责：读取真实页面、执行明确的 input/select/upload/click。
- Computer Use 只做 Browser 语义不足时的 fallback。
- 最终“保存 / 提交申请”保留人工确认。
- 不通过上传简历触发站点自动解析覆盖已填写内容。

## 工作分支与状态

- Project: `agent:mini:webcodex`
- Branch: `fix/campus-application-live-form-context`
- Base / origin/main at branch creation: `676aeb13`
- 当前改动尚未提交。
- 修改范围：
  - `plugins/campus-application/src/form-cache.ts`
  - `plugins/campus-application/src/mapping-memory.ts`
  - `plugins/campus-application/src/plugin.ts`
  - `plugins/campus-application/src/resume.ts`
  - `plugins/campus-application/tests/form-cache.test.mjs`
  - `plugins/campus-application/tests/mapping-memory.test.mjs`
  - `plugins/campus-application/profile.example.json`
  - `plugins/campus-application/README.md`
  - `plugins/campus-application/README.zh-CN.md`
  - 本交接文档

最近验证：

```text
npm run typecheck
npm test
git diff --check

24 tests passed
0 failed
```

regular mini Runner 与 mini-dogfood Runner 均已重新加载 `campus-application`，状态为 ready。

## Plugin 动态开发 / reload 语义

当前 provider 配置执行的是：

```text
node /Users/yyjeqhc/git/webcodex/plugins/campus-application/dist/plugin.js
```

因此开发迭代流程是：

1. 修改 `src/*.ts`
2. `npm run build`（`npm test` 也会先 build）
3. 执行 Plugin reload
4. 新调用立即走新版本

**不需要重启 Runner。**

原因：Plugin 是 Runner 管理的持久 provider 子进程；已经运行的 Node 进程不会自动重新加载修改后的 JS，所以单纯改 TS/重建 dist 后，旧 provider 仍在内存中。Plugin reload 会安全重启 provider 并重新读取 metadata/schema。

当前两条通道：

- regular mini: `plugin_tool(action=reload, runner=mini)`
- Tunnel dogfood: `plugin_tool(action=reload, runner=mini-dogfood)`

如果以后希望更短的 dev loop，可以增加一个 repo helper，例如：

```bash
npm --prefix plugins/campus-application run build
webcodex plugin reload ...
```

Runner restart 没必要。

## 中国电信页面：目前确认的真实困难

### 1. Browser 能看到控件，但大量控件没有字段 label

在线简历编辑页可以被 Browser Bridge attach，snapshot 能返回约 100+ 个可交互控件，但大量节点类似：

```text
role=textbox
name=""
group_label=""
element_id=volatile
```

下拉框大量只暴露：

```text
role=combobox
name=""
value="请选择"
```

所以“页面肉眼能看到字段名” != “Browser semantic node 自身带字段名”。

这是目前最主要瓶颈。

### 2. element_id 是瞬时执行 identity，不能作为长期站点映射

刷新 / 重 attach / 页面重建后 `element_id` 会变。

因此不能把：

```text
element_xxx -> 姓名
```

作为持久知识。

本轮引入了稳定 `mapping_id`：

- 来源于表单结构 key 的 hash
- 忽略 volatile element_id / value / checked
- 同一结构跨 snapshot 可复用
- Plugin blocker / recognized 结果都会返回 `mapping_id`

这样可以对真实页面做“一次教学，后续复用”。

### 3. 旧 resume upload 识别存在严重误判

旧逻辑只要 label 中出现“简历”就认为是 resume upload。

中国电信页面中曾把以下普通 click 控件误判为简历上传：

- “本人同意针对相关工作机会提供简历……”
- “增加更多 简历”

本轮修复：

`isResumeUpload()` 现在要求：

1. 必须具有真实 upload 语义（`actions` 含 `upload_file`，或等价文件选择语义）
2. 同时上下文明确表示“简历 / resume / cv”

普通按钮 / 链接不再因为文字包含“简历”被误判。

对应测试已加入。

### 4. 同一页面有多个 file picker，仅靠“选择文件”无法知道附件类别

中国电信有多个上传区域，例如：

- 最高学历学籍在线验证报告
- 成绩单
- 外语成绩证明
- 身份证
- 简历
- 其他

Browser 当前常只给：

```text
button "选择文件"
value "未选择任何文件"
actions=["upload_file"]
```

但缺少父 section label。

因此 Plugin 现在不再假设所有 file picker 都是 resume。

后续需要：
- Browser snapshot 增强邻近 section/label provenance；或
- 通过 stable `mapping_id` 对这些 picker 做一次人工/模型教学。

### 5. 页面导航后现有 Chrome attachment 会 stale

这是现有 Chrome Bridge 的安全语义：导航/重新 share 后旧 attachment 可能失效，需要重新 Share current tab 并 attach。

不是 Plugin bug，但 dogfood 过程中会产生摩擦。

## 本轮 Plugin 改动

### stable mapping hints

`analyze_form` / `plan_fill` 新增可选：

```json
{
  "mapping_hints": [
    {
      "mapping_id": "...",
      "label": "姓名"
    }
  ]
}
```

或者更明确：

```json
{
  "mapping_id": "...",
  "canonical_field": "full_name",
  "resume_path": "identity.full_name"
}
```

Plugin 会把该 hint 写入当前 structure-signature mapping cache。

目标：真实页面首次看到 unlabeled control 时，由模型利用截图/上下文判定一次；后续不再重新猜。

### recognized / blocker 都返回 mapping_id

方便下一步针对 blocker 增量教学，而不是依赖 element_id。

### SnapshotNode 增加 actions

Plugin 可以区分：
- click
- input_text
- upload_file
- 等 Browser 真实 authority

避免通过 role/name 猜执行能力。

### 新增中国校招常见个人字段

Canonical profile 新增：

- gender
- birth_date
- id_type
- id_number
- ethnicity
- political_status
- native_place
- household_registration
- marital_status

并补充常见中英文 label synonym。

注意：当前 private profile 还需要按新的 `personal` object 更新后才能直接投影这些字段。

## 当前 live page / runtime 状态

中国电信在线简历编辑页已经能够通过 existing Chrome Browser Bridge 正常 attach。

最近一次 raw Plugin 分析，在没有 hints 的情况下仍约有：

```text
recognized: 0
blockers: 96
```

这不是本轮改动失败，而是明确说明：中国电信页面的 Browser semantic snapshot 仍缺 parent/neighbor label。

好消息：

- blocker 现在带稳定 mapping_id
- resume upload 误判已消失
- 可以开始“teach mapping -> cache -> plan_fill”的增量 dogfood

## 建议下一轮工作顺序

### P0：验证 stable mapping hint 真正用于中国电信

1. 获取当前 interactive snapshot
2. 取前几个 blocker 的 `mapping_id`
3. 结合 screenshot 确认控件对应字段
4. 给 `analyze_form(mapping_hints=...)`
5. 重新调用（不传 hints）确认 cache hit 且映射保留
6. 刷新 / reattach，确认 mapping_id 在结构未变时仍稳定

先只验证 3～5 个字段，不填整页。

#### 2026-10-06 P0 live dogfood 结果

已在 `mini-dogfood` + 中国电信真实在线简历页完成 3 个字段的 stable mapping hint 验证，全程未修改字段值、未暂存、未保存、未提交：

- `证件类型` -> `id_type` -> `mapping_id=cbcdb325642d5c1b7069ef92`
- `民族` -> `ethnicity` -> `mapping_id=2a36dc1ea4fb39aece876d3a`
- `政治面貌` -> `political_status` -> `mapping_id=b8c1de6c36a1437770d18396`

验证结论：

- Browser Bridge 在 revoke/re-share 后可正常 `discover -> attach`。
- 三个 `mapping_id` 跨多次 interactive snapshot 保持稳定，虽然 `element_id` 每轮都会变化。
- 带 `mapping_hints` 调用后，三个字段都从“未映射”变成对应 `personal.*` resume path 的已映射状态。
- 下一轮不传 hints、重新取新 snapshot 后，`mapping_cache_hit=true`，三个 mapping 仍被复用。
- 随后已经实现站点隔离的持久 mapping memory，并在真实中国电信页重新教学这 3 个字段。
- 再执行一次 `plugin reload` 后，不传 hints 的新 snapshot 返回 `mapping_cache_hit=false`、`mapping_memory_hit=true`，structure signature 仍为 `4c421a4eeee5699591817ce8`，证明“教学 -> reload -> 自动复用”真实成立。
- 持久 memory 位于 provider 私有 cwd 的 `mapping-memory.json`，按 `site + structure_signature` 隔离；进程 cache 也增加 site scope，避免不同站点恰好同结构时串 mapping。
- 当前 private structured resume resource 尚未提供 `personal.id_type`、`personal.ethnicity`、`personal.political_status`，因此它们仍以 `Mapped resume path ... is unavailable` blocker 返回，而不是进入 `recognized`；这不是 mapping 失败。

因此 stable mapping / persistence 这一层已经闭环。下一步应优先把用户已确认的 personal private profile 接上，再验证这些字段能进入 `recognized/plan_fill`。

### 当前产品优先级（用户确认）

第一原则是 **准确，其次是快速**；隐私最小化不是当前优化目标。后续设计取舍按：

```text
accuracy > speed > privacy minimization
```

但“准确”意味着不能猜测未确认的个人事实，也不为了速度削弱最终提交前的 review boundary。可以在确有收益时让模型看到更多页面上下文/资料上下文，不需要为了 Zero-PII 额外牺牲识别率或增加往返。

### P0：把 personal private profile 接上

把用户已确认的中国电信个人数据写进 private profile 的 `personal`：

- 性别
- 出生日期
- 证件类型/号码
- 民族
- 政治面貌
- 籍贯
- 户籍
- 婚姻状况

敏感值不要写到测试 fixture、commit、日志或交接文档。

### P1：增强 Browser semantic label provenance

这是更通用、长期收益更高的修复。

理想 Browser node 应提供：

- associated `<label for>`
- aria-labelledby
- ancestor fieldset/legend
- nearest form-item label
- section heading / group label
- file picker 所属附件分类

这样 Plugin 不需要为中国电信写站点专用 DOM hack。

### 已完成：站点 mapping persistence

已实现：

```text
site identity + structure_signature -> explicit mapping overrides
```

- 默认持久化到 provider 私有 cwd 的 `mapping-memory.json`
- 最多 256 个持久 entry
- 同一 entry 最多 256 个 mapping hint
- caller 新教学覆盖同 mapping_id 的旧教学
- Plugin reload 后恢复
- 已在中国电信真实页做过 reload 后复用验证

后续再考虑加入 successful execution evidence / user correction history；当前先以“准确复用显式教学”作为最小闭环。

### P1：开发体验 helper

当前快速迭代已经不需要 Runner restart，但仍是：

```text
edit -> build/test -> plugin reload -> dogfood
```

建议增加单命令 helper：

```text
campus-plugin-dev-reload
```

同时 reload regular mini 与 mini-dogfood，减少重复操作。

## 与申请资料相关的其它状态

- MSI `D:\pic` 已注册为 Project：`agent:msi:pic`
- 以后可从 MSI 项目枚举附件；忽略目录、压缩包和文件名含“副本”的文件。
- private 问答资料位于 jianli 仓库 local-only 文件，不应提交。
- 上传简历时要避免站点“解析简历并覆盖表单”的路径；只选择普通附件上传，不触发自动解析。

## mini dogfood / Tunnel / WebUI

当前 dogfood：

```text
Server: 127.0.0.1:18081
primary Tunnel: embedded / Server-owned
secondary Tunnel: stopped
```

WebUI 路径：

```text
http://127.0.0.1:18081/runtime
```

Server 只绑定 loopback，因此从 MSI 查看时最简单、安全的方式是 SSH local forward：

```powershell
ssh -N -L 18081:127.0.0.1:18081 mini
```

然后 MSI 浏览器打开：

```text
http://127.0.0.1:18081/runtime
```

WebUI 要的是 **runtime Bearer credential**，不是 OpenAI Tunnel API key。

mini 已有普通 user token 文件：

```text
/Users/yyjeqhc/.config/webcodex/dogfood-mini/client/http_127.0.0.1_18080/yyjeqhc-dogfood/webcodex-user-token
```

从 MSI 复制到剪贴板而不在聊天/终端回显：

```powershell
$token = ssh mini 'cat /Users/yyjeqhc/.config/webcodex/dogfood-mini/client/http_127.0.0.1_18080/yyjeqhc-dogfood/webcodex-user-token'
$token.Trim() | Set-Clipboard
```

然后粘贴到 WebUI 的 **Access key**。

不要用/复制：

- OpenAI Tunnel API key
- Tunnel ID
- `WEBCODEX_TOKEN` bootstrap/admin secret

`WEBCODEX_TOKEN` 只应留在 Server，本地 Tunnel 会按设计私下使用；它不是日常 WebUI credential。

## 下一窗口第一句话建议

> 继续 `plugins/campus-application/HANDOFF.md` 的中国电信真实表单 dogfood。stable mapping 与 reload 后持久复用已经验证通过；下一步优先接入已确认的 personal private profile 数据并验证 `recognized -> plan_fill`，同时继续以“准确 > 速度 > 隐私最小化”为产品取舍。不要猜未确认的个人事实，也不要提交申请。
