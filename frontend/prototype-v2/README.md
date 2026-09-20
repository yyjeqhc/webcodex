# Runtime WebUI v2 prototype

Exploratory React prototype for a task-oriented WebCodex Runtime UI.

## What this prototype tests

The current Runtime UI exposes several backend domains as peer navigation destinations. This prototype instead organizes the product around three user questions:

- **Work** — what is happening, what needs attention, and what should I open next?
- **Projects** — where is the code and which work belongs to it?
- **Runtime** — what infrastructure or diagnostic evidence explains the current state?

The default Work experience is a three-column workspace:

1. a compact work inventory grouped by running / attention / active / recent;
2. a Codex-like task execution surface showing the current plan, running command/output, and grouped progress instead of a flat tool-call feed;
3. a Context-first inspector, with raw IDs, Window Activity and Job details moved into an Evidence disclosure.

The prototype also preserves both sides of the Project/Session/Window relationship:

- **Project → active Sessions** shows concurrent work in one repository and how many Windows currently observe each Session;
- **Runtime → Window activity** provides a dedicated Window-level workbench showing the Sessions and Projects correlated with a selected Window;
- the UI treats Window ↔ Session correlation as many-to-many evidence rather than ownership.

This is intentionally mock-data-only. It does **not** replace the production Runtime WebUI, call Runtime APIs, change backend routes, or alter embedded console assets.

## Run

```bash
cd frontend/prototype-v2
npm install
npm run dev
```

Vite listens on `127.0.0.1:4178`.

Validation:

```bash
npm run typecheck
npm run build
```

## Migration boundary

If this information architecture proves useful, production migration should be incremental:

1. keep the existing Runtime HTTP API contracts;
2. introduce a framework-built Runtime asset bundle alongside the current implementation;
3. migrate Work / Session timeline first;
4. migrate Projects next;
5. move Window Activity, Runner fleet, Jobs and Agent diagnostics into Runtime;
6. remove the old imperative Runtime implementation once feature and authority parity is proven.

The existing Admin console is intentionally outside this prototype.
