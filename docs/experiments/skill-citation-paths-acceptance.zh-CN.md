# Skill 引用路径验收

[English](skill-citation-paths-acceptance.md)

本文记录 [Issue #1196](https://github.com/yyjeqhc/webcodex/issues/1196) 的 T03
有限范围后续验收。[原知识/Skill 验收报告](knowledge-skill-acceptance-results.zh-CN.md)
曾记录审计事实正确，但引用仅为包内 `SKILL.md` / `reference.md`，未通过项目相对路径规则。
历史结果与评分规则保持原样。

## 改动与来源

`load_skill` 和 `read_skill_file` 的成功结果现在区分 `skill_path`（规范化的包内读取路径）
与 `project_path`（权威项目相对引用路径）。后者来自实际解析的包目录，目录名可以与 Skill
metadata name 不同。Runner-configured 和 managed Skill 不返回 `project_path`。
移除含义不明确的输出 `path`，输入 `read_skill_file.path` 和 Runner wire `path` 保持不变。
当前契约见[编码工作流](../CODING_WORKFLOW.zh-CN.md)。

实现里程碑为 `4f839f25356f9f95614999b4d7d14175bd7e6c71`。
测试环境使用干净且 Server/Runner 对齐的 dogfood 构建
`62671f015a77718a35dd0144d808d0751e5fafb8`：它将该里程碑与 Plugin recovery
提交 `b7ff9e85d1abc36666d9e78483e0cdffc100026a` 在本地集成，保留此前测试环境的
Plugin 行为。发布前，Plugin PR #1193 已合入 upstream `9a2ba651`；本分支合入该目标后得到
`0a47fddb`，在添加本文之前，其完整代码树已核对为与实测集成树完全一致。
PR 相对目标的剩余差异仅包含 Skill 后续改动及文档，不依赖未合并的 Plugin PR。

## 确定性与集成检查

- 工具契约：316 项通过；1 项可选成本探针保持忽略。
- Skill runtime：16 项通过，覆盖包目录与 metadata name 不同、规范化的嵌套引用、行号分页、
  revision、Project 范围、来源冲突、敏感路径与越界拒绝。Configured/managed 读取保留包内
  locator，不编造项目路径。
- 审计隐私：1 项 focused 检查通过；保留引用 metadata，继续排除正文。MCP 描述与序列化体积
  检查各 1 项通过，未增加预算。
- 知识 fixture/evidence/Host driver 的 Python 模块：实现分支 59 项通过；集成 Plugin recovery
  后 61 项通过。历史捕获仍可读取；旧新 locator 混用、错误 scope、路径、哈希和 revision
  不匹配仍被拒绝。
- 干净 dogfood 构建通过。既有六场景 MCP/Runner 脚本矩阵及 Host handoff 在关闭 Apps 时通过，
  包含两个更新场景和实际 Skill 输出的规范路径断言。这些检查的**模型样本数为零**。
  Project Knowledge smoke 的 binding、来源保护、恢复与 provider 移除检查也通过。
- Rust formatting、本地 Markdown 链接和 diff 检查通过。

## 真实 ChatGPT Web 样本

2026-10-19 通过原测试连接执行了 1 个全新对话，使用用户报告的 `6-sol`／极高推理，关闭 Apps。
冻结提示要求通过项目 Skill 及其引用资源回答当前审计事实，并给出项目相对引用；没有提供答案、
承载答案的文件路径、工具顺序或操作员检查点。无需人工纠正后续提示。独立 fixture/oracle 与提示分开保存。

私有捕获与 Workflow Session 审计独立确认了 6 次规范调用：`work_on_project` 1 次、
`load_skill` 1 次、`read_tool_manifest` 3 次、`read_skill_file` 1 次。全部成功，adapter
rejection 为 0。两次实际 Skill 读取返回：

| 读取 | `skill_path` | `project_path` | 实读行号 |
| --- | --- | --- | --- |
| 正文 | `SKILL.md` | `.agents/skills/receipt-audit/SKILL.md` | 1–9 |
| 引用 | `reference.md` | `.agents/skills/receipt-audit/reference.md` | 1–4 |

两次读取均属于精确的 fixture Project，并关联同一个已核验的 definition revision；来源 SHA-256
与范围均对照冻结字节检查。两个结果都不含旧输出 `path` 别名。最终答案使用完整项目相对路径，
从正文第 8 行引用 audit order，从引用资源第 3–4 行引用 marker/batch。
六项严格检查全部通过：fixture/Session/Project 绑定、fixture 完整性、实际观察到来源内容、
答案事实与引用、使用所需 Skill 资源路径，以及协调顺序（本例无需更新）。没有 checkout 编辑、
shell/process/script 调用或 Job。

仅替换测试 runtime；稳定版服务身份及原生 tunnel 进程保持不变。核验后测试服务恢复普通的
metadata-only 捕获模式，保留实测二进制与无自动到期的测试凭据。保留回滚二进制和私有数据库
快照；未重置凭据或 tunnel 保护记录。

## 限制与下一次观察

本次结果是一个可归因的 **pass**，不能说明生产成功率、提速或受控前后对比。提示明确要求
Skill 资源与当前项目引用，不能证明自由选择工具时的采用情况。后台模型 ID、token、缓存命中与
纯模型耗时未知。对话由用户执行并转发回复；捕获核验 WebCodex 侧链路，不独立证明浏览器来源
或后台模型身份。3 次 manifest 读取只是一项观察，尚不足以证明发现能力存在缺陷或扩大修复范围。

下一项有限观察可复用既有自由选择工具的规程问答场景，检查规范引用采用情况；需要独立冻结提示
与样本计数，不改写本次结果，也不自动启动检索/Memory 工作。原始捕获、模型回复、Session/Runner
身份、私有路径及凭据保留在 Git 外；本文仅含脱敏事实与验证摘要。
