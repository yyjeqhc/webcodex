# Demo：通过 WebCodex 安全执行一次 Coding Task

[English](DEMO.md) | [简体中文](DEMO.zh-CN.md)

这个 demo 展示 WebCodex 期望的工作流。它是产品 walkthrough，不是必须逐字复制的脚本。

## Prompt

```text
Use WebCodex on project agent:local-dev:<project_id>.
Inspect this repo, find a small failing or stale documentation issue, fix it,
run appropriate validation, show changes, run workspace hygiene, and finish.
Prefer structured edit tools. Do not touch secrets. Do not use run_shell unless
the structured validation tools are not enough.
```

第一次运行建议只读：

```text
Use WebCodex on project agent:local-dev:<project_id>.
Inspect README.md and docs/QUICK_START.md, summarize how setup works, show
changes without a diff, run workspace hygiene, and finish. Do not edit files.
```

## 预期工具流

1. `start_coding_task`
2. `list_project_files`、`search_project_text` 或 `read_file`
3. `apply_text_edits`（局部精确）、`apply_patch_checked`（多文件）或 `write_project_file`（新建/整文件重写）
4. `validate_patch`、`cargo_fmt`、`cargo_check` 或 `cargo_test`
5. `show_changes`
6. 需要定向 diff 检查时使用 `git_diff_hunks`
7. `workspace_hygiene_check`
8. `finish_coding_task`
9. 需要另一个 operator 或 client 接手时使用 `session_handoff_summary`

`run_shell` 是受限 escape hatch，不应该作为编辑或验证的第一选择。

## 用户需要检查什么

- changed files 是否符合请求范围。
- validation result 是否匹配变更类型。
- `show_changes` 输出，包括 warnings 和 truncation flags。
- `workspace_hygiene_check` findings。
- 最终 `finish_coding_task` task/evidence outcomes。
- handoff notes、open risks 或 rollback instructions。

## 安全提示

- 不要在 prompt、tool output、文档或示例中暴露 secrets。
- 明确写出 project id：`agent:<client_id>:<project_id>`。
- 优先使用结构化编辑，而不是 shell 写入。
- 优先使用结构化验证，而不是宽泛 shell 命令。
- 最终 workspace 应该 clean，或者是为了人工检查而有意保持 dirty。
- 如果任务只是 smoke test，用常规 Git review 流程回滚小修改。

## 下一步

- 第一次设置：[QUICK_START.zh-CN.md](QUICK_START.zh-CN.md)
- 概念：[CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md)
- 安全：[../SECURITY.md](../SECURITY.md)
