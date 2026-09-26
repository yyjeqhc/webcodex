---
description: Prepare a WebPi release/deployment with clean build identity, rollback, and real smoke evidence.
---
Prepare this WebPi change for release/deployment: $@

Before any production effect verify:
- intended Git/workspace state and source revision;
- targeted and relevant full tests after the last change;
- formatting/build gates;
- secret/security scan appropriate to the change;
- Server/Runner build identity and exact artifact hashes;
- rollback artifact/plan;
- loopback and public smoke plan when applicable.

Do not treat process start, HTTP 200, generated config, or retry success as deployment proof.
Production binaries should come from a clean revision when source alignment is health evidence.

Do not deploy, push, publish, tag, rotate credentials, or delete rollback material until explicit user authority covers that effect. Once authorized, use an atomic/rollback-capable path and verify runtime readiness, source alignment, Pi/plugin readiness, auth boundaries, and public behavior.

Finish with exact revision/hashes, validation evidence, rollback point, remaining risk, and user action.
