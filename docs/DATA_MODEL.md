# Data Model — Process Network Inspector

**v2 — revised to fix a real inconsistency:** a provider cannot return an object typed `NetworkConnection` while also being told never to set that object's `lifecycle_state`/`first_seen`/`last_seen` fields. This version splits provider-level raw observations from Engine-owned domain state, formalizes how correlation actually happens, and formalizes status as its own type instead of a bare field. See `DECISIONS.md` for why.

## Entity relationships

```
ProcessObservation (provider) ──> ProcessInfo (Engine-owned, adds status)
   │
   └── NetworkConnection (Engine-owned domain state)
          │  built from ──> SocketObservation (provider-owned raw fact)
          │
          ├── HostnameObservation (1:many — one per source)
          │
          └── TrafficEvent (1:many)
                   │
                   ├── HTTPRequest (redacted, storage-safe)
                   │        built from ──> RawHTTPRequest (transient, in-memory only)
                   └── HTTPResponse (redacted, storage-safe)
                            built from ──> RawHTTPResponse (transient, in-memory only)
          │
          └── Flow (from v0.3 onward — wraps a connection + its observations)

CorrelationEvidence ──> (Engine correlation) ──> NetworkConnection.connection_id
```

## The core rule this document enforces

**Providers report observations. The Engine creates domain state.** Every type below is either a *provider-owned observation* (immutable, no lifecycle/identity fields, exactly what was seen in one call) or *Engine-owned domain state* (carries `connection_id`, lifecycle, timestamps — never constructed by a provider). If a provider ever needs to populate an Engine-owned field, that's a sign the type boundary is wrong, not a sign the field should become optional.

## `ProcessObservation` (provider-owned)

Exactly what `ProcessProvider` saw in one call — no status judgment, same discipline as `SocketObservation` below. An earlier version of this document declared the whole `ProcessInfo` type "provider-owned" while also saying its `status` field was Engine-set — the identical contradiction already fixed for sockets, just missed here. This split closes it.

| Field | Type | Required | Notes |
|---|---|---|---|
| pid | int | yes | |
| name | str | yes | |
| executable_path | str | yes | |
| cpu_percent | float | no | best-effort |
| memory_bytes | int | no | best-effort |

## `ProcessInfo` (Engine-owned)

Built from a `ProcessObservation` plus Engine-derived judgment. A provider never constructs this type.

| Field | Type | Required | Notes |
|---|---|---|---|
| pid | int | yes | copied from the observation |
| name | str | yes | |
| executable_path | str | yes | |
| cpu_percent | float | no | |
| memory_bytes | int | no | |
| status | str | yes | `running` / `exited` — Engine-derived |

## `SocketSnapshot` (provider-owned)

Immutable, point-in-time output of `SocketProvider`. Never mutated after creation.

| Field | Type | Notes |
|---|---|---|
| timestamp | datetime | when this snapshot was taken |
| observations | list[SocketObservation] | raw facts, no identity or lifecycle yet |

## `SocketObservation` (provider-owned — NOT `NetworkConnection`)

Exactly what `SocketProvider` saw for one socket in one snapshot. No `connection_id`, no `lifecycle_state`, no `first_seen`/`last_seen` — those don't exist yet at this point, because identity and lifecycle are things the Engine derives by diffing observations across snapshots, not things a single snapshot can know.

| Field | Type | Required | Notes |
|---|---|---|---|
| pid | int | yes | |
| protocol | Literal["tcp","udp"] | yes | |
| local_addr | str | yes | |
| local_port | int | yes | |
| remote_addr | str | no | absent for LISTEN sockets |
| remote_port | int | no | absent for LISTEN sockets |
| state | str | yes | LISTEN / ESTABLISHED / etc., as reported this instant |
| bytes_sent | int | no | provider-dependent — see `OBSERVATION_CONTRACT.md` |
| bytes_received | int | no | provider-dependent |

## `NetworkConnection` (Engine-owned domain state)

Constructed and owned exclusively by the Observation Engine, by matching `SocketObservation`s across successive snapshots. A provider must never construct this type.

| Field | Type | Required | Notes |
|---|---|---|---|
| connection_id | str | yes | an **Engine session identity, not an OS-level socket identity** — assigned by the Engine the first time an observation is matched, stable across polls only as long as the Engine's matching heuristic holds. Addr/port tuples are not a reliable identity on their own (port reuse, rapid close/reopen, IPv4/IPv6 representation differences), so matching is necessarily heuristic when a provider exposes insufficient identity information. **Rule: prefer creating a new connection over incorrectly merging two distinct ones.** A false split just looks redundant in the UI; a false merge silently corrupts the timeline by attributing one connection's events to another's history — those are not equally bad failure modes. See the identity-matching rule in `TODO.md` Phase 0.1. |
| pid | int | yes | copied from the matched `SocketObservation` |
| protocol, local_addr, local_port, remote_addr, remote_port, state | — | yes/no as above | copied from the latest matched `SocketObservation` |
| lifecycle_state | Literal["discovered","active","closed","expired"] | yes | Engine-derived — see the rule below. Never set by a provider. |
| first_seen | datetime | yes | Engine-derived, from the first snapshot this connection appeared in |
| last_seen | datetime | yes | Engine-derived, from the most recent snapshot it appeared in |
| status | ObservationStatus | yes | see below |

### The `closed` vs `expired` rule — must be enforced in the diffing logic, not left implicit

A connection missing from the current snapshot is **not** automatically `closed`. Between two polls, a missing connection could mean it actually closed, or that `lsof`/`SocketProvider` had a transient failure, or that the process disappeared, or that the poll was simply delayed. The Engine cannot always distinguish these, and must not guess:

- **`closed`** — there is positive evidence the connection terminated. A socket simply missing from a subsequent `lsof`/`psutil` snapshot is **not**, by itself, positive evidence of closure — polling has no distinct "closed" signal separate from "absent," so this case is indistinguishable from `expired` with the tools Phase 0.1–0.2 actually have. `closed` becomes reachable once (if ever) a provider adds a genuine close-event source (an OS-level notification, for instance) — until then, expect it to be rare-to-unreachable in practice.
- **`expired`** — the connection was previously observed, is no longer observable, and the Engine cannot prove it actually closed. This is the default, and — practically, for the polling-only providers in Phase 0.1–0.2 — the outcome you should expect essentially every connection to reach. `discovered → active → expired` is the normal lifecycle for this phase; `discovered → active → closed` is not something the current providers can honestly produce.

Treat `expired` as the common case and `closed` as the case requiring actual evidence — not the other way around.

## `CorrelationEvidence` (provider-owned, traffic-side input to correlation)

What `TrafficProvider` actually has available when it captures a flow — this is *not* a `connection_id`, because the traffic provider has no knowledge of the Engine's internal identity scheme. This is the formal input to the Phase 0.3 correlation spike in `TODO.md`.

| Field | Type | Required | Notes |
|---|---|---|---|
| pid | int | no | available if the capture is process-scoped |
| protocol | str | no | |
| local_addr | str | no | |
| local_port | int | no | |
| remote_addr | str | no | |
| remote_port | int | no | |
| hostname | str | no | e.g. from SNI or the `Host` header |
| timestamp | datetime | yes | |
| source | str | yes | which provider/mechanism produced this evidence |

The Engine matches `CorrelationEvidence` against known `NetworkConnection`s. A confident match assigns the evidence's flow to that `connection_id`. An insufficiently confident match produces `unmatched` (see `OBSERVATION_CONTRACT.md`) — the flow is never attached to a guessed connection, and a `connection_id` is never fabricated to force a match.

## `RawHTTPRequest` / `RawHTTPResponse` (transient, in-memory only — never persisted)

What `TrafficProvider` actually captured, before any redaction, and the source a "show anyway" UI action reveals from. **Highly-sensitive fields (`Authorization`, API keys, passwords, tokens, credentials — see `PRIVACY_AND_SECURITY.md`'s classification) are stripped even here, at capture time, and never exist in raw form anywhere, including memory.** Only the "potentially sensitive" tier (URLs, query params, bodies, cookies, non-auth headers) is held raw transiently.

**Lifetime rule, not left implicit:** these objects exist only as long as the live monitoring session that captured them, not indefinitely just because the process/session object itself stays alive. When a session stops, its `Raw*` objects are destroyed, not merely dereferenced-and-hoped-for-GC. A long-running session must not be allowed to accumulate raw sensitive data without bound — enforce, at minimum: a maximum body-preview size per object, a cap on the number of retained raw requests/responses, and a session-level memory budget with an eviction policy once it's hit. None of this needs full implementation in Phase 0.1, but the rule is established here so a later phase doesn't have to retrofit it onto data that's already been designed to linger.

## `HTTPRequest` / `HTTPResponse` (Engine-owned, redacted — the only form that reaches storage, export, or the API)

| Field (`HTTPRequest`) | Type | Required |
|---|---|---|
| connection_id | str | yes |
| method | str | yes |
| host | str | yes |
| path | str | yes |
| headers | dict[str, str] | yes — redacted per `PRIVACY_AND_SECURITY.md`'s two-tier model |
| body_preview | Optional[str] | no — truncated per the size limit in `PRIVACY_AND_SECURITY.md` |
| timestamp | datetime | yes |

| Field (`HTTPResponse`) | Type | Required |
|---|---|---|
| request_id | str | yes |
| status | int | yes |
| headers | dict[str, str] | yes — redacted |
| body_preview | Optional[str] | no |
| duration_ms | float | yes |

This is the only representation that FastAPI ever serializes to the frontend by default, and the only one the Session Store ever writes. A "show anyway" action operates on the paired `RawHTTPRequest`/`RawHTTPResponse` still held in memory for the current session — it never changes what gets persisted.

## `HostnameObservation` (provider-owned)

Kept as separate rows per source rather than one flattened `resolved_host` string, because reverse DNS, TLS SNI, and the HTTP `Host` header can legitimately disagree.

| Field | Type | Required | Notes |
|---|---|---|---|
| connection_id | str | yes | |
| source | Literal["reverse_dns","sni","http_host"] | yes | |
| hostname | str | yes | |
| confidence | float (0–1) | yes | e.g. reverse DNS on a shared/CDN IP gets lower confidence than an HTTP `Host` header |

## `TrafficEvent` (Engine-owned)

An earlier version used a single opaque `payload_ref: str` field whose meaning (connection ID? request ID? response ID?) depended on reading `type` first — a real ambiguity, not just a style issue, especially once the timeline needs to query these. Replaced with explicit, individually-optional reference fields:

| Field | Type | Notes |
|---|---|---|
| event_id | str | yes |
| timestamp | datetime | yes |
| type | Literal["connection_opened","connection_closed","request","response"] | yes |
| connection_id | Optional[str] | populated for every event type |
| request_id | Optional[str] | populated for `request`/`response` events |
| response_id | Optional[str] | populated for `response` events only |

## `ObservationStatus` (Engine-owned — see `OBSERVATION_CONTRACT.md` for the full status vocabulary)

Attached to every domain object the Engine emits, rather than status being a loose field scattered inconsistently across models.

| Field | Type | Required | Notes |
|---|---|---|---|
| state | ObservationState (enum, see `OBSERVATION_CONTRACT.md`) | yes | |
| observed_at | datetime | yes | when this status was determined |
| last_successful_at | Optional[datetime] | no | set when `state == stale` |
| reason | Optional[str] | no | human-readable detail for denied/unsupported/failure states |
| provider | Optional[str] | no | which provider/layer this status originates from, when relevant |

## `Flow` (type defined now, wired in from v0.3 — see `DECISIONS.md`)

A protocol-agnostic wrapper around a connection plus its attached observations, so HTTP is one kind of observation attached to a flow rather than a parallel top-level entity. This is what makes adding QUIC/WebSocket/TLS metadata later an addition instead of a rewrite.

**`NetworkConnection` and `Flow` are not interchangeable terms, even though both end up holding hostname/request/response references:** `NetworkConnection` represents an observed transport-level connection (a socket); `Flow` represents the higher-level logical conversation associated with that connection. Keep this distinction explicit once `Flow` is wired in at v0.3 — don't let the two grouping mechanisms silently merge into one in code just because their attached fields look similar today.

| Field | Type | Notes |
|---|---|---|
| flow_id | str | |
| connection_id | str | |
| hostname_observations | list[HostnameObservation] | |
| requests | list[HTTPRequest] | |
| responses | list[HTTPResponse] | |

## `ObservationCapabilities` — a distinct concept from `ObservationStatus`, documented now, implemented later

`ObservationStatus` describes one observation's outcome right now (this specific connection's traffic is `unsupported`). `ObservationCapabilities` describes what a provider can *ever* observe, independent of any single attempt — the data behind the report's "Observation Capabilities" panel (process info ✓, HTTPS payload ⚠ Limited, raw packet data ✕). Conflating the two makes the API messy once Phase 0.3/0.4 introduce real per-connection variability: a provider can have the *capability* `http_metadata: available` while a *specific* connection's status is `unsupported` because that one connection happens to be QUIC. Not implemented in Phase 0.1 — documented here so the two concepts don't get merged into one field later:

| Field | Values | Notes |
|---|---|---|
| process | available / unavailable | |
| sockets | available / unavailable | |
| dns | available / unavailable | |
| http_metadata | available / limited / unsupported | |
| https_metadata | available / limited / unsupported | |
| request_body | available / limited / unsupported | |
| response_body | available / limited / unsupported | |

## Rules this document enforces

A provider never constructs `NetworkConnection` or `ProcessInfo` — only `SocketObservation` or `ProcessObservation`. A hostname is never assumed canonical — always carry `source` and `confidence`. `connection_id` is an Engine session identity, not an OS-level identity, assigned exclusively by the Engine's correlation step from `CorrelationEvidence` on the traffic side and snapshot-diffing on the socket side — nothing else invents one, and the Engine prefers a false split over a false merge when matching confidence is insufficient. `lifecycle_state`, `first_seen`, `last_seen` are Engine-owned; `closed` requires positive evidence (rare in practice for polling-only providers), `expired` is the default absent that evidence. Highly-sensitive fields never exist in raw form anywhere, including in `RawHTTPRequest`/`RawHTTPResponse`; potentially-sensitive fields may exist raw transiently in memory, bounded and evicted per session, but never in anything persisted or exported. `bytes_sent`/`bytes_received` are optional everywhere they appear.
