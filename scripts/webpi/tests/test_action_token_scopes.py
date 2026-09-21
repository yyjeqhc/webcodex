from __future__ import annotations

import unittest

from scripts.webpi import standalone


class ActionTokenScopeTests(unittest.TestCase):
    def test_recommended_action_scopes_add_only_read_only_observation_capabilities(self) -> None:
        baseline = [
            "runtime:read",
            "runner:manage",
            "session:collaborate",
            "project:read",
            "project:write",
            "job:run",
        ]
        scopes = standalone.action_token_scopes(baseline)
        for required in (
            "plugin:inspect",
            "plugin:invoke",
            "computer:read",
            "computer:display_read",
            "browser:read",
        ):
            self.assertIn(required, scopes)
        for forbidden in (
            "admin",
            "computer:clipboard_read",
            "computer:clipboard_write",
            "computer:control",
            "computer:pointer_control",
            "computer:launch",
            "browser:control",
            "browser:launch",
        ):
            self.assertNotIn(forbidden, scopes)
        self.assertEqual(len(scopes), len(set(scopes)))

    def test_current_action_token_record_requires_prefix_name_and_exact_scopes(self) -> None:
        token = "wc_pat_123456789abcdefghijklmnopqrstuvwxyz"
        desired = ["runtime:read", "computer:read", "computer:display_read"]
        tokens = [
            {
                "id": "wrong-prefix",
                "name": "webpi-action",
                "token_prefix": "wc_pat_otherpref",
                "scopes": desired,
                "revoked_at": None,
            },
            {
                "id": "too-broad",
                "name": "webpi-action",
                "token_prefix": token[:16],
                "scopes": [*desired, "computer:control"],
                "revoked_at": None,
            },
        ]
        self.assertIsNone(standalone.current_action_token_record(tokens, token, desired))
        tokens[-1]["scopes"] = list(desired)
        self.assertEqual(
            standalone.current_action_token_record(tokens, token, desired)["id"],
            "too-broad",
        )

    def test_stale_action_token_ids_keep_every_other_active_same_name_token(self) -> None:
        token = "wc_pat_123456789abcdefghijklmnopqrstuvwxyz"
        tokens = [
            {"id": "current", "name": "webpi-action", "token_prefix": token[:16], "revoked_at": None},
            {"id": "duplicate", "name": "webpi-action", "token_prefix": "wc_pat_other111", "revoked_at": None},
            {"id": "old", "name": "webpi-action", "token_prefix": "wc_pat_other222", "revoked_at": None},
            {"id": "revoked", "name": "webpi-action", "token_prefix": "wc_pat_other333", "revoked_at": 1},
            {"id": "other-name", "name": "chatgpt-action", "token_prefix": "wc_pat_other444", "revoked_at": None},
        ]
        self.assertEqual(
            standalone.stale_action_token_ids(tokens, token),
            ["duplicate", "old"],
        )


if __name__ == "__main__":
    unittest.main()
