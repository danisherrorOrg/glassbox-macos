# Project Report: Process Network Inspector (macOS)

**Status:** v5 — trimmed to a product/vision document. Architecture, data model, permissions, security, testing, and the stack decision now live in their own authoritative documents under `docs/`; this report no longer duplicates them. That duplication is exactly what let a Swift-era architecture diagram survive three revisions past the stack pivot to FastAPI + React — see `docs/DECISIONS.md` ADR-011. This document answers "what is this and why," nothing else.
**Author context:** Synthesized from an initial architecture proposal, a detailed product/technical breakdown, and several rounds of cross-document review — all grounded in the "Seeing What a Process Talks to on the Network" learning path.

---

## 1. One-line description

> A read-only macOS application that lets users select a running process and understand its network activity — including sockets, connections, domains, and, where observable, HTTP(S) requests and responses — through a correlated timeline.

A process can speak TCP, UDP, QUIC, HTTP, HTTPS, WebSockets, gRPC, Unix domain sockets, or custom binary protocols, and some of that traffic is genuinely not inspectable at the application layer without cooperation from the app itself. The product is ambitious about correlation and honest about visibility limits — that tension is the design center of this whole project.

## 2. Product principle

> The application observes. It does not modify, replay, inject, or control the target process.

This splits into two distinct guarantees, kept separate rather than treated as one vague rule:

**Read-only with respect to the target process.** The app never modifies the target, injects code into it, changes its requests, sends it commands, or pauses its execution.

**Read-only with respect to network traffic.** The app may observe, parse, correlate, display, store, and export traffic — but never modify, replay, inject, or act as an API client using captured data.

### Explicitly out of scope

- Editing or resending captured requests
- Intercepting and pausing traffic mid-flight for modification
- Injecting arbitrary requests/packets
- Changing target-process behavior or executing commands inside it
- Auth bypass or credential extraction as a feature

### Explicitly in scope

Viewing, inspecting, correlating, analyzing, timelining, and exporting — read paths only.

### One clarification about the capture mechanism

HTTPS observation (Phase 0.3+) uses a local MITM (man-in-the-middle) proxy — mitmproxy, per `docs/ARCHITECTURE.md` — which by construction terminates the target process's TLS connection and re-originates each request to the real server, forwarding it onward unchanged. The app therefore sits in the path of the traffic it observes; it does not originate, modify, reorder, replay, or withhold any of it, and forwarding is not a capability exposed to the user in any form — it's an unavoidable mechanical property of how local TLS interception works, not a feature. This also requires the user to trust a locally-generated CA (certificate authority) certificate into their trust store once — an explicit, reversible, user-consented step. See `docs/PRIVACY_AND_SECURITY.md` for where that certificate lives and how to remove it. None of this changes either guarantee above: the app still never modifies the target process, and it still never modifies, replays, or injects the traffic it observes — it just does the observing from a position in the path rather than a passive tap, which is worth stating plainly rather than leaving a reader to infer it from the architecture doc alone.

## 3. Core user workflow

1. **Process list** — searchable table of running processes (name, PID, connection count, status).
2. **Select a process** → **Process detail** — tabs for Overview / Connections / API Traffic / Timeline.
3. **Connections tab** — one row per socket: local/remote address, port, protocol, state (LISTEN/ESTABLISHED/etc.), resolved to a hostname where possible.
4. **API Traffic tab** (HTTP/HTTPS only, where observable) — method, URL, status, duration; click a row to expand full request/response detail with sensitive data redacted by default.
5. **Timeline tab** — chronological log of connection and request/response events, so the answer to "what did this process just do" is a scroll, not a packet dump.

The core differentiator is the correlated view — process → connection → domain → request/response — grouping requests by the domain they went to, rather than a flat list of PID → socket rows. `lsof`, packet analyzers, and HTTP proxies each already solve one layer of this in isolation; the product is the correlation.

### Four levels of visibility (graceful degradation)

The product should never collapse "I can't see the payload" into "I can't see anything." Each level should keep working even when a deeper level isn't available for a given process or connection:

| Level | Question answered | Example | Availability |
|---|---|---|---|
| 1 — Process | Who? | `Claude`, PID 9132 | Generally available |
| 2 — Network | Who is it talking to? | `api.example.com:443`, `localhost:8080` | Where OS visibility permits |
| 3 — Protocol | How are they talking? | TCP/HTTPS, established | Where OS/provider visibility permits |
| 4 — Application payload | What are they saying? | `POST /v1/chat` → `200` | Only where technically observable |

An earlier version of this table said Levels 2–3 were "always available," which quietly contradicted the rest of this document set — socket visibility can legitimately come back `permission_denied`, `unavailable`, or `stale` (see `docs/OBSERVATION_CONTRACT.md`). The product attempts Levels 1–3 wherever technically and legitimately available and explicitly reports when they're not — this table now says that instead of overclaiming it.

A pinned-cert or non-HTTP connection still shows Levels 1–3 in full — "HTTPS, connected, contents unavailable" is a legitimate and useful answer, not a failure state. The precise status vocabulary behind this is defined in `docs/OBSERVATION_CONTRACT.md` — see that document for the authoritative list rather than repeating it here, where it's already drifted out of sync once.

Per-socket `bytes_sent`/`bytes_received` are explicitly *not* part of that Level-3 example above (Round 1/2 drafts of this table used byte counts as the illustrative Level-3 fact; that was overclaiming). The Phase 0 permissions spike (`docs/PERMISSIONS_AND_PLATFORM.md`) verified that none of `sysinfo`, `netstat2`, or direct `libproc` FFI expose a per-socket byte counter on macOS — it isn't a crate limitation, the kernel structures those APIs read (`in_sockinfo`/`tcp_sockinfo`) simply don't carry one. `SocketObservation.bytes_sent`/`bytes_received` (`docs/DATA_MODEL.md`) stay in the type as `Option<u64>` per the field-level-absence rule, but on macOS via this provider stack they render as "not reported" for every connection, not just some — this is a permanent reduction from what an earlier draft of this report implied, not a temporary gap.

### Observation Capabilities panel

For any selected process, surface exactly what the app can currently see, so the user distinguishes "the program isn't communicating" from "the program is communicating, but this traffic isn't observable at the application layer":

```
Observation Capabilities
─────────────────────────────────────
Process information          ✓
Socket information           ✓
Remote addresses             ✓
DNS / hostname                ✓
HTTP metadata                ✓
HTTPS payload                 ⚠ Limited
HTTP body                     ⚠ Limited
Raw packet data               ✕
```

This mock's rows correspond to the fields of `ObservationCapabilities` in `docs/DATA_MODEL.md`, which is the authoritative field list (implemented in Phase 0.3, panel UI built in Phase 1.0) — treat that document as canonical if the two ever appear to disagree, rather than updating one from the other by hand. The mock's single "HTTP body" row deliberately combines `DATA_MODEL.md`'s separate `request_body`/`response_body` fields into one display row for space — that's a display choice, not a missed field.

## 4. Where the rest of the design lives

This report intentionally does not duplicate the following — treat each as the sole authority on its topic, and update them directly rather than reflecting changes back into this document:

| Topic | Authoritative document |
|---|---|
| Architecture, layer responsibilities, dependency rules | `docs/ARCHITECTURE.md` |
| Data model, entity relationships, provider-vs-Engine type ownership | `docs/DATA_MODEL.md` |
| Status/observation vocabulary | `docs/OBSERVATION_CONTRACT.md` |
| macOS permissions, verified vs. assumed platform facts | `docs/PERMISSIONS_AND_PLATFORM.md` |
| Data classification, redaction rules, storage/export handling | `docs/PRIVACY_AND_SECURITY.md` |
| Testing pyramid, `NetworkTestTarget`, mandatory integration tests | `docs/TESTING_STRATEGY.md` |
| Stack decision and every other significant decision, with rationale and what was traded away | `docs/DECISIONS.md` |
| Phase-by-phase build checklist | `docs/TODO.md` |

## 5. High-level roadmap

HTTP observation is not required for the first useful product — process and socket visibility alone, live and correlated, is already a shippable first milestone. Full task-level detail lives in `docs/TODO.md`; this is the shape of it:

```
Foundation → Process/Socket Explorer → Live Monitoring + DNS →
Traffic Observation Backend → HTTP/HTTPS Metadata → API Explorer →
Timeline + Session Recording → Filters/Search/Analytics → Local-Run Polish
```

Packaged, distributable native distribution (code signing, notarization, a possible future Network Extension–based capture upgrade) is explicitly deferred past this roadmap — see `docs/TODO.md`'s "Phase 2 — Packaged Distribution" and `docs/DECISIONS.md` for why that path looks different than it would have under the original native-app plan.
