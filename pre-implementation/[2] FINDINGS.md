# Findings — Pre-Implementation Audit Log

Append-only, like `docs/[2] DECISIONS.md`. Never delete an entry — update its
`Status` field instead. The point is answering "did we check this, and what did we
decide" later, including findings that turned out to be non-issues.

`Status` values: `Open` (needs a decision) → `Fixed` (doc updated, cite the commit/
edit) or `Deferred` (deliberately left for a later phase, with why) or `Not an issue`
(investigated, no change needed, with why).

---

## Round 1 — self-audit, run 2026-09-06 (Claude, same session that wrote the docs)

Run against `docs/` as of commit `208fd72`. Full category-by-category output kept in
the session transcript; this is the distilled, actionable form.

### PIF-001 — `ProcessInfo` has no `ObservationStatus`; list-command envelope undefined
**Status:** Open
**Severity:** BLOCKING
**Location:** `docs/[4] DATA_MODEL.md` (`ProcessInfo` table) vs. `docs/[3] ARCHITECTURE.md`
("Error propagation") vs. `docs/[5] OBSERVATION_CONTRACT.md` ("Applied per layer")
**Issue:** `OBSERVATION_CONTRACT.md` defines a provider-level status axis for the
process layer (`observed | permission_denied | unavailable`), distinct from whether
the process itself is running. `ARCHITECTURE.md` says the Engine aggregates and Tauri
serializes a status "for a given connection/**process**." But `DATA_MODEL.md`'s
`ProcessInfo` table has no `ObservationStatus` field — only `status: enum{Running,
Exited}`, which answers a different question. `NetworkConnection` got the field
explicitly (`status: ObservationStatus`); `ProcessInfo` didn't. There is also no
documented top-level envelope for list-returning commands (`get_processes`,
`get_connections(pid)`) to carry a whole-call failure like `permission_denied` — only
per-item status is specified anywhere.
**Why it matters:** `docs/[9] TODO.md` Phase 0.1's own demo checkpoint requires "a
deliberately permission-denied case rendering as 'permission denied,' not as an empty
list" for the process list. As currently specified there is no field to carry that.
An implementer will invent a shape on the spot when they hit `get_processes`,
inconsistent with the pattern `NetworkConnection` already established.
**Proposed fix:**
1. Add an explicit `ObservationStatus` field to `ProcessInfo` in `DATA_MODEL.md`
   (rename the existing `Running/Exited` field, e.g. to `process_state`, freeing
   `status` for the standard meaning — or name the new field something unambiguous
   like `observation_status` if renaming the existing one is too disruptive).
2. Define the top-level response envelope for list-returning Tauri commands
   explicitly, e.g. `{ status: ObservationStatus, data: Option<Vec<T>> }` — generalize
   the single-object pattern `OBSERVATION_CONTRACT.md`'s JSON example already shows.

---

### PIF-002 — No type holds the `unmatched` correlation outcome
**Status:** Open
**Severity:** SHOULD-FIX-BEFORE-CODING (blocks Phase 0.3, not Phase 0.1)
**Location:** `docs/[4] DATA_MODEL.md` (`TrafficEvent`, `CorrelationEvidence`) vs.
`docs/[8] TESTING_STRATEGY.md` (mandatory test 3)
**Issue:** `unmatched` is described in prose as an outcome of a correlation attempt,
but no documented type has a field to hold it. `TrafficEvent`'s table has no `status`
field; `CorrelationEvidence` is pure input, not a result type.
**Why it matters:** mandatory test 3 says "assert the result is `unmatched`" with
nothing typed to assert against — this gets invented ad hoc during Phase 0.3 coding,
risking drift from how `unmatched`/other statuses are represented elsewhere.
**Proposed fix:** add `status: Option<ObservationStatus>` to `TrafficEvent`, or define
a small `CorrelationResult` type wrapping `CorrelationEvidence` + `ObservationStatus`
+ optional `connection_id`.

---

### PIF-003 — `ObservationState` enum never given a Rust/serde spelling
**Status:** Open
**Severity:** SHOULD-FIX-BEFORE-CODING
**Location:** `docs/[4] DATA_MODEL.md` / `docs/[5] OBSERVATION_CONTRACT.md`
**Issue:** every other enum in `DATA_MODEL.md` is written in explicit Rust notation
(`enum { Discovered, Active, Closed, Expired }`, etc.). `ObservationState` — the one
attached to every emitted object — is only ever written as a lowercase prose list
(`observed | unavailable | ...`), and the JSON example shows lowercase serialization
(`"state": "stale"`), but the actual variant names and serde rename convention needed
to get lowercase JSON from PascalCase Rust variants are never stated.
**Why it matters:** cheap to get wrong (missing `#[serde(rename_all = "snake_case")]`),
and the failure mode is a frontend string-match bug that surfaces at integration time,
not compile time.
**Proposed fix:** one line in `DATA_MODEL.md` giving the actual enum declaration and
serde attribute.

---

### PIF-004 — body-preview size limit needed in Phase 0.4, defined in Phase 0.6
**Status:** Open
**Severity:** SHOULD-FIX-BEFORE-CODING (blocks Phase 0.4, not Phase 0.1)
**Location:** `docs/[9] TODO.md` (Phase 0.4 vs. Phase 0.6) / `docs/[4] DATA_MODEL.md`
(`body_preview`) / `docs/[6] PRIVACY_AND_SECURITY.md`
**Issue:** `HTTPRequest`/`HTTPResponse.body_preview` is documented as "truncated per
the size limit in `PRIVACY_AND_SECURITY.md`," and that doc says the limit is
"enforced at capture time" — but the actual numeric limit isn't defined until TODO
Phase 0.6, two phases after Phase 0.4 implements the `Redactor` that's supposed to
already be truncating against it.
**Why it matters:** Phase 0.4 either blocks on an undefined constant or someone picks
an arbitrary number Phase 0.6 then has to reconcile/migrate.
**Proposed fix:** move the size-limit *definition* (not the full memory-budget/
eviction-policy work, which can stay in 0.6) earlier into Phase 0.4, or explicitly
note Phase 0.4 uses a provisional constant revisited in 0.6.

---

### PIF-005 — connection-matching algorithm underspecified
**Status:** Open
**Severity:** SHOULD-FIX-BEFORE-CODING (Phase 0.1, load-bearing)
**Location:** `docs/[4] DATA_MODEL.md` / `docs/[2] DECISIONS.md` ADR-006, ADR-012 /
`docs/[9] TODO.md` Phase 0.1
**Issue:** the identity-matching heuristic is specified only as policy ("prefer split
over merge") and "deterministic-where-possible, heuristic-where-not" — never which
field combination is the actual match key, or what counts as ambiguous. This is the
product's core differentiator (ADR-001), and the four mandatory tests assume it's
testable.
**Why it matters:** getting this wrong risks the "false merge" failure mode the docs
call out repeatedly as the worse of the two failure modes — worth pinning down before,
not after, it's load-bearing in test fixtures.
**Proposed fix:** not full pseudocode, but name the primary composite key candidates
in `DATA_MODEL.md` (e.g. `pid + protocol + local_addr + local_port` as anchor,
`remote_addr + remote_port` as tiebreaker when present) before Phase 0.1's Engine work
starts.

---

### PIF-006 — staleness threshold formula undefined; `stale` unreachable in Phase 0.1
**Status:** Open
**Severity:** WORTH-NOTING
**Location:** `docs/[5] OBSERVATION_CONTRACT.md` (`stale`) / `docs/[9] TODO.md`
Phase 0.1 frontend state list / Phase 0.2
**Issue:** `stale` "requires knowing the expected polling cadence," but no doc gives
the actual formula relative to the user-configurable interval introduced in Phase
0.2. `stale` also appears in Phase 0.1's frontend state list even though no polling
loop/cadence concept exists yet that phase (manual refresh only) — harmless
(unreachable state) but worth a one-line clarification.
**Why it matters:** low urgency — doesn't block Phase 0.1 — but should be resolved
before Phase 0.2 actually implements "surface last updated in the UI."
**Proposed fix:** add the staleness formula (e.g. "stale if `now - last_successful_at
> 2 × polling_interval`") to `DATA_MODEL.md` or `ARCHITECTURE.md` before Phase 0.2
starts; optionally drop `stale` from Phase 0.1's frontend state list until it's
reachable.

---

### Categories checked, no findings (Round 1)
- **Assumed-vs-verified facts (category 4):** clean. `docs/[9] TODO.md` Phase 0's
  first task is the permissions spike, correctly gating the ASSUMED claims in
  `docs/[7] PERMISSIONS_AND_PLATFORM.md` before any provider code is written.
- **Scope-creep tripwire (category 7):** clean across all phases including Phase 2 —
  nothing edges toward modify/replay/inject; export and session-reopen explicitly
  forbid resend.
- **Phase-boundary leakage (category 6), general:** clean except PIF-006's minor
  note above — Phase 0.1 doesn't otherwise depend on `Flow`, `ObservationCapabilities`,
  or a real `TrafficProvider` implementation.

---

<!-- Round 2 entries go here, once a second model (or a re-run after fixes) produces new findings. -->
