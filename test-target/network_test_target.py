#!/usr/bin/env python3
"""NetworkTestTarget — a small, deterministic traffic generator for testing
Process Network Inspector against traffic you already understand, rather
than a real app you don't control. See docs/[8] TESTING_STRATEGY.md.

This is a standalone script, not part of the app itself (docs/[9] TODO.md's
Cross-cutting section) — plain stdlib, no dependencies, so it runs with
whatever python3 is already on the machine.

It plays both roles: it starts local HTTP and HTTPS servers (the "remote"
side) and, as the same process, makes outbound connections/requests against
them (the side Process Network Inspector actually observes) — so the PID
this script prints is the one to target.

**Why traffic targets this machine's LAN IP, not 127.0.0.1:** the Phase 0.3
mitmproxy spike (`docs/[9] TODO.md`) found that macOS's Network Extension
framework — what `mitmproxy --mode local:<pid>` is built on — does not see
loopback traffic at all, confirmed empirically (traffic to a LAN-IP-bound
server was captured immediately; identical traffic to a 127.0.0.1-bound
server produced zero captured flows over 12+ seconds of active requests).
So most scenarios here bind to `LAN_HOST` (detected automatically) precisely
so a real `mitmproxy --mode local:<pid>` session — or, later, Process
Network Inspector's own `TrafficProvider` — can actually observe them.
`local_connection` is the one scenario that deliberately still uses
`127.0.0.1`, to exercise/demonstrate that specific limitation rather than
silently avoid it. See `docs/PERMISSIONS_AND_PLATFORM.md` "Traffic capture"
for the full verified finding.

Usage:
    python3 network_test_target.py serve
        Start the servers, print this process's PID, then continuously
        cycle through every scenario every few seconds until interrupted
        (Ctrl+C). This is the mode to target with the inspector or with
        `mitmproxy --mode local:<pid>` — leave it running while you attach.

    python3 network_test_target.py <scenario>
        Start the servers, run exactly one scenario once, then exit.
        Scenarios: plain_tcp, short_lived, long_lived, simultaneous,
        local_connection, http, https, large_response, slow_response,
        sensitive_fields
"""

import http.client
import http.server
import json
import os
import socket
import ssl
import subprocess
import sys
import threading
import time
from pathlib import Path


def detect_lan_ip():
    """The machine's outbound-interface IP, found the portable way (no
    platform-specific tools): open a UDP socket "connected" to a public
    address — nothing is actually sent — and read back the local address
    the OS would use. Falls back to loopback if there's no network at all,
    so this script still runs offline (mitmproxy interception just won't
    work in that case, same as it wouldn't for the real app either)."""
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
            s.connect(("8.8.8.8", 80))
            return s.getsockname()[0]
    except OSError:
        return "127.0.0.1"


BIND_HOST = "0.0.0.0"  # servers accept on every interface, including loopback
LAN_HOST = detect_lan_ip()  # what most scenarios connect *to* — see module docstring
LOOPBACK_HOST = "127.0.0.1"  # what `local_connection` deliberately connects to instead
HTTP_PORT = 8765
HTTPS_PORT = 8766
CERT_DIR = Path(__file__).parent / "certs"
CERT_FILE = CERT_DIR / "cert.pem"
KEY_FILE = CERT_DIR / "key.pem"

LARGE_BODY = b"x" * (512 * 1024)  # 512 KiB, to exercise truncation limits

# Intentionally fake sensitive-looking values, for redaction testing
# (docs/[6] PRIVACY_AND_SECURITY.md) — never real credentials.
FAKE_TOKEN = "fake-token-do-not-use-1234567890abcdef"
FAKE_PASSWORD = "hunter2-fake-password"
FAKE_AUTH_HEADER = "Bearer fake-jwt-not-a-real-token.aaa.bbb"


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        pass  # keep stdout to our own scenario prints

    def _write(self, status, body, headers=None):
        self.send_response(status)
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/large":
            self._write(200, LARGE_BODY, {"Content-Type": "application/octet-stream"})
        elif self.path == "/slow":
            time.sleep(1.5)
            self._write(200, b"slow response done")
        elif self.path == "/sensitive":
            auth = self.headers.get("Authorization", "")
            self._write(
                200,
                json.dumps({"received_authorization": auth, "ok": True}).encode(),
                {"Content-Type": "application/json"},
            )
        else:
            self._write(200, b"hello from NetworkTestTarget")

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        self.rfile.read(length)
        self._write(200, b'{"received":true}', {"Content-Type": "application/json"})


def ensure_cert():
    CERT_DIR.mkdir(exist_ok=True)
    if CERT_FILE.exists() and KEY_FILE.exists():
        return
    subprocess.run(
        [
            "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
            "-keyout", str(KEY_FILE), "-out", str(CERT_FILE),
            "-days", "3650", "-subj", "/CN=localhost",
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def start_http_server():
    server = http.server.ThreadingHTTPServer((BIND_HOST, HTTP_PORT), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def start_https_server():
    ensure_cert()
    server = http.server.ThreadingHTTPServer((BIND_HOST, HTTPS_PORT), Handler)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(certfile=str(CERT_FILE), keyfile=str(KEY_FILE))
    server.socket = ctx.wrap_socket(server.socket, server_side=True)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


# ---- Scenarios ----

def plain_tcp():
    """A plain TCP connection: connect, send bytes, receive, close."""
    with socket.create_connection((LAN_HOST, HTTP_PORT), timeout=5) as s:
        s.sendall(b"GET / HTTP/1.0\r\n\r\n")
        s.recv(4096)
    print("plain_tcp: done")


def short_lived():
    """Opens and closes quickly, no data exchanged."""
    with socket.create_connection((LAN_HOST, HTTP_PORT), timeout=5):
        pass
    print("short_lived: done")


def long_lived(seconds=6):
    """Stays open for a while."""
    with socket.create_connection((LAN_HOST, HTTP_PORT), timeout=5) as s:
        time.sleep(seconds)
        s.sendall(b"GET / HTTP/1.0\r\n\r\n")
        s.recv(4096)
    print(f"long_lived: done ({seconds}s)")


def simultaneous(count=5):
    """Several connections open at the same time."""
    socks = [socket.create_connection((LAN_HOST, HTTP_PORT), timeout=5) for _ in range(count)]
    time.sleep(1)
    for s in socks:
        s.sendall(b"GET / HTTP/1.0\r\n\r\n")
    for s in socks:
        s.recv(4096)
        s.close()
    print(f"simultaneous: done ({count} connections)")


def local_connection():
    """Deliberately loopback (127.0.0.1), unlike every other scenario here
    — see the module docstring's note on why `mitmproxy --mode local:<pid>`
    (and, per the same underlying macOS limitation, Process Network
    Inspector's own future `TrafficProvider`) cannot observe this one."""
    with socket.create_connection((LOOPBACK_HOST, HTTP_PORT), timeout=5) as s:
        s.sendall(b"GET / HTTP/1.0\r\n\r\n")
        s.recv(4096)
    print("local_connection: done (127.0.0.1 — not mitmproxy-interceptable)")


def http_request():
    conn = http.client.HTTPConnection(LAN_HOST, HTTP_PORT, timeout=5)
    conn.request("GET", "/")
    resp = conn.getresponse()
    resp.read()
    conn.close()
    print(f"http_request: {resp.status}")


def https_request():
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE  # self-signed local cert; not testing cert validation here
    conn = http.client.HTTPSConnection(LAN_HOST, HTTPS_PORT, timeout=5, context=ctx)
    conn.request("GET", "/")
    resp = conn.getresponse()
    resp.read()
    conn.close()
    print(f"https_request: {resp.status}")


def large_response():
    conn = http.client.HTTPConnection(LAN_HOST, HTTP_PORT, timeout=5)
    conn.request("GET", "/large")
    resp = conn.getresponse()
    body = resp.read()
    conn.close()
    print(f"large_response: {len(body)} bytes")


def slow_response():
    conn = http.client.HTTPConnection(LAN_HOST, HTTP_PORT, timeout=10)
    start = time.time()
    conn.request("GET", "/slow")
    resp = conn.getresponse()
    resp.read()
    conn.close()
    print(f"slow_response: {time.time() - start:.1f}s")


def sensitive_fields():
    """A request carrying fake sensitive-looking fields, for redaction
    testing (docs/[6] PRIVACY_AND_SECURITY.md) — values are all fake."""
    conn = http.client.HTTPConnection(LAN_HOST, HTTP_PORT, timeout=5)
    body = json.dumps({"password": FAKE_PASSWORD, "token": FAKE_TOKEN}).encode()
    conn.request(
        "POST",
        "/sensitive",
        body=body,
        headers={
            "Authorization": FAKE_AUTH_HEADER,
            "Content-Type": "application/json",
            "Cookie": "session=fake-session-cookie-value",
        },
    )
    resp = conn.getresponse()
    resp.read()
    conn.close()
    print("sensitive_fields: done")


SCENARIOS = {
    "plain_tcp": plain_tcp,
    "short_lived": short_lived,
    "long_lived": long_lived,
    "simultaneous": simultaneous,
    "local_connection": local_connection,
    "http": http_request,
    "https": https_request,
    "large_response": large_response,
    "slow_response": slow_response,
    "sensitive_fields": sensitive_fields,
}


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in {"serve", *SCENARIOS}:
        print(__doc__)
        sys.exit(1)

    start_http_server()
    start_https_server()

    mode = sys.argv[1]
    if mode == "serve":
        print(f"NetworkTestTarget pid={os.getpid()}")
        print(f"HTTP  server: http://{LAN_HOST}:{HTTP_PORT} (also on 127.0.0.1)")
        print(f"HTTPS server: https://{LAN_HOST}:{HTTPS_PORT} (also on 127.0.0.1)")
        print("Cycling through all scenarios every 3s. Ctrl+C to stop.")
        try:
            while True:
                for name, fn in SCENARIOS.items():
                    try:
                        fn()
                    except Exception as e:
                        print(f"{name}: error: {e}")
                    time.sleep(3)
        except KeyboardInterrupt:
            print("\nstopped")
    else:
        print(f"NetworkTestTarget pid={os.getpid()}")
        SCENARIOS[mode]()


if __name__ == "__main__":
    main()
