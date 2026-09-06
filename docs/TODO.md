# TODO / Roadmap — Process Network Inspector

**v4 — migrated to the FastAPI + React stack (ADR-009) and updated for the data-model/status consistency pass (ADR-011).** Earlier versions of this file described Xcode/SwiftUI/Swift-concurrency tasks that no longer apply — see `docs/DECISIONS.md` for why the stack changed. Ordered so each phase produces something demoable before the next one starts — don't jump ahead to a later phase's checkboxes while earlier ones are unchecked.

Two things stay true across every phase below, not just the ones that mention them: **redaction of highly-sensitive fields happens at capture time and never produces a raw value anywhere; potentially-sensitive fields are redacted before anything touches disk, not just before it touches the screen** (`docs/PRIVACY_AND_SECURITY.md`), and **unknown/denied/stale is never displayed as empty/zero** — see the Observation Contract tasks below.

---

## Phase 0 — Foundation (project setup, not a product milestone)

- [ ] Set up the backend project: Python, FastAPI, a `pyproject.toml`/`requirements.txt`, an ASGI entrypoint (`uvicorn`)
- [ ] Set up the frontend project: React (via Vite or similar), talking to the backend's REST/WebSocket endpoints
- [ ] `git init`, initial commit, `.gitignore` for Python + Node
- [ ] Set up folder structure: `backend/` (`app/`, `models/`, `providers/`, `engine/`, `utils/`, `tests/`), `frontend/` (`src/components/`, `src/api/`)
- [ ] **Permissions spike (do this before writing any provider code):** confirm exactly what an ordinary Python process can read about other processes' sockets, and what `lsof`/`psutil` refuse without elevation. Write findings into `docs/PERMISSIONS_AND_PLATFORM.md`'s VERIFIED section — this determines how much of Phase 0.1 is trivial vs. blocked.
- [ ] Decide and configure the FastAPI server's binding now, not later: `127.0.0.1` only, CORS restricted to the frontend's own origin (`docs/PRIVACY_AND_SECURITY.md`) — this is a default to get right from the first commit, not a hardening pass to do at the end.

## Phase 0.1 — Process Explorer + Socket Explorer (first product milestone)

*Goal: process list → click a process → see its live socket table. First demo.*

**Models & providers**

- [ ] Define `ProcessInfo` model (Pydantic)
- [ ] Define `SocketObservation` (provider-owned, raw — no identity or lifecycle fields) and `SocketSnapshot` (immutable, point-in-time list of `SocketObservation`s) per `docs/DATA_MODEL.md`
- [ ] Define `NetworkConnection` (Engine-owned domain state — `connection_id`, `lifecycle_state`, `first_seen`, `last_seen`) — a provider must never construct this type directly
- [ ] Define `ObservationStatus` (state, observed_at, last_successful_at?, reason?, provider?) per `docs/DATA_MODEL.md` / `docs/OBSERVATION_CONTRACT.md`
- [ ] Define `ProcessProvider` protocol (Python `Protocol` or ABC)
- [ ] Implement `ProcessProvider` (`psutil`, fallback to `ps` parsing if needed)
- [ ] Define `SocketProvider` protocol (returns a `SocketSnapshot` of `SocketObservation`s — never a `NetworkConnection`)
- [ ] Implement `SocketProvider` (`psutil`/system APIs where available, `lsof -i -n -P` fallback)
- [ ] Sketch (protocol only, no second implementation yet) `DNSProvider` and `TrafficProvider` interfaces so later phases don't require reshaping earlier code
- [ ] Implement provider-level statuses: `observed` / `unavailable` / `permission_denied` / `unsupported` / `transient_failure` per `docs/OBSERVATION_CONTRACT.md` — providers never emit `stale` or `unmatched`, those are Engine-derived

**Observation Engine**

- [ ] Define `ObservationEngine` responsibilities explicitly: consume `SocketSnapshot`s, diff successive snapshots into `NetworkConnection` lifecycle events, merge in `ProcessProvider` output, maintain current process/connection state, attach `ObservationStatus` to everything it emits, and emit normalized updates over the API/WebSocket layer — providers never talk to FastAPI or React directly
- [ ] Implement connection identity: a deterministic strategy for matching a `SocketObservation` on snapshot N to the same logical connection on snapshot N+1, producing/reusing the right `connection_id`
- [ ] Implement the `closed` vs. `expired` rule explicitly (`docs/DATA_MODEL.md`): `closed` requires positive evidence the connection ended; a connection that simply stops appearing in snapshots becomes `expired`, never silently `closed`
- [ ] Test connection matching against reused local ports and rapidly closed/reopened connections
- [ ] Ensure one unavailable/failing provider is isolated and doesn't take down the whole observation session (surface its status instead)

**Concurrency (asyncio, not Swift concurrency)**

- [ ] Define the asyncio model for providers and `ObservationEngine` (async tasks, a polling loop as an `asyncio` task)
- [ ] Ensure polling tasks can be cancelled cleanly on shutdown
- [ ] Prevent overlapping polling cycles (a slow `lsof` subprocess call must not let poll #2 start before poll #1 finishes — guard with a lock or by checking task completion)
- [ ] Ensure WebSocket pushes to the frontend don't block the polling loop (run them as separate tasks / use a queue)

**Backend API**

- [ ] `GET /processes` — list of `ProcessInfo` with status
- [ ] `GET /processes/{pid}/connections` — list of `NetworkConnection` with `ObservationStatus`
- [ ] WebSocket endpoint for live connection updates, scoped to a selected process

**Frontend**

- [ ] Build the process list view — searchable table (name, PID, connection count, status)
- [ ] Build the process detail view shell with tab structure (Overview / Connections / API Traffic / Timeline — only Connections functional this phase)
- [ ] Build the connections view — one row per socket: protocol, local/remote addr+port, state
- [ ] Wire up manual refresh (button, not yet live polling)
- [ ] Define a shared state model for views: `loading` / `loaded` / `empty` / `permission_denied` / `error` / `stale` — driven directly by the backend's `ObservationStatus`, not inferred from data shape

**Demo checkpoint:** select a real running process, see its actual open sockets, values match `lsof -p <PID> -i -n -P` run by hand — including a deliberately permission-denied case rendering as "permission denied," not as an empty list.

## Phase 0.2 — Live Monitoring + DNS/hostname correlation

- [ ] Add polling loop with configurable interval; start/stop controls in UI
- [ ] Define monitoring state machine: `idle` / `starting` / `running` / `stopping` / `stopped` / `failed`
- [ ] Track observation timestamp separately from `first_seen`/`last_seen`; surface "last updated" in the UI so a stalled poll never silently implies a live "ESTABLISHED right now" — this is exactly what `ObservationStatus.last_successful_at` is for
- [ ] Handle selected-process termination: detect the PID no longer exists, stop monitoring it, mark it "process exited" in the UI, and keep its historical observations for the current session rather than clearing them
- [ ] Emit `TrafficEvent`s for connection-opened/connection-closed (from the lifecycle transitions already implemented in `ObservationEngine`)
- [ ] Build first pass of the timeline view driven by those events
- [ ] Define `HostnameObservation` model (connection_id, source, hostname, confidence)
- [ ] Implement `DNSProvider` (reverse DNS lookup to start; SNI/Host-header sources land in Phase 0.4 once HTTP exists)
- [ ] Surface resolved hostnames in the connections view (e.g. `api.example.com:443` instead of raw IP)
- [ ] Add "Observation Capabilities" data plumbing (even if the UI panel itself ships in 1.0, start tracking what's actually available per connection now)

**Demo checkpoint:** open a connection-heavy app (browser, Slack), watch new connections appear/disappear live with resolved hostnames; quit that app and confirm the UI reports "process exited" instead of a stale connection list.

## Phase 0.3 — Traffic Observation Backend

*Hardest phase. Don't start until 0.1–0.2 are solid and reliable.*

- [ ] Spike: confirm `mitmproxy --mode local:<pid>` works standalone against a process you control, before wiring anything into the app
- [ ] Design the backend-to-helper boundary: how the FastAPI backend launches/manages the `mitmdump`-based helper process, and the IPC format (local socket/websocket + JSON) between them
- [ ] Write the mitmproxy addon script: consume `request`/`response` events only — no `intercept()`, no `set()`, no replay hooks, by construction
- [ ] Implement `TrafficProvider` protocol + mitmproxy-backed implementation
- [ ] Define `TrafficProvider` capability reporting (`processScoped`, `http`, `httpsMetadata`, `requestBody`, `responseBody`, ...) — distinguish "provider unavailable" from "this traffic type is unsupported"
- [ ] Handle the `unsupported`/`unavailable` path explicitly: pinned certs, QUIC, non-HTTP traffic must degrade to "connected, bytes only" rather than erroring
- [ ] Define `CorrelationEvidence` (pid?, protocol?, local/remote addr+port?, hostname?, timestamp, source) per `docs/DATA_MODEL.md` — the actual input to correlation, since `TrafficProvider` never has the Engine's internal `connection_id`
- [ ] **Spike, independently of the UI: `CorrelationEvidence` → `NetworkConnection` matching.** Determine which identifiers the traffic provider actually exposes (PID? local port? both?) and whether they're sufficient to reliably map a captured flow back to an existing connection
- [ ] Define and implement the `unmatched` path: evidence that can't be confidently mapped produces `ObservationStatus.state = unmatched` and appears in the UI as such — never silently dropped and never attached to the wrong connection by guessing
- [ ] Principle to enforce in code review, not just docs: never fabricate a `connection_id` when correlation confidence is insufficient
- [ ] Scope capture strictly to the user-selected PID — verify traffic from unrelated processes never leaks into a session

**Demo checkpoint:** select a process, trigger a known HTTP(S) request from it, see the raw flow show up in a debug log correctly attached to its connection (UI integration is next phase).

## Phase 0.4 — HTTP/HTTPS Metadata

- [ ] Define `RawHTTPRequest`/`RawHTTPResponse` (transient, in-memory only — potentially-sensitive fields held raw here) and `HTTPRequest`/`HTTPResponse` (Engine-owned, redacted, the only form that reaches storage/export/API) per `docs/DATA_MODEL.md`
- [ ] Implement capture-time redaction for the highly-sensitive tier (`Authorization`, API keys, passwords, tokens) — these must never populate even the `Raw*` types; there is no reveal path for them, by design
- [ ] Wire `TrafficProvider` output into the Observation Engine, attached to the correct `NetworkConnection` via the correlation logic from Phase 0.3
- [ ] Add SNI and HTTP-`Host`-header as additional `HostnameObservation` sources (alongside reverse DNS from 0.2)
- [ ] Implement the `Redactor` utility for the potentially-sensitive tier: header-based (`Cookie`, `Set-Cookie`) + body/query-field heuristics (configurable list), producing `HTTPRequest`/`HTTPResponse` from `RawHTTPRequest`/`RawHTTPResponse`
- [ ] Unit-test the `Redactor`: header redaction, cookie redaction, query-parameter redaction, nested-JSON body-field redaction, case-insensitive matching, configurable custom field names
- [ ] **Verify redaction happens before persistence/export, not only before display** — write a test that captures a request with a fake credential, saves/exports it, and asserts the credential never reaches disk unredacted (see the three mandatory integration tests in `docs/TESTING_STRATEGY.md`)
- [ ] Build the API traffic view — method/URL/status/duration table
- [ ] Build the request/response detail expansion view: redacted by default, "show anyway" reveals the paired `Raw*` object for the potentially-sensitive tier only, with highly-sensitive fields never displayable at all

**Demo checkpoint:** trigger a real HTTPS request from a test process, see method/headers/body in the UI with credentials redacted, and confirm a saved session file on disk is also redacted.

## Phase 0.5 — API Explorer

- [ ] Introduce `Flow` model, wiring HTTP observations onto it (per `docs/DATA_MODEL.md` — this is the point where it earns its keep)
- [ ] Group captured requests by host → endpoint in a dedicated API view
- [ ] Compute per-endpoint stats: request count, error rate, average latency
- [ ] Build the "killer feature" tree view: process → domain → requests, collapsible

**Demo checkpoint:** run a chatty process for a few minutes, view its full API surface grouped and summarized.

## Phase 0.6 — Timeline + Session Recording & Playback

- [ ] Finalize the timeline view to interleave connection events and HTTP request/response events chronologically
- [ ] Design session storage format (local file, e.g. JSON/SQLite) capturing a full observation window
- [ ] Decide and document: sessions are stored **redacted-only by default** — raw/unredacted storage, if ever offered, is an explicit opt-in, not the default
- [ ] Define maximum body-preview size and header/session memory limits; truncate oversized captures safely instead of holding them in full
- [ ] Ensure captured request/response contents are never written into general application logs, and reconfigure `uvicorn`'s access logging so request URLs (which can carry query-string secrets) aren't logged by the web framework itself, bypassing the `Redactor` entirely
- [ ] Document where session files are stored on disk
- [ ] Implement "start session" / "stop & save session"
- [ ] Implement session list + reopen-for-viewing (explicitly not "resend" — no code path should be able to turn a saved request back into an outbound one)

**Demo checkpoint:** record a session, quit the app, relaunch, reopen and replay the timeline visually; inspect the saved session file directly and confirm no unredacted credential is present.

## Phase 0.7 — Filters + Search + Analytics

- [ ] Implement query-style filtering: `host:`, `status:`, `method:`, `port:`
- [ ] Add filter bar to the API traffic view and connections view
- [ ] Add basic analytics: totals, error counts, latency distribution (per process and per session)

## Phase 1.0 — Local-Run Polish

*Distribution as a packaged native app is explicitly out of scope for 1.0 — see the deferred phase below.*

- [ ] Build the actual "Observation Capabilities" panel UI (data has existed since Phase 0.2, capability reporting since 0.3)
- [ ] Dark mode pass
- [ ] Export (session as JSON, or a single flow as text) — export only, never re-send; export must go through the same redacted-by-default path as session storage
- [ ] Performance pass: polling overhead, large-session memory use, confirm resource limits from 0.6 actually hold under sustained high-traffic load
- [ ] Write a reliable local install/run guide: backend setup (`pip install`/`uvicorn`), frontend setup (`npm install`/dev server or a built static bundle the backend serves), and how to run both together
- [ ] Write end-user README: what this tool does, what it deliberately does not do, and the "things you own or have permission to inspect" scope note from the original learning path
- [ ] Run a full end-to-end regression pass before calling this 1.0: process discovery → socket observation → lifecycle tracking → DNS correlation → HTTP observation → redaction → session storage → session reopening. A checklist walkthrough is enough at this project's scale — this doesn't need CI infrastructure, just a deliberate pass through the whole chain instead of assuming the individual phase demo checkpoints still compose correctly together

## Phase 2 — Packaged Distribution (future, optional — not required to consider this project done)

Only pursue this if "I want other people to install this like a normal app" becomes an actual goal. Not part of 1.0's definition of done.

- [ ] Decide on a packaging approach (e.g. wrapping the backend + a built frontend bundle in `pywebview` or similar) for a double-clickable local app
- [ ] If pursuing a Network Extension–based `TrafficProvider` upgrade: this requires introducing a separate, separately-signed native helper (see `docs/DECISIONS.md` ADR-009) — file the entitlement request early and treat Apple's review lead time as its own milestone
- [ ] Code signing, notarization, or App Store submission prep, if a distributed native wrapper is built

---

## Cross-cutting (ongoing, not a single phase)

- [ ] Build a small deterministic `NetworkTestTarget` Python script early (useful starting in Phase 0.1, essential by 0.3) that generates known traffic on demand: a plain TCP connection, a short-lived connection, a long-lived connection, several simultaneous connections, a couple of HTTP(S) requests once Phase 0.3 exists, and requests carrying intentionally fake sensitive-looking fields for redaction testing. Same principle the source learning path opened with — verify the tool against traffic you already understand before pointing it at anything else.
- [ ] Unit tests per provider (`pytest`, mock the system-call boundary so tests don't depend on real running processes)
- [ ] Add integration tests for the observation pipeline using `NetworkTestTarget`: generate known connections/HTTP requests and verify they come out the other end with correct process attribution, connection identity, lifecycle events, hostname correlation, HTTP correlation, and redaction — unit tests per provider don't catch a correlation bug in `ObservationEngine`, only a test that exercises the full chain does. This includes the three mandatory tests in `docs/TESTING_STRATEGY.md`: process termination, polling-gap → `expired`, and correlation ambiguity → `unmatched`.
- [ ] Keep `docs/PERMISSIONS_AND_PLATFORM.md`'s VERIFIED/ASSUMED/DECISION tags current as permission reality gets discovered
- [ ] Re-check the "explicitly out of scope" list (`docs/process-network-inspector-report.md` Section 2) at the start of every phase — no feature in this roadmap should ever grow into edit/replay/inject

## Definition of done for the whole project

Select a running process and obtain Levels 1–3 (process, network, protocol) wherever those observations are technically and legitimately available, with the UI explicitly distinguishing **observed**, **denied**, **unsupported**, and **stale** information rather than presenting any of them as empty or current. Level 4 (HTTP/application payload) is available only where technically and legitimately observable, degrading to the same explicit categories rather than silence. Highly-sensitive data is never captured in raw form at all; potentially-sensitive data is redacted by default and irreversibly before it ever reaches storage or export. Nothing in the app is capable of modifying, replaying, or injecting traffic.
