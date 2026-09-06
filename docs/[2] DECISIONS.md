# Decisions — Process Network Inspector

Lightweight ADR log, one entry per decision that took real deliberation to reach. Kept as a single file rather than one-file-per-decision for easier scanning at this project's size. Update the `Status` field when a later decision supersedes an earlier one — never delete a superseded entry, since the point of this document is answering "why did we build it this way" months later, including the false starts.

---

### ADR-001: Correlated view (process → connection → domain → request) is the product
**Status:** Accepted
**Decision:** The differentiator isn't any single capability (`lsof`, a packet analyzer, an HTTP proxy already do one layer each) — it's joining them into one correlated tree per process.
**Why:** Established early in the project report and never revisited since; every later architectural choice (the provider pattern, the Observation Engine as a distinct layer) exists to serve this.

---

### ADR-002: Read-only, split into two guarantees
**Status:** Accepted
**Decision:** "Read-only" means both (a) never modifying/injecting/pausing the target process, and (b) never modifying/replaying/injecting the traffic it's observed making.
**Why:** Keeping these separate in the architecture (rather than one vague rule) makes it unambiguous which capabilities are permanently out of scope regardless of future feature requests.

---

### ADR-003: Provider-pattern architecture (`ProcessProvider`/`SocketProvider`/`DNSProvider`/`TrafficProvider`)
**Status:** Accepted
**Decision:** Each concern lives behind a small interface; no single layer's implementation choice (a specific tool, a specific API) leaks into the rest of the app.
**Why:** The traffic-capture layer in particular was known to be uncertain from the start — this lets "no traffic provider" become "mitmproxy-based" become "something else later" without touching the Engine or the UI.
**Guardrail:** Define interfaces early since they're cheap; don't build swappable-backend machinery (multiple concrete implementations, runtime selection) until there's an actual second implementation to swap in.

---

### ADR-004: `Flow` deferred to v0.3, defined now
**Status:** Accepted
**Decision:** The `Flow` type (wrapping a connection plus its attached observations) is written as a struct/model immediately, but the Engine and UI don't route through it until HTTP observation exists in v0.3.
**Why:** Wiring it in earlier is architecture for a shape (multiple observation types per connection) that doesn't exist until HTTP arrives. Same YAGNI guardrail as ADR-003, applied to a data type instead of a provider.

---

### ADR-005: Redaction is mandatory before persistence/export, optional (reversible) before display
**Status:** Accepted
**Decision:** Two separate checkpoints, not one — see `PRIVACY_AND_SECURITY.md`.
**Why:** A `Redactor` that only filters at display time while sessions store raw bodies to disk is a credential-leak vector, caught during TODO review rather than during an actual incident.

---

### ADR-006: Connection identity via snapshot diffing, not addr/port re-matching
**Status:** Accepted
**Decision:** `SocketProvider` returns an immutable `SocketSnapshot` per poll; the Observation Engine — not the provider — is responsible for matching connections across snapshots and producing lifecycle transitions.
**Why:** Addr/port tuples aren't a reliable identity across polls (port reuse, rapid close/reopen). Separating "what do you see right now" (provider) from "what changed since last time" (Engine) keeps the hard part in one place.

---

### ADR-007: Provider status is a typed enum, never a bare success/failure
**Status:** Accepted
**Decision:** Every provider resolves to one of `observed` / `unavailable` / `permission_denied` / `unsupported` / `transient_failure` / `stale` / `unmatched` — see `OBSERVATION_CONTRACT.md`.
**Why:** "No data" was on track to mean five different things across the codebase (permission denied vs. genuinely empty vs. stale vs. not yet supported), which is one of the most common failure modes in monitoring tools specifically.

---

### ADR-008: Initial stack — native Swift + SwiftUI
**Status:** Superseded by ADR-009
**Decision (at the time):** Build as a native macOS app: SwiftUI frontend, Swift backend logic, with the mitmproxy-based `TrafficProvider` implementation quarantined behind the provider interface as the one place a Python dependency would show up.
**Why (at the time):** This is fundamentally a macOS system-observation application — permissions, process/socket APIs, and a theoretical future Network Extension–based `TrafficProvider` all fit more naturally into a signed native app than a browser-facing service.
**What changed:** See ADR-009.

---

### ADR-009: Revised stack — FastAPI backend + React frontend
**Status:** Accepted, supersedes ADR-008
**Decision:** Full Python backend (FastAPI, REST + WebSocket) with a React frontend, run locally rather than shipped as a native `.app`.
**Why:** Debuggability, for a project being built incrementally rather than shipped once — standard browser devtools, hot reload, and Python's ordinary debugging tools matter more here than native polish. It also collapses the earlier hybrid design: mitmproxy no longer needs to be quarantined behind a language boundary inside a Swift app, since the whole backend is already Python.
**What this costs, explicitly (don't rediscover this later):**
- The Mac App Store / notarized native distribution path is gone; this ships as source you run locally, not a signed app (see `PERMISSIONS_AND_PLATFORM.md`).
- The Network Extension upgrade path for `TrafficProvider`, previously listed as a "known future cost" in the project report, is no longer achievable as a pure Python component — `NetworkExtension.framework` entitlements go to signed native apps, not Python processes. It isn't permanently impossible, but it now requires introducing a separate, separately-signed native helper alongside this stack rather than a Python-only upgrade; the mitmproxy-based approach is this project's practical ceiling for traffic capture unless and until that helper gets built.
- A FastAPI server introduces an actual local network-facing surface that a native SwiftUI app never had — must be bound to `127.0.0.1` and CORS-locked (see `PRIVACY_AND_SECURITY.md`); this risk didn't exist before this decision.
**What doesn't change:** every architectural decision above this one (ADR-001 through ADR-007) — the provider pattern, the Observation Engine, the data model, the redaction rules, and the observation contract are all language-agnostic and carry over unmodified.

---

### ADR-010: Testing strategy centers on a deterministic `NetworkTestTarget`
**Status:** Accepted
**Decision:** Build a small script/executable that generates known, repeatable network and HTTP traffic on demand, and write both unit and integration tests against it rather than relying on manually poking at Chrome/Slack.
**Why:** Directly mirrors the source learning path's own opening principle — verify a tool against traffic you already understand before pointing it at anything else. Applies equally to testing this app's own providers.

---

### ADR-011: Cross-document consistency pass — split provider-owned vs. Engine-owned types, formalize correlation and status
**Status:** Accepted
**Decision:** A review of the design docs as a set (not individually) found real contradictions rather than just missing depth, and this ADR records the fixes:
- `SocketProvider` returns `SocketObservation` (raw, no identity/lifecycle), never `NetworkConnection` (Engine-owned, carries `connection_id`/`lifecycle_state`/`first_seen`/`last_seen`). The earlier data model had the provider returning `NetworkConnection` directly while also saying the Engine exclusively owns some of its fields — a real type-level contradiction, not a style issue.
- Introduced `CorrelationEvidence` as the formal, explicit input to traffic-to-connection correlation, since a `TrafficProvider` never has the Engine's internal `connection_id` to begin with — it has PID/port/hostname evidence that the Engine must match.
- `closed` requires positive evidence a connection ended; `expired` is the default when a connection simply stops appearing in polls and closure can't be proven. An earlier draft risked collapsing "missing from a poll" straight into "closed," which would have quietly overclaimed certainty the app doesn't have.
- Split the Observation Contract's status vocabulary into provider-determinable statuses (`observed`, `unavailable`, `permission_denied`, `unsupported`, `transient_failure`) and Engine-derived statuses (`stale`, `unmatched`) that require context — polling cadence, another provider's output — no single provider has. Formalized as an `ObservationStatus` type attached to every domain object, rather than a bare status field.
- Resolved a genuine contradiction between `ARCHITECTURE.md` (raw values survive in memory for "show anyway") and `DATA_MODEL.md` (headers redacted "before this object exists"): potentially-sensitive fields get the raw-transient/redacted-persistent split (`RawHTTPRequest` vs `HTTPRequest`); highly-sensitive fields are redacted irreversibly at capture time with no raw form ever existing, and therefore no reveal path.
- Softened "the Network Extension upgrade path is effectively foreclosed" to the accurate version: not achievable as a pure Python component, but not permanently impossible — it requires a separate native helper, which the TODO already anticipated elsewhere. The stronger wording was inconsistent with that already-documented escape hatch.
**Why this matters:** every fix above makes an already-agreed principle ("providers report observations, the Engine creates domain state," "never claim more certainty than the system actually has") true at the type level instead of just true in prose. None of it is new architectural surface area.

---

### ADR-012: Second consistency pass — finish the provider/Engine split, tighten lifecycle honesty, bound raw-data lifetime
**Status:** Accepted. This is the last review-driven pass before Phase 0.1 — further changes should come from implementation experience, not another reading of these documents.
**Decision:**
- Applied the `SocketObservation`/`NetworkConnection` split (ADR-011) to processes too: `ProcessObservation` (provider) vs. `ProcessInfo` (Engine, adds `status`). The exact same contradiction ADR-011 fixed for sockets had been missed on the process side.
- Tightened the `closed` definition: a socket missing from a snapshot is not positive evidence of closure, since polling has no distinct close signal separate from absence. `closed` is expected to be rare-to-unreachable for the polling-only providers through Phase 0.2; `expired` is the normal outcome.
- Added an explicit identity-matching principle: `connection_id` is an Engine session identity, not an OS-level one, and the Engine prefers a false split over a false merge when matching confidence is insufficient — a false merge corrupts the timeline, a false split is merely redundant.
- Replaced `TrafficEvent.payload_ref` (one ambiguous string) with explicit `connection_id`/`request_id`/`response_id` fields.
- Established a lifetime rule for `RawHTTPRequest`/`RawHTTPResponse`: destroyed when a session ends, bounded by a memory budget and eviction policy, never retained indefinitely just because the app keeps running.
- Removed "raw storage, if ever offered" from `PRIVACY_AND_SECURITY.md` — raw data never reaching disk is now an unconditional invariant, not a default with a hypothetical opt-out nobody asked for.
- Documented `ObservationCapabilities` (what a provider can ever observe) as distinct from `ObservationStatus` (one observation's outcome right now) — not implemented yet, but named now so Phase 0.3/0.4 don't collapse the two into one field.
- Added a fourth mandatory integration test: a provider's `transient_failure` must never be misread by the lifecycle-diffing logic as every tracked connection disappearing at once.
- Consolidated "the Engine is the only component permitted to create or mutate domain state" into one explicit rule in `ARCHITECTURE.md`, rather than leaving it inferable from several scattered notes.
**Why:** all of the above are either finishing a fix from ADR-011 that was applied inconsistently, or tightening wording to match what the rest of the document set already implied. None of it introduces new capability surface or a new document.
