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
- 2026-10-07 已再次 fetch/rebase 到当时最新 `origin/main`；本交接不依赖 rebased commit hash，最终状态以 `git log origin/main..HEAD` 为准。
- 当前分支已按独立 commit 收口：persistent mapping、China campus fallback、Browser semantic labels、bounded form context、live dogfood fixes、Adapter v2 nearby/group labels、choice labels，以及 Plugin 对通用 form provenance 的消费。
- 不 push；完成每轮后保持 worktree clean。
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
cargo fmt --all
cargo test --profile dogfood -p webcodex-browser
cargo test --profile dogfood -p webcodex-tool-contracts --lib
npm --prefix plugins/campus-application run typecheck
npm --prefix plugins/campus-application test
git diff --check

webcodex-browser: 87 passed, 0 failed, 1 ignored
webcodex-tool-contracts: 289 passed, 0 failed
campus-application: 35 passed, 0 failed
```

上一轮 regular mini Runner 与 mini-dogfood Runner 的 `campus-application` provider 状态均为 ready；本轮 Adapter v1 源码尚未部署/重启 Runner，新的 Browser `form_context` 也尚未进入 live runtime。

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

#### 2026-10-06 下一轮：真实页面 fallback / 中国字段语义

这一轮继续以同一中国电信页面验证，未保存、未提交申请。

实现与验证：

- 修正了一个会真实填错的语义：`籍贯` 与 `生源地/高考生源地` 不再共用 `native_place`；新增独立 `student_origin`。
- 增加健康状况、应届状态，以及籍贯/户籍/生源地/现居地/院校所在地的省市拆分 canonical field。
- `mapping_hints` 增加 `choice_value`，用于无 label radio/checkbox 的一次教学；该值也进入持久 mapping memory。
- `analyze_form` / `plan_fill` 新增 `missing_profile_fields` 与 `unmapped_candidates`，把“资料缺失”和“仍需教学”直接分开返回。
- 在截图确认后，真实教学了：
  - `性别-男` -> `gender`, `choice_value=男`, `mapping_id=8dbcda7e9b5d4ac3f26f0377`
  - `性别-女` -> `gender`, `choice_value=女`, `mapping_id=2f2c06e0d6ce9cde89275274`
  - `健康状况` -> `health_status`, `mapping_id=79d1456a5200dafb1eaf181d`
- 再次 `plugin reload` 后，不传 hints：`mapping_cache_hit=false`、`mapping_memory_hit=true`，以上 mapping 均自动恢复。
- live 输出现在把缺失资料去重为 5 项：`id_type`、`gender`、`ethnicity`、`political_status`、`health_status`；性别的两个 radio 合并为同一个 profile requirement。
- 搜索了现有 private profile、`jianli` 本地资料以及 WebCodex 本地配置，没有找到这些字段的已确认真实值，因此没有猜测或写入任何个人事实。

#### 2026-10-07 下一轮：Browser semantic label provenance

本轮开始时已先拉取最新 main；`origin/main` 从 `25af63a9` 前进到 `301a4a66`，两轮 Plugin commit 已无冲突 rebase。

原计划继续在中国电信真实页教学籍贯/户籍/高考生源地/现居地/院校所在地/应届状态，但当前 Chrome extension 没有共享任何 tab：`observe_browser discover` 返回空 attachments、旧 browser id 已 stale。因此没有伪造 live 结果，也没有为了恢复页面去操作用户浏览器。

转而完成更通用的 P1 Browser 修复，目标是从源头减少“控件存在但 name/group_label 为空”：

- 非空 AX accessible name 始终优先，不被 DOM fallback 覆盖。
- AX name 为空时，按高置信度语义补充：
  - `aria-label`
  - `aria-labelledby` 引用文本
  - `<label for=...>`
  - wrapping/nested `<label>`
- `fieldset > legend` 会补成 `group_label`，使 radio/checkbox 可直接得到“性别”等组语义。
- 对常见 form-item wrapper 增加保守 fallback：只有一个容器存在 **恰好一个 native input/select/textarea control** 且有 **恰好一个未绑定的直接 `<label>` 子节点** 时，才把该 label 投影给唯一控件。
- 多控件容器明确不猜；回归测试覆盖“高考生源地”唯一控件可识别，以及“一个 label + 两个 textbox”保持无名。
- Browser-private native shadow control owner 的 promoted node 也继承同一语义 label / fieldset group。
- 这些变化只增强 snapshot 观察语义，不新增 Browser effect、不放宽 control capability/authority，也不引入 selector/script 控制路径。

验证：

```text
cargo test --profile dogfood -p webcodex-browser
84 passed, 0 failed, 1 ignored

npm --prefix plugins/campus-application test
26 passed, 0 failed
```

这一轮尚未做真实中国电信页面回归，因为 tab 已不再共享，而且当前源码改动没有未经授权重启/部署 Runner。下一次 live dogfood 应先部署该已验证 commit，再由用户把目标 tab 重新 share；若页面 DOM 使用标准 label/aria 或单控件 form-item 结构，原本需要手工教学的一批字段应直接进入自动 mapping。

#### 2026-10-07 下一轮：Browser Bridge Resume/Form Adapter v1

用户已重新 Share 中国电信在线简历 tab，本轮先重新 attach 并记录旧部署 Runner 的真实基线：

- interactive snapshot: `node_count=256`, `truncated=true`
- actionable controls: `184`
- 有 `name/group_label`：约 `69`
- 仍无 `name/group_label`：约 `115`
- snapshot 中已经能看到 `72` 个 native option（例如证件类型、民族的实际选项）

这进一步确认问题不是“Browser 完全看不到 DOM”，而是 **页面表单上下文没有被结构化投影给 Agent**；同时 256-node 页面已经截断，不适合继续靠肉眼逐项猜映射。

本轮没有另写第二个 Chrome Extension。现有 `extensions/browser-bridge` 已通过 `chrome.debugger + nativeMessaging` 把用户显式 Share 的 tab 的 Accessibility/DOM/Page/Input 等 CDP 能力安全桥接给 Runner，继续复用它更直接。Adapter v1 放在 Browser snapshot 层，并新增可选 `form_context`：

- `field_signature`：24 hex，排除当前 value 和 opaque `element_id`
- DOM tag / input type / HTML name
- placeholder / autocomplete
- 最近的直接 section heading
- component hint：native / Ant Design / Element / iView / Arco / TDesign / Semi / sd-Select / select2 等
- `aria-invalid`
- `aria-errormessage` / `aria-describedby` 的 bounded validation hint
- native select `option_count`

实现原则：

- `form_context` 是观察语义，不新增 Browser effect/selector/script authority。
- 现有 AX name/group_label 仍是第一证据；`form_context` 只补充缺失语义。
- campus Plugin 已能用 `html_name/placeholder/autocomplete` 补映射，用 `section_label` 补重复教育/实习/项目路径，并把完整 context 放进 `unmapped_candidates` 方便快速 fallback。
- radio/checkbox 若由 form context 确认 canonical field，仍用可见 choice label 与 profile desired value 比对后才产生 click。
- **不把 `form_context` 纳入现有 mapping_id / structure signature**，避免升级后让中国电信已经积累的 persistent mapping memory 全部失效。
- mapping cache 每次重新派生自动语义，只把显式 hint 当跨 snapshot authoritative override；因此同一 structure 先被旧 Browser 观察过，也不会压住后续新 Browser 提供的 form context。

当前源码测试已经覆盖 field signature 对 value 变化稳定、section/component/validation/option metadata、旧 mapping identity 兼容、旧空 cache 不阻止新 form context、生源地自动映射、重复教育 section 和简历附件语义。

**新版 Runner 已完成部署与 smoke。** 用户已明确允许部署类操作直接执行。`mini-dogfood` Runner 已从旧 `5024ed07be2b` 版本切换到包含 Resume/Form Adapter 的当前分支构建；LaunchAgent `cn.yyjeqhc.webcodex-runner.dogfood-mini` 重启后正常注册到 `127.0.0.1:18081`，transport=`websocket`，plugin `campus-application` check 为 `ready` / 3 tools。regular mini Runner 使用独立 `/Users/yyjeqhc/.local/lib/webcodex-dev/webcodex-runner`，本次未替换。

部署后的 owned Browser 访问 `https://httpbin.org/forms/post` 做了端到端 smoke：interactive snapshot 共 13 个 actionable node，13/13 都带 `form_context`；例如 `custtel`/`custemail`、radio/checkbox `html_name`、time input 和 textarea 均正确投影。把同一 snapshot 直接交给 `campus-application.analyze_form` 后，Plugin 正常消费新结构：自动识别 phone/email，并在 `unmapped_candidates` 中保留其它字段的完整 `form_context`，证明 Browser -> Runner output schema -> Plugin input schema -> analyzer 整条链已通。

**中国电信真实页已在用户重新 Share 后完成 Adapter v1 live 回归。** `discover -> attach` 正常，interactive snapshot 仍为 `node_count=256 / truncated=true / actionable=184`；旧版约 `69` 个 actionable 有 `name/group_label`、`115` 个没有。新版这 `115/115` 个无 label actionable 全部获得了 `form_context`，整个 actionable 集合中 `168/184` 个有 `form_context`。这证明 Adapter v1 已把 DOM-level field identity/shape 全量带到了原本最困难的控件上。

但 live 结果也明确暴露下一瓶颈：该站大量 HTML `name` 是 `11_28_1`、`14_53_1`、`123_2129_1` 这类站点内部数字编码，`form_context.section_label` 也尚未捕获页面肉眼可见的“姓名 / 证件类型 / 性别 / 民族 / 籍贯 / 高考生源地”等 form-item label，所以仅凭 v1 自动 mapping 并不会把这 115 个控件直接转成 canonical field。当前 `analyze_form` 仍主要依赖之前的 persistent mapping memory：`structure_signature=4c421a4eeee5699591817ce8`、`mapping_memory_hit=true`，已教学的 `id_type / gender / ethnicity / political_status / health_status` 仍能稳定恢复。

真实页还发现并立即修掉了一个准确性问题：`form_context.html_name=phoneArea` 曾因 contains(`phone`) 被误映射成 `contact.phone`，甚至会计划把手机号填进“中国大陆”区号下拉。现在 machine-oriented `html_name/autocomplete` 只允许 **exact canonical match**；用户可见 `name/group/placeholder` 仍保留 contains 语义。新增回归测试确认 `phoneArea` 不再映射，而 exact `phone` / `studentOrigin` 仍可自动映射。Plugin 已在 `mini-dogfood` reload，真实中国电信页复测后 `recognized_count` 从错误的 1 降为安全的 0，`phone_mappings=[]`；测试总数更新为 30/30。

因此下一轮 Browser Adapter 应集中做 **nearby visible label provenance**：从每个 form-item 的 DOM 邻近文本/label cell/aria/组容器中保守提取用户肉眼看到的字段名，并允许多控件组（例如籍贯省+市、性别男+女）共享一个 group label。不要再扩大静态 alias 或根据 `11_28_1` 这类站点编码猜字段。

#### 2026-10-07 Browser Adapter v2：通用 nearby/group label provenance

这一轮严格按“收益必须适用于所有网站”实现，没有加入中国电信 URL、数字字段名或站点专用 DOM selector。Browser DOM semantic 层新增通用规则：

- 唯一可见前置文本 + 单控件容器 -> `nearby_label`
- table cell / nested wrapper 等同样按结构传播，不要求特定 CSS framework class
- 一个可见标题 + 多个控件 -> `group_label + group_index + group_size`
- 省/市、日期范围等真正 composite group 只暴露组语义，不自动把多个控件映射到同一个 canonical field
- 多个候选 label、隐藏文本、heading、尾随 help text 等不确定结构 fail closed
- radio/checkbox 的小型单选项容器允许一个短的尾随可见文本作为 choice label；普通 textbox/select 不使用尾随文本，避免把校验/帮助文案误当字段名

Adapter v2 的 `feat(browser): infer nearby form labels` 已部署到 `mini-dogfood` 并在用户重新 Share 的中国电信真实页做了 live dogfood。结果：

```text
interactive node_count:       230 (truncated=true)
actionable:                   184
v1 name/group coverage:        69 / 184
v1 unlabeled actionable:      115 / 184
v2 name/group coverage:       176 / 184
v2 unlabeled actionable:        8 / 184
form_context coverage:        168 / 184
nearby_label:                  62 actionable
group_label context:           54 actionable
```

也就是说原来最困难的 115 个无 label actionable 中，约 107 个现在已经直接获得 model-visible `name` 或 `group_label`；这是 Browser 层的通用 DOM provenance 收益，不是 campus plugin 的站点 hack。真实页已经能直接看到诸如姓名、证件类型、性别、民族、政治面貌、是否应届、是否接受调剂、籍贯/生源地/户口所在地/现居住城市、教育字段、附件类别等语义。

Adapter v2 改变了 model-visible semantic shape，因此该页面结构签名从 v1 的 `4c421a4eeee5699591817ce8` 变为 v2 的 `0e393a93e7f78d89768b42f4`，旧 persistent mapping entry 本次不会命中。大部分旧教学现在已经被自动 DOM provenance 取代；后续若还需要跨 semantic-version 迁移 explicit teaching，应单独设计 versioned mapping identity，而不是猜旧 mapping id。

v2 live dogfood 还暴露并修复了三类 **通用准确性问题**：

1. `专业课程 / 专业资格证书 / 专业资格证书等级 / 专业资格证书获得时间` 不应因为 contains(`专业`) 被误映射为 `major`。现在 `专业` 保留 exact match，并补 `专业名称 / 所学专业 / 主修专业 / 专业方向` 等明确语义；模糊 contains 不再匹配裸 `专业`。
2. 附件按钮的 AX name 往往只是“选择文件”，真正类别在 nearby label。`isResumeUpload` 现在会读取 `form_context.nearby_label`，因此“简历”附件可自动识别为 `resume_path`，而成绩单/身份证/其它附件仍保持 unmapped。
3. framework 自定义 select 经常一个逻辑字段含多个 DOM/internal control。Plugin 不再仅因 DOM `group_size > 1` 就拒绝 scalar group；它按 **model-visible projected controls** 判断：同组只有一个 projected data control 时允许 exact group mapping，真实 composite（例如省+市、电话国家区号+号码）有多个 projected controls 时继续 fail closed。

当前 China Telecom `analyze_form` 在不依赖旧 mapping memory 的情况下已能自动建立 `id_type / gender / ethnicity / political_status / is_fresh_graduate / accept_transfer` 等映射；由于 private profile 尚缺相应值，它们表现为 `missing_profile_fields` 而不是错误 fill plan。当前 profile 可用值下 recognized 包括主姓名、邮箱、毕业时间、学历/学位、专业名称和普通简历附件；live 发现的错误 `专业*` major 映射已经清除。

源码还加入了 generic choice-label 规则（例如 `<div><input type=radio><span>Male</span></div>`），`mini-dogfood` 已部署包含该 Browser commit 与 Plugin follow-up 的分支构建，service 正常 running。用户重新 Share 后已完成真实页验证：`性别` 的两个 radio 现在分别是 `name=男/女`，`是否为应届毕业生`、`是否有运营商实习经验`、`是否接受岗位调剂`、`是否有亲属在中国电信集团（系统）从业`、`是否最高学历` 的 choice 也都稳定投影为 `是/否`，同时保留各自 group label、group index/size 和 checked state。interactive snapshot 本轮为 `node_count=216 / actionable=184 / unlabeled=7`，比 v2 初版的 8 个又少 1 个；剩余 7 个主要是 privacy checkbox 的 duplicate/native wrapper 和无语义 click-only button，不属于普通资料字段。

同一真实 snapshot 重新交给 `campus-application.analyze_form`：Plugin `ready / 3 tools`，不依赖旧 mapping memory 即能把 `id_type / gender / ethnicity / political_status / health_status / is_fresh_graduate / accept_transfer` 等字段识别为 **已映射但 profile 缺值**，而不是 unmapped；当前 `missing_profile_fields` 包含 `id_number / birth_date / id_type / gender / ethnicity / political_status / health_status / is_fresh_graduate / accept_transfer`。`recognized` 中保持姓名、邮箱、毕业时间、学历/学位、专业名称和简历附件等已有可用项，`专业课程/专业资格证书*` 的误识别未复发。说明 Adapter v2 的核心 Browser provenance + Plugin 消费链在真实站点已经闭环。

### 当前产品优先级（用户确认）

第一原则是 **准确，其次是快速**；隐私最小化不是当前优化目标。后续设计取舍按：

```text
accuracy > speed > privacy minimization
```

但“准确”意味着不能猜测未确认的个人事实，也不为了速度削弱最终提交前的 review boundary。可以在确有收益时让模型看到更多页面上下文/资料上下文，不需要为了 Zero-PII 额外牺牲识别率或增加往返。

### P0：把 personal private profile 接上

把用户已确认的中国电信个人数据写进 private profile 的 `personal`。当前优先缺口已经可以直接从 `missing_profile_fields` 读取；已确认需要补的至少有：

- 性别
- 证件类型/号码
- 民族
- 政治面貌
- 健康状况
- 出生日期
- 籍贯（与生源地分开）
- 户籍
- 高考生源地
- 是否应届毕业生
- 现居住地 / 院校所在地等页面要求的位置字段

本机现有资料没有找到这些字段的可靠值；在用户或其它权威本地资料确认前不要猜。真实值只写 private profile，不写测试 fixture、commit、日志或交接文档。

### P1：增强 Browser semantic label provenance

这一层已完成第一阶段，并有 Browser package 回归测试：

- [x] associated `<label for>`
- [x] `aria-labelledby`
- [x] wrapping/nested `<label>`
- [x] ancestor `fieldset/legend`
- [x] 保守 nearby visible label（单控件容器 / table cell / nested wrapper）
- [x] multi-control `group_label + group_index + group_size`
- [x] radio/checkbox 单 choice 的 bounded trailing visible label
- [x] ambiguous multi-label / help-text / hidden-text fail closed
- [x] direct section heading 的 bounded `form_context.section_label`
- [x] file picker 所属附件分类可由 `nearby_label` 投影给 caller；campus 当前只自动消费“简历”类别，其它附件保持显式映射
- [x] bounded form context（HTML name / placeholder / autocomplete / component / validation / native option count）

下一步继续只补可跨站复用的结构：优先验证 choice option label、重复 section heading/provenance，以及 composite field 的安全拆分；不要写中国电信字段编号或站点专用 DOM hack。

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
