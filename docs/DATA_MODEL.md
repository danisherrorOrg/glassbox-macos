# Data Model — Process Network Inspector

The correlation model is the product (see report Section 3) — this document is the precise spec behind the tables sketched in the report and TODO. Types are described as Pydantic models (FastAPI's native serialization layer); use `Optional[...]` fields exactly where marked optional below, nowhere else.

## Entity relationships

```
ProcessInfo
   │
   └── NetworkConnection (1:many)
          │
          ├── HostnameObservation (1:many — one per source)
          │
          └── TrafficEvent (1:many)
                   │
                   ├── HTTPRequest
                   └── HTTPResponse
          │
          └── Flow (from v0.3 onward — wraps a connection + its observations)
```

## `ProcessInfo`

| Field | Type | Required | Notes |
|---|---|---|---|
| pid | int | yes | |
| name | str | yes | |
| executable_path | str | yes | |
| cpu_percent | float | no | best-effort |
| memory_bytes | int | no | best-effort |
| status | str | yes | `running` / `exited` — set by the Engine, not the raw provider snapshot |

## `SocketSnapshot`

Immutable, point-in-time output of `SocketProvider`. Never mutated after creation — the Engine diffs successive snapshots, the provider never tracks lifecycle itself.

| Field | Type | Notes |
|---|---|---|
| timestamp | datetime | when this snapshot was taken |
| connections | list[NetworkConnection] | as observed at that instant, before lifecycle merge |

## `NetworkConnection`

| Field | Type | Required | Notes |
|---|---|---|---|
| connection_id | str | yes | stable identity across polls — see the identity-matching rule in `TODO.md` Phase 0.1 |
| pid | int | yes | |
| protocol | Literal["tcp","udp"] | yes | |
| local_addr | str | yes | |
| local_port | int | yes | |
| remote_addr | str | no | absent for LISTEN sockets |
| remote_port | int | no | absent for LISTEN sockets |
| state | str | yes | LISTEN / ESTABLISHED / etc. |
| lifecycle_state | Literal["discovered","active","closed","expired"] | yes | set by the Engine, never by the provider |
| first_seen | datetime | yes | |
| last_seen | datetime | yes | |
| bytes_sent | int | no | provider-dependent — see `OBSERVATION_CONTRACT.md` |
| bytes_received | int | no | provider-dependent |

## `HostnameObservation`

Kept as separate rows per source rather than one flattened `resolved_host` string, because reverse DNS, TLS SNI, and the HTTP `Host` header can legitimately disagree.

| Field | Type | Required | Notes |
|---|---|---|---|
| connection_id | str | yes | |
| source | Literal["reverse_dns","sni","http_host"] | yes | |
| hostname | str | yes | |
| confidence | float (0–1) | yes | e.g. reverse DNS on a shared/CDN IP gets lower confidence than an HTTP `Host` header |

## `HTTPRequest` / `HTTPResponse`

| Field (`HTTPRequest`) | Type | Required |
|---|---|---|
| connection_id | str | yes |
| method | str | yes |
| host | str | yes |
| path | str | yes |
| headers | dict[str, str] | yes — redacted per `PRIVACY_AND_SECURITY.md` before this object exists |
| body_preview | Optional[str] | no — truncated per the size limit in `PRIVACY_AND_SECURITY.md` |
| timestamp | datetime | yes |

| Field (`HTTPResponse`) | Type | Required |
|---|---|---|
| request_id | str | yes |
| status | int | yes |
| headers | dict[str, str] | yes — redacted |
| body_preview | Optional[str] | no |
| duration_ms | float | yes |

## `TrafficEvent`

| Field | Type | Notes |
|---|---|---|
| timestamp | datetime | |
| type | Literal["connection_opened","connection_closed","request","response"] | |
| payload_ref | str | id of the referenced `NetworkConnection` / `HTTPRequest` / `HTTPResponse` |

## `Flow` (type defined now, wired in from v0.3 — see `DECISIONS.md`)

A protocol-agnostic wrapper around a connection plus its attached observations, so HTTP is one kind of observation attached to a flow rather than a parallel top-level entity. This is what makes adding QUIC/WebSocket/TLS metadata later an addition instead of a rewrite.

| Field | Type | Notes |
|---|---|---|
| flow_id | str | |
| connection_id | str | |
| hostname_observations | list[HostnameObservation] | |
| requests | list[HTTPRequest] | |
| responses | list[HTTPResponse] | |

## Rules this document enforces

A hostname is never assumed canonical — always carry the `source` and `confidence`, and let the UI show disagreement rather than picking one silently. `connection_id` is the only thing anything correlates on; nothing correlates by re-matching addr/port tuples downstream of the Engine. `lifecycle_state`, `first_seen`, and `last_seen` are Engine-owned fields — a provider that tries to set them directly is a bug. `bytes_sent`/`bytes_received` are optional everywhere they appear; code must never assume they're present.
