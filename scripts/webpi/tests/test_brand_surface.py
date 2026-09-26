from __future__ import annotations

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[3]

# These are active human/model-facing product surfaces. Internal compatibility
# identifiers such as crate names, webcodex.db, credential prefixes, legacy
# migration paths, and published SDK package names intentionally live elsewhere
# or do not contain the human-facing token-with-space checked below.
ACTIVE_SURFACES = (
    "src/project_entry.rs",
    "src/project_entry_setup.rs",
    "src/project_entry_share.rs",
    "src/project_entry_cloudflared.rs",
    "src/project_entry_openai_tunnel.rs",
    "src/project_entry_regular_tunnel.rs",
    "src/oauth_http/html.rs",
    "src/oauth_http/metadata.rs",
    "src/oauth_http/managed_authorize.rs",
    "src/oauth_http/shared_key_bridge.rs",
    "src/plugin_gateway.rs",
    "src/ssh_resource_gateway.rs",
    "src/mcp_gateway.rs",
    "src/mcp/response.rs",
    "src/mcp/resources.rs",
    "src/mcp/tools.rs",
    "src/mcp_computer_app.html",
    "src/mcp_goal_plan_app.html",
    "src/mcp_result_app.html",
    "src/mcp_work_result_app.html",
    "src/mcp_job_terminal_continuation_app.html",
    "src/mcp_agent_continuation_app.html",
    "src/tool_runtime/helpers.rs",
    "src/job_terminal_attention.rs",
    "src/auth/middleware.rs",
    "src/lib.rs",
    "src/bin/webcodex-server.rs",
    "crates/webcodex-store/src/server_instance.rs",
    "crates/webcodex-cli/src/webcodex_cli/plugin_init.rs",
    "crates/webcodex-cli/src/webcodex_cli/token_commands.rs",
    "crates/webcodex-cli/src/webcodex_cli/service.rs",
    "crates/webcodex-cli/src/webcodex_cli/connect/process.rs",
    "crates/webcodex-tool-contracts/src/tool_definition/skills.rs",
    "crates/webcodex-tool-contracts/src/tool_definition/plugins.rs",
    "crates/webcodex-tool-contracts/src/tool_call.rs",
    "crates/webcodex-tool-contracts/src/tool_catalog.rs",
    "crates/webcodex-tool-contracts/src/registry/tool_specs/skills.rs",
    "src/tool_runtime/startup_brief.rs",
    "crates/webcodex-tool-contracts/src/tool_definition/jobs.rs",
    "crates/webcodex-tool-contracts/src/registry/output_schemas/coding_tasks.rs",
    "crates/webcodex-tool-contracts/src/registry/output_schemas/jobs.rs",
    "crates/webcodex-tool-contracts/src/registry/output_schemas/goals.rs",
    "crates/webcodex-tool-contracts/src/registry/output_schemas/sessions.rs",
)

# Explicit compatibility/history strings that remain intentionally user-visible.
ALLOWED_LINES = {
    "src/project_entry_setup.rs": ("webcodex/projects", "WebCodex/state/projects"),
}

# Rust doc comments on these tool-contract files are emitted into model-visible
# schemas, so they are product surface even when the source line has no string
# literal. Stable wire/package identifiers are handled separately.
MODEL_SCHEMA_DOC_SURFACES = (
    "crates/webcodex-tool-contracts/src/tool_call.rs",
    "crates/webcodex-tool-contracts/src/tool_catalog.rs",
    "crates/webcodex-tool-contracts/src/registry/tool_specs/skills.rs",
    "src/tool_runtime/startup_brief.rs",
)

# Active prose surfaces must use the WebPi product name throughout. Lowercase
# `webcodex-*` compatibility crate/wire identifiers are intentionally outside
# this check; an unqualified human-facing `WebCodex` token is not.
ACTIVE_PROSE_SURFACES = (
    "docs/ARCHITECTURE.md",
    "docs/INDEX.md",
    "docs/CLI.md",
    "docs/RUNNER.md",
    "docs/DEPLOYMENT.md",
    "docs/AUTH_MODEL.md",
    "docs/MCP.md",
    "docs/GPT_ACTIONS.md",
    "docs/PLUGINS.md",
    "docs/QUICK_START.md",
    "docs/PERSONAL_SETUP.md",
    "docs/TROUBLESHOOTING.md",
    "docs/INDEX.zh-CN.md",
    "docs/CLI.zh-CN.md",
    "docs/RUNNER.zh-CN.md",
    "docs/DEPLOYMENT.zh-CN.md",
    "docs/AUTH_MODEL.zh-CN.md",
    "docs/MCP.zh-CN.md",
    "docs/GPT_ACTIONS.zh-CN.md",
    "docs/PLUGINS.zh-CN.md",
    "docs/QUICK_START.zh-CN.md",
    "docs/PERSONAL_SETUP.zh-CN.md",
    "docs/TROUBLESHOOTING.zh-CN.md",
    "docs/WINDOWS_OPENAI_TUNNEL.md",
    "docs/WINDOWS_OPENAI_TUNNEL.zh-CN.md",
    "docs/PERFORMANCE_OPTIMIZATION_BENCHMARK.md",
    "frontend/DESIGN.md",
)

# These operator-facing references describe the current executable/config
# contract. Historical/retired command mentions may retain the old spelling when
# the line explicitly says so; current commands, binaries, service units, and env
# keys must use WebPi-native names.
OPERATIONAL_IDENTITY_SURFACES = (
    "docs/CLI.md",
    "docs/RUNNER.md",
    "docs/DEPLOYMENT.md",
    "docs/AUTH_MODEL.md",
    "docs/MCP.md",
    "docs/GPT_ACTIONS.md",
    "docs/QUICK_START.md",
    "docs/PERSONAL_SETUP.md",
    "docs/TROUBLESHOOTING.md",
    "docs/CLI.zh-CN.md",
    "docs/RUNNER.zh-CN.md",
    "docs/DEPLOYMENT.zh-CN.md",
    "docs/AUTH_MODEL.zh-CN.md",
    "docs/MCP.zh-CN.md",
    "docs/GPT_ACTIONS.zh-CN.md",
    "docs/QUICK_START.zh-CN.md",
    "docs/PERSONAL_SETUP.zh-CN.md",
    "docs/TROUBLESHOOTING.zh-CN.md",
)


class WebPiBrandSurfaceTests(unittest.TestCase):
    def test_active_e2e_harnesses_use_webpi_runtime_contract(self) -> None:
        active = (
            "scripts/mcp_conformance.sh",
            "scripts/e2e_zero_config_ws.sh",
            "scripts/e2e_shared_key_ws.sh",
            "scripts/e2e_reconnect_ws.sh",
            "scripts/e2e_hosted_connect.sh",
            "scripts/e2e_job_reconciliation_ws.sh",
            "scripts/e2e_job_recovery_failures_ws.sh",
            "scripts/smoke_deployment.sh",
            "scripts/smoke_artifact_transfer.sh",
            "scripts/eval_coding_loop.sh",
            "scripts/test-runner-config-reload-e2e.sh",
            "scripts/test-claude-provider-e2e.sh",
            "scripts/e2e_linux_socket_activation.py",
        )
        violations: list[str] = []
        for relative in active:
            text = (ROOT / relative).read_text(encoding="utf-8")
            for marker in (
                "WEBCODEX_",
                "webcodex-server",
                "--bin webcodex-runner",
                "target/debug/webcodex-runner",
                "target/dogfood/webcodex-runner",
            ):
                if marker in text:
                    violations.append(f"{relative}: {marker}")
        self.assertEqual(violations, [], "\n".join(violations))

    def test_mcp_conformance_harness_uses_webpi_environment_contract(self) -> None:
        script = (ROOT / "scripts/mcp_conformance.sh").read_text(encoding="utf-8")
        self.assertNotIn("WEBCODEX_MCP_CONFORMANCE_", script)
        self.assertIn("WEBPI_MCP_CONFORMANCE_URL_FILE=", script)
        self.assertIn("WEBPI_MCP_CONFORMANCE_STOP_FILE=", script)

    def test_release_checklist_matches_fail_closed_publication_workflows(self) -> None:
        disabled_workflows: list[str] = []
        for relative in (
            ".github/workflows/release-build.yml",
            ".github/workflows/release-image.yml",
        ):
            text = (ROOT / relative).read_text(encoding="utf-8")
            if "if: ${{ false }}" in text:
                disabled_workflows.append(relative)
        if not disabled_workflows:
            self.skipTest("public release workflows are enabled in this checkout")
        checklist = (ROOT / "docs/RELEASE_CHECKLIST.md").read_text(encoding="utf-8")
        self.assertIn("PUBLIC RELEASE IS FAIL-CLOSED", checklist)
        self.assertIn("Do not tag, publish npm, publish a GitHub Release, or publish GHCR", checklist)
        self.assertNotIn("For the normal path, the sequence below may be driven", checklist)

    def test_docs_index_does_not_recommend_unavailable_desktop(self) -> None:
        if (ROOT / "desktop").exists():
            self.skipTest("Desktop implementation is present in this checkout")
        for relative, marker in (
            ("docs/INDEX.md", "recommended Windows/macOS entry"),
            ("docs/INDEX.zh-CN.md", "Windows / macOS 推荐入口"),
        ):
            text = (ROOT / relative).read_text(encoding="utf-8")
            self.assertNotIn(marker, text, relative)

    def test_active_product_strings_use_webpi_identity(self) -> None:
        violations: list[str] = []
        for relative in ACTIVE_SURFACES:
            path = ROOT / relative
            self.assertTrue(path.is_file(), relative)
            allowed = ALLOWED_LINES.get(relative, ())
            for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if not ('"' in line or "'" in line):
                    continue
                if any(marker in line for marker in allowed):
                    continue
                stable_wire_id = "webcodex-runner/1" in line
                if (
                    "WebCodex" in line
                    or "webcodex-server" in line
                    or ("webcodex-runner" in line and not stable_wire_id)
                    or "standalone.py" in line
                ):
                    violations.append(f"{relative}:{line_no}: {line.strip()}")
        self.assertEqual(violations, [], "\n".join(violations))

    def test_model_schema_docs_use_webpi_identity(self) -> None:
        violations: list[str] = []
        for relative in MODEL_SCHEMA_DOC_SURFACES:
            path = ROOT / relative
            self.assertTrue(path.is_file(), relative)
            for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if "WebCodex" in line:
                    violations.append(f"{relative}:{line_no}: {line.strip()}")
        self.assertEqual(violations, [], "\n".join(violations))

    def test_active_prose_uses_webpi_identity(self) -> None:
        violations: list[str] = []
        for relative in ACTIVE_PROSE_SURFACES:
            path = ROOT / relative
            self.assertTrue(path.is_file(), relative)
            for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if relative in {
                    "docs/WINDOWS_OPENAI_TUNNEL.md",
                    "docs/WINDOWS_OPENAI_TUNNEL.zh-CN.md",
                } and line.startswith("## Historical dogfood note"):
                    break
                if "WebCodex" in line:
                    violations.append(f"{relative}:{line_no}: {line.strip()}")
        self.assertEqual(violations, [], "\n".join(violations))

    def test_current_operator_contract_uses_webpi_names(self) -> None:
        violations: list[str] = []
        for relative in OPERATIONAL_IDENTITY_SURFACES:
            path = ROOT / relative
            for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                historical = any(
                    marker in line.lower()
                    for marker in ("legacy", "retired", "historical", "compatibility", "旧", "历史", "兼容")
                )
                if "WEBCODEX_" in line:
                    violations.append(f"{relative}:{line_no}: {line.strip()}")
                    continue
                stable_wire_id = "webcodex-runner/1" in line
                internal_cargo_packages = "cargo build" in line and "-p webcodex" in line
                if not historical and not stable_wire_id and not internal_cargo_packages and (
                    "`webcodex" in line
                    or "webcodex-server" in line
                    or "webcodex-runner" in line
                    or "webcodex.service" in line
                    or "webcodex.socket" in line
                ):
                    violations.append(f"{relative}:{line_no}: {line.strip()}")
        self.assertEqual(violations, [], "\n".join(violations))


if __name__ == "__main__":
    unittest.main()
