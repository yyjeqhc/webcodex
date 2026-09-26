---
name: webpi-pi-fusion
description: Decide how a capability should be implemented in the WebPi + Pi fused architecture. Use when evaluating new tools, skills, prompts, Pi packages, extensions, MCP servers, subagents, plan/goal/memory/browser/LSP integrations, or when deciding whether to build, install, adapt, or reject an ecosystem capability.
---
# WebPi + Pi Fusion Router

Use this skill before adding a new capability to WebPi.

## Architectural rule

WebPi is the authoritative execution and security runtime. Pi is the progressive-disclosure customization layer.

Prefer these layers in order:

1. **Existing WebPi native capability** — use it when the need is already covered by guarded file/Git/process/Job/LSP/Browser/Computer/artifact/Workflow Session/Plugin primitives.
2. **WebPi/Pi prompt** — use for a repeatable task framing that does not need new tools or executable code.
3. **Pi skill** — use for specialized workflow/domain knowledge loaded on demand; prefer this over an extension when instructions + existing WebPi tools are enough.
4. **Pi extension** — use only when a missing primitive needs executable TypeScript, lifecycle hooks, custom tool/command registration, or Pi-native resource behavior.
5. **MCP bridge** — use only when a valuable external service is available primarily as MCP and cannot be represented safely as a narrower WebPi Plugin/native tool.
6. **Second model/subagent loop** — avoid by default. WebPi's primary reasoning agent is ChatGPT; Pi provider/model/session-tree features are not the authority for the web agent.

## Duplicate-capability check

Before recommending an install, compare the candidate against WebPi:

- Git/checkpoints/commits -> WebPi Git tools.
- Shell/process/background work -> WebPi structured process + durable Jobs.
- LSP/navigation/diagnostics -> WebPi LSP/code-intelligence tools.
- Plan mode -> WebPi read-only research/plan prompts + authority separation.
- Goals/autonomous continuation -> WebPi Goals/AgentTasks/Workflow Sessions/Jobs.
- Permission prompts/path guards -> WebPi scopes, project authority, sensitive-path policy, exact edit fences.
- Browser automation -> WebPi Browser tools unless a missing browser primitive is proven.
- Desktop screenshots -> WebPi Computer + artifact delivery.
- MCP client -> only when an external MCP server is uniquely valuable; otherwise prefer direct WebPi Plugin/native integration.
- Persistent memory -> prefer project docs/Workflow Session evidence unless the user explicitly wants another memory store and its privacy model is understood.
- Provider/model switching/usage -> usually not applicable because the primary model runs in ChatGPT, not Pi ModelRuntime.

If the candidate duplicates a stronger WebPi primitive, recommend **do not install** and adapt its workflow ideas into a prompt/skill instead.

## Third-party candidate review

For any Pi package/extension/MCP candidate, collect before installation:

- exact package/repository/version/ref;
- maintenance freshness and usage/adoption signal;
- license;
- package type: skill / prompt / extension / theme / MCP bridge;
- dependencies and lifecycle/install scripts;
- network destinations and credential needs;
- filesystem/process/subprocess behavior;
- tool/command names and collision risk;
- whether it starts a second model/subagent loop;
- whether it duplicates WebPi authority;
- rollback/remove path.

Prefer pinned versions/refs and project-local scope for evaluation.

## WebPi controlled install flow

Never install or approve merely because discovery found a package.

```text
research candidate
→ inspect package source/dependencies/lifecycle scripts
→ explain value, duplication, and risk
→ obtain explicit user authority for package mutation
→ install project-local/pinned where practical
→ reload Pi resources
→ inspect candidate status and exact SHA-256
→ obtain explicit approval for executable extension fingerprint
→ reload
→ list/describe exact tools or commands
→ run the smallest smoke test
→ validate and document rollback
```

A package mutation and an extension approval are separate consequential decisions.

## Recommendation classes

Use these labels in ecosystem reviews:

- **ADOPT AS NATIVE/PROMPT** — valuable idea, but WebPi already owns the primitive.
- **SKILL CANDIDATE** — low-runtime-risk workflow/domain knowledge; inspect content before adding.
- **EXTENSION CANDIDATE** — fills a genuine missing primitive; needs source review and fingerprint approval.
- **MCP CANDIDATE** — external service is valuable and MCP is its best interface; review transport/credentials/tool authority.
- **NOT APPLICABLE** — depends on Pi owning the LLM/provider/TUI/session tree, which WebPi intentionally does not expose as the primary web-agent loop.
- **REJECT / DUPLICATE** — conflicts with or weakens a WebPi authority boundary.

## Output format

For each evaluated capability report:

- Need / user benefit
- Best architectural layer
- Existing WebPi overlap
- Candidate(s) and evidence
- Security/privacy/runtime risk
- Recommendation class
- Exact next action
- Whether explicit user approval is required

Do not install, approve, or elevate trust unless the user has explicitly authorized that consequential effect.
