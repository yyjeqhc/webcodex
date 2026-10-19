# Bounded project formatting lifecycle (#962)

**Contract freeze v1 (2026-10-10)**. Base: `8c7a1c3fd1cf8baeac40652ca8b252cc89e2c685`; this design and implementation live in **one complete PR**, not separate design/API/runtime PRs. Preliminary independent source audit found full-project Cargo/Ruff output-set discovery cannot be proven by simply scanning source roots; v1 therefore selects explicit files.

## Product and scope

Expose a model-facing `project_format` with exact Runner Project, optional project-relative cwd, adapter `auto|rust|python|go`, and **required** bounded ordered file paths (1..8) under the selected recipe root. File selection is explicit; no implicit workspace/project traversal, package-manager script selection or arbitrary CLI flags. Rust `.rs`, Python `.py/.pyi` and Go `.go` are supported only when their installed formatter and local configuration satisfy the closed profile. More expansive project/workspace formatting is unavailable, not silently approximated.

The tool is an intentional source **Mutate** operation with non-idempotent dispatch and both `project:write` and `job:run` authorization. It is distinct from `project_validate(action=format_check)`, existing `cargo_fmt`, and the canonical `edit_project_files` receipt; do not change their behavior or fake their authority. A formatting subprocess is not an OS sandbox or an assertion that no other filesystem effects are possible.

## Trusted execution

- Runner resolves exact registered Project and nearest unambiguous recipe root, refuses cross-root, symlink and nonregular inputs; filenames have length/count bounds, files have per-file and total-byte caps, and inputs stay private.
- Runner computes a closed plan from project/cwd/adapter/selected files, bounded manifest/config, exact fixed command choices and content witnesses. The plan and files are revalidated at admission and immediately before worker execution. Each selected file and the final known-result boundary also revalidate current Project write permission, recipe ownership, root identity, and manifest/profile provenance through the same bounded planner. Context comparison excludes file witnesses legitimately changed by earlier writes in the same batch; original per-file compare-and-write and final candidate verification remain separate. Changed context stops further writes and leaves a partial batch unknown.
- Rust path: selected files only; run trusted installed `rustfmt` on **stdin**, with `--emit=stdout`, fixed edition from bounded local Cargo manifest and `skip_children=true`; project custom rustfmt configs and unsupported source/manifest topology are unavailable unless an exact bounded configuration rule is independently proven. This is **not** workspace `cargo fmt`.
- Python path: selected files only; use existing Runner-authorized isolated Python interpreter and installed Ruff module. Run `python -I -B -m ruff format --no-cache --config pyproject.toml --stdin-filename <selected-file> -` with bounded UTF-8 stdin/stdout; inherit the existing Ruff manifest requirements (local `[tool.ruff]`, explicit target-version, no `extend`) and environment sanitization, with no installs/fallback. Validate actual tool version/behavior before adding backend support. Treat stderr failures/oversized output as unknown and never write.
- Go path: explicit `.go` files under one nearest unambiguous `go.mod` recipe; use installed `gofmt` with **no arguments**, source bytes on stdin and candidates on stdout. No filename arguments, `-w`, `-s`, rewrite rules, `go fmt`, `goimports`, module loading or dependency/toolchain downloads. The bounded UTF-8 `go.mod` content and file identity are scope/provenance witnesses only, not a module-syntax verdict; module loading, compilation and build-tag coverage are not performed. Native stdin formatting accepts full programs and Go fragments. Preserve the prepared Runner executable/environment owner. Missing gofmt fails without installation or fallback. Go requires both `project_format_v1` and additive `project_format_go_v1` at planning and typed Job admission, including `auto` inferred from selected `.go` files; Rust/Python retain their old capability contract. Older Runners default the Go flag to false.
- The native formatters generate **candidate bytes**, never edit project files directly. Compare exact original bytes: identical candidate -> known no-op. If changed, Runner performs guarded compare-and-write to each exact selected file, verifying file type, canonical path and source content immediately before commit. Multi-file write is not falsely described as atomic; on a partial failure, interrupted write, changed path/contents, timeout or lost receipt, mutation truth becomes **unknown**, not false/success. Only complete verified outcomes return `state_changed=true|false`.
- All formatting phases belong to **one durable Runner Job**, created before execution. At most one write attempt per file per Job. Sync completion may return a terminal result; pending returns the **same** Job for observe/recovery. Runner persists a bounded three-state mutation report in the canonical Job/receipt before projecting it to the model. Host kernel may capture the authoritative `state_changed` before projection, never infer it from user-visible status or exit 0. Crash windows remain unknowable; **at-most-once admission, not cross-crash exactly-once completion**.
- Preserve source mutation observation, session identity, runner capability gates, replacement Runner fail-closed behavior and existing Code Mode dependent-validation gate (which still requires `edit_project_files`). Never auto retry or redispatch a mutating Job. No network permission is inferred from a successful formatter.

## Regression boundary — included in the same feature PR

| ID | Acceptance |
|---|---|
| PF01 | ToolSpec/ToolCall/input-output schema has one closed `project_format` and Mutate + ProjectWrite + JobRun authority, non-idempotent. |
| PF02 | Missing capability, old Runner, wrong auth, changed Runner identity and unauthorized project reject without dispatch. |
| PF03 | Recipe/cwd auto vs explicit, ambiguous root, cross-root path, traversal, Windows separators, control/NUL and extension mismatch fail closed. |
| PF04 | Too many, duplicate, oversized, unreadable, symlink or nonregular file targets reject without writing. |
| PF05 | Valid Rust edition selection and fixed stdin/stdout rustfmt behavior; external/config-extended/custom rustfmt topology unavailable. |
| PF06 | Ruff missing/unsupported interpreter/module, pyproject missing target, invalid/extend config, injected output/cache env fail closed. |
| PF07 | Output identical to input proves no-op without write, not a guessed zero. |
| PF08 | Formatter candidate differs and guarded write commits only selected files, proving positive change. |
| PF09 | Manifest/file/source change between plan and enqueue/worker rejects; between candidate and guarded write detects conflict. |
| PF10 | Formatter failure/truncation, unexpected stdout, concurrent modification, postwrite observation failure and multi-file partial failure never claim known full completion. |
| PF11 | Cancellation, timeout, dropped Server response, reconnection and replacement use original Job; unknown cannot authorize replay. |
| PF12 | Durable terminal report and synchronous result retain the same mutation truth; missing old receipt defaults unknown. |
| PF13 | Kernel pre-projection canonical truth, model sparse result, Code Mode receipt and source invalidation preserve authority boundaries. |
| PF14 | Exact failure kind, no test counts or validation-summary reuse; process logs and model output bounded. |
| PF15 | Existing `project_validate`, `project_build`, `cargo_fmt`, editors and unrelated languages stay unchanged. |
| PF16 | Focused regression, native formatter fixtures, Windows/Linux/macOS CI, independent reviews and fork full CI before upstream PR. |

## Execution sequence

Implement **vertically within one branch/one PR**: Core typed plan and durable mutation report → Runner plan/probe/admission/guarded writer → Runner Job protocol/persistence/recovery → Registry capability/permission → ToolCall/ToolSpec/Host projection → tests and real formatter cases → review/Codex/agy/fork complete CI. No new abstraction duplicating `project_build` planning, no universal task tool, no implicit full-workspace scanning. If a critical source-truth gate cannot be closed, keep feature branch unsubmitted and report exact blocker; do not ship a misleading API.

No automatic push, fork PR, or upstream PR until staged evidence is ready and upstream submission is explicitly approved.

## Go adapter acceptance (#962 additive stage)

The Go adapter reuses PF01–PF15 and the existing guarded commit and durable
mutation report. Focused coverage adds explicit/auto selection and closed wire
profiles; rolling-upgrade rejection before dispatch and after Runner replacement;
module marker/source identity fences; nested/ambiguous recipes; input limits;
real gofmt candidate generation, selected-only changes, unchanged no-write,
syntax/output-limit failures, missing tooling, and stop/deadline handling.
A replaced module marker during a batch prevents later writes and does not
produce a known success receipt. Symlink, alias and path guards remain shared.

Run the focused Core, Tool Contracts, Registry, Runner and Server project-format
suites with `--profile dogfood`. With a verified Go toolchain available on the
test process PATH, also run:

```sh
cargo test --locked --profile dogfood -p webcodex-runner --features runner-real-process-tests runner_real_process_project_format -- --ignored --test-threads=1
python3 scripts/e2e_go_project_format_ws.py --bin-dir target/dogfood --expected-commit FULL_BUILD_COMMIT
```

The E2E uses temporary authenticated loopback Server/Runner/Project state and
real gofmt, checks exact retained Job identity and mutation receipts, and never
contacts an existing deployment or model. Fast formatting may complete before
the supported one-second handoff ceiling; that case still verifies the original
retained Job through `observe_jobs`, without claiming a pending handoff occurred.
Linux native acceptance does not establish Windows/macOS native behavior.
