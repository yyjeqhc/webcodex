"""Focused report/transport checks; these do not claim real-service acceptance."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import struct
import zlib
import tempfile
import unittest
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from e2e_generic_agent_ws import GenericSmoke, SENTINEL, EXTERNAL_SENTINEL, binary_fixtures, totals
import agent_loop_report as report


class GenericAcceptanceTests(unittest.TestCase):
    def test_stale_edit_checks_disk_bytes_before_reread_and_after_recovery(self):
        for profile in ("baseline", "upgraded"):
            for corruption in (None, "rejected", "recovered"):
                with self.subTest(profile=profile, corruption=corruption), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    args = argparse.Namespace(bin_dir=root, server=None, runner=None, profile=profile)
                    smoke = GenericSmoke(args, root)
                    smoke.project_dir, smoke.project_ref, smoke.session_ref = root, "fixture", "session"
                    trace = []
                    def call(name, args, **kwargs):
                        path = root / "stale.txt"
                        if name == "read_files":
                            revision = hashlib.sha256(path.read_bytes()).hexdigest()
                            trace.append(("read", revision))
                            return {"items": [{"output": {"read_revision": revision}}]}
                        change = args["changes"][0]
                        trace.append(("edit", change["expected_read_revision"], kwargs["success"] if "success" in kwargs else True))
                        if kwargs.get("success") is False:
                            self.assertIn(EXTERNAL_SENTINEL.encode(), path.read_bytes())
                            if corruption == "rejected":
                                path.write_bytes(b"clobbered despite false success\n")
                            return {"error_kind": "stale_file_revision", "state_changed": False}
                        path.write_bytes(path.read_bytes().replace(b"task value: before", b"task value: after"))
                        if corruption == "recovered":
                            path.write_bytes(b"task value: after\n")
                        return {}
                    smoke.call = call
                    if corruption:
                        with self.assertRaisesRegex(AssertionError, "external writer bytes"):
                            smoke.stale_edit()
                    else:
                        evidence = smoke.stale_edit()
                        self.assertEqual([step[0] for step in trace], ["read", "edit", "read", "edit"])
                        self.assertEqual(trace[0][1], trace[1][1])
                        self.assertEqual(trace[2][1], trace[3][1])
                        self.assertNotEqual(trace[0][1], trace[2][1])
                        self.assertTrue(evidence["rejected_bytes_unchanged"])
                        self.assertTrue(evidence["external_change_preserved"])
                        self.assertEqual(evidence["final_sha256"], hashlib.sha256((root / "stale.txt").read_bytes()).hexdigest())

    def test_generic_cases_load_through_existing_reporter(self):
        manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
        expected = {"generic_mixed_files_conflict", "generic_encoded_csv", "generic_failure_repair",
                    "generic_pending_independent_join", "generic_stale_read_revision", "generic_missing_output"}
        cases = {case["id"]: case for case in manifest["cases"] if case["id"].startswith("generic_")}
        self.assertEqual(set(cases), expected)
        for case_id, case in cases.items():
            with self.subTest(case=case_id):
                for variant, surface in (("direct", "direct"), ("host_code_mode", "host_code_mode"),
                                         ("code_mode", case["code_mode_surface"])):
                    metadata = report._benchmark_metadata(case_manifest=None, case_id=case_id,
                        variant=variant, surface=surface, base_revision="a" * 40)
                    self.assertEqual(metadata["case_fingerprint"], report._case_fingerprint(case))
                    self.assertTrue(metadata["validation_required"])
                self.assertIn("fixture", case["correctness"])
                self.assertIn("independent", case["validation"]["expectation"].lower())

    def test_measurement_counts_exact_transport_bytes_and_duplicate_content(self):
        # Preserve deliberate whitespace and non-ASCII bytes: reserialization must
        # not undercount the actual MCP envelope or conceal duplicate log tails.
        reply = {"jsonrpc": "2.0", "id": 3, "result": {
            "content": [{"type": "text", "text": SENTINEL + " 中文"}],
            "structuredContent": {"success": True, "output": {"stdout": SENTINEL}}}}
        raw = json.dumps(reply, ensure_ascii=False, indent=3).encode()
        with tempfile.TemporaryDirectory() as directory:
            args = argparse.Namespace(bin_dir=Path(directory), server=None, runner=None)
            smoke = GenericSmoke(args, Path(directory))
            response = Mock()
            response.read.return_value = raw
            response.__enter__ = Mock(return_value=response)
            response.__exit__ = Mock(return_value=False)
            smoke.opener = Mock()
            smoke.opener.open.return_value = response
            body = {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                    "params": {"name": "run_script", "arguments": {"script": "pass"}}}
            self.assertEqual(smoke.post("/mcp", body, "never-record-this-token"), reply)
            call = smoke.calls[0]
            self.assertEqual(call["response_bytes"], len(raw))
            self.assertEqual(call["response_sha256"], hashlib.sha256(raw).hexdigest())
            self.assertEqual(call["sentinel_occurrences"], 2)
            self.assertNotIn("never-record-this-token", json.dumps(call))
            request = smoke.opener.open.call_args.args[0]
            self.assertEqual(call["request_bytes"], len(request.data))
            self.assertEqual(json.loads(request.data)["params"]["_meta"]["io.modelcontextprotocol/clientCapabilities"], {})
            smoke.app_fixture = True
            smoke.post("/mcp", body, "never-record-this-token")
            app_request = smoke.opener.open.call_args.args[0]
            meta = json.loads(app_request.data)["params"]["_meta"]
            self.assertEqual(meta["io.modelcontextprotocol/clientCapabilities"]["extensions"]["io.modelcontextprotocol/ui"]["mimeTypes"], ["text/html;profile=mcp-app"])
            self.assertEqual(meta["openai/session"], "generic-acceptance-app-fixture-window")
            self.assertEqual(smoke.calls[-1]["client_role"], "independent_mcp_app_fixture")


    def test_png_and_pdf_fixtures_have_valid_structural_checksums_and_offsets(self):
        fixtures = binary_fixtures()
        png = fixtures["pixel.png"]
        self.assertEqual(png[:8], b"\x89PNG\r\n\x1a\n")
        offset, chunks = 8, {}
        while offset < len(png):
            length = struct.unpack(">I", png[offset:offset + 4])[0]
            kind = png[offset + 4:offset + 8]
            data = png[offset + 8:offset + 8 + length]
            crc = struct.unpack(">I", png[offset + 8 + length:offset + 12 + length])[0]
            self.assertEqual(crc, zlib.crc32(kind + data) & 0xffffffff)
            chunks[kind] = data
            offset += length + 12
        self.assertEqual(struct.unpack(">II", chunks[b"IHDR"][:8]), (1, 1))
        self.assertEqual(zlib.decompress(chunks[b"IDAT"]), b"\0\xff\0\0")
        self.assertIn(b"IEND", chunks)
        pdf = fixtures["document.pdf"]
        self.assertTrue(pdf.startswith(b"%PDF-1.4\n"))
        xref_offset = int(pdf.split(b"startxref\n")[1].splitlines()[0])
        entries = pdf[xref_offset:].splitlines()
        self.assertEqual(entries[:2], [b"xref", b"0 5"])
        for number, entry in enumerate(entries[3:7], 1):
            pointer = int(entry.split()[0])
            self.assertTrue(pdf[pointer:].startswith(f"{number} 0 obj\n".encode()))
        self.assertTrue(pdf.endswith(b"%%EOF\n"))

    def test_large_output_checks_real_file_metadata_and_exact_retained_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args = argparse.Namespace(bin_dir=root, server=None, runner=None)
            smoke = GenericSmoke(args, root)
            smoke.project_dir, smoke.project, smoke.owner = root, "fixture", "unused"
            receipts, sessions, finish_args = {}, [], []
            def finish(name, args, **kwargs):
                finish_args.append(args)
                if "outputs" not in args:
                    receipts.pop(args["session_id"], None)
                    return {}
                path = args["outputs"][0]
                data = (root / path).read_bytes()
                receipt = {"items": [{"path": path, "status": "verified",
                    "file_bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}],
                    "verified_count": 1, "missing_count": 0, "unavailable_count": 0, "observed_at": 123}
                receipts[args["session_id"]] = receipt
                return {"task_outputs": receipt}
            smoke.call = finish
            def api(path, body, token):
                self.assertEqual(body["tool"], "work_on_project")
                session = "wc_sess_fixture" + str(len(sessions))
                sessions.append(session)
                return {"success": True, "output": {"session_id": session}}
            smoke.post = api
            smoke.app_state = lambda project, session: {"work_result": {"task_outputs": receipts.get(session)}}
            evidence = smoke.large_output()
            self.assertEqual(evidence["file_bytes"], 10 * 1024 * 1024 + 1)
            self.assertTrue(evidence["work_result_retained"])
            self.assertTrue(evidence["foreign_recorder_preserved_own_receipt"])
            self.assertTrue(evidence["latest_business_omission_clears_receipt"])
            self.assertEqual(evidence["canonical_api_calls_outside_mcp_metrics"], 2)
            self.assertNotIn("_wc", finish_args[0])
            self.assertEqual(finish_args[2]["session_id"], sessions[0])
            self.assertEqual(finish_args[2]["_wc"], {"record": sessions[1]})
            self.assertNotIn("outputs", finish_args[3])
            self.assertNotIn(sessions[0], receipts)
            self.assertEqual(receipts[sessions[1]]["items"][0]["path"], "recorder-owned.txt")
            smoke.app_state = lambda project, session: {"work_result": {"task_outputs": {"observed_at": 456}}}
            with self.assertRaisesRegex(AssertionError, "exact observed output receipt"):
                smoke.large_output()

    def test_totals_distinguish_negative_tool_results_from_unknown(self):
        calls = [{"request_bytes": 2, "response_bytes": 3, "wall_seconds": .1, "success": s}
                 for s in [True, False, None]]
        self.assertEqual(totals(calls), {"meaningful_outer_calls": 3,
            "actual_mcp_response_bytes": 9, "actual_request_bytes": 6,
            "tool_failures": 1, "wall_seconds": .3})

    def test_pending_runs_independent_action_then_one_join_same_job(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args = argparse.Namespace(bin_dir=root, server=None, runner=None)
            smoke = GenericSmoke(args, root)
            smoke.project_dir = root
            trace = []
            def start(*args):
                trace.append("start")
                smoke.calls.append({"tool": "run_process"})
                return "original-job"
            smoke.start_pipe = start
            def independent(script):
                trace.append("independent")
                smoke.calls.append({"tool": "run_script"})
                (root / "independent.txt").write_text("independent work completed")
            smoke.script = independent
            def write(job, *args, **kwargs):
                trace.append(("release", job))
                smoke.calls.append({"tool": "write_job_input"})
            smoke.write = write
            def terminal(job):
                trace.append(("join-observe", job))
                smoke.calls.extend([{"tool": "wait_for_job_readiness"}, {"tool": "observe_jobs"}])
                return {"job_id": job, "exit_code": 0}
            smoke.terminal = terminal
            evidence = smoke.pending()
            self.assertEqual(trace, ["start", "independent", ("release", "original-job"),
                                     ("join-observe", "original-job")])
            self.assertEqual(evidence["readiness_joins"], 1)
            self.assertEqual(evidence["redispatches"], 0)
            def wrong_job(job):
                terminal(job)
                return {"job_id": "replacement-job", "exit_code": 0}
            smoke.terminal = wrong_job
            with self.assertRaisesRegex(AssertionError, "original Job"):
                smoke.pending()
            def repeated_wait(job):
                smoke.calls.append({"tool": "wait_for_job_readiness"})
                return terminal(job)
            smoke.terminal = repeated_wait
            with self.assertRaisesRegex(AssertionError, "call order or dispatch/wait count"):
                smoke.pending()


if __name__ == "__main__":
    unittest.main()
