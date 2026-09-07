# Data Model — Process Network Inspector

**v3 — closes the provider-status gap and finishes the split ADR-011/012 started.** v2 split provider-level raw observations from Engine-owned domain state for sockets and processes, but left `ObservationStatus` itself Engine-owned with no provider-side counterpart, left `HostnameObservation` provider-owned while requiring an Engine-owned `connection_id`, and left several other type-level contradictions the pre-implementation Round 2 audit found before any of them became code. This version adds `ProviderStatus`, splits `HostnameObservation`/`ResolvedHostname` the same way sockets and processes were already split, fixes `HTTPRequest`/`HTTPResponse`'s broken identity fields, and pins down several rules that were previously only policy, not mechanism. See `DECISIONS.md` ADR-014 for the full list and why.

## Entity relationships

```
ProcessObservation (provider) ──> ProcessSnapshot (provider, adds ProviderStatus)
                                        │
                                        ▼
                                   ProcessInfo (Engine-owned, adds ObservationStatus)
   │
   └── NetworkConnection (Engine-owned domain state)
          │  built from ──> SocketObservation (provider-owned raw fact)
          │                      via SocketSnapshot (provider, adds ProviderStatus)
          │
          ├── HostnameObservation (provider) ──> ResolvedHostname (Engine-owned, 1:many — one per source)
          │
          └── TrafficEvent (1:many)
                   │
                   ├── HTTPRequest (redacted, storage-safe)
                   │        built from ──> RawHTTPRequest (transient, in-memory only)
                   └── HTTPResponse (redacted, storage-safe)
                            built from ──> RawHTTPResponse (transient, in-memory only)
          │
          └── Flow (from Phase 0.5 onward — wraps a connection + its observations)

CorrelationEvidence ──> (Engine correlation) ──> NetworkConnection.connection_id

ProviderStatus (provider-owned) ──> (Engine wraps/promotes) ──> ObservationStatus (Engine-owned)
```

## The core rule this document enforces

**Providers report observations. The Engine creates domain state.** Every type below is either a *provider-owned observation* (immutable, no lifecycle/identity fields, exactly what was seen in one call) or *Engine-owned domain state* (carries `connection_id`, lifecycle, timestamps — never constructed by a provider). If a provider ever needs to populate an Engine-owned field, that's a sign the type boundary is wrong, not a sign the field should become optional. This now applies to status the same way it applies to everything else: a provider constructs `ProviderStatus`; only the Engine constructs `ObservationStatus`, by wrapping one or more `ProviderStatus` values and optionally promoting to `stale`/`unmatched`.

## Notation

Types below are given in Rust (per `DECISIONS.md` ADR-013 — this document's types were always designed to be language-agnostic, and only the notation changed, not the shape): `Option<T>` for an optional field, `Vec<T>` for a list, enums for the fixed-value fields that were previously written as `Literal[...]`, `DateTime<Utc>` (`chrono`) for timestamps, and `HashMap<String, String>` for header-shaped maps. Ports use `u16` (their actual range); PIDs and byte counts use unsigned integer types since neither is ever negative.

**Serde convention:** every enum that crosses the IPC boundary is serialized `#[serde(rename_all = "snake_case")]` — e.g. `ObservationState::PermissionDenied` → `"permission_denied"`, `NetworkConnection.lifecycle_state`'s `Active` → `"active"`. Prose throughout these documents uses the wire form (`permission_denied`, `active`); Rust code below uses the variant form (`PermissionDenied`, `Active`). This applies to every enum in this document, not just `ObservationState`.

## `ProviderStatus` (provider-owned)

What a provider can legitimately assert about its own call, in isolation — the provider-level half of `OBSERVATION_CONTRACT.md`'s two-kinds-of-status split. A provider constructs and returns this; only the Engine constructs `ObservationStatus`.

| Field | Type | Required | Notes |
|---|---|---|---|
| state | ProviderState (enum: Observed, Unavailable, PermissionDenied, Unsupported, TransientFailure) | yes | the five provider-determinable statuses — see `OBSERVATION_CONTRACT.md`; a provider never produces `Stale` or `Unmatched` |
| observed_at | DateTime\<Utc\> | yes | when the provider produced this status |
| reason | Option\<String\> | no | human-readable detail for denied/unsupported/failure states — display/log only, see the note on `ObservationStatus.reason` below; the same rule applies here |

## `ProcessObservation` (provider-owned)

Exactly what `ProcessProvider` saw in one call — no status judgment, same discipline as `SocketObservation` below. An earlier version of this document declared the whole `ProcessInfo` type "provider-owned" while also saying its `status` field was Engine-set — the identical contradiction already fixed for sockets, just missed here. This split closes it.

| Field | Type | Required | Notes |
|---|---|---|---|
| pid | u32 | yes | |
| name | String | yes | |
| executable_path | String | yes | |
| cpu_percent | f32 | no | best-effort |
| memory_bytes | u64 | no | best-effort |

## `ProcessSnapshot` (provider-owned)

Immutable, point-in-time output of `ProcessProvider` — mirrors `SocketSnapshot` below so both providers carry a status the same way. Never mutated after creation.

| Field | Type | Required | Notes |
|---|---|---|---|
| timestamp | DateTime\<Utc\> | yes | when this snapshot was taken |
| observations | Vec\<ProcessObservation\> | yes | raw facts |
| status | ProviderStatus | yes | e.g. `permission_denied` when enumerating other users' processes without elevation |

## `ProcessInfo` (Engine-owned)

Built from a `ProcessObservation` plus Engine-derived judgment. A provider never constructs this type.

| Field | Type | Required | Notes |
|---|---|---|---|
| pid | u32 | yes | copied from the observation |
| name | String | yes | |
| executable_path | String | yes | |
| cpu_percent | f32 | no | |
| memory_bytes | u64 | no | |
| process_state | enum { Running, Exited } | yes | Engine-derived — renamed from `status` to free that name for the standard `ObservationStatus` meaning below, matching `NetworkConnection.lifecycle_state`'s naming |
| status | ObservationStatus | yes | this process's own observation status (e.g. `permission_denied` if `ProcessSnapshot.status` was denied) — distinct from `process_state`, which is about whether the process is running, not whether it was observable |
| active_connection_count | Option\<u32\> | no | Engine-derived: the number of this PID's connections currently in `lifecycle_state ∈ {discovered, active}`. `None` when the socket layer's status for this PID is not `observed` — rendered as "—", never as `0` (see the field-level-absence rule in `OBSERVATION_CONTRACT.md`) |

## `SocketSnapshot` (provider-owned)

Immutable, point-in-time output of `SocketProvider`. Never mutated after creation.

| Field | Type | Required | Notes |
|---|---|---|---|
| timestamp | DateTime\<Utc\> | yes | when this snapshot was taken |
| observations | Vec\<SocketObservation\> | yes | raw facts, no identity or lifecycle yet |
| status | ProviderStatus | yes | e.g. `transient_failure` for one failed polling cycle — see the 4th mandatory test in `TESTING_STRATEGY.md` |

## `SocketObservation` (provider-owned — NOT `NetworkConnection`)

Exactly what `SocketProvider` saw for one socket in one snapshot. No `connection_id`, no `lifecycle_state`, no `first_seen`/`last_seen` — those don't exist yet at this point, because identity and lifecycle are things the Engine derives by diffing observations across snapshots, not things a single snapshot can know.

| Field | Type | Required | Notes |
|---|---|---|---|
| pid | u32 | yes | |
| protocol | enum { Tcp, Udp } | yes | |
| local_addr | String | yes | |
| local_port | u16 | yes | |
| remote_addr | Option\<String\> | no | absent for LISTEN sockets |
| remote_port | Option\<u16\> | no | absent for LISTEN sockets |
| state | String | yes | LISTEN / ESTABLISHED / etc., as reported this instant |
| bytes_sent | Option\<u64\> | no | provider-dependent — see the field-level-absence rule in `OBSERVATION_CONTRACT.md` |
| bytes_received | Option\<u64\> | no | provider-dependent — see the field-level-absence rule in `OBSERVATION_CONTRACT.md` |

## `NetworkConnection` (Engine-owned domain state)

Constructed and owned exclusively by the Observation Engine, by matching `SocketObservation`s across successive snapshots. A provider must never construct this type.

| Field | Type | Required | Notes |
|---|---|---|---|
| connection_id | String | yes | an **Engine session identity, not an OS-level socket identity** — assigned by the Engine the first time an observation is matched, stable across polls only as long as the Engine's matching heuristic holds. Addr/port tuples are not a reliable identity on their own (port reuse, rapid close/reopen, IPv4/IPv6 representation differences), so matching is necessarily heuristic when a provider exposes insufficient identity information. **Rule: prefer creating a new connection over incorrectly merging two distinct ones.** A false split just looks redundant in the UI; a false merge silently corrupts the timeline by attributing one connection's events to another's history — those are not equally bad failure modes. See the matching rule immediately below. |
| pid | u32 | yes | copied from the matched `SocketObservation` |
| protocol, local_addr, local_port, remote_addr, remote_port, state | — | yes/no as above | copied from the latest matched `SocketObservation` |
| lifecycle_state | enum { Discovered, Active, Closed, Expired } | yes | Engine-derived — see the rules below. Never set by a provider. |
| first_seen | DateTime\<Utc\> | yes | Engine-derived, from the first snapshot this connection appeared in |
| last_seen | DateTime\<Utc\> | yes | Engine-derived, from the most recent snapshot it appeared in |
| status | ObservationStatus | yes | see below |

### Connection-identity matching rule (Phase 0.1)

Two `SocketObservation`s match — i.e. are treated as the same `NetworkConnection` across polls — **iff** `(pid, protocol, local_addr, local_port, remote_addr, remote_port)` are all equal to a tracked connection's most recently matched observation, **and** that connection appears in the immediately preceding *successful* snapshot (`SocketSnapshot.status.state == observed`). `state` does not participate in matching — a connection legitimately changes state (e.g. `SYN_SENT` → `ESTABLISHED`) without becoming a different connection.

If a single new observation matches more than one currently-tracked connection equally well, **no match is made** and a new connection is created — per the split-over-merge policy above, an ambiguous match is treated the same as no match.

A connection absent from one successful snapshot and reappearing later with the same tuple is a **new** connection, not a resumption of the old one — port reuse after a real close is more likely than a poll simply missing a live socket for one cycle.

Phase 0.1 uses no numeric confidence score for this step — there is no `confidence` field on `SocketObservation`/`NetworkConnection`. "Insufficient confidence" (`TODO.md` Phase 0.1) reduces here to "not an exact tuple match." (This is distinct from `OBSERVATION_CONTRACT.md`'s `unmatched` status, which is about traffic-to-connection correlation via `CorrelationEvidence`, not this socket-to-connection matching step — that one *does* need a confidence judgment, made by the Engine at correlation time, not here.)

### The `closed` vs `expired` rule — must be enforced in the diffing logic, not left implicit

A connection missing from the current snapshot is **not** automatically `closed`. Between two polls, a missing connection could mean it actually closed, or that `SocketProvider` had a transient failure, or that the process disappeared, or that the poll was simply delayed. The Engine cannot always distinguish these, and must not guess:

- **`closed`** — there is positive evidence the connection terminated. A socket simply missing from a subsequent system-API/`SocketProvider` snapshot is **not**, by itself, positive evidence of closure. The one positive closure signal available to the polling-only providers in this phase is **process exit**: when `ProcessProvider` confirms a PID no longer exists (`ProcessSnapshot.status.state == observed` but the PID is absent from `observations`), that PID's connections transition to `closed`, not `expired` — the kernel has definitively closed them. `closed` becomes reachable through any other path once (if ever) a provider adds a genuine close-event source (an OS-level notification, for instance) — until then, process exit is the only route to it, and expect it to be otherwise rare-to-unreachable.
- **`expired`** — the connection was previously observed, is no longer observable, and the Engine cannot prove it actually closed (i.e. its process is still running, or process status itself is unknown). This is the default, and — practically, for the polling-only providers in Phase 0.1–0.2 — the outcome you should expect essentially every connection to reach absent the process-exit signal above. `discovered → active → expired` is the normal lifecycle for this phase; `discovered → active → closed` happens only via the process-exit path.

**Only a successful snapshot's absence counts as "no longer observable."** A `SocketSnapshot` carrying `status.state ∈ {transient_failure, unavailable, permission_denied}` is not evidence of anything about any connection's lifecycle. On such a snapshot, the Engine leaves every previously-tracked connection's `lifecycle_state` and `last_seen` unchanged and updates only its `status` (reflecting `transient_failure` or `stale` as appropriate — see the 4th mandatory test in `TESTING_STRATEGY.md`).

Treat `expired` as the common case and `closed` as the case requiring actual evidence — not the other way around.

### The `discovered` vs `active` transition

- **`discovered`** — seen in exactly one snapshot so far; identity not yet corroborated by a second observation.
- **`active`** — matched in at least two consecutive successful snapshots, **or** observed once with socket `state == ESTABLISHED`. A `LISTEN` socket follows the same rule as any other — listening is a legitimate active state, not a special case.

`ConnectionOpened` `TrafficEvent`s (`TODO.md` Phase 0.2) are emitted on the `discovered → active` transition, not on first sight — a connection that's `discovered` for exactly one poll and then disappears never generates an opened event.

## `CorrelationEvidence` (provider-owned, traffic-side input to correlation)

What `TrafficProvider` actually has available when it captures a flow — this is *not* a `connection_id`, because the traffic provider has no knowledge of the Engine's internal identity scheme. This is the formal input to the Phase 0.3 correlation spike in `TODO.md`.

| Field | Type | Required | Notes |
|---|---|---|---|
| pid | Option\<u32\> | no | available if the capture is process-scoped |
| protocol | Option\<String\> | no | |
| local_addr | Option\<String\> | no | |
| local_port | Option\<u16\> | no | |
| remote_addr | Option\<String\> | no | |
| remote_port | Option\<u16\> | no | |
| hostname | Option\<String\> | no | e.g. from SNI or the `Host` header |
| timestamp | DateTime\<Utc\> | yes | |
| source | String | yes | which provider/mechanism produced this evidence |

The Engine matches `CorrelationEvidence` against known `NetworkConnection`s. A confident match assigns the evidence's flow to that `connection_id`. An insufficiently confident match produces `unmatched` (see `OBSERVATION_CONTRACT.md`) — the flow is never attached to a guessed connection, and a `connection_id` is never fabricated to force a match. The `evidence` that produced (or failed to produce) a match is retained on the resulting `HTTPRequest` — see below.

## `RawHTTPRequest` / `RawHTTPResponse` (provider-owned, transient, in-memory only — never persisted)

What `TrafficProvider` actually captured — returned directly from its trait method, after the mitmproxy addon's tier-1 (highly-sensitive) redaction has already run, before the Engine's `Redactor` produces `HTTPRequest`/`HTTPResponse` from it. This is a provider-owned type, same discipline as `SocketObservation`/`ProcessObservation`: it carries **no Engine-assigned identity of any kind** — no `connection_id`, no `request_id`/`response_id` — for the same reason `SocketObservation` carries no `connection_id`: that identity doesn't exist yet at capture time, and a provider-owned type must never hold a field only the Engine is allowed to populate. It's also the source a "show anyway" UI action reveals from, via `reveal_raw(request_id)` — see the lookup mechanism below for how that command actually finds one of these once neither the request nor the response can carry an Engine ID itself.

**Highly-sensitive fields (`Authorization`, API keys, passwords, tokens, credentials — see `PRIVACY_AND_SECURITY.md`'s classification) are stripped even here, at capture time, and never exist in raw form anywhere, including memory.** Only the "potentially sensitive" tier (URLs, query params, bodies, cookies, non-auth headers) is held raw transiently. Capture-time (tier-1) redaction runs inside the mitmproxy helper addon itself, before any value crosses the helper→core IPC socket — see `PRIVACY_AND_SECURITY.md`'s "two redaction checkpoints" for exactly where.

| Field (`RawHTTPRequest`) | Type | Required |
|---|---|---|
| method | String | yes |
| host | String | yes |
| path | String | yes |
| headers | HashMap\<String, String\> | yes — tier-1 fields already redacted by the mitmproxy addon; tier-2 fields present raw |
| body | Option\<String\> | no — the full body, not a truncated preview; truncation to the `body_preview` limit happens only when the `Redactor` produces `HTTPRequest` |
| timestamp | DateTime\<Utc\> | yes |

| Field (`RawHTTPResponse`) | Type | Required |
|---|---|---|
| status_code | u16 | yes |
| headers | HashMap\<String, String\> | yes — tier-1 fields already redacted by the mitmproxy addon; tier-2 fields present raw |
| body | Option\<String\> | no — full body, same truncation note as `RawHTTPRequest.body` |
| duration_ms | f64 | yes |

**How a captured flow reaches the Engine, and how `reveal_raw` finds it again:** `TrafficProvider`'s trait method returns each captured flow as a triple — `(RawHTTPRequest, Option<RawHTTPResponse>, CorrelationEvidence)` — not the raw types alone. The `CorrelationEvidence` is what the Engine correlates against `NetworkConnection`s (per the rule under `CorrelationEvidence` below); the `Raw*` pair is what the `Redactor` consumes to produce `HTTPRequest`/`HTTPResponse`. Only once the Redactor processes a pair does the Engine mint `request_id`/`response_id` and record `request_id → (RawHTTPRequest, Option<RawHTTPResponse>)` in a session-scoped in-memory map — that map, not a field on the `Raw*` objects themselves, is what `reveal_raw(request_id)` looks up. The map entry is destroyed under the same lifetime rule as the `Raw*` objects it references (below), and is never itself persisted.

**Lifetime rule, not left implicit:** these objects exist only as long as the live monitoring session that captured them, not indefinitely just because the process/session object itself stays alive. When a session stops, its `Raw*` objects are destroyed, not merely dropped-and-hoped-for-cleanup. A long-running session must not be allowed to accumulate raw sensitive data without bound — enforce, at minimum: a maximum body-preview size per object, a cap on the number of retained raw requests/responses, and a session-level memory budget with an eviction policy once it's hit. None of this needs full implementation in Phase 0.1, but the rule is established here so a later phase doesn't have to retrofit it onto data that's already been designed to linger.

## `HTTPRequest` / `HTTPResponse` (Engine-owned, redacted — the only form that reaches storage, export, or the frontend)

An earlier version required `connection_id` and gave both types no way to reference each other or to represent an unmatched observation. Fixed here: explicit `request_id`/`response_id` identity, an optional `connection_id` for the `unmatched` case, an `ObservationStatus`, and the retained `CorrelationEvidence` for anything that didn't match.

| Field (`HTTPRequest`) | Type | Required | Notes |
|---|---|---|---|
| request_id | String | yes | Engine-assigned, like `connection_id` |
| connection_id | Option\<String\> | no | `None` when correlation was insufficiently confident; the paired `status.state` is then `unmatched` |
| method | String | yes | |
| host | String | yes | |
| path | String | yes | |
| headers | HashMap\<String, String\> | yes | redacted per `PRIVACY_AND_SECURITY.md`'s two-tier model |
| body_preview | Option\<String\> | no | truncated to 8 KiB (8192 bytes) — provisional; the full memory-budget/eviction-policy design lands in Phase 0.6, but this is the actual enforced limit from Phase 0.4 onward, not a placeholder to invent then |
| redacted_fields | Vec\<String\> | yes | field paths whose values were replaced (empty if none); lets the frontend render a distinct "redacted" affordance instead of guessing from content |
| status | ObservationStatus | yes | this request's own observation status — `observed` for a normal capture, `unmatched` when correlation failed |
| evidence | Option\<CorrelationEvidence\> | no | retained when `status.state == unmatched`, so the UI can show what this request was scored against; `None` otherwise |
| timestamp | DateTime\<Utc\> | yes | |

| Field (`HTTPResponse`) | Type | Required | Notes |
|---|---|---|---|
| response_id | String | yes | Engine-assigned |
| request_id | String | yes | the `HTTPRequest.request_id` this response pairs with — always present even if that request ended up `unmatched` |
| status_code | u16 | yes | the HTTP status code — renamed from `status` (v2) to avoid colliding with the `ObservationStatus` field below |
| headers | HashMap\<String, String\> | yes | redacted |
| body_preview | Option\<String\> | no | truncated to 8 KiB (8192 bytes) — provisional, same as `HTTPRequest.body_preview` |
| redacted_fields | Vec\<String\> | yes | same meaning as `HTTPRequest.redacted_fields` |
| status | ObservationStatus | yes | this response's own observation status |
| duration_ms | f64 | yes | |

This is the only representation the Rust core ever sends to the frontend by default (over an `invoke` response or event payload), and the only one the Session Store ever writes. A "show anyway" action invokes the dedicated `reveal_raw(request_id)` command, which returns the paired `RawHTTPRequest`/`RawHTTPResponse` still held in memory for the current session, for the potentially-sensitive tier only — it never changes what gets persisted, and highly-sensitive fields are never displayable through it or any other path because they were never captured in retrievable form.

## `HostnameObservation` (provider-owned)

What `DNSProvider` (or the SNI/`Host`-header sources added in Phase 0.4) actually saw for one hostname signal — kept as separate rows per source rather than one flattened `resolved_host` string, because reverse DNS, TLS SNI, and the HTTP `Host` header can legitimately disagree.

| Field | Type | Required | Notes |
|---|---|---|---|
| queried_addr | String | yes | the IP address this observation is about |
| source | enum { ReverseDns, Sni, HttpHost } | yes | |
| hostname | String | yes | |
| confidence | f32 (0.0–1.0) | yes | see the starter values below |
| observed_at | DateTime\<Utc\> | yes | |

**Starter confidence values, per source:** `HttpHost = 0.95`, `Sni = 0.90`, `ReverseDns = 0.50` (`0.30` if the PTR resolves to a shared/CDN-suffixed name).

## `ResolvedHostname` (Engine-owned)

Attaches a `HostnameObservation` to a specific connection once the Engine has matched it — the same provider/Engine split every other observation type gets. A provider never constructs this type; it has no way to obtain a `connection_id`.

| Field | Type | Required | Notes |
|---|---|---|---|
| connection_id | String | yes | Engine-assigned, from matching this observation to a known `NetworkConnection` |
| source | enum { ReverseDns, Sni, HttpHost } | yes | copied from the `HostnameObservation` |
| hostname | String | yes | copied from the `HostnameObservation` |
| confidence | f32 (0.0–1.0) | yes | copied from the `HostnameObservation` |
| status | ObservationStatus | yes | e.g. `unmatched` if resolution succeeded for a connection that's already gone |

**Display rule when sources disagree:** the connections view shows the highest-confidence `ResolvedHostname` for a connection, with its source named on hover. When two sources disagree and both score above `0.85`, the UI shows both rather than silently picking one. All observations are retained regardless of what's displayed — nothing is discarded just because it wasn't the one shown.

## `TrafficEvent` (Engine-owned)

An earlier version used a single opaque `payload_ref: String` field whose meaning (connection ID? request ID? response ID?) depended on reading `type` first — a real ambiguity, not just a style issue, especially once the timeline needs to query these. Replaced with explicit, individually-optional reference fields:

| Field | Type | Required | Notes |
|---|---|---|---|
| event_id | String | yes | |
| timestamp | DateTime\<Utc\> | yes | |
| type | enum { ConnectionOpened, ConnectionClosed, ConnectionExpired, Request, Response } | yes | |
| connection_id | Option\<String\> | no | normally populated for every event type; absent only for a `Request`/`Response` event whose `HTTPRequest.status.state == unmatched` |
| request_id | Option\<String\> | no | populated for `Request`/`Response` events |
| response_id | Option\<String\> | no | populated for `Response` events only |

## `ObservationStatus` (Engine-owned — see `OBSERVATION_CONTRACT.md` for the full status vocabulary)

Attached to every domain object the Engine emits, rather than status being a loose field scattered inconsistently across models. Constructed by the Engine by wrapping one or more `ProviderStatus` values (see above) and optionally promoting to `stale`/`unmatched` — a provider never constructs this type directly.

| Field | Type | Required | Notes |
|---|---|---|---|
| state | ObservationState (enum, see `OBSERVATION_CONTRACT.md`) | yes | the full seven-value vocabulary; only the Engine may set `Stale`/`Unmatched` — everything else is copied up from a `ProviderStatus.state` |
| observed_at | DateTime\<Utc\> | yes | when this status was determined |
| last_successful_at | Option\<DateTime\<Utc\>\> | no | always set once any successful observation has occurred for this object; `None` only before the first success. Required for the "last updated" display (`TODO.md` Phase 0.2) and the `stale` threshold computation (`OBSERVATION_CONTRACT.md`) |
| reason | Option\<String\> | no | human-readable detail for denied/unsupported/failure states. Display-and-log only: no code, test, or frontend branch may ever parse, match on, or switch on its contents — if behavior needs to depend on something, that belongs in `state` |
| provider | Option\<enum { Process, Socket, Dns, Traffic, Engine }\> | no | which provider/layer this status originates from, when relevant |

## `Flow` (type defined now, wired in from Phase 0.5 — see `DECISIONS.md`)

A protocol-agnostic wrapper around a connection plus its attached observations, so HTTP is one kind of observation attached to a flow rather than a parallel top-level entity. This is what makes adding QUIC/WebSocket/TLS metadata later an addition instead of a rewrite.

**`NetworkConnection` and `Flow` are not interchangeable terms, even though both end up holding hostname/request/response references:** `NetworkConnection` represents an observed transport-level connection (a socket); `Flow` represents the higher-level logical conversation associated with that connection. Keep this distinction explicit once `Flow` is wired in at Phase 0.5 — don't let the two grouping mechanisms silently merge into one in code just because their attached fields look similar today.

| Field | Type | Notes |
|---|---|---|
| flow_id | String | |
| connection_id | String | |
| hostname_observations | Vec\<ResolvedHostname\> | |
| requests | Vec\<HTTPRequest\> | |
| responses | Vec\<HTTPResponse\> | |

## `ObservationCapabilities` — a distinct concept from `ObservationStatus`, documented now, implemented in Phase 0.3

`ObservationStatus` describes one observation's outcome right now (this specific connection's traffic is `unsupported`). `ObservationCapabilities` describes what a provider can *ever* observe, independent of any single attempt — the data behind the report's "Observation Capabilities" panel (process info ✓, HTTPS payload ⚠ Limited, raw packet data ✕). Conflating the two makes the API messy once Phase 0.3/0.4 introduce real per-connection variability: a provider can have the *capability* `http_metadata: available` while a *specific* connection's status is `unsupported` because that one connection happens to be QUIC.

**Ownership:** Engine-owned, aggregated from each provider's self-report, **per provider** — never per connection. Per-connection variability belongs in `ObservationStatus`, not here; don't add a capability field to `NetworkConnection`. Each provider exposes its own capability set via a `capabilities() -> ProviderCapabilities` method on its trait (the provider-owned counterpart, same pattern as every other type in this document); the Engine aggregates these into `ObservationCapabilities` on `get_capabilities()`. `ProviderCapabilities`'s shape mirrors the table below, scoped to whatever subset of fields that one provider is responsible for.

Not implemented in Phase 0.1; implemented in Phase 0.3 (see `TODO.md` — this is a phase-assignment fix from the pre-implementation Round 2 audit, aligning `TODO.md`'s Phase 0.2 bullet with `ARCHITECTURE.md` and this document, both of which already said 0.3):

| Field | Values | Notes |
|---|---|---|
| process | available / unavailable | |
| sockets | available / unavailable | |
| dns | available / unavailable | |
| remote_addresses | available / unavailable | |
| http_metadata | available / limited / unsupported | |
| https_metadata | available / limited / unsupported | |
| request_body | available / limited / unsupported | |
| response_body | available / limited / unsupported | |
| raw_packet_data | available / unavailable | not pursued unless Phase 2's Network Extension upgrade happens; `unavailable` throughout the mitmproxy-based Phase 0–1 roadmap |

## Rules this document enforces

A provider never constructs `NetworkConnection`, `ProcessInfo`, `ResolvedHostname`, `HTTPRequest`/`HTTPResponse`, or `ObservationStatus` — only `SocketObservation`, `ProcessObservation`, `HostnameObservation`, `RawHTTPRequest`/`RawHTTPResponse`, or `ProviderStatus`. A hostname is never assumed canonical — always carry `source` and `confidence`. `connection_id` is an Engine session identity, not an OS-level identity, assigned exclusively by the Engine's correlation step from `CorrelationEvidence` on the traffic side and snapshot-diffing (per the matching rule above) on the socket side — nothing else invents one, and the Engine prefers a false split over a false merge when matching confidence is insufficient. `lifecycle_state`, `first_seen`, `last_seen` are Engine-owned; `closed` requires positive evidence (process exit is the only route to it through Phase 0.2), `expired` is the default absent that evidence, and only a *successful* snapshot's absence counts as evidence of anything. Highly-sensitive fields never exist in raw form anywhere, including in `RawHTTPRequest`/`RawHTTPResponse`; potentially-sensitive fields may exist raw transiently in memory, bounded and evicted per session, but never in anything persisted or exported. `bytes_sent`/`bytes_received`/`cpu_percent`/`memory_bytes` are optional everywhere they appear — a `None` on any of them is the one field-level exception to the no-inference rule (`OBSERVATION_CONTRACT.md`) and never changes the object's own `ObservationStatus`.
