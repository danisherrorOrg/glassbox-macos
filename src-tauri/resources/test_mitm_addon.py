"""Unit tests for mitm_addon.py's tier-1 redaction functions.

Phase 0.3 code-review gap 1/6 (docs/[9] TODO.md): these functions had zero
automated coverage — only ever hand-checked via ad-hoc `python3 -c "..."`
calls during implementation, which left no trace in the repo. This file is
that missing coverage, stdlib `unittest` only (no new dependency), run
directly against `mitm_addon.py` with the real
`tier1_redaction_fields.json` sitting next to it (the single source of
truth both this addon and the Rust core's Phase 0.4 Redactor read) — not a
hand-picked fixture list that could drift from what actually ships.

Run with: python3 src-tauri/resources/test_mitm_addon.py
"""

import json
import os
import sys
import unittest
from pathlib import Path
from urllib.parse import parse_qs

HERE = Path(__file__).parent
os.environ["PNI_TIER1_FIELDS_PATH"] = str(HERE / "tier1_redaction_fields.json")
sys.path.insert(0, str(HERE))

import mitm_addon  # noqa: E402


class IsTier1Key(unittest.TestCase):
    def test_matches_known_tier1_keys(self):
        for key in ["password", "Token", "API_KEY", "client_secret"]:
            self.assertTrue(mitm_addon._is_tier1_key(key), key)

    def test_substring_match_catches_variants(self):
        # Matching is substring-based (module docstring / field list
        # $comment) so "user_password" and "old_token" must also match.
        self.assertTrue(mitm_addon._is_tier1_key("user_password"))
        self.assertTrue(mitm_addon._is_tier1_key("old_token"))

    def test_non_matching_key_passes(self):
        for key in ["username", "host", "path", "content-type"]:
            self.assertFalse(mitm_addon._is_tier1_key(key), key)


class RedactHeaders(unittest.TestCase):
    def test_redacts_authorization_case_insensitively(self):
        out = mitm_addon._redact_headers({"Authorization": "Bearer secret-jwt"})
        self.assertEqual(out["Authorization"], mitm_addon.REDACTED_MARKER)

    def test_leaves_non_tier1_headers_untouched(self):
        out = mitm_addon._redact_headers({"Content-Type": "application/json", "Host": "example.com"})
        self.assertEqual(out["Content-Type"], "application/json")
        self.assertEqual(out["Host"], "example.com")

    def test_tier2_header_passes_through_raw(self):
        # Cookie is deliberately tier-2 (PRIVACY_AND_SECURITY.md), not
        # redacted at this layer.
        out = mitm_addon._redact_headers({"Cookie": "session=abc123"})
        self.assertEqual(out["Cookie"], "session=abc123")


class RedactBody(unittest.TestCase):
    def test_no_body_returns_none(self):
        self.assertIsNone(mitm_addon._redact_body(b""))
        self.assertIsNone(mitm_addon._redact_body(None))

    def test_json_body_redacts_nested_tier1_fields(self):
        raw = json.dumps({
            "username": "alice",
            "password": "hunter2",
            "nested": {"api_key": "abc", "note": "keep me"},
            "items": [{"refresh_token": "xyz"}],
        }).encode()
        redacted = json.loads(mitm_addon._redact_body(raw))
        self.assertEqual(redacted["username"], "alice")
        self.assertEqual(redacted["password"], mitm_addon.REDACTED_MARKER)
        self.assertEqual(redacted["nested"]["api_key"], mitm_addon.REDACTED_MARKER)
        self.assertEqual(redacted["nested"]["note"], "keep me")
        self.assertEqual(redacted["items"][0]["refresh_token"], mitm_addon.REDACTED_MARKER)

    def test_form_urlencoded_body_redacts_tier1_fields(self):
        raw = b"username=alice&password=hunter2"
        redacted = mitm_addon._redact_body(raw)
        params = parse_qs(redacted)
        self.assertEqual(params["username"], ["alice"])
        self.assertEqual(params["password"], [mitm_addon.REDACTED_MARKER])

    def test_non_matching_body_passes_through_unredacted(self):
        raw = b"plain text body, no structure at all"
        self.assertEqual(mitm_addon._redact_body(raw), raw.decode())

    def test_binary_body_flagged_not_redaction_checked(self):
        raw = bytes([0xFF, 0xFE, 0x00, 0x01])
        self.assertEqual(mitm_addon._redact_body(raw), "[binary body, not redaction-checked]")


class RedactQueryString(unittest.TestCase):
    def test_redacts_tier1_query_param(self):
        out = mitm_addon._redact_query_string("/login?user=alice&password=hunter2")
        path, _, query = out.partition("?")
        params = parse_qs(query)
        self.assertEqual(params["user"], ["alice"])
        self.assertEqual(params["password"], [mitm_addon.REDACTED_MARKER])

    def test_no_query_string_returned_unchanged(self):
        self.assertEqual(mitm_addon._redact_query_string("/login"), "/login")

    def test_non_matching_query_params_pass_through(self):
        out = mitm_addon._redact_query_string("/search?q=hello")
        self.assertIn("q=hello", out)


if __name__ == "__main__":
    unittest.main()
