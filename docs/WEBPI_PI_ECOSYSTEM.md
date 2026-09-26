# WebPi + Pi ecosystem strategy

This document is the integration policy and curated ecosystem watchlist for the WebPi + Pi fused agent architecture.

## Product stance

Pi is intentionally a minimal coding harness that is customized with Extensions, Skills, Prompt Templates, Themes, and Packages. WebPi should preserve that strength while keeping execution authority in WebPi.

The fused architecture is therefore asymmetric:

- **ChatGPT / WebPi Agent** owns primary reasoning and user interaction.
- **WebPi runtime** owns project authority, files, guarded mutation, Git, structured processes, durable Jobs, LSP/code intelligence, Workflow Sessions, Browser/Computer authority, artifacts, credentials/scopes, and audit evidence.
- **Pi ResourceLoader / pi-bridge** owns progressive-disclosure skills, prompts, project context, package resources, and reviewed extension tools/commands.
- **Pi ModelRuntime/provider/session tree/TUI** is not a second authoritative agent loop in WebPi.

This avoids duplicate state machines and conflicting safety systems.

## Selection rule

Before adding anything from the ecosystem, ask:

1. Does WebPi already have a stronger native primitive?
2. Can a prompt or skill express the workflow using existing primitives?
3. Does the capability require executable Pi hooks/tools, or only instructions?
4. Does it need a third-party service that is best reached by MCP?
5. Does it start a second model/session/autonomy loop that conflicts with WebPi's primary-agent design?
6. What credentials, network access, subprocesses, lifecycle scripts, and persistent state are introduced?

Use `.pi/skills/webpi-pi-fusion/SKILL.md` for the full routing checklist.

## Official Pi patterns to adopt

These are concepts from Pi's official customization model that fit WebPi well:

### Skills for progressive disclosure

Skills expose only name/description in normal context and load full instructions on demand. This is the preferred WebPi mechanism for domain workflows that do not need new executable primitives.

Good WebPi uses:

- security review playbooks;
- architecture/decision frameworks;
- incident response;
- release gates;
- domain-specific testing/document workflows;
- ecosystem/package review procedures.

### Prompt templates for explicit task modes

Prompts are ideal for reusable user-invoked modes such as research, feature, bugfix, review, release, and incident work. The `webpi-*` prompts under `.pi/prompts/` implement this pattern.

### Extensions only for missing primitives

Pi extensions can register tools/commands and lifecycle hooks and run with the user's host permissions. WebPi should use them only where a real primitive is missing, with exact fingerprint approval and WebPi project trust.

### Project-local, pinned evaluation

When evaluating a package, prefer project-local scope and pinned npm/git versions/refs where practical. Review source and install/lifecycle behavior before mutation.

## Curated ecosystem watchlist

The package catalog is large and changes quickly. Popularity is only a discovery signal, not a trust signal.

### 1. MCP clients: useful, but not a default

**Candidates:**

- `pi-mcp-adapter` — very high adoption in the Pi catalog; broad MCP client bridge.
- `pi-mcp-extension` — smaller alternative with multi-transport MCP, tool refresh, cancellation, and reconnection.
- `pi-agent-plugins` — portable Agent Plugins client that can load Skills and MCP definitions; depends on an MCP adapter for MCP functionality.

**WebPi assessment:** **MCP CANDIDATE, only for uniquely valuable external services.**

WebPi already exposes native tools/plugins and itself has MCP-facing surfaces. Installing a generic Pi MCP client by default would create a second external-tool authority path with its own subprocess/network/credential semantics. Use only when a required service is substantially better/only available through MCP, and scope/configure individual servers narrowly.

### 2. LSP extensions: useful idea, currently duplicate

**Candidates:** `@narumitw/pi-lsp`, `pi-lsp`.

**WebPi assessment:** **REJECT / DUPLICATE by default.**

WebPi already owns LSP status, diagnostics, symbols, definition, references, and workspace navigation through guarded Runner-native tools. A second LSP process manager would duplicate lifecycle/config/state and create inconsistent diagnostics. Borrow UX ideas, not the runtime.

### 3. Plan mode: adopt the workflow, not the extension

**Candidates:** `@narumitw/pi-plan-mode`, community `plan-mode` packages.

**WebPi assessment:** **ADOPT AS PROMPT/SKILL.**

Read-only planning is valuable, but Pi plan-mode extensions are designed around Pi's interactive/TUI loop. WebPi already separates read-only tools from mutation authority and now ships `webpi-research`, `webpi-review`, and the prompt handbook. Keep the workflow in prompts/skills rather than install another mode switch.

### 4. Goal/autonomous continuation: native WebPi overlap

**Candidates:** `@narumitw/pi-goal`, `pi-goal`, verifier-oriented goal packages.

**WebPi assessment:** **ADOPT IDEAS AS NATIVE; do not install by default.**

WebPi already has Goals, AgentTasks, Workflow Sessions, durable Jobs, terminal waits, and handoff evidence. Pi goal packages generally assume Pi owns the model loop and idle continuation. Useful ideas include explicit GoalSpec, budget bounds, independent verification, and pause/resume semantics; implement those in WebPi's authoritative Goal/Task layer.

### 5. Subagents / dynamic workflows: usually not applicable

**Candidates:** `@tintinweb/pi-subagents`, `@quintinshaw/pi-dynamic-workflows`, `pi-subagents`, `pi-agent-suite`.

**WebPi assessment:** **NOT APPLICABLE by default / high architectural overlap.**

These packages create Pi-owned model/subagent loops. WebPi's primary model remains ChatGPT and already has AgentTask/Goal/Workflow Session primitives. Adding a parallel Pi model hierarchy would fragment cost, identity, context, and authority. Revisit only if WebPi intentionally adds a bounded, explicit multi-model executor with first-class accounting and authorization.

### 6. Side conversations (`/btw`): TUI-specific

**Candidates:** `@narumitw/pi-btw`, `pi-btw`.

**WebPi assessment:** **NOT APPLICABLE in the web-primary runtime.**

The value proposition—side questions without polluting the main context—is good, but the implementations use Pi sub-sessions/TUI/model selection. WebPi should solve this with explicit research/side-task Workflow Sessions or future scoped assistant threads, not a second Pi model loop.

### 7. Permission systems / protected paths: duplicate and risky

**Candidates:** `@gotgenes/pi-permission-system`, official Pi examples `permission-gate`, `protected-paths`, `dirty-repo-guard`, sandbox examples.

**WebPi assessment:** **ADOPT CONCEPTS AS NATIVE; reject a second authority layer.**

WebPi already enforces OAuth scopes, project roots, sensitive paths, exact edit fences, stale identity, dirty-worktree handling, Plugin/Pi fingerprint approval, and consequential-effect confirmation. A Pi permission extension can only cover Pi-owned tools and risks giving users a false impression that it controls canonical WebPi tools. Official Pi safety examples remain excellent design references.

### 8. Browser automation: usually duplicate

**Candidates:** `pi-browser-actions`, `pi-browser-cdp-extension`, `pi-browser-debug`.

**WebPi assessment:** **REJECT / DUPLICATE by default.**

WebPi already separates Browser observation from launch/control and has Computer screenshots/artifact delivery. A second Playwright/CDP path would add browser lifecycle/network/credential semantics outside WebPi scope enforcement. Consider only if a specific missing browser primitive is proven and can be wrapped behind WebPi authority.

### 9. Web access/research packages: usually redundant for ChatGPT Web

**Candidate:** `pi-web-access` and search/fetch skills.

**WebPi assessment:** **NOT NEEDED for the primary ChatGPT agent.**

The web agent already has current web search/fetch capabilities. Installing another extension would add API keys and external providers without improving the main reasoning loop. A skill that teaches research discipline is preferable.

### 10. Code simplification/review skills: high-value skill candidates

**Candidates:** `pi-simplify`, `@ferologics/pi-skills` (`code-review`, `code-simplifier`, `context-packer`), `@stayradiated/pi-skills` (`hone`, `pair-review`).

**WebPi assessment:** **SKILL CANDIDATE / ADOPT WORKFLOW.**

These are close to the ideal Pi role: specialized on-demand review procedures using existing tools. Prefer source-reviewed skill-only packages or adapt the best procedures into WebPi-local skills to avoid adding executable extension authority.

### 11. Engineering skill collections: promising, curate narrowly

**Candidates:** `@artale/pi-skills`, `tsubus-agent-skills`, focused Agent Skills collections.

**WebPi assessment:** **SKILL CANDIDATE.**

Potentially useful for language/domain expertise (security, Kubernetes, databases, Go, Python, observability), but large catalogs can pollute discovery/context and contain uneven instructions. Prefer selected, reviewed skills rather than installing an entire methodology package globally.

### 12. Skill catalog/context reducers: interesting at scale

**Candidates:** `pi-skill-shiori`, skill visibility managers.

**WebPi assessment:** **EXTENSION CANDIDATE only when the skill catalog becomes large.**

Pi already uses progressive disclosure, but hundreds of skill descriptions can still consume context. A reviewed on-demand retrieval layer may become valuable once WebPi ships/loads a much larger skill catalog. Not needed for the current small inventory.

### 13. Persistent memory packages: opt-in only

**Candidates:** `pi-hermes-memory`, `pi-memory`, Mem0 integrations, `open-zk-kb`.

**WebPi assessment:** **HIGH-RISK / OPT-IN.**

Persistent memory changes privacy, retention, prompt injection, and deletion semantics and may duplicate ChatGPT memory plus WebPi Workflow Session evidence. Do not enable globally. If requested, prefer project-owned Markdown/SQLite with explicit retention and secret scanning over opaque hosted memory.

### 14. Observability/telemetry: useful only with explicit data policy

**Candidates:** `@langchain/langsmith-pi-extension`, `@raindrop-ai/pi-agent`.

**WebPi assessment:** **OPT-IN EXTERNAL SERVICE.**

Tracing can improve debugging and evals, but source/tool arguments/results may contain proprietary code or secrets. Require a clear data destination/retention/redaction policy and explicit user/organization approval.

### 15. Provider/model/usage extensions: mostly not applicable

**Candidates:** `@narumitw/pi-usage`, provider routers, LiteLLM/provider extensions.

**WebPi assessment:** **NOT APPLICABLE to the primary WebPi agent.**

The primary model is hosted by ChatGPT, not Pi ModelRuntime. Pi provider events, model cycling, quota widgets, and OAuth do not govern the web model. They remain useful only for an explicitly launched standalone Pi TUI session.

### 16. Human plan/review UI: idea worth borrowing

**Candidate:** `@plannotator/pi-extension`.

**WebPi assessment:** **ADOPT UX IDEAS, not the TUI extension.**

Interactive plan/code annotation is valuable; the current package is Pi-TUI-centric. A future WebPi web UI could surface structured plan review/approval through Workflow Session artifacts instead.

### 17. Extension testing: useful for WebPi's Pi bridge

**Candidate:** `pi-coding-agent-test` and Pi's own extension examples/tests.

**WebPi assessment:** **SKILL/DEV-TOOL CANDIDATE.**

Real Pi extension E2E testing is highly relevant to `pi-bridge`, especially after Pi version upgrades. Prefer project-local pinned evaluation. This is a developer-only capability, not a default end-user package.

## Package preflight in WebPi

Before any npm-backed Pi package mutation, prefer `pi_package_inspect` with the exact `npm:` source. It reads only the npm registry manifest and reports the resolved version, license/repository metadata, dependency sets, Pi manifest metadata, and lifecycle scripts such as `preinstall`, `install`, `postinstall`, and `prepare`.

The preflight intentionally does **not** download the tarball, execute scripts, write Pi settings, or claim source review. `sourceReviewComplete=false` is part of the result contract. A package that passes metadata preflight still requires source/dependency review before installation, and any executable Pi extension still requires separate exact fingerprint approval after resolution.

## Verified metadata snapshot — 2026-09-21

The following values were read with WebPi's read-only `pi_package_inspect` implementation against the npm registry. They are a dated observation, not a permanent trust decision.

| Candidate | Resolved version | Runtime deps | Lifecycle metadata | Pi manifest | WebPi decision |
| --- | ---: | ---: | --- | --- | --- |
| `pi-mcp-adapter` | `2.35.0` | 15 | `prepack` present; no install/postinstall in the resolved manifest | executable extension `./index.ts` | keep as an opt-in MCP candidate; do not default-enable |
| `pi-simplify` | `0.2.3` | 0 | `prepack` present | executable extension `dist/index.js` | absorb the workflow into the local `webpi-code-simplifier` skill rather than add executable authority |
| `pi-skill-shiori` | `0.6.10` | 1 (`yaml`) | no lifecycle script in the resolved manifest | executable extension `./src/index.ts` | revisit only when WebPi's skill catalog is large enough to justify retrieval/indexing overhead |

All three still require source/dependency review before installation; package metadata alone is not sufficient. `pi-mcp-adapter` in particular introduces a second external-tool transport with its own MCP server subprocess/network/credential lifecycle, so adoption must be justified by a concrete service that WebPi cannot expose more narrowly.

## Recommended WebPi default bundle

Keep the default installation intentionally small:

- first-party `pi-bridge`;
- WebPi `webpi-*` prompts;
- reviewed project-local WebPi skills (`webpi-pi-fusion`, emergency/fallback, architecture/build/debug/review workflows);
- no third-party executable Pi package enabled by default;
- no generic MCP client enabled by default;
- no second Pi model/subagent loop;
- no third-party persistent memory or telemetry by default.

This gives high capability with low ambient authority and low context overhead.

## Candidate evaluation order

If expanding the default ecosystem later, investigate in this order:

1. **Skill-only review/simplification/domain packs** — highest upside with lowest runtime authority.
2. **Pi extension E2E testing/dev tooling** — improves bridge reliability without changing end-user authority.
3. **Skill catalog retrieval** — only when WebPi's skill list becomes large enough to justify it.
4. **One narrowly scoped MCP service** — only for a concrete external integration.
5. **A genuinely missing native primitive extension** — source-reviewed and fingerprint-approved.

Do not select packages by download count alone.

## Installation/approval boundary

Third-party package installation is consequential: Pi packages may execute lifecycle scripts and extensions execute with host permissions. WebPi must preserve two separate decisions:

1. **Package mutation approval** — permission to download/install/update/remove the package.
2. **Executable extension fingerprint approval** — permission to import/run a specific reviewed extension SHA-256.

Discovery or a high package-catalog ranking grants neither.
