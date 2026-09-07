"""Process Network Inspector's mitmproxy addon.

Consumes `response`/`error` events only — no `request()` interception hook
that could modify a flow, no `intercept()`, no `set()`/replay machinery, by
construction (docs/[9] TODO.md Phase 0.3: "no intercept(), no set(), no
replay hooks"). This addon observes; it never touches `flow.request`/
`flow.response` in a way that changes what's sent or received.

Applies tier-1 (highly-sensitive) redaction — Authorization headers, API
keys, passwords, tokens (the shared field list in
`tier1_redaction_fields.json`, also read by the Rust core's Phase 0.4
Redactor for the tier-2 list) — before anything is written to the IPC
socket. This is deliberate and irreversible: per docs/[6]
PRIVACY_AND_SECURITY.md, these values must never exist in raw form
anywhere, including in this process's own memory beyond the redaction
step itself, and there is no reveal path for them because there is
nothing left to reveal downstream.

Started by the Rust core (src-tauri/src/providers/traffic.rs) via
`mitmdump -s mitm_addon.py --mode local:<pid>`, with two environment
variables set on the child process:

    PNI_IPC_SOCKET_PATH   Unix domain socket path to connect to and stream
                          captured flows over, one JSON object per line.
    PNI_TIER1_FIELDS_PATH Path to tier1_redaction_fields.json.

Known limitation, not yet handled: tier-1 redaction below only recognizes
JSON and form-urlencoded bodies. A binary or otherwise-unstructured body
containing a credential in some other format would not be caught by this
heuristic. Phase 0.4's Redactor (tier 2) inherits the same limitation for
the same reason -- documented here rather than silently assumed away.
"""

import json
import os
import socket
import sys
import time
from urllib.parse import parse_qs, urlencode, urlparse, urlunparse

IPC_SOCKET_PATH = os.environ.get("PNI_IPC_SOCKET_PATH")
TIER1_FIELDS_PATH = os.environ.get("PNI_TIER1_FIELDS_PATH")
REDACTED_MARKER = "[redacted]"


def _load_tier1_fields():
    if not TIER1_FIELDS_PATH:
        return {"header_names": [], "body_query_keys": []}
    with open(TIER1_FIELDS_PATH) as f:
        data = json.load(f)
    return {
        "header_names": {h.lower() for h in data.get("header_names", [])},
        "body_query_keys": {k.lower() for k in data.get("body_query_keys", [])},
    }


TIER1 = _load_tier1_fields()


def _is_tier1_key(key: str) -> bool:
    key_lower = key.lower()
    return any(marker in key_lower for marker in TIER1["body_query_keys"])


def _redact_headers(headers) -> dict:
    out = {}
    for k, v in headers.items():
        key_lower = k.lower()
        if any(marker in key_lower for marker in TIER1["header_names"]):
            out[k] = REDACTED_MARKER
        else:
            out[k] = v
    return out


def _redact_json_value(value):
    if isinstance(value, dict):
        return {
            k: (REDACTED_MARKER if _is_tier1_key(k) else _redact_json_value(v))
            for k, v in value.items()
        }
    if isinstance(value, list):
        return [_redact_json_value(v) for v in value]
    return value


def _redact_body(raw_bytes):
    """Returns a text-safe body string with tier-1 fields redacted, or None
    if there's no body. Best-effort: JSON and form-urlencoded bodies get
    real structural redaction; anything else passes through undecoded as
    tier-1-unaware (see module docstring's known limitation)."""
    if not raw_bytes:
        return None
    try:
        text = raw_bytes.decode("utf-8")
    except UnicodeDecodeError:
        return "[binary body, not redaction-checked]"

    stripped = text.strip()
    if stripped.startswith("{") or stripped.startswith("["):
        try:
            parsed = json.loads(text)
            return json.dumps(_redact_json_value(parsed))
        except (json.JSONDecodeError, ValueError):
            pass

    # form-urlencoded heuristic: only treat as such if it actually parses
    # into key=value pairs (parse_qs never errors, so gate on '=' presence).
    if "=" in stripped and "\n" not in stripped:
        try:
            parsed = parse_qs(text, keep_blank_values=True)
            redacted = {
                k: (REDACTED_MARKER if _is_tier1_key(k) else v)
                for k, v in parsed.items()
            }
            return urlencode(redacted, doseq=True)
        except ValueError:
            pass

    return text


def _redact_query_string(path: str) -> str:
    parsed = urlparse(path)
    if not parsed.query:
        return path
    params = parse_qs(parsed.query, keep_blank_values=True)
    redacted = {
        k: (REDACTED_MARKER if _is_tier1_key(k) else v) for k, v in params.items()
    }
    new_query = urlencode(redacted, doseq=True)
    return urlunparse(parsed._replace(query=new_query))


class IPCConnection:
    """Connects once, retries briefly since the Rust core's listener should
    already be up by the time this addon loads (it binds before spawning
    mitmdump) but a small race is cheap to tolerate anyway."""

    def __init__(self, path, attempts=10, delay=0.2):
        self.sock = None
        if not path:
            return
        for _ in range(attempts):
            try:
                s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                s.connect(path)
                self.sock = s
                return
            except OSError:
                time.sleep(delay)
        print(f"mitm_addon: could not connect to {path}", file=sys.stderr)

    def send(self, obj):
        if self.sock is None:
            return
        try:
            line = json.dumps(obj) + "\n"
            self.sock.sendall(line.encode("utf-8"))
        except OSError:
            # Rust core went away (session stopped) -- nothing to do but
            # stop trying; mitmdump itself will be killed by the core
            # shortly after this happens in practice.
            self.sock = None


class ObservationAddon:
    def __init__(self):
        self.ipc = IPCConnection(IPC_SOCKET_PATH)

    def _build_request(self, flow):
        return {
            "method": flow.request.method,
            "host": flow.request.pretty_host,
            "path": _redact_query_string(flow.request.path),
            "headers": _redact_headers(flow.request.headers),
            "body": _redact_body(flow.request.raw_content),
            "timestamp": flow.request.timestamp_start,
        }

    def _build_response(self, flow):
        if flow.response is None:
            return None
        duration_ms = None
        if flow.response.timestamp_end and flow.request.timestamp_start:
            duration_ms = (flow.response.timestamp_end - flow.request.timestamp_start) * 1000
        return {
            "status_code": flow.response.status_code,
            "headers": _redact_headers(flow.response.headers),
            "body": _redact_body(flow.response.raw_content),
            "duration_ms": duration_ms or 0.0,
        }

    def _build_evidence(self, flow):
        # client_conn reflects the local redirector stub's own loopback
        # connection to mitmproxy, not the target process's real local
        # socket -- confirmed empirically during the Phase 0.3 spike
        # (docs/[7] PERMISSIONS_AND_PLATFORM.md). local_addr/local_port are
        # therefore never populated here; pid is filled in by the Rust core
        # (which already knows it -- it's the pid this mitmdump session was
        # launched for), not derived from flow data.
        remote_addr = None
        remote_port = None
        if flow.server_conn and flow.server_conn.address:
            remote_addr, remote_port = flow.server_conn.address[0], flow.server_conn.address[1]
        hostname = flow.client_conn.sni or flow.request.pretty_host
        return {
            "pid": None,
            "protocol": "tcp",
            "local_addr": None,
            "local_port": None,
            "remote_addr": remote_addr,
            "remote_port": remote_port,
            "hostname": hostname,
            "timestamp": flow.request.timestamp_start,
            "source": "mitmproxy-local",
        }

    def response(self, flow):
        self.ipc.send({
            "request": self._build_request(flow),
            "response": self._build_response(flow),
            "evidence": self._build_evidence(flow),
        })

    def error(self, flow):
        if flow.request is None:
            return
        self.ipc.send({
            "request": self._build_request(flow),
            "response": None,
            "evidence": self._build_evidence(flow),
        })


addons = [ObservationAddon()]
