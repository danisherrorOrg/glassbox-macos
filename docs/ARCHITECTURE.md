# Architecture — Process Network Inspector

**Status:** Frozen for Phase 0.1. Revise only when implementation reveals a real problem — not preemptively.

## Stack

- **Backend:** Python, FastAPI (REST + WebSocket)
- **Frontend:** React, served locally, talks to the backend only
- **Traffic capture:** a mitmproxy-based helper process (Python), launched and managed by the backend
- **Runs entirely on localhost.** This is a local developer tool, not a hosted service — see `PRIVACY_AND_SECURITY.md` for what that implies.

## Layer diagram

```
┌───────────────────────────┐
│      React (browser)      │
└─────────────┬─────────────┘
              │  REST (snapshot / query)  +  WebSocket (live updates)
              ▼
┌───────────────────────────┐
│   FastAPI (transport only) │
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

- **React** renders whatever state the backend hands it and sends user actions (select process, start/stop monitoring, apply a filter) to the backend. It never talks to the OS, never runs a provider, and never contains redaction logic — everything it displays is already redacted by the time it arrives.
- **FastAPI** is transport only. Route handlers call into the Observation Engine and serialize the result; they do not call `lsof`, `psutil`, or mitmproxy directly, and contain no correlation logic of their own.
- **Observation Engine** is the correlation layer the whole project report is built around: it merges provider outputs, tracks connection lifecycle via snapshot diffing (see `DATA_MODEL.md`), attaches traffic to the right connection, applies the Observation Contract's status model to everything it emits, and is the single owner of in-memory state. Nothing else holds authoritative state.
- **Providers** each answer exactly one question and return typed data plus an explicit status (`OBSERVATION_CONTRACT.md`) — never partial data with silent gaps.
- **Session Store** persists Engine state to disk, and only ever receives already-redacted data.
- **Redactor** sits at two distinct checkpoints, not one: a *default display transform* (reversible — a user's "show anyway" can reveal a raw value for the current session only) and a *mandatory pre-persistence/export transform* (irreversible — nothing written to the Session Store or an export file skips this, regardless of what's toggled on screen). Conflating these two was flagged explicitly in the TODO's redaction tests; this document is where the distinction is made permanent.

## Dependency rules (enforce these in review, they're what keeps this clean)

- React components never construct or interpret raw provider data — only the API/WebSocket contract types the backend defines.
- FastAPI route handlers contain no provider or correlation logic: call the Engine, serialize, return.
- Providers never import FastAPI, the Engine, or each other.
- Nothing writes to the Session Store except through the Redactor's mandatory path.

## Error propagation

A provider returns an Observation Contract status. The Engine aggregates statuses across providers for a given connection/process. FastAPI serializes that status into the API/WebSocket payload's `status` field, always alongside (never instead of) whatever partial data is available. React renders the status explicitly — "permission denied," "unsupported," "stale" — and never infers any of those from an empty or missing field.

## Future provider replacement

`TrafficProvider`'s interface is the only thing that needs reimplementing if the capture strategy ever changes. Because this is now a Python-only stack, there's no cross-language boundary the way a future Swift + `NetworkExtension` path would have required — see `DECISIONS.md` for what that trade-off actually costs (the Network Extension upgrade path is effectively foreclosed by this stack choice, not just deferred).

## Local network surface — new since the FastAPI/React pivot

A native SwiftUI app has no network-facing surface at all. This architecture runs an actual HTTP/WebSocket server, even though it's only ever meant to serve your own browser. Non-negotiable defaults: bind to `127.0.0.1` only, never `0.0.0.0`; restrict CORS to the frontend's own origin; treat the port as something another local process could reach unless explicitly locked down. Full treatment in `PRIVACY_AND_SECURITY.md`.

## Explicitly not part of this architecture

Any modify/replay/inject capability anywhere in the stack. A FastAPI server reachable from anything other than localhost. Providers reaching into each other directly instead of through the Engine. Swappable-backend machinery for a provider that only has one implementation so far (see `DECISIONS.md` on `Flow` and provider abstraction timing).
