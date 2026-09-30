"""Exercise the existing shell harness's parameter/counter paths, without network
or service startup. This is not an end-to-end evaluation or Apps integration.
"""
import json
import os
from pathlib import Path
import subprocess
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "eval_coding_loop.sh"


class CodingLoopContractTests(unittest.TestCase):
    def shell(self, body):
        env = dict(os.environ, CARGO_BIN="true")
        for key in ("EVAL_SKIP_RUN", "EVAL_MODE", "EVAL_TOKEN", "EVAL_SERVER_BIN", "EVAL_RUNNER_BIN"):
            env.pop(key, None)
        result = subprocess.run(
            ["bash", "-c", 'source "$1"; ' + body, "contract-fixture", str(SCRIPT)],
            stdin=subprocess.DEVNULL, capture_output=True, text=True, env=env,
            timeout=15, check=True,
        )
        return result.stdout.strip()

    def test_both_flows_bootstrap_with_the_published_tool(self):
        for flow in ("baseline", "guided"):
            output = self.shell('''
call_tool() { python3 -c 'import json,sys; print(json.dumps({"tool":sys.argv[1],"params":json.loads(sys.argv[2])}))' "$1" "$2"; LAST_BODY='{}'; }
assert_session_created() { :; }
start_case_session ''' + flow + ''' 'fixture title'
''')
            request = json.loads(output)
            self.assertEqual(request["tool"], "work_on_project")
            self.assertEqual(request["params"]["instruction"], "fixture title")
            self.assertNotIn("title", request["params"])
            self.assertNotIn("mode", request["params"])

    def test_current_editor_is_counted_and_failure_is_not_silently_success(self):
        output = self.shell('''
api_post() { printf '%s' '{"success":false,"output":{"state_changed":false}}'; }
call_tool edit_project_files '{}'
printf '%s %s' "$CASE_STRUCTURED_EDIT_CALLS" "$CASE_FAILED_TOOL_CALLS"
''')
        self.assertEqual(output, "1 1")

    def test_json_envelope_is_the_api_contract_not_a_guessed_mcp_wrapper(self):
        output = self.shell('json_body edit_project_files \'{"project":"agent:r:p","changes":[]}\'')
        self.assertEqual(json.loads(output), {"tool": "edit_project_files", "params": {"project": "agent:r:p", "changes": []}})


if __name__ == "__main__":
    unittest.main()
