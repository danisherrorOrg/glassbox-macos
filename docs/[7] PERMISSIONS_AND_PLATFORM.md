# Permissions & Platform — Process Network Inspector

This document exists to separate what's actually been tested from what's assumed. Every claim below is tagged **VERIFIED**, **ASSUMED**, or **DECISION** — update tags as the Phase 0 spike (`TODO.md`) produces real findings. Nothing in this file should be treated as fact until it carries a VERIFIED tag from an actual test on this machine.

## Framing: two different problems, not one

**Process and socket inspection** (`ProcessProvider`/`SocketProvider`) and **traffic capture** (`TrafficProvider`) have entirely different permission stories. Conflating them (as an earlier draft of the project report briefly did) overstates the cost of Phases 1–2 and understates why Phase 3 is genuinely harder.

## Process and socket inspection

- **ASSUMED:** A Python process run as your own user can enumerate its own and other same-user processes' basic info via `psutil`, and can shell out to `lsof -i -n -P` for socket info, without needing elevated privileges.
- **ASSUMED:** Enumerating sockets belonging to *other users'* processes needs root (`sudo lsof`), same as it would from a Terminal session.
- **ASSUMED:** Because this project no longer ships as a sandboxed Mac App Store app, Apple's app-sandbox entitlement restrictions (which would have constrained a Swift/SwiftUI build) don't apply — a plain Python process run from Terminal or a launch script has the same OS-level visibility as any other unprivileged process you run.
- **TO VERIFY (Phase 0 spike):** whether `psutil` on this machine can read connection info for other users' processes without `sudo`, and what exactly `lsof` refuses without elevation.

## Traffic capture

- **ASSUMED:** mitmproxy's `--mode local:<pid>` capture requires accepting its CA certificate into your trust store once, and needs to run with enough privilege to redirect that specific process's traffic — verify exactly what privilege level this needs on your machine (some local-capture modes need `sudo`, some don't, depending on the macOS version and mitmproxy's implementation).
- **DECISION:** Because this stack is Python-only (FastAPI backend, no Swift/ObjC layer), a Network Extension–based `TrafficProvider` is not achievable as a pure Python component — `NetworkExtension.framework` entitlements are granted to signed native apps, not to a Python process. This is not a permanent dead end for the project, but it does mean that path requires introducing a separate, separately-signed native helper alongside this stack, rather than a Python-only upgrade. If deeper packet-level capture is ever needed beyond what mitmproxy's local mode offers, that native-helper option or accepting the mitmproxy-based approach as this project's practical ceiling are the two realistic choices. See `DECISIONS.md`.
- **ASSUMED:** Certificate pinning will still defeat this approach on some target processes exactly as it would for a native app — this isn't a platform-permission problem, it's inherent to how the target app is written, and is a legitimate stopping point per the original learning path this project is based on.

## Running the app itself

- **DECISION:** No code signing, notarization, or App Store review path applies — this ships as source you run locally (`uvicorn` + a React dev/build server), not a distributed `.app`. Revisit only if a packaged, double-clickable distribution is ever wanted later (e.g. wrapping the same backend in `pywebview` or similar); that's an explicit future decision, not assumed.
- **DECISION:** Because there's no sandboxing to design around, the FastAPI backend's own listening socket becomes the security boundary that matters most for this project — see `PRIVACY_AND_SECURITY.md`.

## First technical spike (still applies, framing unchanged by the stack pivot)

Confirm exactly what an ordinary Python process can read about other processes' sockets on this machine without elevated privileges, and what `lsof`/`psutil` refuse. This determines whether Phase 0.1 is trivial or the project's first real blocker — same spike as originally planned, now scoped to Python's system-call boundary instead of Swift's.
