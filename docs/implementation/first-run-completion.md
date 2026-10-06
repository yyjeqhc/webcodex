# First Run completion and Runner labels

Fresh Desktop launch now presents Create/Join before any Environment, Runner or service configuration. Saved environments retain their existing roles and startup/resume behavior. Advanced Quick Share remains explicit. Empty projects do not disable the Runner.

After explicit Environment setup, Create reuses the existing Connection editor for Tunnel configuration or an explicit later action. Join has no central Server Tunnel/bootstrap input. Completion is inferred from existing Environment state rather than stored as a second setup authority.

Runner labels use the existing optional `display_name`, shared 200-character/no-NUL validation and existing identity generation. Legacy requests default to no label; reopening saved configuration does not overwrite a name or identity. The optional field is passed by Desktop and CLI (`--runner-name`) through setup and recovery.

Project-read guidance binds the saved Runner `client_id` and Project `runtime_project_id`. Copying its ChatGPT instruction is explicit. The user's read confirmation is ephemeral and labelled as user-reported; readiness observations and actual ChatGPT execution evidence remain separate.

## Validation, Linux development checkout, 2026-10-06

- Focused frontend behavior/locale tests: 156 passed during implementation; updated identity/localization checks: 32 passed (overlapping).
- Environment Runner-name/default/recovery tests: 3 passed; projectless setup: 3 passed; Runner InitOptions validation: 1 passed.
- Native invitation/setup projection tests: 7 passed after integrating main `545d5caf`.
- Shared path inventory regressions: 18 passed after that integration.
- CLI/registry dogfood checks, Desktop TypeScript/frontend build, CSS contract, Rust formatting and diff checks passed during implementation. Existing large-bundle/dead-code advisories remain.

An upstream fixture compilation failure was corrected by giving the new optional label its legacy `None` value in two added updater tests. No production behavior changed in that correction. Windows/macOS native installation, Tunnel/ChatGPT reads, package replacement, OS logout/reboot and service ownership acceptance were not executed. No existing service was restarted and no installation, deployment or Release was performed.

After main `94c6e24a` integration, the six relevant frontend suites passed
160 tests, followed by typecheck, frontend build and CSS checks. New upstream
update-view fixtures received the same legacy optional-label default.
CLI Environment 12 and Runner-name 3 tests passed after `03c2c313`.
