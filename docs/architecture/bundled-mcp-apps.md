# Bundled MCP App registry

`src/mcp/app_registry.rs` composes immutable, bundled App templates. The inventory
in `app_registry/builtin.rs` owns each exact resource URI, HTML, optional public
listing, tool bindings, and resource/descriptor presentation metadata together.
It does not register a tool or provide an external UI plugin API.

## Preserved surfaces

The six listed resources keep their existing order, names, descriptions, MIME
and current URIs: Computer, Projects & Resources, Work Result, Goal Plan, Agent
Continuation, and Job Continuation. The Changes v4 template remains readable by
its exact URI for cached descriptors, but is neither listed nor bound to any new
tool descriptor. No retired URI alias is accepted.

Template logic remains owned by each App. Existing folding, frozen-diff paging,
activity detail loading, draft retention, origin checks, message sequencing,
observation deadlines and disposal stay in their current templates. Consequently
the original registry extraction needed no App URI bump; template or incompatible
App-tool changes must advance the affected URI. The later Workbench/Work Result
display changes and identity advances are documented in
[workspace presentation](../implementation/mcp-workspace-experience.md).

The registry preserves three distinct projections rather than merging metadata:

- `resources/list` carries common closed CSP, optional configured public domain,
  and the existing public listing, not resource-read-only display modes.
- `resources/read` carries the exact bundled template and common metadata.
  Workbench prefers fullscreen and also supports inline; Work Result supports
  inline/fullscreen. Both consume Host theme and display-context notifications
  and report content height for inline embedding.
  Only Computer overrides the enclosing stateless cache TTL with the existing
  zero-millisecond diagnostic policy.
- An already-admitted tool descriptor gets its App resource binding. Workbench
  additionally owns its title and single global entrypoint. The separate Work
  Result launcher owns the thread entrypoint. Existing unrelated
  descriptor metadata survives attachment.

Computer remains an independently readable resource, not an automatic binding
for ordinary computer tools. Likewise, normal execution, validation, read/edit,
review, observation, Plugin and closeout tools do not acquire cards merely by
appearing in this registry.

## Authority and execution boundaries

MCP discovery still decides visibility, OAuth scopes, supported protocol and UI
capability before binding a template. `for_tool` is not admission. In particular,
retained Agent continuation bindings do not make the hidden model descriptor
visible. App-only bridge tools remain private data helpers without rendering
resources, and the user-opened Work Result thread entry keeps its existing
separate Host/capability/Window authorization path.

Resource requests still go through the original MCP scope checks and Server App
switch. The registry adds no runtime-read bypass. Artifact export, bounded image
snapshots and domain resource references retain their existing authorization,
streaming and lifetime paths independently of Apps being enabled or disabled.
No registration may alter ToolCall, canonical results, permissions, Session/Job
selection, audit, replay or effect certainty.

There is no mutable registry, lock, background worker, provider process, network
lookup, dynamic JavaScript loading or reload/dispose protocol. Rust static data
owns bundled definitions; existing App instances continue to own their timers,
listeners and request cleanup. Third-party UI would require a separate isolated
host boundary, not a reuse of native-provider trust.

## Incremental adoption and validation

This PR is independent of Desktop panel composition and the already-merged
Context/Guidance/ResultProjection registries. It changes no wire, database,
settings file or Native Tool Plugin protocol. An installation can adopt any of
these internal composition stages without needing the others first.

Registry tests assert unique identities/bindings, exact retired-URI rejection,
stable listing, unlisted cached Changes behavior, isolated metadata/cache rules,
metadata merge safety and capability/model-visibility preservation. Existing MCP
App integration tests continue to own real descriptor/resource handling and
scope checks. Existing Node DOM/message-order tests continue to exercise the
unchanged templates. A baseline comparison also checks all URI constants,
public descriptions and seven template byte sequences against the parent main.
