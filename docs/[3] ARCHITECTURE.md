# Architecture — Process Network Inspector

**Status:** Frozen for Phase 0.1. Revise only when implementation reveals a real problem — not preemptively.

## Stack

- **Core:** Rust, running inside a Tauri shell
- **Frontend:** React, rendered in Tauri's native webview, talks to the Rust core only
- **Traffic capture:** a mitmproxy-based helper process (Python), launched and managed by the Rust core — unchanged from the original design; this boundary was always language-agnostic (`ADR-003`, `ADR-013`)
- **Distribution target:** a single native, code-signed, notarizable `.app` — see `PERMISSIONS_AND_PLATFORM.md`

## Layer diagram

```
┌───────────────────────────┐
│   React (Tauri webview)   │
└─────────────┬─────────────┘
              │  invoke commands (snapshot / query)  +  events (live updates)
              ▼
┌───────────────────────────┐
│   Tauri IPC (transport only) │
└─────────────┬─────────────┘
              ▼
┌───────────────────────────┐
│      Observation Engine    │   correlation, lifecycle, state
└─────────────┬─────────────┘
              │
   ┌──────────┼───────────┬───────────────┐
   ▼          ▼           ▼               ▼
ProcessProvider  SocketProvider  DNSProvider  TrafficProvider
                                                    │
                                                    ▼
                                        mitmproxy-based helper
```

## Responsibilities

- **React** renders whatever state the core hands it and sends user actions (select process, start/stop monitoring, apply a filter) as `invoke` commands. It never talks to the OS, never runs a provider, and never contains redaction logic — everything it displays is already redacted by the time it arrives.
- **Tauri IPC** is transport only. Command handlers call into the Observation Engine and return the result; they do not call `sysinfo`, `libproc`, or mitmproxy directly, and contain no correlation logic of their own. There is no HTTP server, no listening port, and no REST/WebSocket layer to secure — commands and events cross the webview/native boundary directly.
- **Observation Engine** is the correlation layer the whole project report is built around: it merges provider outputs, tracks connection lifecycle via snapshot diffing (see `DATA_MODEL.md`), attaches traffic to the right connection, applies the Observation Contract's status model to everything it emits, and is the single owner of in-memory state. Nothing else holds authoritative state.
- **Providers** each answer exactly one question and return typed data plus an explicit *provider-level* status (`OBSERVATION_CONTRACT.md`) — never partial data with silent gaps. Providers never assert `stale` or `unmatched` about their own output — those are Engine-derived judgments layered on top, since they require context (polling cadence, another provider's output) a single provider doesn't have. A provider also never constructs Engine-owned domain types directly (e.g. `SocketProvider` returns `SocketObservation`, never `NetworkConnection` — see `DATA_MODEL.md`). Providers are expressed as Rust traits; a provider implementation calls into macOS system APIs (via `sysinfo`/`netstat2`, or direct `libproc` FFI where those don't cover a need) rather than shelling out to CLI tools and parsing text.
- **Session Store** persists Engine state to disk, and only ever receives already-redacted data.
- **Redactor** sits at two distinct checkpoints, not one, and treats the two sensitivity tiers from `PRIVACY_AND_SECURITY.md` differently: highly-sensitive fields (`Authorization`, API keys, passwords, tokens) are redacted irreversibly at capture time and never exist raw anywhere, including in memory. Potentially-sensitive fields (URLs, query params, bodies, cookies, non-auth headers) get a *default display transform* (reversible — a user's "show anyway" reveals the paired transient `RawHTTPRequest`/`RawHTTPResponse` still held in memory for the current session, per `DATA_MODEL.md`) and a *mandatory pre-persistence/export transform* (irreversible — nothing written to the Session Store or an export file skips this, regardless of what's toggled on screen, and the raw variant is never itself persisted).

## Dependency rules (enforce these in review, they're what keeps this clean)

- **The Observation Engine is the only component permitted to create or mutate authoritative domain state** (`NetworkConnection`, `ProcessInfo`, `Flow`, and anything else defined as Engine-owned in `DATA_MODEL.md`). No shortcuts: not `Provider → IPC`, not `Provider → Session Store`, not `Provider → React`, not `TrafficProvider → NetworkConnection` directly. This is mostly already implied by the other rules below and by `DATA_MODEL.md`'s per-type ownership notes — stated once, explicitly, here, so it's one rule to check in review rather than something inferred from several scattered notes.
- React components never construct or interpret raw provider data — only the command/event contract types the Rust core defines (via `serde`-derived types shared across the IPC boundary).
- Tauri command handlers contain no provider or correlation logic: call the Engine, serialize, return.
- Providers never import Tauri command/event types, the Engine, or each other.
- Nothing writes to the Session Store except through the Redactor's mandatory path.
- `ObservationStatus` (a specific observation's outcome right now) and `ObservationCapabilities` (what a provider can ever observe, independent of any one attempt) are distinct concepts — see `DATA_MODEL.md`. Don't collapse them into one status field once capability reporting is implemented in Phase 0.3.

## Error propagation

A provider returns an Observation Contract status. The Engine aggregates statuses across providers for a given connection/process. Tauri serializes that status into the `invoke` response or event payload's `status` field, always alongside (never instead of) whatever partial data is available. React renders the status explicitly — "permission denied," "unsupported," "stale" — and never infers any of those from an empty or missing field.

## Future provider replacement

`TrafficProvider`'s interface is the only thing that needs reimplementing if the capture strategy ever changes. A Network Extension–based implementation is still not a pure Rust/Tauri component — `NetworkExtension.framework` entitlements require a Swift/ObjC system-extension target that subclasses Apple's own framework classes. What this stack changes is where that target lives: it can be embedded inside this app's already-native, already-signed `.app` bundle using Apple's normal system-extension tooling, rather than needing to be bolted onto a separate distribution mechanism from scratch (which is what the FastAPI-era stack would have required). See `DECISIONS.md` ADR-013 for the full trade-off, and ADR-009 for the original framing this supersedes.

## No local network surface

This was a real, new risk introduced by the FastAPI/React-era stack (see `DECISIONS.md` ADR-009) — a native SwiftUI app never had one, and this architecture removes it again rather than continuing to manage it. Tauri's `invoke`/`event` bridge is IPC between the webview and the native process, not a network socket: there is no port to bind, no CORS policy to restrict, and no access log that could leak query-string secrets outside the Redactor's control. Nothing in this architecture opens a listening socket for application traffic. (The mitmproxy helper's local IPC socket to the Rust core is not reachable from outside the machine and carries no user-facing surface of its own.)

## Explicitly not part of this architecture

Any modify/replay/inject capability anywhere in the stack. Any network-facing server reachable from anything other than this app's own IPC boundary. Providers reaching into each other directly instead of through the Engine. Swappable-backend machinery for a provider that only has one implementation so far (see `DECISIONS.md` on `Flow` and provider abstraction timing).
