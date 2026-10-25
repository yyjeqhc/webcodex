from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import tempfile
import unittest

from scripts import continuation_capture as capture


def sample(now_ms=100000):
    return {"schema_version": 1, "observed_at_ms": now_ms, "db_identity": [1, 2],
            "selectors": {"goal_id": "g", "agent_id": "a", "session_id": "s"},
            "goal": {"revision": 1, "lifecycle": "active", "checkpoint_at_unix_ms": None},
            "agent": {"current_controller_generation": 1}, "endpoints": [],
            "attention": [], "wakes": [], "attempts": []}


class Clock:
    def __init__(self):
        self.value = 100.0

    def now(self):
        return self.value

    def sleep(self, seconds):
        self.value += seconds


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()  # Canonicalize OS temp aliases; keep production symlink rejection.
        self.output = self.root / "output"
        self.output.mkdir(mode=0o700)
        self.source = self.root / "source.json"
        self.source.write_text(json.dumps({"source": "a" * 40, "components": {
            r: {"source": "a" * 40, "sha256": "b" * 64} for r in capture.ROLES}}))
        self.source.chmod(0o600)
        self.args = argparse.Namespace(database=self.root / "db", source_record=self.source,
                                       expected_source="a" * 40, output=self.output,
                                       goal_id="g", agent_id="a", session_id="s",
                                       watch_seconds=10, interval_seconds=5)
        self.clock = Clock()

    def run_capture(self, reader=None, **kwargs):
        return capture.capture(self.args, read_snapshot=reader or (lambda *a, **k: sample(k["now_ms"])),
                               clock=self.clock.now, monotonic=self.clock.now,
                               sleep=kwargs.pop("sleep", self.clock.sleep), **kwargs)

    def receipts(self):
        return [json.loads(p.read_text()) for p in sorted(self.output.iterdir())]

    def test_delayed_human_preparation_does_not_spend_observation_budget(self):
        self.clock.value += 24 * 3600  # No watcher runs before exact resources exist.
        self.assertEqual(self.run_capture(), 0)
        rows = self.receipts()
        self.assertEqual([r["stage"] for r in rows], ["start", "sample", "sample", "end"])
        self.assertEqual(rows[-1]["elapsed_seconds"], 10)
        self.assertEqual(rows[0]["coverage"], "before_first_checkpoint")
        self.assertEqual(rows[-1]["assessment"]["unattended"], "not_observed")

    def test_late_snapshot_never_reconstructs_prospective_capture(self):
        self.args.watch_seconds = 0
        value = sample(); value["goal"].update(lifecycle="completed", revision=4, checkpoint_at_unix_ms=1)
        self.assertEqual(self.run_capture(lambda *a, **k: value), 0)
        self.assertEqual(self.receipts()[0]["coverage"], "late_or_snapshot_only")
        self.assertEqual(len(self.receipts()), 2)

    def test_missing_exact_resource_exits_immediately_without_setup_poll(self):
        def reader(*a, **k):
            raise capture.EvidenceError("goal_missing")
        self.assertEqual(self.run_capture(reader), 1)
        self.assertEqual(self.clock.value, 100)
        self.assertEqual(self.receipts()[0]["reason"], "goal_missing")

    def test_replacement_and_regression_stop_without_claiming_end(self):
        for change in ("database", "selectors", "revision", "generation"):
            with self.subTest(change=change):
                target = self.root / change; target.mkdir(mode=0o700); self.args.output = target
                calls = []
                def reader(*a, **k):
                    value = sample(k["now_ms"])
                    if calls:
                        if change == "database": value["db_identity"] = [1, 3]
                        if change == "selectors": value["selectors"]["goal_id"] = "other"
                        if change == "revision": value["goal"]["revision"] = 0
                        if change == "generation": value["agent"]["current_controller_generation"] = 0
                    calls.append(1)
                    return value
                self.assertEqual(self.run_capture(reader), 1)
                self.assertFalse(any("end.json" in p.name for p in target.iterdir()))

    def test_generation_successor_is_observed_without_redispatch(self):
        calls = []
        def reader(*a, **k):
            value = sample(k["now_ms"])
            value["agent"]["current_controller_generation"] = 1 + len(calls)
            calls.append(1)
            return value
        self.assertEqual(self.run_capture(reader), 0)
        self.assertEqual(len(calls), 3)

    def test_source_change_stops_capture(self):
        def sleep(seconds):
            self.clock.sleep(seconds)
            self.source.write_text("{}")
        self.assertEqual(self.run_capture(sleep=sleep), 1)
        self.assertEqual(self.receipts()[-1]["stage"], "incomplete")

    def test_cancellation_and_private_error_do_not_leak(self):
        def reader(*a, **k):
            raise RuntimeError("secret-token-value")
        self.assertEqual(self.run_capture(reader), 1)
        self.assertNotIn("secret-token-value", json.dumps(self.receipts()))
        other = self.root / "cancel"; other.mkdir(mode=0o700); self.args.output = other
        def cancel(seconds):
            raise KeyboardInterrupt()
        self.assertEqual(self.run_capture(sleep=cancel), 130)
        self.assertTrue(any("incomplete" in p.name for p in other.iterdir()))

    def test_nonempty_output_and_symlink_are_rejected_without_overwrite(self):
        marker = self.output / "existing"; marker.write_text("retained")
        with self.assertRaises(capture.EvidenceError): self.run_capture()
        self.assertEqual(marker.read_text(), "retained")
        self.assertEqual(len(list(self.output.iterdir())), 1)
        link = self.root / "alias"; link.symlink_to(self.output, target_is_directory=True)
        self.args.output = link
        with self.assertRaises(OSError): self.run_capture()

    def test_source_record_permissions_and_fifo_fail_without_blocking(self):
        self.source.chmod(0o644)
        self.assertEqual(self.run_capture(), 1)
        other = self.root / "fifo-out"; other.mkdir(mode=0o700); self.args.output = other
        fifo = self.root / "fifo"; os.mkfifo(fifo, 0o600); self.args.source_record = fifo
        self.assertEqual(self.run_capture(), 1)

    def test_clock_jump_and_broken_sleep_are_bounded(self):
        wall = self.clock.now
        def jump(seconds):
            self.clock.value += 1000
        fixed = lambda: 100.0
        code = capture.capture(self.args, read_snapshot=lambda *a, **k: sample(),
                               clock=wall, monotonic=fixed, sleep=jump)
        self.assertEqual(code, 1)
        self.assertEqual(self.receipts()[-1]["reason"], "clock_changed")
        other = self.root / "stuck"; other.mkdir(mode=0o700); self.args.output = other
        code = capture.capture(self.args, read_snapshot=lambda *a, **k: sample(),
                               clock=fixed, monotonic=fixed, sleep=lambda _: None)
        self.assertEqual(code, 1)

    def test_receipts_are_private(self):
        self.assertEqual(self.run_capture(), 0)
        for path in self.output.iterdir():
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)


class SummaryTests(unittest.TestCase):
    def wake_sample(self):
        value = sample()
        value["attention"] = [{"event_id": "e", "last_meaningful_activity_at_ms": 1000}]
        value["wakes"] = [{"wake_id": "w", "source_event_id": "e", "state": "pending",
                           "created_at_unix_ms": 301000, "consumed_at_unix_ms": None}]
        return value

    def test_unknown_delivery_never_suggests_retry_or_claims_fresh_turn(self):
        value = self.wake_sample()
        value["attempts"] = [{"attempt_id": "a", "wake_id": "w", "prepared_at_unix_ms": 302000,
                              "delivery_unknown_at_unix_ms": 303000}]
        report = capture.summarize(value)
        self.assertEqual(report["wakes"][0]["stage"], "delivery_unknown_do_not_resend")
        self.assertEqual(report["host_fresh_turn"], "not_observed")

    def test_expired_carrier_does_not_infer_browser_cause(self):
        value = sample()
        value["endpoints"] = [{"controller_generation": 1, "lifecycle": "attached",
                               "lease_expires_at_unix_ms": 99999}]
        report = capture.summarize(value)
        self.assertEqual(report["carrier"], "lease_expired")
        self.assertEqual(report["browser_interruption_cause"], "unknown")

    def test_consumption_and_multi_epoch_history_do_not_become_unattended_proof(self):
        value = self.wake_sample()
        value["wakes"][0].update(state="consumed", consumed_at_unix_ms=304000)
        value["attention"].append({"event_id": "e2", "last_meaningful_activity_at_ms": 400000})
        report = capture.summarize(value)
        self.assertEqual(report["wakes"][0]["intervals"]["wake_to_consume_ms"], 3000)
        self.assertEqual(report["findings"], [])
        self.assertEqual(report["unattended"], "not_observed")

    def test_duplicate_epoch_negative_clock_and_multiple_prepares_are_visible(self):
        value = self.wake_sample()
        value["attention"].append({"event_id": "e2", "last_meaningful_activity_at_ms": 1000})
        value["wakes"][0]["consumed_at_unix_ms"] = 1
        value["attempts"] = [{"attempt_id": "a", "wake_id": "w", "prepared_at_unix_ms": 302000}] * 2
        self.assertEqual(len(capture.summarize(value)["findings"]), 3)

    def test_delivery_intervals_use_only_selected_attempt(self):
        value = self.wake_sample()
        value["wakes"][0].update(state="consumed", consumed_at_unix_ms=305000, claimed_attempt_id="a")
        value["attempts"] = [{"attempt_id": "old", "wake_id": "w"},
                             {"attempt_id": "a", "wake_id": "w", "prepared_at_unix_ms": 302000,
                              "delivered_at_unix_ms": 303000, "consumed_at_unix_ms": 305000}]
        intervals = capture.summarize(value)["wakes"][0]["intervals"]
        self.assertEqual(intervals["wake_to_prepare_ms"], 1000)
        self.assertEqual(intervals["prepare_to_delivery_ms"], 1000)
        self.assertEqual(intervals["delivery_to_consume_ms"], 2000)


if __name__ == "__main__":
    unittest.main()
