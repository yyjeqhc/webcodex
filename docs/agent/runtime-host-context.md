# Runtime Host Context — Design Direction

Status: design direction only; not implemented yet.

This note defines a small future extension to Runner registration and
`runtime_status`: bounded host context that helps a model or operator choose the
right execution path for a known machine.

The need is practical rather than a new inventory system. WebCodex already knows
which Runner is online, what it can do, which projects it owns, and its live
build/process state. What it cannot currently express is stable local knowledge
such as:

- one Runner is running on the same host as the WebCodex Server, so Server
  operations should normally use that Runner directly instead of first trying
  SSH back into the same machine;
- one development host normally sends Internet traffic through its host proxy,
  while campus/internal destinations are intentionally direct;
- a host is primarily a high-performance development machine rather than a
  control/server machine;
- service management on a host uses the ordinary host-local service mechanism
  rather than a special WebCodex lifecycle.

These statements are useful planning context, but none of them is execution
truth or authority.

## 1. Product boundary

Host context is **declarative model/operator guidance supplied by the Runner
configuration**. It is not another source of runtime state.

The existing boundaries stay authoritative:

1. live registration, capabilities, `agent_instance_id`, build identity, Job
   state, project registration, and connection observations remain runtime facts;
2. authority mode, OAuth scopes, project policy, `allowed_roots`, session guards,
   and tool-specific safety checks remain authorization/safety facts;
3. host context may influence which already-authorized path a model prefers, but
   can never authorize a path, prove that a service is running, or turn an
   unavailable capability into an available one.

If host context conflicts with a current observed fact, the observed fact wins.
For example, `role = "server_host"` does not make an offline Runner callable,
and a network note saying that Internet traffic normally uses a proxy does not
prove that the proxy is currently reachable.

## 2. Minimal proposed shape

Do not start with arbitrary labels, a generic metadata map, a policy language,
or a fleet model. The first useful shape is a small closed object with bounded
human-authored descriptions:

```json
{
  "host_context": {
    "source": "runner_config",
    "role": "server_host",
    "runtime": "Prefer this Runner for operations on this host instead of SSHing back into the same host.",
    "service": "WebCodex Server lifecycle uses the normal host-local service mechanism.",
    "network": null,
    "architecture": "This host is the WebCodex control/server host."
  }
}
```

Proposed fields:

| Field | Meaning |
|---|---|
| `source` | Fixed provenance marker, initially `runner_config`. |
| `role` | Short stable role slug such as `server_host` or `primary_development`. |
| `runtime` | Preferred way to operate workloads already reachable on this host. |
| `service` | Stable service-management expectation; not current service state. |
| `network` | Stable network-routing expectation; never proxy credentials or live reachability. |
| `architecture` | Stable topology/workload description useful for choosing where work should run. |

All fields except `source` are optional. The object should be omitted when the
Runner has no configured context.

The exact byte limits can be chosen during implementation, but the contract
should be deliberately small: one short role plus short bounded strings, with a
bounded total serialized size. Unknown fields should fail configuration
validation rather than silently becoming an extension mechanism.

## 3. Concrete examples

### `sf`: Server host

```toml
[host_context]
role = "server_host"
runtime = "Prefer this Runner for Server-host operations instead of SSHing back into the same machine."
service = "WebCodex Server lifecycle uses the ordinary host-local service mechanism."
architecture = "This host runs the WebCodex Server/control plane."
```

The planning consequence is intentionally narrow: when the model needs to
inspect or operate the Server host and `sf` is online/capable, it should prefer
`agent:sf:...` execution over first constructing an SSH route to `sf`.

This annotation does **not** mean:

- every Server operation is allowed;
- the Server service is currently healthy;
- SSH is forbidden;
- the Runner may escape its registered project/policy boundary.

### `special`: Primary development host

```toml
[host_context]
role = "primary_development"
runtime = "Prefer this host for ordinary Linux development, builds, tests, and CLI work when it is available."
network = "Internet egress normally uses the host proxy; campus/internal NEU destinations are intended to bypass that proxy and connect directly."
architecture = "High-performance Linux development environment used as the primary coding host."
```

The network field describes routing intent only. It must not contain proxy URLs,
credentials, tokens, private keys, raw environment variables, or assumed current
reachability. Whether a particular destination is reachable is still a live
observation problem.

## 4. Stable context versus dynamic facts

The easiest way to keep this surface useful is to be strict about what belongs
here.

Good host context:

- stable machine role or workload purpose;
- preference to use the local Runner for work on the same host;
- ordinary service-management convention;
- stable network-routing intent at the level needed to choose a path;
- stable architecture/topology explanation that changes rarely.

Do **not** put these in host context:

- IP addresses, SSH aliases, relay state, ports, or current network paths;
- PID, `agent_instance_id`, process start time, current service status, or health;
- current Git branch/HEAD, binary SHA, build commit, or deployed version;
- active Jobs, current capacity, project availability, or current capability
  advertisement;
- credentials, proxy URLs containing credentials, environment dumps, private
  paths that are not already part of an authorized runtime surface;
- instructions that attempt to override authorization, safety, Session guards,
  or tool semantics.

Those values either already have authoritative runtime projections or must be
queried at the time they matter.

## 5. Placement in `runtime_status`

The natural first projection is per Runner alongside the existing identity,
capability, policy, build, and liveness facts:

```text
runtime_status
  agents.clients[]
    client_id
    agent_instance_id
    status / connected / capabilities / ...
    host_context?       <- declarative planning context
```

`host_context` should also be available wherever the full Runner view is
already intentionally exposed. It does not need a new top-level resource, new
session identity, or durable Server table.

A Runner can send the validated context as part of registration. The Server can
retain it with the current Runner registration and discard it when that live
registration is replaced. The source of truth remains the Runner's local
configuration; Server restart or Runner reconnect simply republishes it.

This keeps two classes visibly separate:

```text
observed runtime facts  -> current truth, freshness/lifecycle semantics apply
host_context            -> configured preference, source=runner_config
```

Do not place configured context inside `connection_layers`: those layers are an
observation contract and must continue to represent only facts that were
actually observed.

## 6. Model behavior and precedence

A consumer should interpret host context with this precedence:

```text
hard safety / authorization
    > observed feasibility constraints (online/capability/ownership)
    > explicit task routing preference
    > host_context planning preference
    > generic default heuristic
```

The important rule is that host context only breaks ties between otherwise valid
execution choices. Observations constrain what is feasible; they are not a
competing preference. Host context must never suppress a user instruction or a
current failure signal.

Examples:

- User asks to inspect the live Server and `sf` is online: prefer the `sf`
  Runner instead of looking for an SSH route first.
- `sf` is offline: the `server_host` annotation does not prevent another
  explicitly valid path from being used.
- Work needs Internet access on `special`: preserve the host's configured proxy
  environment rather than inventing a different route; an internal/campus
  destination may still need an explicit direct-path execution decision.
- User explicitly asks to use another host: the user instruction wins.

## 7. Security and privacy requirements

The first implementation should enforce the same style of boundedness as other
model-facing runtime metadata:

- strict schema and small total payload;
- no secret-bearing values or arbitrary environment projection;
- no automatic interpolation or execution of annotation text;
- no host paths, tokens, headers, credential material, or command output added by
  the Server;
- no annotation-controlled authorization or tool exposure;
- no persistence requirement beyond the live Runner registration unless a later
  concrete consumer needs durability.

Because the values are human-authored guidance, they should be returned as data,
not copied into shell commands, environment variables, or policy expressions.

## 8. Non-goals

This direction intentionally does not create:

- fleet deployment or upgrade management;
- a scheduler or automatic workload placement engine;
- a generic service manager or Windows SCM abstraction;
- SSH discovery/routing state;
- a host health system;
- a generic metadata/plugin framework;
- a new authority/policy DSL;
- a replacement for `runtime_status.connection_layers`, capabilities, project
  registration, or build/version diagnostics.

If a repeated concrete need later requires a machine-readable field (for example
an exact local-vs-remote execution preference), promote that one concept into a
closed typed field. Do not preemptively turn every prose hint into enums.

## 9. Suggested implementation slice

A later implementation can remain small:

1. add an optional bounded `host_context` object to Runner configuration;
2. validate it locally and include it in Runner registration;
3. retain it in the current Server-side Runner view;
4. project it in full `runtime_status` / `list_agents` Runner entries;
5. add focused registration, schema, redaction/bounds, reconnect, and projection
   tests;
6. configure the first real dogfood examples on `sf` and `special` only after the
   product change is reviewed.

No model-routing code is required in the first slice. Exposing accurate context
first lets dogfood show whether models consume it naturally before adding more
mechanism.
