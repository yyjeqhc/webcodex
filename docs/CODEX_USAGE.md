# Codex Usage Workflow

Use this as the default evidence loop for Codex or GPT Actions work on this
repository.

## Start With Evidence

Before changing code, run:

```bash
python3 scripts/pdctl.py workflow private-drop --mode snapshot --json
```

Use the response to anchor the task:

- `git_before`
- `git_after`
- `warnings`
- `recommended_next_action`

Do not treat a plan as a result. If the task requires code changes, make the
changes and then gather new evidence.

## After Modifying Code

Run the project precommit hook:

```bash
python3 scripts/pdctl.py precommit private-drop --json
```

When reporting, cite the response fields that matter:

- commit hash, when a commit was created
- `git_before` and `git_after`
- `hook_result`
- `warnings`
- `recommended_next_action`

## Final Report For Code Changes

Use this shape:

```text
changed files:
tests run:
final git status:
commit hash:
```

If no commit was requested or created, say so directly.

## Review-Only Report

Use this shape:

```text
checked commit:
checked files:
grep/test evidence:
concrete risks:
```

Keep findings tied to file paths, line numbers, command output, or API evidence.

## Useful Commands

Snapshot:

```bash
python3 scripts/pdctl.py snapshot private-drop --json
```

Doctor without running hooks:

```bash
python3 scripts/pdctl.py doctor private-drop --json
```

Doctor with its configured hook:

```bash
python3 scripts/pdctl.py doctor private-drop --run-hook --json
```

Specific hook:

```bash
python3 scripts/pdctl.py hook private-drop doctor --json
```

Precommit:

```bash
python3 scripts/pdctl.py precommit private-drop --json
```
