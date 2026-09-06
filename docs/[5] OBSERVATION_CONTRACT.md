# Observation Contract — Process Network Inspector

**v2 — revised to fix a category error:** the original version put `stale` and `unmatched` in the same flat enum as things a provider can determine about itself (`unavailable`, `permission_denied`, etc.). A provider doesn't know the expected polling cadence, so it can't determine staleness; a provider never sees another provider's output, so it can't determine a correlation mismatch. Both are Engine-derived judgments layered on top of provider-reported status, not provider states themselves. This version makes that boundary explicit.

## Two different kinds of status, one visible vocabulary

**Provider status** — what a provider can legitimately assert about its own call, in isolation:

```
observed | unavailable | permission_denied | unsupported | transient_failure
```

**Engine-derived status** — judgments only the Engine can make, because they require context no single provider has:

```
stale       (requires knowing the expected polling cadence)
unmatched   (requires comparing two providers' output against each other)
```

The full vocabulary a consumer of the API ever sees is the union of both — `observed | unavailable | permission_denied | unsupported | transient_failure | stale | unmatched` — but only the Engine is ever allowed to produce `stale` or `unmatched`. A `TrafficProviderStatus.UNMATCHED` or a provider unilaterally deciding its own output is stale is a bug, not a valid state — if code ever needs that, the correlation or freshness logic has leaked into the wrong layer.

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

`observed` and `stale` carry data alongside the status. The other five carry a `reason` (see `ObservationStatus` in `DATA_MODEL.md`) and no data — never a bare empty payload with no explanation.

## Applied per layer

**Process/socket layer**

```
processes: observed | permission_denied | unavailable         (provider-level)
sockets:   observed | permission_denied | unavailable | stale  (stale added by the Engine on top)
```

**DNS layer**

```
hostname: observed | unavailable                                 (provider-level)
          | unmatched (Engine — e.g. resolution succeeded for a connection that's already gone)
```

**Traffic layer** — the richest failure modes, because "no HTTP data" can mean any of:

```
HTTPS payload:
    observed          → payload available                                    (provider)
    unsupported        → QUIC / non-HTTP / other unsupported protocol        (provider)
    unavailable         → TrafficProvider helper isn't running                (provider)
    permission_denied  → the capability itself couldn't be granted           (provider)
    unmatched           → traffic was captured but not attached to a connection (Engine, from CorrelationEvidence)
```

## How this surfaces in the Tauri command/event contract

Every payload the Rust core sends — whether as an `invoke` command's response or a pushed event — carries an `ObservationStatus` (see `DATA_MODEL.md`) alongside its data, never instead of it:

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

A `null`/missing `data` field is only ever paired with `unavailable`, `permission_denied`, `unsupported`, or `unmatched` — never silently omitted with no status attached.

## The one rule this document exists to enforce

React never infers a status from the shape of the data (e.g. "the array is empty, so I'll show 'No connections'"). It renders whatever `status.state` the core sent, explicitly. If a view ever needs to guess what an empty response means, that's a sign a Tauri command is missing a status field, not a reason to add inference logic in the frontend. And if a provider's own code ever needs to set `stale` or `unmatched` on its own output, that's a sign correlation or freshness logic has leaked into the provider layer — move it back into the Engine.
