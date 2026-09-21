# WebPi upstream policy

WebPi is a separate project derived initially from `yyjeqhc/webcodex`.

Initial upstream baseline:

- repository: `https://github.com/yyjeqhc/webcodex`
- observed branch: `main`
- baseline commit: `3cb489dce8557a94a3e56c51ed71c634e3169939`

## Policy

WebCodex is an upstream/reference implementation, not a runtime dependency.

The canonical Windows WebPi development/deployment root is
`E:\WebPi\webpi-core`, physically outside the WebCodex Desktop tree. The
WebCodex GitHub remote is named `upstream`, is used for fetch/review only, and has
its push URL disabled. A future WebPi-owned repository should be added separately as
`origin`; do not repurpose upstream credentials or push targets.

WebPi may deliberately port selected upstream changes for:

- web GPT transport and OpenAPI integration;
- Project and Workflow Session authority;
- stale-write guards;
- process-tree ownership and durable Jobs;
- Runner recovery;
- bounded tool contracts;
- authentication and secret-handling fixes;
- cross-platform reliability.

WebPi does **not** automatically mirror upstream feature growth. Pi capability discovery, extension adaptation, package selection, tool-surface policy, model-facing Pi workflows, and WebPi-specific UX belong to this repository.

Upstream updates should be reviewed as explicit ports or cherry-picks with focused validation. WebPi deployment state, Runner configuration, databases, credentials, project registry, Plugin registry, and Pi extension registry must remain independent from any WebCodex installation.
