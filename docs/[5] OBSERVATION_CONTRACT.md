# Observation Contract — Process Network Inspector

**v3 — fixes a self-contradiction and closes the envelope/threshold gaps the pre-implementation Round 2 audit found.** v2 fixed the category error between provider-determinable and Engine-derived status. This version fixes: a direct contradiction between two lines of this document about whether `transient_failure` carries data (which also contradicted the 4th mandatory test); a phantom `TrafficProviderStatus` reference that named a type `DATA_MODEL.md` never defined (now `ProviderStatus`); per-layer status lists that omitted statuses Phase 0.1 tasks and tests require; an undefined `stale` threshold; an unstated whole-layer failure envelope; and an unstated field-level-absence exception to this document's own no-inference rule. See `DECISIONS.md` ADR-014.

## Two different kinds of status, one visible vocabulary

**Provider status** (`ProviderStatus` in `DATA_MODEL.md`) — what a provider can legitimately assert about its own call, in isolation:

```
observed | unavailable | permission_denied | unsupported | transient_failure
```

**Engine-derived status** — judgments only the Engine can make, because they require context no single provider has:

```
stale       (requires knowing the expected polling cadence)
unmatched   (requires comparing two providers' output against each other)
```

The full vocabulary a consumer of the API ever sees, in `ObservationStatus.state`, is the union of both — `observed | unavailable | permission_denied | unsupported | transient_failure | stale | unmatched` — but only the Engine is ever allowed to produce `stale` or `unmatched`. A provider unilaterally setting `stale` or `unmatched` on its own `ProviderStatus` is a bug, not a valid state — if code ever needs that, the correlation or freshness logic has leaked into the wrong layer.

## Meaning of each status

| Status | Who determines it | Meaning | Example |
|---|---|---|---|
| `observed` | Provider | The data is present and current | A socket is open right now; a request/response was captured |
| `unavailable` | Provider | The capability isn't running or reachable | `TrafficProvider`'s helper process isn't started |
| `permission_denied` | Provider | macOS declined the required access | `lsof` couldn't see another user's process without elevation |
| `unsupported` | Provider | This kind of traffic is categorically out of reach for this provider | QUIC, a pinned-cert HTTPS connection, a non-HTTP TCP stream |
| `transient_failure` | Provider | A one-off error, expected to clear on the next attempt | A single polling cycle's `lsof` call timed out |
| `stale` | Engine | Last known value, not confirmed current, because too much time has passed since the last successful observation | Polling stopped or lagged; connection state is from N seconds ago |
| `unmatched` | Engine | An observation exists but couldn't be attached to anything else with sufficient confidence | Captured `CorrelationEvidence` whose PID/port couldn't be mapped to a known `NetworkConnection` (see the Phase 0.3 correlation spike in `TODO.md`) |

`observed`, `stale`, and `transient_failure` carry data alongside the status — `transient_failure` carries the *last known* values, unchanged, not fresh data (there is none). `unavailable`, `permission_denied`, `unsupported`, and `unmatched` carry a `reason` (see `ObservationStatus` in `DATA_MODEL.md`) and no data — never a bare empty payload with no explanation. (An earlier version of this document said only `observed`/`stale` carry data, which directly contradicted the 4th mandatory integration test in `TESTING_STRATEGY.md` — that test requires `transient_failure` to accompany the previous snapshot's still-tracked connections, which is only possible if it carries data. This line now matches the test.)

## Staleness threshold

The Engine marks an observation `stale` when `now - status.last_successful_at > 3 × the configured poll interval`. The default poll interval (`TODO.md` Phase 0.2) is 2 seconds, so 6 seconds by default once polling exists. **In Phase 0.1**, where refresh is manual and no polling interval is configured yet, the threshold is a flat `now - last_successful_at > 30s` — a manually-refreshed view is `observed` immediately after a successful refresh, and `stale` only if the user leaves it open without refreshing past that window. This makes `stale` a reachable state in Phase 0.1's frontend (`TODO.md`), not just a listed-but-unreachable one.

## Applied per layer

**Process/socket layer**

```
processes: observed | permission_denied | unavailable | transient_failure | stale   (stale added by the Engine on top, same as sockets)
sockets:   observed | permission_denied | unavailable | transient_failure | stale   (stale added by the Engine on top)
```

**DNS layer**

```
hostname: observed | unavailable | permission_denied | transient_failure   (provider-level)
          | unmatched (Engine — e.g. resolution succeeded for a connection that's already gone)
```

**Traffic layer** — the richest failure modes, because "no HTTP data" can mean any of:

```
HTTPS payload:
    observed          → payload available                                    (provider)
    unsupported        → QUIC / non-HTTP / other unsupported protocol        (provider)
    unavailable         → TrafficProvider helper isn't running                (provider)
    permission_denied  → the capability itself couldn't be granted           (provider)
    transient_failure   → a one-off capture error, expected to clear         (provider)
    unmatched           → traffic was captured but not attached to a connection (Engine, from CorrelationEvidence)
```

(An earlier version of this document omitted `transient_failure` from every per-layer list above, and `permission_denied` from the DNS layer, even though `TODO.md` Phase 0.1 requires providers to implement `transient_failure` and the 4th mandatory test requires `SocketProvider` to produce it. Every provider may return any of the five `ProviderStatus` values regardless of layer — the lists above name what's expected in practice for that layer, not an exhaustive allowlist.)

## Field-level absence

`ObservationStatus` is attached per *object* (a `NetworkConnection`, a `ProcessInfo`), not per *field*. Some fields on an otherwise-`observed` object are themselves optional and provider-dependent — `SocketObservation.bytes_sent`/`bytes_received`, `ProcessObservation.cpu_percent`/`memory_bytes` (see `DATA_MODEL.md`). A `None` on one of these fields means *this provider does not supply this field*, and is the one permitted exception to the no-inference rule below: React renders it as an explicit "not reported" affordance (e.g. an em-dash), **never** as `0` or blank. Field-level absence never changes the object's own `ObservationStatus` — an object can be fully `observed` with some optional fields absent.

## How this surfaces in the Tauri command/event contract

Every payload the Rust core sends — whether as an `invoke` command's response or a pushed event — carries an `ObservationStatus` (see `DATA_MODEL.md`) alongside its data, never instead of it. **List-returning commands (`get_processes`, `get_connections(pid)`) carry two levels of status, not one:** an outer, call-level `ObservationStatus` describing the query as a whole (e.g. `permission_denied` if the socket layer itself couldn't be reached for this PID), and each item in the returned list carries its own `ObservationStatus` the same way a single-object response does. The outer status is what lets a whole-layer failure render as "permission denied" instead of an empty list — see `ARCHITECTURE.md`'s "one rule" note on why an empty array must never be inferred as "nothing to report."

Single-object example (an event payload):

```json
{
  "connection_id": "c-4821",
  "status": {
    "state": "stale",
    "observed_at": "2026-09-06T10:31:12Z",
    "last_successful_at": "2026-09-06T10:31:02Z"
  },
  "data": { "remote_addr": "104.18.32.123", "remote_port": 443, "state": "ESTABLISHED" }
}
```

List-returning command example (`get_connections(pid)`, denied case):

```json
{
  "status": { "state": "permission_denied", "observed_at": "2026-09-06T10:31:12Z", "reason": "other user's process" },
  "data": null
}
```

List-returning command example (`get_connections(pid)`, success case — outer status `observed`, one item itself `stale`):

```json
{
  "status": { "state": "observed", "observed_at": "2026-09-06T10:31:12Z" },
  "data": [
    { "connection_id": "c-4821", "status": { "state": "stale", "observed_at": "...", "last_successful_at": "..." }, "remote_addr": "104.18.32.123", "remote_port": 443, "state": "ESTABLISHED" }
  ]
}
```

A `null`/missing `data` field is only ever paired with `unavailable`, `permission_denied`, `unsupported`, or `unmatched` — never silently omitted with no status attached.

## The one rule this document exists to enforce

React never infers a status from the shape of the data (e.g. "the array is empty, so I'll show 'No connections'"). It renders whatever `status.state` the core sent — at both the outer (call) and inner (per-item) level for list-returning commands — explicitly. If a view ever needs to guess what an empty response means, that's a sign a Tauri command is missing a status field, not a reason to add inference logic in the frontend. And if a provider's own code ever needs to set `stale` or `unmatched` on its own output, that's a sign correlation or freshness logic has leaked into the provider layer — move it back into the Engine.
