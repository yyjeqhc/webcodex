# campus-application Native Tool Plugin

`campus-application` 是 WebCodex 的 first-party Native Tool Plugin，用于配合 Browser Use 规划安全的简历表单填写。它把 bounded Browser semantic snapshot 映射到结构化简历字段，支持重复经历、动态新增经历和明确的多步骤网申流程，并且**不会自动提交申请**。

职责边界保持清晰：

- Browser observation 提供语义控件和 opaque element identity；
- `campus-application` 只负责把控件映射到结构化 Resume Profile，并输出 bounded action plan；
- caller 使用普通 Browser action 执行计划；任何 DOM 结构变化或页面步骤变化之后都必须重新 snapshot；
- 发现最终“提交申请”控件时进入 `ready_for_review`，不会生成提交 click。

## 批量填写与 compact reconciliation

普通 section 优先一次 `snapshot(query={fields_only:true})`，可加 section/group，
直接把 nodes 与 snapshot_generation 交给 `plan_fill`。返回的 `batch` 可原样调用
`control_browser`，单次 1～32 项，无需逐字段构造调用。执行后一次 fresh query，
把 nodes、generation 和完整 Browser output receipt 交给 `reconcile_fill`。
成功仅返回 `{"confirmed":20,"needs_attention":[]}`；不重复返回 profile、成功字段或节点。

直调共 5 次工具调用；在编排代码中把 readback 直接传给 reconcile、只输出 delta，
则为 4 轮模型交互。20 字段相对 23 次逐字段流程减少 18 次（约 78%），
0 screenshot、0 pointer、0 单字段执行调用。65 字段按 32/32/1 执行，
每次 reconciliation 从新观察产生下一批，confirmed 不再进入后续计划。

优先原生 `set_value` 覆盖文本框；旧 Runner 只有插入式 `input_text` 时仅填写空框。
partial、unknown、缺失计数或 unstable receipt 不自动产生重试 batch。
目标/URL 变化、过期 snapshot 会拒绝；不复用旧 element id。只针对异常字段恢复。
计划仅在 provider 内存保留（最多 32 个、每个最多 256 字段、15 分钟），重启/过期后
重新观察与规划，不能重放旧 batch。mapping-memory 私有文件格式保持不变。
复杂 combobox/cascader/date picker、上传、选择组仍走异常处理，不自动提交申请。

完整契约、调用预算和本地 Chrome 32 字段验证命令见 [英文说明](README.md)。

## 安装、构建与测试

```bash
npm ci
npm run typecheck
npm test
```

需要 Node.js 18 或更新版本。仓库内版本使用 `../../npm/plugin-sdk` 本地依赖，因此 CI 会持续验证当前 WebCodex checkout 的 SDK。

## 配置简历

仓库只保存虚构 example。把 `profile.example.json` 复制到 Runner 本机的私有目录，并命名为 `profile.json`，例如：

```text
~/.config/webcodex/campus-application/profile.json
```

`profile.json` 已被 Git ignore。默认情况下 Plugin 从 provider 配置的 `cwd` 读取它；也可以通过 `WEBCODEX_CAMPUS_APPLICATION_PROFILE` 显式指定其它 profile 文件。

结构化 profile 覆盖身份/联系方式、中国校招常见个人字段（性别、生日、证件、民族、政治面貌、健康状况、籍贯、户籍、生源地、应届状态、婚姻状态）、省/市拆分位置字段、个人链接、多段教育/实习/项目/校园经历、技能/语言、求职偏好、申请文案和附件。`籍贯` 与 `生源地/高考生源地` 是独立字段，不会互相复用。

`attachments.resume_path` 最终会交给 Browser `upload_file`，它必须相对于 caller 为上传授权的 WebCodex Project 有效，而不是相对于 Plugin profile 目录解析。

## 配置 Runner

```toml
[[plugins.providers]]
id = "campus-application"
name = "Campus Application"
command = "node"
args = ["/absolute/path/to/webcodex/plugins/campus-application/dist/plugin.js"]
cwd = "/absolute/path/to/private/campus-application-data"
timeout_secs = 30
```

Runner restart/reload 后可以检查：

```text
webcodex plugin check --runner <runner> --plugin campus-application
webcodex plugin reload --runner <runner>
webcodex plugin describe --runner <runner> --plugin campus-application --tool plan_fill
```

实际调用仍走 canonical `plugin_tool describe -> call` 路径。

### 实际页面调试循环

Plugin provider 是 Runner 管理的常驻子进程，因此 **TS/JS 代码改动不会被已经运行的 provider 自动拾取**。推荐循环是：

```text
修改 src/*.ts
  -> npm run typecheck / npm test（test 会重新生成 dist）
  -> plugin_tool check(runner, plugin)   # disposable candidate，不影响现有 provider
  -> plugin_tool reload(runner)          # 原子替换完整 provider set
  -> describe -> call
```

整个过程不需要重启 Runner。`profile.json` 则在每次 tool execute 时重新读取，因此只改结构化个人资料时**不需要 build、check 或 reload**。

对于真实 ATS 中 semantic snapshot 无 label 的控件，`analyze_form` / `plan_fill` 会返回稳定的 `mapping_id`。caller 可以传 `mapping_hints`（label 或 canonical field + resume path）教会当前结构；radio/checkbox 还可以附带 `choice_value`，把无文本的选项一次教学为“男/女”“是/否”等实际语义。hint 不依赖易变的 Browser `element_id`。学习结果同时进入站点隔离的进程内 cache，并持久化到 provider 私有目录的 `mapping-memory.json`（可用 `WEBCODEX_CAMPUS_APPLICATION_MAPPING_MEMORY` 改路径），键为 `site + structure_signature`，因此 Plugin reload 后仍可复用。

较新的 WebCodex Browser snapshot 还会为表单控件附带可选 `form_context`：稳定 `field_signature`、DOM tag/type/name、placeholder/autocomplete、section heading、组件提示、`aria-invalid`/校验提示和 native select option 数量。Plugin 会把这些信息作为 **补充证据**，用于原本无 AX label 的字段和重复经历 section；它们不会改变既有 `mapping_id` / structure signature，因此历史 mapping memory 仍可复用。旧 Runner 没有 `form_context` 时仍按原路径工作。

## 工具与流程

`profile_get` 返回配置的结构化 Resume Resource 和当前 bounded canonical view；`analyze_form` 返回字段映射、indexed resume path、blocker 和 form-structure signature；`plan_fill` 返回下一步安全 phase：

`analyze_form` / `plan_fill` 还会把失败原因压缩成两个可直接处理的集合：`missing_profile_fields` 去重列出“映射已经确定但本地资料缺失/为空”的 profile path；`unmapped_candidates` 列出仍需要教学的可操作控件（含 `mapping_id`、role、actions）。这样 caller 不需要从大量 blocker 文本中重新推断下一步。

- `expand_sections`：只返回一个“新增经历” click，之后必须重新 snapshot；
- `fill_fields`：返回 `input_text`、`select_option`、`set_value`、`upload_file` 或 grouped `click`；
- `advance_step`：只有存在明确 step/progress 语义时才返回一个“下一步/继续” click，之后必须重新 snapshot；
- `ready_for_review`：已经看到最终提交控件，不返回 submit click。

重复经历映射到精确来源，例如 `education[1].school`、`experience[1].start_date`、`projects[1].technologies`。

Plugin 使用最多 64 项、按站点隔离的进程内 mapping cache，并额外保存最多 256 个持久 mapping-memory entry。structure signature 有意忽略易变的 element/group identity、当前值、checked 状态、被动 AX 文本、submit button 和原生 picker affordance；显式教学的 mapping 会在 Plugin reload 后从私有 `mapping-memory.json` 恢复。

## 本地 fixtures

运行：

```bash
npm run fixtures
```

然后访问 `http://127.0.0.1:32124/`。fixtures 覆盖单页、中文秋招、多段经历、动态新增经历和四步网申，所有 submit event 都会在本地拦截。

## 安全边界

这个 Plugin 是 planner，不是自动投递服务。它不会自己调用 Browser 工具，不访问外部招聘 API，也不会提交表单。Browser 权限仍由 WebCodex 和 caller 掌握，最终提交始终保留为人工 review boundary。
