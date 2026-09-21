from __future__ import annotations

import unittest
from unittest.mock import patch
from scripts.webpi.security_smoke import NoRedirect, authentication_checks, public_surface_checks, require_authenticated_origin, validate_origin, verify


class SecurityAcceptanceTests(unittest.TestCase):
    def test_origins_reject_cleartext_remote_credentials_and_controls(self):
        for value in ("http://example.com", "https://user:password@example.com", "https://example.com/api", "https://example.com?token=x", "https://exam\nple.com", "https://example.com/#x"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_origin(value)
        self.assertEqual(validate_origin("https://example.com/"), "https://example.com")
        self.assertEqual(validate_origin("http://127.0.0.1:56542"), "http://127.0.0.1:56542")

    def test_redirects_are_not_followed(self):
        self.assertIsNone(NoRedirect().redirect_request(None, None, 302, "", {}, "https://other.example/"))

    @patch("scripts.webpi.security_smoke.request_json")
    def test_all_protected_routes_require_401_even_when_200_contains_an_error(self, request):
        for status, body, error in ((200, {"success": False}, None), (530, None, "not_json"), (302, None, "not_json"), (None, None, "unreachable")):
            request.return_value = (status, body, error)
            self.assertFalse(all(item["passed"] for item in authentication_checks("https://example.com")))
            with self.assertRaises(RuntimeError):
                require_authenticated_origin("https://example.com")
        request.return_value = (401, {"error": "Unauthorized"}, None)
        self.assertTrue(all(item["passed"] for item in authentication_checks("https://example.com")))

    @patch("scripts.webpi.security_smoke.request_json")
    def test_public_schema_is_allowed_but_wrong_server_origin_is_not(self, request):
        def reply(_base, route, _payload, authorization=None, timeout=8.0):
            if route == "/openapi.json":
                return 200, {"openapi": "3.1.0", "info": {"title": "WebPi GPT Actions"}, "paths": {}, "servers": [{"url": "https://example.com"}], "components": {"schemas": {}}}, None
            if authorization == "Bearer synthetic-valid-token":
                return 200, {"success": True, "output": {"auth_enabled": True, "service": "webpi"}}, None
            if route in ("/api/tools/call", "/mcp", "/admin"):
                return 404, {"error": "Not Found"}, None
            return 401, {"error": "Unauthorized"}, None
        request.side_effect = reply
        report = verify("https://example.com", "https://example.com", "synthetic-valid-token")
        self.assertTrue(report["passed"])
        self.assertNotIn("synthetic-valid-token", str(report))
        self.assertFalse(verify("https://example.com", "https://wrong.example")["passed"])

    @patch("scripts.webpi.security_smoke.request_json")
    def test_public_surface_hides_non_action_protocols(self, request):
        def reply(_base, route, _payload, authorization=None, timeout=8.0):
            if route == "/api/actions/runtime_status":
                return 401, {"error": "Unauthorized"}, None
            return 404, {"error": "Not Found"}, None
        request.side_effect = reply
        checks = public_surface_checks("https://example.com")
        self.assertTrue(all(item["passed"] for item in checks))
        self.assertEqual({item["route"] for item in checks if item["check"] == "public_surface_hidden"}, {"/api/tools/call", "/mcp", "/admin"})

    @patch("scripts.webpi.security_smoke.request_json")
    def test_loopback_expected_origin_keeps_full_local_auth_matrix(self, request):
        def reply(_base, route, _payload, authorization=None, timeout=8.0):
            if route == "/openapi.json":
                return 200, {"openapi": "3.1.0", "info": {"title": "WebPi GPT Actions"}, "paths": {}, "servers": [{"url": "http://127.0.0.1:56542"}], "components": {"schemas": {}}}, None
            if authorization == "Bearer synthetic-valid-token":
                return 200, {"success": True, "output": {"auth_enabled": True, "service": "webpi"}}, None
            return 401, {"error": "Unauthorized"}, None
        request.side_effect = reply
        report = verify("http://127.0.0.1:56542", "http://127.0.0.1:56542", "synthetic-valid-token")
        self.assertTrue(report["passed"])
        protected = [item for item in report["checks"] if item.get("route") in ("/api/actions/runtime_status", "/api/tools/call", "/mcp") and item.get("check") != "public_openapi_is_schema_not_authenticated_execution"]
        self.assertEqual(len(protected), 12)
        self.assertTrue(all(item["status"] == 401 for item in protected))

    @patch("scripts.webpi.security_smoke.request_json")
    def test_chatgpt_actions_requires_components_schemas_object(self, request):
        def reply(_base, route, _payload, authorization=None, timeout=8.0):
            if route == "/openapi.json":
                return 200, {"openapi": "3.1.0", "info": {"title": "WebPi GPT Actions"}, "paths": {}, "servers": [{"url": "https://example.com"}], "components": {"schemas": []}}, None
            return 401, {"error": "Unauthorized"}, None
        request.side_effect = reply
        self.assertFalse(verify("https://example.com")["passed"])
        with self.assertRaises(RuntimeError):
            require_authenticated_origin("https://example.com")

    @patch("scripts.webpi.security_smoke.request_json")
    def test_other_product_is_not_accepted_as_webpi(self, request):
        def reply(_base, route, _payload, authorization=None, timeout=8.0):
            if route == "/openapi.json":
                return 200, {"openapi": "3.1.0", "info": {"title": "WebCodex GPT Actions"}, "paths": {}}, None
            return 401, {"error": "Unauthorized"}, None
        request.side_effect = reply
        self.assertFalse(verify("https://example.com")["passed"])
        with self.assertRaises(RuntimeError): require_authenticated_origin("https://example.com")


if __name__ == "__main__":
    unittest.main()
