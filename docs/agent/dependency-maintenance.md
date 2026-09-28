# Dependency and warning maintenance

This review targets the default Linux production graphs and the root Cargo
workspace. Its starting point was `f8124ef6`; the changes were rebased onto
`3088938f` (including the recipe and contract-mirror refactors). Desktop has a
separate workspace and lockfile. Package counts below are not build-time benchmarks.

## Production dependencies

| Entry point | Initial unique normal/build packages | After cleanup |
| --- | ---: | ---: |
| Server (`webcodex`) | 300 | 295 |
| Runner (`webcodex-runner`) | 223 | 220 |

Count package identities once, including workspace packages and distinct external
versions, not repeated `(*)` entries. Do not count everything in Cargo.lock as an
active production dependency, or use an all-targets build to infer production
features: test features and workspace feature unification can hide mistakes.

Changes:

- Remove the unused Server -> persistent-shell and environment -> admin direct
  edges, not the crates themselves. The Runner still needs persistent shells;
  operator/CLI clients still need admin support.
- Keep Salvo's existing production features explicit and enable `test` only under
  dev-dependencies. This removes Brotli and its three support packages from the
  normal Server graph, while retaining the HTTP test client in tests.
- Make validation's Session evidence projection opt-in through `session-evidence`.
  The Server enables it explicitly. The Runner uses only recipes and adapters and
  no longer compiles tool-contracts or workflow-session through validation.
  Validation's own tests retain their explicit development dependencies.
- State the `reqwest/system-proxy` requirement explicitly in environment and
  Runner. Previously the unused admin edge enabled it transitively. Removing an
  unused crate must not silently remove proxy discovery from live HTTP clients.
- Align the Runner's direct webpki-roots dependency with the already locked 1.x
  package. This does not eliminate 0.26 from every transitive graph.
- Refresh both lockfiles without a general dependency upgrade.

Keep the existing process, transport, configuration, persistence and validation
boundaries. Crate count alone does not demonstrate overdesign. Do not merge domain
crates solely to reduce the number of Cargo.toml files. Both production graphs
still contain ring and aws-lc; provider consolidation needs its own TLS/platform
compatibility review, not a blind manifest deletion. The default graphs exclude V8.

## Drift and lint policy

The local `coding_agent_state!` table owns the enum variants, serde wire spellings,
`as_str`, and strict `from_wire` parsing for two CodingAgent state types. Semantic
state-transition logic remains ordinary Rust. Independent expected-value and
round-trip tests protect the existing wire and persisted vocabulary. There is no
new procedural-macro crate, global code-generation framework, or protocol change.

Schema tests reuse their parent module's `assert_schema_fields!` instead of keeping
an identical child-module copy. Reuse an existing definition before introducing
another abstraction. A macro is useful for repeated mechanical projections of one
closed table, not for hiding authorization, lifecycle, or business decisions.

The initial default workspace/all-targets Clippy baseline contained 536 unique
warning locations/messages, including 10 dead-code diagnostics. Mechanical fixes
are reviewed rather than trusted blindly. In particular, Linux's unsupported
Computer image stub uses `()`: accepting `let_unit_value` fixes there would discard
real image values on macOS/Windows. Preserve the image bindings and scope the two
lint exemptions to unsupported platforms only. Platform/test-only helpers use cfg
instead of global dead-code allowances.

Do not globally allow Clippy or impose `-D warnings` on an existing warning backlog.
Large error representations, large enums, excessive function parameters and complex
types require focused API/ownership work with their own regression coverage; moving
all arguments into an unstructured context or boxing every value just to satisfy a
threshold can increase complexity. Keep authority checks, bounded execution,
idempotency, replay and process ownership intact.

## Recorded validation

After the final proxy-feature declarations, unfiltered default workspace/all-targets
Clippy succeeds with **381 unique warnings and zero rustc unused/dead-code warnings**.
The starting baseline was 536 warnings / 10 dead-code diagnostics; the comparison
also includes the two upstream refactors pulled in by the rebase, so the entire
reduction must not be attributed solely to this branch. This is not a clean
`clippy -- -D warnings` result. Remaining categories include 149 too-many-arguments,
65 result-large-err, 16 large-enum-variant, and 12 type-complexity warnings.

- `cargo test --locked --workspace`: **6230 passed, 0 failed, 29 ignored**, including
  the default test/doc-test targets. The Server accounts for 3040 passing tests;
  the Runner accounts for 945.
- `cargo test --locked -p webcodex-validation --lib`, without session-evidence:
  **101 passed, 0 failed, 0 ignored**.
- Isolated Server, Runner, environment, validation-default and validation-evidence
  production checks: pass. The final isolated graphs retain `system-proxy` and the
  reduced package counts above.
- `webcodex-computer --lib` cross-checks for `x86_64-pc-windows-msvc` and
  `aarch64-apple-darwin`: pass. These are type/build checks, not native GUI tests or
  full Runner/Desktop builds.
- Cargo machete, formatting, workspace boundary validation and its **16 self-tests**,
  Desktop locked dependency resolution, and Git whitespace checks: pass.

Only existing locked packages were used; checks ran with `--locked --offline`
where applicable. Detailed local logs and deduplicated JSON summaries are under
`target/maintenance-audit/`, deliberately excluded from version control. The final
local commit range is reported with the task closeout, rather than embedding a
self-referential commit hash in this document.

## Reproduction

```sh
cargo machete
bash scripts/workspace_boundary_check.sh --self-test
bash scripts/workspace_boundary_check.sh
cargo check --locked -p webcodex-validation
cargo check --locked -p webcodex-validation --features session-evidence
cargo check --locked -p webcodex-runner --bins
cargo check --locked -p webcodex --lib --bins
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets
cargo test --locked --workspace
cargo tree --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --depth 0
```

Inspect each entry point separately with `cargo tree -p PACKAGE -e normal,build`
and inspect `reqwest` features as well as package names. Default Runner production
must omit admin, tool-contracts and workflow-session, but retain `system-proxy`.
Default Server production must omit persistent-shell and Brotli. Salvo test support
still belongs in test graphs. Native Computer cross-checks need their target
standard libraries and, where applicable, platform SDK/build prerequisites.

Ignored real-process tests, V8-enabled experimental Code Mode and full Desktop
builds are distinct validation scopes; a default Linux workspace pass does not
prove those configurations.
