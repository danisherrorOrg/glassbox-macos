# Project Report: Process Network Inspector (macOS)

**Status:** Pre-implementation — v3, incorporating a second review pass on data model and permissions accuracy
**Author context:** Synthesized from an initial architecture proposal, a detailed product/technical breakdown, and two critical review passes — all grounded in the "Seeing What a Process Talks to on the Network" learning path.

---

## 1. One-line description

> A read-only macOS application that lets users select a running process and understand its network activity — including sockets, connections, domains, and, where observable, HTTP(S) requests and responses — through a correlated timeline.

(Deliberately more conservative than an earlier draft's "see everything it's talking to." A process can speak TCP, UDP, QUIC, HTTP, HTTPS, WebSockets, gRPC, Unix domain sockets, or custom binary protocols, and some of that traffic is genuinely not inspectable at the application layer without cooperation from the app itself. The product should be ambitious about correlation and honest about visibility limits.)

## 2. Product principle

> The application observes. It does not modify, replay, inject, or control the target process.

This is the constraint that shapes everything else. It splits into two distinct guarantees, worth keeping separate in the architecture rather than treating as one vague rule:

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

## 3. Core user workflow

1. **Process list** — searchable table of running processes (name, PID, connection count, status).
2. **Select a process** → **Process detail** — tabs for Overview / Connections / API Traffic / Timeline.
3. **Connections tab** — one row per socket: local/remote address, port, protocol, state (LISTEN/ESTABLISHED/etc.), resolved to a hostname where possible.
4. **API Traffic tab** (HTTP/HTTPS only, where observable) — method, URL, status, duration; click a row to expand full request/response detail with sensitive data redacted by default.
5. **Timeline tab** — chronological log of connection and request/response events, so the answer to "what did this process just do" is a scroll, not a packet dump.

The core differentiator is the correlated view — process → connection → domain → request/response — grouping requests by the domain they went to, rather than a flat list of PID → socket rows. `lsof`, packet analyzers, and HTTP proxies each already solve one layer of this in isolation; the product is the correlation.

### Four levels of visibility (graceful degradation)

The product should never collapse "I can't see the payload" into "I can't see anything." Each level should keep working even when a deeper level isn't available for a given process or connection:

| Level | Question answered | Example | Always available? |
|---|---|---|---|
| 1 — Process | Who? | `Claude`, PID 9132 | Yes |
| 2 — Network | Who is it talking to? | `api.example.com:443`, `localhost:8080` | Yes |
| 3 — Protocol | How are they talking? | TCP/HTTPS, established, bytes sent/received | Yes |
| 4 — Application payload | What are they saying? | `POST /v1/chat` → `200` | Only where observable |

A pinned-cert or non-HTTP connection still shows Levels 1–3 in full — "HTTPS, connected, bytes sent/received, contents unavailable" is a legitimate and useful answer, not a failure state.

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

## 4. Architecture

Provider-based: each concern lives behind a small interface, so no single layer's implementation choice (a specific tool, a specific macOS API) leaks into the rest of the app. This is the most important structural decision in the whole project — it's what lets "TrafficProvider = none" become "TrafficProvider = mitmproxy-based helper" become "TrafficProvider = Network Extension" later, without touching the UI or the correlation logic.

```
                         ┌───────────────────────┐
                         │        SwiftUI        │
                         │          Views        │
                         └───────────┬───────────┘
                                     │
                                     ▼
                         ┌───────────────────────┐
                         │   Observation Engine   │
                         │  correlation + state   │
                         │ (joins raw provider    │
                         │  facts into the        │
                         │  process→domain→       │
                         │  request tree)         │
                         └───────────┬───────────┘
                                     │
              ┌──────────────┬──────┴──────┬──────────────┐
              ▼              ▼             ▼              ▼
       ProcessProvider  SocketProvider DNSProvider  TrafficProvider
              │              │             │              │
              ▼              ▼             ▼              ▼
         macOS process   macOS socket   reverse DNS /  HTTP(S) observer:
         APIs            APIs / lsof    SNI / Host     mitmproxy-based
                                        header signals helper today →
                                        (kept separate, Network Extension
                                        see Section 6)  later (optional)
```

The distinction matters: **providers collect raw observations, the Observation Engine understands them.** Each provider just reports what it independently sees (a socket exists, a hostname resolves, a request happened); joining those into "this process talked to this domain via these requests" is a distinct piece of logic with its own state (Section 6's connection lifecycle) and is where the product's actual value lives — not in any single provider.

Each provider is a protocol/interface with one job:

- **ProcessProvider** — enumerate processes; PID, name, executable path, CPU/memory.
- **SocketProvider** — sockets for a given PID; protocol, local/remote addr+port, state. Backed by system APIs where available, `lsof`/`nettop` where not.
- **DNSProvider** — resolve IPs to hostnames so the UI shows `api.example.com:443` instead of `104.18.32.123:443`. Produces a `HostnameObservation` per signal rather than one flattened string (see Section 6) — reverse DNS, TLS SNI, and the HTTP `Host` header can legitimately disagree (CDN fronting, shared IPs, stale DNS), and that disagreement is itself useful information for the correlation layer, not noise to collapse away.
- **TrafficProvider** — HTTP(S) request/response observation, scoped to one process, in pure event-consumption mode (no intercept/modify hooks wired up). Returns "unavailable" for pinned, QUIC, or non-HTTP traffic rather than failing.

**A scoping note, not a hedge:** the practical first implementation of `TrafficProvider` is very likely to be a helper process driving `mitmdump` — which is a Python codebase. That's fine and expected; it's quarantined behind the interface as one provider's implementation detail, not a statement that "this project uses Python." The app's identity, UI, and every other layer stay native Swift. A Network Extension–based provider is the natural long-term replacement, not a prerequisite for v0.1.

**Scope this now, don't build it now:** sketch the four provider protocols as Swift interfaces early since that costs almost nothing, but don't build out swappable-backend machinery (multiple concrete implementations, runtime provider selection, etc.) until there's an actual second implementation to swap in. One clean interface plus one concrete implementation per provider is enough until Phase 0.3.

Same principle applies to `Flow` (Section 6): define the type now since it costs one struct, but don't route the v0.2 UI or Observation Engine through it. It only earns its keep once there's a second kind of observation (HTTP, in v0.3) to attach alongside a connection — before that, it's structure built for a shape that doesn't exist yet.

## 5. Sensitive data handling

"Credentials are redacted" is a product requirement, not something header-matching alone can guarantee — a password can live in a body field named anything (`credential`, `data`, a nested object), or in a query parameter, not just in an `Authorization` header. The realistic requirement:

> Sensitive information is redacted by default using configurable header, cookie, query-parameter, and body-field heuristics. The inspector must never intentionally expose credentials as a discovery feature.

Default redaction targets: `Authorization`, `Cookie`, `Set-Cookie`, `X-API-Key`, and body/query fields matching `password`, `token`, `secret`, `credential` (case-insensitive, configurable).

## 6. Data model

| Entity | Fields |
|---|---|
| `ProcessInfo` | pid, name, executablePath, cpu, memory, status |
| `NetworkConnection` | **connectionId**, pid, protocol (TCP/UDP), localAddr, localPort, remoteAddr, remotePort, **lifecycleState** (discovered / active / closed / expired), **firstSeen**, **lastSeen**, **bytesSent/bytesReceived** *(optional — see note)*, resolvedHost |
| `HostnameObservation` | connectionId, **source** (reverseDNS / SNI / HTTPHost), hostname, confidence — kept as separate signals rather than one flattened `resolvedHost` string, since the three sources can legitimately disagree |
| `HTTPRequest` | connectionId, method, host, path, headers (redacted), body preview, timestamp |
| `HTTPResponse` | requestId, status, headers, body preview, durationMs |
| `TrafficEvent` | timestamp, type (connection-opened/closed, request, response), payload ref |
| `Flow` *(type defined now, wired in from v0.3 — see Section 4)* | a protocol-agnostic wrapper around a connection plus its observations (DNS, TLS metadata, HTTP requests/responses), so HTTP becomes one attached observation type rather than a parallel top-level entity — this is what makes adding QUIC/WebSocket/TLS metadata later an addition instead of a rewrite |

A stable `connectionId` on every `NetworkConnection` is what makes the process → connection → request → response tree in the UI possible; without it, correlating a request back to the right socket relies on fragile addr/port matching.

**Lifecycle state is not optional.** A poll-based tracker has no other way to distinguish "this socket closed" from "we missed it between polling intervals" — the Timeline feature depends on these transitions existing. `firstSeen`/`lastSeen` fall out of the same tracking for free.

**`bytesSent`/`bytesReceived` are provider-dependent, not guaranteed.** `lsof` alone doesn't expose traffic counters — getting them needs `nettop`, `netstat -b`, or a TCP_INFO-style syscall, and availability depends on which `SocketProvider` backend ends up implemented. Treat these as best-effort fields subject to the same graceful-degradation principle as HTTP payload visibility (Section 3), not as something every connection is guaranteed to have.

## 7. Permissions & platform constraints (verify early, don't design around assumptions)

These are two different permission problems, worth keeping separate rather than lumping under one "you'll need special access" line:

- **Process and socket inspection** (Phases 1–2, `ProcessProvider`/`SocketProvider`). For processes you own (same user), enumerating sockets is generally accessible to an ordinary unsandboxed app via system APIs or `lsof`/`nettop` — no Network Extension entitlement involved. Cross-user visibility (another user's processes) is the case that typically needs elevated privileges (root). Sandboxing (Mac App Store distribution) is the main variable that can restrict even same-user introspection, which is exactly why the distribution decision below needs to happen early.
- **Traffic capture** (Phase 3+, `TrafficProvider`). This is the separate, heavier capability: observing packet or application-layer content requires either a privileged helper process or the `com.apple.developer.networking.networkextension` entitlement (Network Extension / Content Filter APIs) for an in-process implementation. Don't let this requirement bleed into the cost estimate for Phases 1–2, which don't need it at all.
- App sandboxing (required for Mac App Store distribution) restricts raw socket/process introspection significantly — decide App Store vs. notarized standalone distribution before locking architecture, not after.
- **Known future cost:** a real Network Extension–based `TrafficProvider` requires an Apple-granted entitlement, and that approval has its own non-trivial lead time. Plan around the mitmproxy-based helper for as long as it's sufficient, and treat the entitlement request as a project milestone in its own right when the time comes, not an assumed drop-in upgrade.

**First technical spike, regardless of anything else:** confirm exactly what an ordinary macOS app can read about other processes' sockets without elevated privileges, and what the escalation path looks like. This determines whether Phase 1 is trivial or the project's first real blocker.

## 8. Stack decision

**Swift + SwiftUI**, native, as the app's primary language and identity — this is a macOS system-observation application, which is exactly where native development earns its keep: permissions, process/socket APIs, and eventual Network Extension integration all live more naturally in a signed native app than a browser-facing Python service.

This is not "no Python anywhere" — the `TrafficProvider`'s first implementation will likely still be a helper process driving `mitmdump`, and that's fine, because it's contained behind the provider interface (Section 4) rather than woven through the app. External tools/helpers are used only where they solve a problem macOS APIs don't conveniently expose, and swapped out later without touching the UI or correlation layer.

```
SwiftUI
   +
Swift
   +
macOS APIs (ProcessProvider, SocketProvider, DNSProvider)
   +
external helper only for TrafficProvider (mitmproxy-based, today)
```

## 9. Roadmap

HTTP observation is not required for the first useful product — process and socket visibility alone, live and correlated, is already a shippable v0.1.

| Version | Goal | Notes |
|---|---|---|
| 0.1 | Process Explorer + Socket Explorer | List processes, select one, show its live sockets (LISTEN/ESTABLISHED). No network-capture code. **This is the first demo milestone.** |
| 0.2 | Live Monitoring + DNS/hostname correlation | Start/stop, auto-refresh, connection-opened/closed events, timeline; IPs resolved to hostnames |
| 0.3 | Traffic Observation Backend | Stand up the `TrafficProvider` interface and its mitmproxy-based implementation, scoped to one process, in pure observation mode |
| 0.4 | HTTP/HTTPS Metadata | Request/response capture surfaced in the UI, redaction applied by default |
| 0.5 | API Explorer | Group by host/endpoint, request counts, latency, error rates |
| 0.6 | Timeline + Session Recording & Playback | Save/reopen past observation sessions for viewing — never for resending |
| 0.7 | Filters + Search + Analytics | `host:`, `status:`, `method:`, `port:` style filters |
| 1.0 | Polish + Packaging + Permissions | Observation Capabilities panel, dark mode, export, distribution decision (App Store vs. notarized standalone) |

## 10. Suggested repo structure

```
ProcessNetworkInspector/
├── App/                  # entry point, app state
├── Models/               # ProcessInfo, NetworkConnection, HostnameObservation, HTTPRequest/Response, TrafficEvent, Flow
├── Providers/            # ProcessProvider, SocketProvider, DNSProvider, TrafficProvider (protocols + concrete impls)
├── Views/                # ProcessList, ProcessDetail, Connections, Traffic, Timeline, API, ObservationCapabilities
├── Utilities/            # Redactor, Formatter, Logger
└── Tests/
```

## 11. Next step

Turn Phase 0.1 into an actual technical spec: exact macOS APIs/commands for `ProcessProvider` and `SocketProvider`, the concrete Swift types, the SwiftUI view hierarchy, and a sequence of build steps sized for incremental implementation.
