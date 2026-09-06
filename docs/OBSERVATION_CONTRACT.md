# Observation Contract — Process Network Inspector

Answers one question precisely: **what does this application mean when it says it observed something?** Without this document, "no data" ends up meaning five different things across the codebase, and the UI has no reliable way to distinguish them. Every provider, and the Engine, must resolve to exactly one of these statuses for every observation it makes — never a bare success/failure boolean, never a silently empty result.

## The canonical status set

| Status | Meaning | Example |
|---|---|---|
| `observed` | The data is present and current | A socket is open right now; a request/response was captured |
| `unavailable` | The capability isn't running or reachable | `TrafficProvider`'s helper process isn't started |
| `permission_denied` | macOS declined the required access | `lsof` couldn't see another user's process without elevation |
| `unsupported` | This kind of traffic is categorically out of reach for this provider | QUIC, a pinned-cert HTTPS connection, a non-HTTP TCP stream |
| `transient_failure` | A one-off error, expected to clear on the next attempt | A single polling cycle's `lsof` call timed out |
| `stale` | Last known value, but not confirmed as current | Polling stopped or lagged; connection state is from N seconds ago |
| `unmatched` | An observation exists but couldn't be attached to anything else | A captured HTTP flow whose PID/port couldn't be mapped to a known `NetworkConnection` (see Phase 0.3's correlation spike in `TODO.md`) |

`observed` and `stale` differ from the other five in an important way: they still carry data. The other five carry a reason and no data (or, for `stale`, the last-known data plus an explicit staleness flag) — never a bare empty payload.

## Applied per layer

**Process/socket layer**

```
processes: observed | permission_denied | unavailable
sockets:   observed | permission_denied | unavailable | stale
```

**DNS layer**

```
hostname: observed | unavailable | unmatched (rare — e.g. resolution succeeded but for a connection that's already gone)
```

**Traffic layer** — the layer with the richest failure modes, because "no HTTP data" can mean any of:

```
HTTPS payload:
    observed         → payload available
    unsupported       → QUIC / non-HTTP / other unsupported protocol
    unavailable        → TrafficProvider helper isn't running
    permission_denied → the capability itself couldn't be granted
    unmatched          → traffic was captured but not attached to a known connection
```

## How this surfaces in the API/WebSocket contract

Every payload the backend sends carries its status alongside its data, never instead of it:

```json
{
  "connection_id": "c-4821",
  "status": "stale",
  "last_updated": "2026-09-06T10:31:02Z",
  "data": { "remote_addr": "104.18.32.123", "remote_port": 443, "state": "ESTABLISHED" }
}
```

A `null`/missing `data` field is only ever paired with `unavailable`, `permission_denied`, `unsupported`, or `unmatched` — never silently omitted with no status attached.

## The one rule this document exists to enforce

React never infers a status from the shape of the data (e.g. "the array is empty, so I'll show 'No connections'"). It renders whatever `status` the backend sent, explicitly. If a view ever needs to guess what an empty response means, that's a sign a backend endpoint is missing a status field, not a reason to add inference logic in the frontend.
