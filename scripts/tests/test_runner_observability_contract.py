"""Exact-name regression guard for Runner projections, not the Agent domain."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
RUST_PROJECTIONS = (
    "src/tool_runtime/runtime_info.rs",
    "src/tool_runtime/projects.rs",
    "src/tool_runtime/coding_task.rs",
    "src/admin_http.rs",
    "src/runtime_console_http.rs",
    "src/pairing_http/runner_capabilities.rs",
    "crates/webcodex-tool-contracts/src/registry/output_schemas/discovery.rs",
    "crates/webcodex-cli/src/webcodex_cli/ops.rs",
    "crates/webcodex-cli/src/webcodex_cli/server.rs",
    "crates/webcodex-cli/src/webcodex_cli/connect/probe.rs",
    "crates/webcodex-cli/src/webcodex_cli/runner_service.rs",
)
SCRIPTS = (
    "e2e_job_reconciliation_ws.sh", "e2e_job_recovery_failures_ws.sh",
    "e2e_linux_socket_activation.py", "e2e_reconnect_ws.sh",
    "e2e_shared_key_ws.sh", "e2e_zero_config_ws.sh", "eval_coding_loop.sh",
    "test-claude-provider-e2e.sh", "test-runner-config-reload-e2e.sh",
)
# Exact string tokens catch output keys, reads and serde renames. Test modules
# may intentionally assert absence. Wire/build-info DTOs and durable Agent
# modules are outside this observation surface, not blanket legacy aliases.
LEGACY = re.compile(r'"(?:agents|agent_instance_id|agent_protocol_generation|agents_total|agents_online|(?:source_)?mismatched_agents_count)"|"/agents(?:/|\")')
SCRIPT_READ = re.compile(r'output\.agents|\["output"\]\["agents"\]|\.get\("agents"')


def projection_violations(source):
    return LEGACY.findall(source)


def read_source(relative):
    source = (ROOT / relative).read_text(encoding="utf-8")
    if not source.strip():
        raise AssertionError(f"empty contract scope: {relative}")
    return source


class RunnerObservabilityContract(unittest.TestCase):
    def test_production_projections_use_runner_names(self):
        for relative in RUST_PROJECTIONS:
            with self.subTest(path=relative):
                # Only inline test modules are excluded; all production code,
                # including both Console DTOs, remains in the guarded scope.
                source = re.split(r"(?m)^#\[cfg\(test\)\]\s*\nmod \w+\s*\{", read_source(relative), maxsplit=1)[0]
                self.assertTrue(source.strip(), relative)
                self.assertEqual(projection_violations(source), [], relative)

    def test_platform_status_reads_use_runner_names(self):
        for filename in SCRIPTS:
            with self.subTest(path=filename):
                self.assertIsNone(SCRIPT_READ.search(read_source("scripts/" + filename)))
        for filename in ("windows_runner_readiness.ps1", "deploy_windows_runner_dogfood.ps1"):
            source = read_source("scripts/" + filename)
            self.assertNotIn(".agent_instance_id", source)
            self.assertNotIn("AgentInstanceId", source)

    def test_frontend_runner_fields_use_runner_names(self):
        for relative in ("frontend/src/runtime-v2/model/types.ts",
                         "frontend/src/admin-react/AdminApp.tsx", "scripts/ui-smoke/fixtures.mjs"):
            source = read_source(relative)
            for field in ("agent_protocol_generation", "agents_online", "agents_total"):
                self.assertNotIn(field, source, relative)
        # The same fixtures legitimately include communication/agents.
        self.assertIn("communication/agents", read_source("scripts/ui-smoke/fixtures.mjs"))

    def test_guard_detects_keys_reads_and_serialization_aliases(self):
        for source in ('json!({"agents": []})', '.pointer("/agents/clients")',
                       '#[serde(rename = "agent_protocol_generation")]',
                       'output["agent_instance_id"]'):
            self.assertTrue(projection_violations(source), source)
        for source in ('"agent_id"', '"/api/agents/ws"', '"agent:runner:project"',
                       '"coding_agents"', '"no_online_agents"'):
            self.assertEqual(projection_violations(source), [], source)


if __name__ == "__main__":
    unittest.main()
