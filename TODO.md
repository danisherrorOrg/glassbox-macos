# TODO / Roadmap — Process Network Inspector

End-to-end build checklist, derived from `process-network-inspector-report.md` (v3) plus a review pass on engineering granularity (v2 of this file). Ordered so each phase produces something demoable before the next one starts — don't jump ahead to a later phase's checkboxes while earlier ones are unchecked.

Two things stay true across every phase below, not just the ones that mention them: **redaction happens before anything touches disk, not just before it touches the screen**, and **unknown/denied/stale is never displayed as empty/zero** — see the Cross-cutting section and the state-model tasks in Phase 0.1.

---

## Phase 0 — Project setup

- [ ] Decide distribution target now (Mac App Store vs. notarized standalone) — this affects sandboxing and how much Phase 1 can actually see. See report Section 7.
- [ ] Create Xcode project (macOS App, SwiftUI lifecycle) inside `ProcessNetworkInspector/`
- [ ] `git init`, initial commit, `.gitignore` for Xcode/Swift
- [ ] Set up folder structure per report Section 10: `App/`, `Models/`, `Providers/`, `Views/`, `Utilities/`, `Tests/`
- [ ] **Permissions spike (do this before writing any provider code):** confirm exactly what an ordinary unsandboxed macOS app can read about other processes' sockets, and what breaks under sandboxing. Write findings into a `PERMISSIONS_FINDINGS.md` — this determines how much of Phase 1 is trivial vs. blocked.

## Phase 0.1 — Process Explorer + Socket Explorer

*Goal: process list → click a process → see its live socket table. First demo milestone.*

**Models & providers**

- [ ] Define `ProcessInfo` model (pid, name, executablePath, cpu, memory, status)
- [ ] Define `NetworkConnection` model, including `connectionId`, `lifecycleState`, `firstSeen`/`lastSeen` from day one (not bolted on later)
- [ ] Define `SocketSnapshot` — an immutable, point-in-time result from `SocketProvider`: `{ timestamp, connections[] }`. The provider only ever answers "what do I see right now" — it does not track lifecycle itself.
- [ ] Define `ProcessProvider` protocol
- [ ] Implement `ProcessProvider` (system APIs, fallback to `ps`/`sysctl` parsing if needed)
- [ ] Define `SocketProvider` protocol (returns a `SocketSnapshot`)
- [ ] Implement `SocketProvider` (system APIs where available, `lsof -i -n -P` fallback)
- [ ] Sketch (protocol only, no second implementation yet) `DNSProvider` and `TrafficProvider` interfaces so later phases don't require reshaping earlier code
- [ ] Define provider error states: `unavailable` / `permissionDenied` / `commandFailed` / `unsupported` / `transientFailure` — every provider protocol returns one of these instead of silently producing an empty result

**Observation Engine**

- [ ] Define `ObservationEngine` responsibilities explicitly: consume `SocketSnapshot`s, diff successive snapshots into connection lifecycle events, merge in `ProcessProvider` output, maintain current process/connection state, and emit normalized updates to the UI — providers never talk to SwiftUI directly
- [ ] Implement connection identity: a deterministic strategy for matching "connection A on snapshot N" to "connection A on snapshot N+1" (this is not free from having a `connectionId` field — the field needs a matching rule behind it, e.g. local+remote addr/port tuple with reuse handling)
- [ ] Test connection matching against reused local ports and rapidly closed/reopened connections
- [ ] Implement `ObservationEngine` using `ProcessProvider` + `SocketProvider`
- [ ] Ensure one unavailable/failing provider is isolated and doesn't take down the whole observation session (surface its error state instead)

**Concurrency**

- [ ] Define the Swift concurrency model for providers and `ObservationEngine` (structured tasks, actor isolation)
- [ ] Ensure polling/tasks can be cancelled cleanly
- [ ] Prevent overlapping polling cycles (a slow `lsof` shell-out must not let poll #2 start before poll #1 finishes)
- [ ] Ensure UI-facing updates land on the correct actor/main thread

**UI**

- [ ] Build `ProcessListView` — searchable table (name, PID, connection count, status)
- [ ] Build `ProcessDetailView` shell with tab structure (Overview / Connections / API Traffic / Timeline — only Connections functional this phase)
- [ ] Build `ConnectionsView` — one row per socket: protocol, local/remote addr+port, state
- [ ] Wire up manual refresh (button, not yet live polling)
- [ ] Define a shared state model for views: `loading` / `loaded` / `empty` / `permissionDenied` / `error` / `stale` — used consistently instead of ad hoc per-view logic (this single task replaces what would otherwise be three overlapping concerns: error display, staleness, and empty-vs-denied)

**Demo checkpoint:** select a real running process, see its actual open sockets, values match `lsof -p <PID> -i -n -P` run by hand — including a deliberately permission-denied case rendering as "permission denied," not as an empty list.

## Phase 0.2 — Live Monitoring + DNS/hostname correlation

- [ ] Add polling loop with configurable interval; start/stop controls in UI
- [ ] Define monitoring state machine: `idle` / `starting` / `running` / `stopping` / `stopped` / `failed`
- [ ] Track observation timestamp separately from connection `firstSeen`/`lastSeen`; surface "last updated" in the UI so a stalled poll never silently implies a live "ESTABLISHED right now"
- [ ] Handle selected-process termination: detect the PID no longer exists, stop monitoring it, mark it "process exited" in the UI, and keep its historical observations for the current session rather than clearing them
- [ ] Emit `TrafficEvent`s for connection-opened/connection-closed (from the lifecycle transitions already implemented in `ObservationEngine`)
- [ ] Build first pass of `TimelineView` driven by those events
- [ ] Define `HostnameObservation` model (connectionId, source, hostname, confidence)
- [ ] Implement `DNSProvider` (reverse DNS lookup to start; SNI/Host-header sources land in Phase 0.4 once HTTP exists)
- [ ] Surface resolved hostnames in `ConnectionsView` (e.g. `api.example.com:443` instead of raw IP)
- [ ] Add "Observation Capabilities" data plumbing (even if the UI panel itself ships in 1.0, start tracking what's actually available per connection now)

**Demo checkpoint:** open a connection-heavy app (browser, Slack), watch new connections appear/disappear live with resolved hostnames; quit that app and confirm the UI reports "process exited" instead of a stale connection list.

## Phase 0.3 — Traffic Observation Backend

*Hardest phase. Don't start until 0.1–0.2 are solid and reliable.*

- [ ] Spike: confirm `mitmproxy --mode local:<pid>` works standalone against a process you control, before wiring anything into the app
- [ ] Design the helper-process boundary: how the Swift app launches/manages the mitmdump-based helper, and the IPC format (local socket/websocket + JSON) between them
- [ ] Write the mitmproxy addon script: consume `request`/`response` events only — no `intercept()`, no `set()`, no replay hooks, by construction
- [ ] Implement `TrafficProvider` protocol + mitmproxy-backed implementation
- [ ] Define `TrafficProvider` capability reporting (`processScoped`, `http`, `httpsMetadata`, `requestBody`, `responseBody`, ...) — distinguish "provider unavailable" from "this traffic type is unsupported," and "not observed" from "not observable"
- [ ] Handle the "unavailable" path explicitly: pinned certs, QUIC, non-HTTP traffic must degrade to "connected, bytes only" rather than erroring
- [ ] **Spike, independently of the UI: traffic-event → connection correlation.** Determine which identifiers the traffic provider actually exposes (PID? local port? both?) and whether they're sufficient to reliably map a captured flow back to an existing `NetworkConnection`
- [ ] Define fallback behavior for a traffic event that can't be confidently mapped to a connection — it must appear somewhere in the UI as "unmatched," never silently dropped and never attached to the wrong connection by guessing
- [ ] Principle to enforce in code review, not just docs: never fabricate a `connectionId` when correlation confidence is insufficient
- [ ] Scope capture strictly to the user-selected PID — verify traffic from unrelated processes never leaks into a session

**Demo checkpoint:** select a process, trigger a known HTTP(S) request from it, see the raw flow show up in a debug log correctly attached to its connection (UI integration is next phase).

## Phase 0.4 — HTTP/HTTPS Metadata

- [ ] Define `HTTPRequest` / `HTTPResponse` models
- [ ] Wire `TrafficProvider` output into the Observation Engine, attached to the correct `NetworkConnection` via the correlation logic from Phase 0.3
- [ ] Add SNI and HTTP-`Host`-header as additional `HostnameObservation` sources (alongside reverse DNS from 0.2)
- [ ] Implement `Redactor` utility: header-based (Authorization, Cookie, Set-Cookie, X-API-Key) + body/query-field heuristics (password, token, secret, credential) — configurable list
- [ ] Unit-test the `Redactor`: header redaction, cookie redaction, query-parameter redaction, nested-JSON body-field redaction, case-insensitive matching, configurable custom field names
- [ ] **Verify redaction happens before persistence/export, not only before display** — write a test that captures a request with a fake credential, saves/exports it, and asserts the credential never reaches disk unredacted
- [ ] Build `APITrafficView` — method/URL/status/duration table
- [ ] Build request/response detail expansion view, redaction applied by default, with a "show anyway" toggle understood as a deliberate risk, not a default

**Demo checkpoint:** trigger a real HTTPS request from a test process, see method/headers/body in the UI with credentials redacted, and confirm a saved session file on disk is also redacted.

## Phase 0.5 — API Explorer

- [ ] Introduce `Flow` model, wiring HTTP observations onto it (per report Section 6 — this is the point where it earns its keep)
- [ ] Group captured requests by host → endpoint in a dedicated `APIView`
- [ ] Compute per-endpoint stats: request count, error rate, average latency
- [ ] Build the "killer feature" tree view: process → domain → requests, collapsible

**Demo checkpoint:** run a chatty process for a few minutes, view its full API surface grouped and summarized.

## Phase 0.6 — Timeline + Session Recording & Playback

- [ ] Finalize `TimelineView` to interleave connection events and HTTP request/response events chronologically
- [ ] Design session storage format (local file, e.g. JSON/SQLite) capturing a full observation window
- [ ] Decide and document: sessions are stored **redacted-only by default** — raw/unredacted storage, if ever offered, is an explicit opt-in, not the default
- [ ] Define maximum body-preview size and header/session memory limits; truncate oversized captures safely instead of holding them in full
- [ ] Ensure captured request/response contents are never written into general application logs (os_log/NSLog), only into the session store's own redacted format
- [ ] Document where session files are stored on disk
- [ ] Implement "start session" / "stop & save session"
- [ ] Implement session list + reopen-for-viewing (explicitly not "resend" — no code path should be able to turn a saved request back into an outbound one)

**Demo checkpoint:** record a session, quit the app, relaunch, reopen and replay the timeline visually; inspect the saved session file directly and confirm no unredacted credential is present.

## Phase 0.7 — Filters + Search + Analytics

- [ ] Implement query-style filtering: `host:`, `status:`, `method:`, `port:`
- [ ] Add filter bar to `APITrafficView` and `ConnectionsView`
- [ ] Add basic analytics: totals, error counts, latency distribution (per process and per session)

## Phase 1.0 — Polish + Packaging + Permissions

- [ ] Build the actual "Observation Capabilities" panel UI (data has existed since Phase 0.2, capability reporting since 0.3)
- [ ] Dark mode pass
- [ ] Export (session as JSON, or a single flow as text) — export only, never re-send; export must go through the same redacted-by-default path as session storage
- [ ] Performance pass: polling overhead, large-session memory use, confirm resource limits from 0.6 actually hold under sustained high-traffic load
- [ ] Finalize distribution path (code signing, notarization, or App Store submission prep per the Phase 0 decision)
- [ ] **If pursuing the Network Extension upgrade for `TrafficProvider`:** file the entitlement request early — treat Apple's review lead time as its own milestone, not a drop-in swap (see report Section 7)
- [ ] Write end-user README: what this tool does, what it deliberately does not do, and the "things you own or have permission to inspect" scope note from the original learning path

---

## Cross-cutting (ongoing, not a single phase)

- [ ] Build a small deterministic `NetworkTestTarget` executable early (useful starting in Phase 0.1, essential by 0.3) that generates known traffic on demand: a plain TCP connection, a short-lived connection, a long-lived connection, several simultaneous connections, a couple of HTTP(S) requests once Phase 0.3 exists, and requests carrying intentionally fake sensitive-looking fields for redaction testing. Same principle the source learning path opened with — verify the tool against traffic you already understand before pointing it at anything else.
- [ ] Unit tests per provider (mock the system-call boundary so tests don't depend on real running processes)
- [ ] Keep `PROVIDERS.md` or inline doc comments noting which provider capabilities are verified vs. assumed, updated as permission reality gets discovered
- [ ] Re-check the "explicitly out of scope" list (report Section 2) at the start of every phase — no feature in this roadmap should ever grow into edit/replay/inject

## Definition of done for the whole project

Select a running process and obtain Levels 1–3 (process, network, protocol) wherever those observations are technically and legitimately available, and Level 4 (HTTP payload) wherever technically and legitimately observable — with the UI explicitly communicating unavailable, denied, or stale observations rather than presenting them as empty or current. Sensitive data is redacted by default at the point of capture, before it ever reaches storage or export. Nothing in the app is capable of modifying, replaying, or injecting traffic.
