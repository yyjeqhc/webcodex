"""Deterministic accounting fixtures; never launch a model client."""
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from local_agent_metrics import client_metrics, discovery_metrics
from e2e_local_agent_execution import next_calls


class LocalAgentMetricsTests(unittest.TestCase):
    def test_discovery_matches_compact_result_budget(self):
        inventory = {"tools": [{"name": "read_files", "inputSchema": {"type": "object"}, "description": "UTF-8 中文"}]}
        measured = discovery_metrics(inventory)
        self.assertEqual(measured["json_bytes"], len(json.dumps(inventory, separators=(",", ":")).encode()))
        self.assertEqual(measured["json_bytes"], measured["tool_bytes"]["read_files"] + len('{"tools":[]}'))

    def test_claude_per_message_usage_is_not_double_counted_or_added_to_totals(self):
        frames = [
            {"type":"system", "subtype":"init", "tools":["mcp__webcodex__read_files"]},
            {"type":"assistant", "message":{"id":"a", "usage":{"input_tokens":10}, "content":[
                {"type":"tool_use", "id":"call-1", "name":"mcp__webcodex__read_files"}]}},
            {"type":"assistant", "message":{"id":"a", "usage":{"input_tokens":10,"output_tokens":7}, "content":[]}},
            {"type":"user", "message":{"content":[{"type":"tool_result","tool_use_id":"call-1",
                "content":[{"type":"text","text":"中文\n"},{"type":"image","data":"not counted as text"}]}]}},
            {"type":"result", "usage":{"input_tokens":20}, "result":"done"}]
        report = client_metrics("claude", frames)
        self.assertEqual(report["total_calls"], 1)
        self.assertEqual(report["tool_results"][0]["returned_text_chars"], 3)
        self.assertEqual(report["model_round_usage"], [{"event_index":2,"usage":{"input_tokens":10,"output_tokens":7}}])
        self.assertEqual(report["usage"], [{"input_tokens":20}])
        self.assertEqual(report["available_tools"], ["mcp__webcodex__read_files"])

    def test_claude_duplicate_calls_and_missing_results_are_explicit(self):
        call = {"type":"assistant", "message":{"content":[{"type":"tool_use", "id":"a", "name":"Read"}]}}
        report = client_metrics("claude", [call, call, {"type":"user", "message":{"content":"text"}}])
        self.assertEqual(report["total_calls"], 1)
        self.assertEqual(report["tool_results"][0]["tool"], "Read")
        self.assertIsNone(report["tool_results"][0]["returned_text_chars"])

    def test_codex_completion_counts_do_not_count_command_updates_as_polls(self):
        frames = [
            {"type":"item.started", "item":{"id":"a","type":"command_execution"}},
            {"type":"item.updated", "item":{"id":"a","type":"command_execution","aggregated_output":"partial"}},
            {"type":"item.completed", "item":{"id":"a","type":"command_execution","aggregated_output":"full\n"}},
            {"type":"item.completed", "item":{"id":"b","type":"mcp_tool_call","tool":"observe_jobs",
                "result":{"content":[{"type":"text","text":"ready"}]}}},
            {"type":"item.completed", "item":{"id":"c","type":"file_change","changes":[]}},
            {"type":"turn.completed", "usage":{"input_tokens":100,"cached_input_tokens":80}}]
        report = client_metrics("codex", frames)
        self.assertEqual(report["total_calls"], 3)
        self.assertEqual([r["returned_text_chars"] for r in report["tool_results"]], [5, 5, None])
        self.assertIsNone(report["native_poll_calls"])
        self.assertIsNone(report["model_round_usage"])
        self.assertEqual(report["usage"][0]["cached_input_tokens"], 80)

    def test_next_parser_preserves_escaped_arguments_and_omission(self):
        line = 'next: observe_jobs items=[{"job_id":"job","after_observation_token":"opaque /+=","page":true}] tail_lines=200'
        tool, args = next(next_calls(line))
        self.assertEqual(tool, "observe_jobs")
        self.assertNotIn("wake_on", args)
        self.assertEqual(args["items"][0]["after_observation_token"], "opaque /+=")
        self.assertEqual(args["tail_lines"], 200)


if __name__ == "__main__":
    unittest.main()
