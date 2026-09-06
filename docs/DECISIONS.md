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
- The Network Extension upgrade path for `TrafficProvider`, previously listed as a "known future cost" in the project report, is now effectively foreclosed — `NetworkExtension.framework` entitlements go to signed native apps, not Python processes. The mitmproxy-based approach is this project's practical ceiling for traffic capture unless a separate native helper is built later.
- A FastAPI server introduces an actual local network-facing surface that a native SwiftUI app never had — must be bound to `127.0.0.1` and CORS-locked (see `PRIVACY_AND_SECURITY.md`); this risk didn't exist before this decision.
**What doesn't change:** every architectural decision above this one (ADR-001 through ADR-007) — the provider pattern, the Observation Engine, the data model, the redaction rules, and the observation contract are all language-agnostic and carry over unmodified.

---

### ADR-010: Testing strategy centers on a deterministic `NetworkTestTarget`
**Status:** Accepted
**Decision:** Build a small script/executable that generates known, repeatable network and HTTP traffic on demand, and write both unit and integration tests against it rather than relying on manually poking at Chrome/Slack.
**Why:** Directly mirrors the source learning path's own opening principle — verify a tool against traffic you already understand before pointing it at anything else. Applies equally to testing this app's own providers.
