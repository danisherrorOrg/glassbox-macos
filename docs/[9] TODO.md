# TODO / Roadmap — Process Network Inspector

**v5 — migrated to the Tauri (Rust) + React stack (ADR-013).** Earlier versions of this file described FastAPI/uvicorn tasks, and before that Xcode/SwiftUI/Swift-concurrency tasks, that no longer apply — see `docs/DECISIONS.md` for why the stack changed each time. Ordered so each phase produces something demoable before the next one starts — don't jump ahead to a later phase's checkboxes while earlier ones are unchecked.

Two things stay true across every phase below, not just the ones that mention them: **redaction of highly-sensitive fields happens at capture time and never produces a raw value anywhere; potentially-sensitive fields are redacted before anything touches disk, not just before it touches the screen** (`docs/PRIVACY_AND_SECURITY.md`), and **unknown/denied/stale is never displayed as empty/zero** — see the Observation Contract tasks below.

---

**Before Phase 0:** complete `pre-implementation/[3] TODO.md` — a design-verification
gate run against this document set, separate from and prior to the checklist below.

## Phase 0 — Foundation (project setup, not a product milestone)

- [ ] Set up the Tauri project: `cargo tauri init`, a `Cargo.toml` for the Rust core, `tauri.conf.json` configured with a minimal command allowlist/capabilities set (not "allow everything")
- [ ] Set up the frontend project: React (via Vite, Tauri's default template integration), talking to the Rust core only via `invoke`/`event` — no REST/WebSocket endpoints to stand up
- [ ] `git init`, initial commit, `.gitignore` for Rust + Node (`target/`, `node_modules/`, `dist/`)
- [ ] Set up folder structure: `src-tauri/` (`src/` with `models/`, `providers/`, `engine/`, `commands/`, and `tests/`), `src/` for the React frontend (`components/`, `api/` for the `invoke` wrapper functions)
- [ ] **Permissions spike (do this before writing any provider code):** run the five-question checklist in `docs/PERMISSIONS_AND_PLATFORM.md`'s "First technical spike" section — privilege (same-user and other-users' visibility) *and* the three data-shape questions (PID attribution, byte counters, socket state) that `SocketObservation`'s fields actually depend on. Write findings into that document's VERIFIED section — this determines how much of Phase 0.1 is trivial vs. blocked, and a privilege-only check is not sufficient to answer that.
- [ ] Decide the Tauri capabilities/allowlist scope now, not later: only the specific commands the frontend needs, nothing broader (`docs/PRIVACY_AND_SECURITY.md`) — this is a default to get right from the first commit, not a hardening pass to do at the end. (There is no server binding/CORS decision to make — see `docs/ARCHITECTURE.md`'s "No local network surface.")

## Phase 0.1 — Process Explorer + Socket Explorer (first product milestone)

*Goal: process list → click a process → see its live socket table. First demo.*

**Models & providers**

- [ ] Define `ProcessObservation` (provider-owned, raw), `ProcessSnapshot` (provider-owned, adds `ProviderStatus`), and `ProcessInfo` (Engine-owned, adds `process_state` and `status: ObservationStatus`) per `docs/DATA_MODEL.md` — a provider must never construct `ProcessInfo` directly, same discipline as the socket split below
- [ ] Define `SocketObservation` (provider-owned, raw — no identity or lifecycle fields) and `SocketSnapshot` (immutable, point-in-time list of `SocketObservation`s, plus a `ProviderStatus`) per `docs/DATA_MODEL.md`
- [ ] Define `NetworkConnection` (Engine-owned domain state — `connection_id`, `lifecycle_state`, `first_seen`, `last_seen`, `status`) — a provider must never construct this type directly
- [ ] Define `ProviderStatus` (state, observed_at, reason?) and `ObservationStatus` (state, observed_at, last_successful_at?, reason?, provider?) per `docs/DATA_MODEL.md` / `docs/OBSERVATION_CONTRACT.md` — a provider constructs `ProviderStatus`; only the Engine constructs `ObservationStatus`
- [ ] Define the `Flow` struct per `docs/DATA_MODEL.md` — defined now, not wired into the Engine or UI until Phase 0.5 (`docs/DECISIONS.md` ADR-004)
- [ ] Define `ProcessProvider` as a Rust trait returning a `ProcessSnapshot`
- [ ] Implement `ProcessProvider` (`sysinfo` crate, falling back to direct `libproc` FFI if `sysinfo` doesn't cover a needed field)
- [ ] Define `SocketProvider` trait (returns a `SocketSnapshot` of `SocketObservation`s — never a `NetworkConnection`)
- [ ] Implement `SocketProvider` (`netstat2`/`sysinfo` where available, direct `libproc` FFI fallback — see the Phase 0 permissions spike's findings on which is actually required)
- [ ] Stub bare trait definitions for `DNSProvider` and `TrafficProvider` — signatures only, minimal. Don't design their final shape now: the Phase 0.3 mitmproxy spike will reveal real constraints a premature interface would likely get wrong, and ADR-003's guardrail (no swappable-backend machinery before a second implementation exists) applies to interface *detail*, not just to whether an interface exists at all
- [ ] Implement provider-level statuses via `ProviderStatus`: `observed` / `unavailable` / `permission_denied` / `unsupported` / `transient_failure` per `docs/OBSERVATION_CONTRACT.md` — providers never emit `stale` or `unmatched`, those are Engine-derived, and a provider never constructs `ObservationStatus` directly

**Observation Engine**

- [ ] Define `ObservationEngine` responsibilities explicitly: consume `SocketSnapshot`s, diff successive snapshots into `NetworkConnection` lifecycle events, merge in `ProcessProvider` output, maintain current process/connection state, wrap each provider's `ProviderStatus` into the `ObservationStatus` it attaches to everything it emits, and emit normalized updates via Tauri commands/events — providers never talk to Tauri or React directly
- [ ] Implement connection identity as an Engine session identity, not an OS-level one, per the explicit matching rule in `docs/DATA_MODEL.md` (`NetworkConnection`'s "Connection-identity matching rule" subsection): exact-tuple match against the immediately preceding *successful* snapshot, no numeric confidence score in this phase. When a match is ambiguous (matches more than one tracked connection equally well), **prefer creating a new connection over merging into an existing one** — a false split is cosmetic, a false merge corrupts the timeline
- [ ] Implement the `closed` vs. `expired` rule explicitly (`docs/DATA_MODEL.md`): `closed` requires positive evidence the connection ended — in this phase, that's exclusively confirmed process exit (`ProcessProvider` reporting the PID gone); a connection that simply stops appearing in snapshots becomes `expired`, never silently `closed`. Expect `discovered → active → expired` to be the normal path this phase, `→ closed` only via the process-exit path. A `SocketSnapshot` whose own `status` isn't `observed` (`transient_failure`/`unavailable`/`permission_denied`) is not evidence of anything — leave every tracked connection's `lifecycle_state`/`last_seen` untouched on such a poll and update only `status`.
- [ ] Implement the `discovered` vs. `active` transition per `docs/DATA_MODEL.md`: `discovered` on first sight, `active` on a second consecutive successful match or an observed `ESTABLISHED` state (including `LISTEN` sockets, which follow the same rule)
- [ ] Test connection matching against reused local ports and rapidly closed/reopened connections
- [ ] Ensure one unavailable/failing provider is isolated and doesn't take down the whole observation session (surface its status instead) — and specifically, ensure a `transient_failure` from `SocketProvider` never gets misread as "all connections disappeared" (see the 4th mandatory test in `docs/TESTING_STRATEGY.md`)

**Concurrency (`tokio`, not asyncio or Swift concurrency)**

- [ ] Define the `tokio` task model for providers and `ObservationEngine` (a polling loop as a spawned `tokio` task)
- [ ] Ensure polling tasks can be cancelled cleanly on shutdown (`tokio` cancellation tokens or dropping the task handle)
- [ ] Prevent overlapping polling cycles (a slow system-call/subprocess call must not let poll #2 start before poll #1 finishes — guard with a `Mutex`/atomic flag or by checking task completion)
- [ ] Ensure event pushes to the frontend don't block the polling loop (emit via Tauri's event API from a separate task, or through a channel)

**Tauri commands**

- [ ] `get_processes` command — returns the two-level envelope from `docs/OBSERVATION_CONTRACT.md` (`{ status: ObservationStatus, data: Option<Vec<ProcessInfo>> }`): an outer status for the query as a whole, each `ProcessInfo` carrying its own `status`
- [ ] `get_connections(pid)` command — same envelope shape, `data: Option<Vec<NetworkConnection>>`, each with its own `status`
- [ ] Event stream (Tauri's `emit`/`listen`) for live connection updates, scoped to a selected process

**Frontend**

- [ ] Build the process list view — searchable table (name, PID, `active_connection_count`, status)
- [ ] Build the process detail view shell with tab structure (Overview / Connections / API Traffic / Timeline — only Connections functional this phase)
- [ ] Build the connections view — one row per socket: protocol, local/remote addr+port, state; `bytes_sent`/`bytes_received` render as an explicit "not reported" affordance when absent, never as `0` (field-level-absence rule, `docs/OBSERVATION_CONTRACT.md`)
- [ ] Wire up manual refresh (button, not yet live polling)
- [ ] Define a shared state model for views: `loading` (client-side only, never a backend status) plus the seven `ObservationStatus` states rendered verbatim — `observed` / `unavailable` / `permission_denied` / `unsupported` / `transient_failure` / `stale` / `unmatched` — driven directly by the backend's `ObservationStatus`, not inferred from data shape. There is no `empty` state: an `observed` status with zero rows renders "no connections observed"; any other status renders that status. There is no `error` state: transport failures render as `unavailable`. `stale` uses the Phase 0.1 flat 30s threshold from `docs/OBSERVATION_CONTRACT.md` (no configurable polling interval exists yet this phase).

**Demo checkpoint:** select a real running process, see its actual open sockets, values match `lsof -p <PID> -i -n -P` run by hand — including a deliberately permission-denied case rendering as "permission denied," not as an empty list (the outer `status` on `get_connections(pid)`'s envelope, per `docs/OBSERVATION_CONTRACT.md`).

## Phase 0.2 — Live Monitoring + DNS/hostname correlation

- [ ] Add polling loop with configurable interval (default 2s — see `docs/OBSERVATION_CONTRACT.md`'s staleness formula, which is relative to this value); start/stop controls in UI
- [ ] Define monitoring state machine: `idle` / `starting` / `running` / `stopping` / `stopped` / `failed`
- [ ] Track observation timestamp separately from `first_seen`/`last_seen`; surface "last updated" in the UI so a stalled poll never silently implies a live "ESTABLISHED right now" — this is exactly what `ObservationStatus.last_successful_at` is for (now always set once any successful observation has occurred, not only once `stale`)
- [ ] Switch `stale`'s threshold from Phase 0.1's flat 30s to the formula in `docs/OBSERVATION_CONTRACT.md`: `now - last_successful_at > 3 × configured poll interval`
- [ ] Handle selected-process termination: detect the PID no longer exists via `ProcessProvider`, stop monitoring it, mark it "process exited" in the UI, transition its connections to `closed` (the process-exit path in `docs/DATA_MODEL.md`'s `closed`/`expired` rule), and keep its historical observations for the current session rather than clearing them
- [ ] Emit `TrafficEvent`s for connection-opened (on the `discovered → active` transition) / connection-closed / connection-expired (from the lifecycle transitions already implemented in `ObservationEngine`), using the explicit `connection_id`/`request_id`/`response_id` fields per event type rather than one ambiguous reference field (`docs/DATA_MODEL.md`)
- [ ] Build first pass of the timeline view driven by those events
- [ ] Define `HostnameObservation` (provider-owned) and `ResolvedHostname` (Engine-owned, adds `connection_id`/`status`) per `docs/DATA_MODEL.md` — a provider must never construct `ResolvedHostname` directly, same discipline as every other Engine-owned type
- [ ] Implement `DNSProvider` (reverse DNS lookup to start; SNI/Host-header sources land in Phase 0.4 once HTTP exists)
- [ ] Surface resolved hostnames in the connections view (e.g. `api.example.com:443` instead of raw IP) using the display rule in `docs/DATA_MODEL.md` (`ResolvedHostname`'s "Display rule when sources disagree") for the case where reverse DNS, SNI, and `Host` disagree

**Demo checkpoint:** open a connection-heavy app (browser, Slack), watch new connections appear/disappear live with resolved hostnames; quit that app and confirm the UI reports "process exited" instead of a stale connection list, with that app's connections now shown `closed`.

## Phase 0.3 — Traffic Observation Backend

*Hardest phase. Don't start until 0.1–0.2 are solid and reliable.*

- [ ] Spike: confirm `mitmproxy --mode local:<pid>` works standalone against a process you control, before wiring anything into the app
- [ ] Design the core-to-helper boundary: how the Rust core launches/manages the `mitmdump`-based helper process (`std::process::Command`/`tokio::process`), and the IPC format (local socket + JSON via `serde`) between them
- [ ] Write the mitmproxy addon script: consume `request`/`response` events only — no `intercept()`, no `set()`, no replay hooks, by construction
- [ ] Implement `TrafficProvider` trait + mitmproxy-backed implementation
- [ ] Write the mitmproxy addon so it applies tier-1 (highly-sensitive) redaction before emitting anything over the helper→core IPC socket, per the shared field-name list in `docs/PRIVACY_AND_SECURITY.md`'s "Starter sensitive-field list" — the IPC schema must have no field capable of carrying an unredacted tier-1 value (`docs/PRIVACY_AND_SECURITY.md`'s "two redaction checkpoints")
- [ ] Add `ObservationCapabilities` **per provider** (not per connection) as an Engine-aggregated struct, per `docs/DATA_MODEL.md`, reported by a `get_capabilities()` command — per-connection variability stays in `ObservationStatus`, don't add a capability field to `NetworkConnection`. (Moved here from an earlier Phase 0.2 placement to match `ARCHITECTURE.md` and `DATA_MODEL.md`, both of which already said Phase 0.3.)
- [ ] Define `TrafficProvider` capability reporting (`process`, `http_metadata`, `https_metadata`, `request_body`, `response_body`, ... — the `ObservationCapabilities` fields in `docs/DATA_MODEL.md`, not a separately-invented name set) — distinguish "provider unavailable" from "this traffic type is unsupported"
- [ ] Handle the `unsupported`/`unavailable` path explicitly: pinned certs, QUIC, non-HTTP traffic must degrade to "connected, bytes only" rather than erroring
- [ ] Define `CorrelationEvidence` (pid?, protocol?, local/remote addr+port?, hostname?, timestamp, source) per `docs/DATA_MODEL.md` — the actual input to correlation, since `TrafficProvider` never has the Engine's internal `connection_id`
- [ ] **Spike, independently of the UI: `CorrelationEvidence` → `NetworkConnection` matching.** Determine which identifiers the traffic provider actually exposes (PID? local port? both?) and whether they're sufficient to reliably map a captured flow back to an existing connection
- [ ] Define and implement the `unmatched` path: evidence that can't be confidently mapped produces an `HTTPRequest` with `connection_id = None`, `status.state = Unmatched`, and `evidence` set to the `CorrelationEvidence` it was scored against (`docs/DATA_MODEL.md`) — appears in the UI as such, never silently dropped and never attached to the wrong connection by guessing
- [ ] Principle to enforce in code review, not just docs: never fabricate a `connection_id` when correlation confidence is insufficient
- [ ] Scope capture strictly to the user-selected PID — verify traffic from unrelated processes never leaks into a session

**Demo checkpoint:** select a process, trigger a known HTTP(S) request from it, see the raw flow show up in a debug log correctly attached to its connection (UI integration is next phase).

## Phase 0.4 — HTTP/HTTPS Metadata

- [ ] Define `RawHTTPRequest`/`RawHTTPResponse` (transient, in-memory only — potentially-sensitive fields held raw here) and `HTTPRequest`/`HTTPResponse` (Engine-owned, redacted, the only form that reaches storage/export/frontend — now with explicit `request_id`/`response_id`, `status`, `redacted_fields`, and optional `connection_id`/`evidence` for the `unmatched` case) per `docs/DATA_MODEL.md`
- [ ] Confirm capture-time redaction for the highly-sensitive tier (`Authorization`, API keys, passwords, tokens — the starter list in `docs/PRIVACY_AND_SECURITY.md`) is actually happening in the mitmproxy addon (Phase 0.3), not here — these must never populate even the `Raw*` types on the Rust-core side; there is no reveal path for them, by design
- [ ] Enforce the `body_preview` size limit (8 KiB, `docs/PRIVACY_AND_SECURITY.md`) at truncation time — this is the real enforced limit now, not a Phase 0.6 placeholder
- [ ] Wire `TrafficProvider` output into the Observation Engine, attached to the correct `NetworkConnection` via the correlation logic from Phase 0.3
- [ ] Add SNI and HTTP-`Host`-header as additional `HostnameObservation` sources (alongside reverse DNS from 0.2), using the confidence starter values in `docs/DATA_MODEL.md`
- [ ] Implement the `Redactor` utility for the potentially-sensitive tier: header-based (`Cookie`, `Set-Cookie`) + body/query-field heuristics, starting from the list in `docs/PRIVACY_AND_SECURITY.md`'s "Starter sensitive-field list" (tier 2 portion) — user-extendable, never user-shrinkable — producing `HTTPRequest`/`HTTPResponse` from `RawHTTPRequest`/`RawHTTPResponse` and populating `redacted_fields` with whatever was actually replaced
- [ ] Unit-test the `Redactor`: header redaction, cookie redaction, query-parameter redaction, nested-JSON body-field redaction, case-insensitive matching, configurable custom field names
- [ ] **Verify redaction happens before persistence/export, not only before display** — write a test that captures a request with a fake credential, saves/exports it, and asserts the credential never reaches disk unredacted (see the four mandatory integration tests in `docs/TESTING_STRATEGY.md`)
- [ ] Build the API traffic view — method/URL/status/duration table
- [ ] Build the request/response detail expansion view: redacted by default (using `redacted_fields` to render a distinct "redacted" affordance rather than inferring from content), a per-field "show anyway" action invokes the `reveal_raw(request_id)` Tauri command (added to the capability allowlist alongside the other Phase 0.1 commands) and reveals the paired `Raw*` object for the potentially-sensitive tier only, with highly-sensitive fields never displayable at all

**Demo checkpoint:** trigger a real HTTPS request from a test process, see method/headers/body in the UI with credentials redacted, and confirm a saved session file on disk is also redacted.

## Phase 0.5 — API Explorer

- [ ] Wire the already-defined `Flow` model (Phase 0.1) into the Engine and UI, attaching HTTP observations to it (per `docs/DATA_MODEL.md` — this is the point where it earns its keep, per ADR-004)
- [ ] Group captured requests by host → endpoint in a dedicated API view
- [ ] Compute per-endpoint stats: request count, error rate, average latency
- [ ] Build the "killer feature" tree view: process → domain → requests, collapsible

**Demo checkpoint:** run a chatty process for a few minutes, view its full API surface grouped and summarized.

## Phase 0.6 — Timeline + Session Recording & Playback

- [ ] Finalize the timeline view to interleave connection events and HTTP request/response events chronologically
- [ ] Design session storage format (local file, e.g. JSON/SQLite) capturing a full observation window
- [ ] Implement and document: sessions are stored **redacted-only, unconditionally** — no raw/unredacted storage mode, opt-in or otherwise (`docs/PRIVACY_AND_SECURITY.md`)
- [ ] Revisit the body-preview size limit (8 KiB, defined in Phase 0.4 per `docs/PRIVACY_AND_SECURITY.md`) as part of the full memory-budget/eviction-policy design — this phase covers the header/session memory limits and eviction policy the number itself already exists for, not a re-decision of the number
- [ ] Implement the `RawHTTPRequest`/`RawHTTPResponse` lifetime rule from `docs/DATA_MODEL.md`: destroy raw transient objects when a session ends (not just dereference-and-hope), cap the number of retained raw objects per session, and define the eviction policy once that cap is hit
- [ ] Ensure captured request/response contents are never written into general application logs, and audit whatever logging crate (`log`/`tracing`) and Tauri's own logging plugin are configured with so request URLs (which can carry query-string secrets) aren't logged independently of application code, bypassing the `Redactor` entirely
- [ ] Document where session files are stored on disk
- [ ] Implement "start session" / "stop & save session"
- [ ] Implement session list + reopen-for-viewing (explicitly not "resend" — no code path should be able to turn a saved request back into an outbound one)

**Demo checkpoint:** record a session, quit the app, relaunch, reopen and replay the timeline visually; inspect the saved session file directly and confirm no unredacted credential is present.

## Phase 0.7 — Filters + Search + Analytics

- [ ] Implement query-style filtering: `host:`, `status:`, `method:`, `port:`
- [ ] Add filter bar to the API traffic view and connections view
- [ ] Add basic analytics: totals, error counts, latency distribution (per process and per session)

## Phase 1.0 — Local-Run Polish + Packaged Distribution

*Under the FastAPI-era stack, packaged distribution was explicitly deferred past 1.0 (see the old Phase 2 below) because it wasn't achievable without a separate rewrite. Under Tauri (`DECISIONS.md` ADR-013), a signed native build is close to free — `tauri build` plus signing config, not a separate project — so it belongs in 1.0's actual definition of done, not a "someday" phase.*

- [ ] Build the actual "Observation Capabilities" panel UI, rendering the `ObservationCapabilities` fields from `docs/DATA_MODEL.md` directly (data plumbing and capability reporting both since Phase 0.3)
- [ ] Dark mode pass
- [ ] Export (session as JSON, or a single flow as text) — export only, never re-send; export must go through the same redacted-by-default path as session storage
- [ ] Performance pass: polling overhead, large-session memory use, confirm resource limits from 0.6 actually hold under sustained high-traffic load
- [ ] Code signing and notarization: Apple Developer certificate, `tauri.conf.json` signing identity, `notarytool` submission — set this up early enough in the phase to catch certificate/entitlement issues before they're a release blocker
- [ ] Write a reliable build/run guide: `cargo tauri build` for the distributable signed `.app`, plus a `cargo tauri dev` note for running from source during development
- [ ] Write end-user README: what this tool does, what it deliberately does not do, and the "things you own or have permission to inspect" scope note from the original learning path
- [ ] Run a full end-to-end regression pass before calling this 1.0: process discovery → socket observation → lifecycle tracking → DNS correlation → HTTP observation → redaction → session storage → session reopening. A checklist walkthrough is enough at this project's scale — this doesn't need CI infrastructure, just a deliberate pass through the whole chain instead of assuming the individual phase demo checkpoints still compose correctly together

## Phase 2 — Network Extension Upgrade (future, optional — not required to consider this project done)

Only pursue this if mitmproxy's local-mode ceiling (certificate pinning, no packet-level capture) becomes an actual blocker for something you need to see. Not part of 1.0's definition of done — 1.0 ships with the mitmproxy-based `TrafficProvider` as its traffic-capture mechanism, packaged and signed.

- [ ] Design and build a Swift/ObjC system-extension target implementing `NEPacketTunnelProvider`/`NEFilterDataProvider`, embedded inside this app's existing signed `.app` bundle (see `docs/DECISIONS.md` ADR-013 and `docs/PERMISSIONS_AND_PLATFORM.md` — this is still real, separate native work even though the host app is already native). **The extension is observe-only by construction:** a `NEFilterDataProvider` implementation must return an allow verdict on every flow unconditionally, with no configuration path making that conditional — `NEFilterDataProvider` is a filtering API by nature, and any deviation from unconditional-allow is a violation of `docs/process-network-inspector-report.md` §2 that requires reopening that section first, not a Phase 2 implementation detail.
- [ ] File the Network Extension entitlement request early and treat Apple's review lead time as its own milestone
- [ ] Design the XPC or equivalent IPC boundary between the main Rust core and the Swift extension, following the same `TrafficProvider` interface contract so the rest of the Engine doesn't need to change

---

## Cross-cutting (ongoing, not a single phase)

- [ ] Build a small deterministic `NetworkTestTarget` script early (useful starting in Phase 0.1, essential by 0.3) that generates known traffic on demand: a plain TCP connection, a short-lived connection, a long-lived connection, several simultaneous connections, a couple of HTTP(S) requests once Phase 0.3 exists, and requests carrying intentionally fake sensitive-looking fields for redaction testing. This is a standalone traffic-generating script, not part of the app itself — Python is a fine, simple choice for it regardless of the app's own stack. Same principle the source learning path opened with — verify the tool against traffic you already understand before pointing it at anything else.
- [ ] Unit tests per provider (`cargo test`, mock the system-call boundary so tests don't depend on real running processes)
- [ ] Add integration tests for the observation pipeline using `NetworkTestTarget`: generate known connections/HTTP requests and verify they come out the other end with correct process attribution, connection identity, lifecycle events, hostname correlation, HTTP correlation, and redaction — unit tests per provider don't catch a correlation bug in `ObservationEngine`, only a test that exercises the full chain does. This includes the four mandatory tests in `docs/TESTING_STRATEGY.md`: process termination, polling-gap → `expired`, correlation ambiguity → `unmatched`, and provider failure must not manufacture expiry.
- [ ] Keep `docs/PERMISSIONS_AND_PLATFORM.md`'s VERIFIED/ASSUMED/DECISION tags current as permission reality gets discovered
- [ ] Re-check the "explicitly out of scope" list (`docs/process-network-inspector-report.md` Section 2) at the start of every phase — no feature in this roadmap should ever grow into edit/replay/inject

## Definition of done for the whole project

Select a running process and obtain Levels 1–3 (process, network, protocol) wherever those observations are technically and legitimately available, with the UI explicitly distinguishing **observed**, **denied**, **unsupported**, and **stale** information rather than presenting any of them as empty or current. Level 4 (HTTP/application payload) is available only where technically and legitimately observable, degrading to the same explicit categories rather than silence. Highly-sensitive data is never captured in raw form at all; potentially-sensitive data is redacted by default and irreversibly before it ever reaches storage or export. Nothing in the app is capable of modifying, replaying, or injecting traffic.
