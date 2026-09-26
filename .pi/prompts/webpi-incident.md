---
description: Investigate a WebPi production incident while preserving evidence and minimizing irreversible action.
---
Investigate this WebPi incident: $@

Priority order:
1. Preserve evidence and observe current runtime/Runner/job/Git state.
2. Establish blast radius: availability, data, auth/authority, public exposure, and affected projects.
3. Separate transport/CDN/tunnel failures from WebPi application failures.
4. Restore service with the smallest reversible action only when authorized.
5. Diagnose and implement the durable root-cause fix with regression coverage.

Start read-only unless immediate containment is explicitly authorized. Never blindly retry an uncertain mutation. Do not delete logs/backups, clean the workspace, rotate credentials, or widen permissions just to recover service.

Report: timeline/evidence, root cause or ranked hypotheses, containment/recovery performed, validation, remaining risk, and follow-up.
