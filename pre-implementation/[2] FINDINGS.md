# Findings — Pre-Implementation Audit Log

Append-only, like `docs/[2] DECISIONS.md`. Never delete an entry — update its
`Status` field instead. The point is answering "did we check this, and what did we
decide" later, including findings that turned out to be non-issues.

`Status` values: `Open` (needs a decision) → `Fix now` (decided in step B, doc edit
not yet applied — becomes `Fixed` once step C lands the edit, at which point this
entry is amended with the commit/edit citation) or `Deferred` (deliberately left for
a later phase, with which phase and why) or `Not an issue` (investigated, no change
needed, with why).

---

## Round 1 — self-audit, run 2026-09-06 (Claude, same session that wrote the docs)

Run against `docs/` as of commit `208fd72`. Full category-by-category output kept in
the session transcript; this is the distilled, actionable form.

### At a glance

| ID | Severity | Status | Summary |
|---|---|---|---|
| [PIF-001](#pif-001--processinfo-has-no-observationstatus-list-command-envelope-undefined) | BLOCKING | Fixed | `ProcessInfo` has no `ObservationStatus`; list-command envelope undefined |
| [PIF-002](#pif-002--no-type-holds-the-unmatched-correlation-outcome) | SHOULD-FIX-BEFORE-CODING (blocks Phase 0.3) | Fixed | No type holds the `unmatched` correlation outcome |
| [PIF-003](#pif-003--observationstate-enum-never-given-a-rustserde-spelling) | SHOULD-FIX-BEFORE-CODING | Fixed | `ObservationState` enum never given a Rust/serde spelling |
| [PIF-004](#pif-004--body-preview-size-limit-needed-in-phase-04-defined-in-phase-06) | SHOULD-FIX-BEFORE-CODING (blocks Phase 0.4) | Fixed (definition only) | body-preview size limit needed in Phase 0.4, defined in Phase 0.6 |
| [PIF-005](#pif-005--connection-matching-algorithm-underspecified) | SHOULD-FIX-BEFORE-CODING (Phase 0.1, load-bearing) | Fixed | Connection-matching algorithm underspecified |
| [PIF-006](#pif-006--staleness-threshold-formula-undefined-stale-unreachable-in-phase-01) | WORTH-NOTING (Round 2: BLOCKING) | Fixed | Staleness threshold formula undefined; `stale` unreachable in Phase 0.1 |

**Step B decisions recorded 2026-09-07; step C applied 2026-09-07 in commit
`fc4ec5e`** — see each entry below for reasoning. 36 of 37 combined findings
(Round 1 + Round 2) are `Fixed`; one (PIF-016) is `Deferred`. Nothing came back
`Not an issue` — every finding was independently confirmed against the actual doc
text while deciding. See `[3] TODO.md` step B note on the two severity disagreements
(PIF-002, PIF-006) between Round 1 and Round 2, both resolved as fix-now regardless
of exactly which severity label applies.

---

### PIF-001 — `ProcessInfo` has no `ObservationStatus`; list-command envelope undefined

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: BLOCKING per Round 1's own severity, confirmed independently by Round 2 (which also nailed the envelope shape — see cross-check below); `TODO.md` Phase 0.1's demo checkpoint is unimplementable without this. Cheap doc edit, no reason to defer. |
| **Severity** | BLOCKING |
| **Location** | `docs/[4] DATA_MODEL.md` (`ProcessInfo` table) vs. `docs/[3] ARCHITECTURE.md` ("Error propagation") vs. `docs/[5] OBSERVATION_CONTRACT.md` ("Applied per layer") |

**Issue**

`OBSERVATION_CONTRACT.md` defines a provider-level status axis for the
process layer (`observed | permission_denied | unavailable`), distinct from whether
the process itself is running. `ARCHITECTURE.md` says the Engine aggregates and Tauri
serializes a status "for a given connection/**process**." But `DATA_MODEL.md`'s
`ProcessInfo` table has no `ObservationStatus` field — only `status: enum{Running,
Exited}`, which answers a different question. `NetworkConnection` got the field
explicitly (`status: ObservationStatus`); `ProcessInfo` didn't. There is also no
documented top-level envelope for list-returning commands (`get_processes`,
`get_connections(pid)`) to carry a whole-call failure like `permission_denied` — only
per-item status is specified anywhere.

**Why it matters**

`docs/[9] TODO.md` Phase 0.1's own demo checkpoint requires "a
deliberately permission-denied case rendering as 'permission denied,' not as an empty
list" for the process list. As currently specified there is no field to carry that.
An implementer will invent a shape on the spot when they hit `get_processes`,
inconsistent with the pattern `NetworkConnection` already established.

**Proposed fix**

1. Add an explicit `ObservationStatus` field to `ProcessInfo` in `DATA_MODEL.md`
   (rename the existing `Running/Exited` field, e.g. to `process_state`, freeing
   `status` for the standard meaning — or name the new field something unambiguous
   like `observation_status` if renaming the existing one is too disruptive).
2. Define the top-level response envelope for list-returning Tauri commands
   explicitly, e.g. `{ status: ObservationStatus, data: Option<Vec<T>> }` — generalize
   the single-object pattern `OBSERVATION_CONTRACT.md`'s JSON example already shows.

**Round 2 cross-check:** independently rediscovered, in more detail, by a different
model with no access to this entry. Round 2 additionally names the exact field-naming
collision (`ProcessInfo.status` used for both `Running/Exited` *and* would-be
`ObservationStatus`) and shows the envelope gap forces the empty-array-means-unknown
inference the design explicitly forbids. See PIF-007 for the related provider-side
gap this connects to. Two independent findings on the same root cause — treat as a
stronger signal in step B, not a duplicate.

---

### PIF-002 — No type holds the `unmatched` correlation outcome

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: doesn't block Phase 0.1 (confirmed — `HTTPRequest` isn't touched until Phase 0.3/0.4), but Round 2 showed the current `HTTPRequest.connection_id: String` (required) actively forbids constructing the unmatched case, not just "lacks a status field" — worth closing now, cheap, while `DATA_MODEL.md` is already open in this pass. Not escalating the recorded severity to Round 2's BLOCKING since it genuinely isn't Phase-0.1-blocking, but treating it with the same urgency by fixing now anyway. |
| **Severity** | SHOULD-FIX-BEFORE-CODING (blocks Phase 0.3, not Phase 0.1) |
| **Location** | `docs/[4] DATA_MODEL.md` (`TrafficEvent`, `CorrelationEvidence`) vs. `docs/[8] TESTING_STRATEGY.md` (mandatory test 3) |

**Issue**

`unmatched` is described in prose as an outcome of a correlation attempt,
but no documented type has a field to hold it. `TrafficEvent`'s table has no `status`
field; `CorrelationEvidence` is pure input, not a result type.

**Why it matters**

Mandatory test 3 says "assert the result is `unmatched`" with
nothing typed to assert against — this gets invented ad hoc during Phase 0.3 coding,
risking drift from how `unmatched`/other statuses are represented elsewhere.

**Proposed fix**

Add `status: Option<ObservationStatus>` to `TrafficEvent`, or define
a small `CorrelationResult` type wrapping `CorrelationEvidence` + `ObservationStatus`
+ optional `connection_id`.

**Round 2 cross-check:** independently rediscovered — and rated **BLOCKING**, not
SHOULD-FIX, because Round 2 traced the actual mechanism: `HTTPRequest.connection_id`
is `String`, required, and the design text explicitly forbids both a guessed
`connection_id` and a silently-dropped flow, so an unmatched observation currently
has *no* legal representation at all (not just "no status field" — the required-field
type actively forbids constructing the unmatched case). Round 2's proposed fix:
`HTTPRequest.connection_id` → `Option<String>`, add `status: ObservationStatus` to
both `HTTPRequest`/`HTTPResponse`, and add `evidence: Option<CorrelationEvidence>` to
retain what an unmatched request was scored against. Flag the severity mismatch
(SHOULD-FIX vs. BLOCKING) for step B.

---

### PIF-003 — `ObservationState` enum never given a Rust/serde spelling

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: one-line addition; every Phase 0.1 type crossing the IPC boundary depends on the convention being stated once. Bundling `lifecycle_state`'s identical gap in (Round 2 cross-check) rather than writing two near-duplicate notes. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[4] DATA_MODEL.md` / `docs/[5] OBSERVATION_CONTRACT.md` |

**Issue**

Every other enum in `DATA_MODEL.md` is written in explicit Rust notation
(`enum { Discovered, Active, Closed, Expired }`, etc.). `ObservationState` — the one
attached to every emitted object — is only ever written as a lowercase prose list
(`observed | unavailable | ...`), and the JSON example shows lowercase serialization
(`"state": "stale"`), but the actual variant names and serde rename convention needed
to get lowercase JSON from PascalCase Rust variants are never stated.

**Why it matters**

Cheap to get wrong (missing `#[serde(rename_all = "snake_case")]`),
and the failure mode is a frontend string-match bug that surfaces at integration time,
not compile time.

**Proposed fix**

One line in `DATA_MODEL.md` giving the actual enum declaration and
serde attribute.

**Round 2 cross-check:** independently rediscovered (same gap: no stated serde-rename
convention across the IPC boundary). Round 2 also noted `lifecycle_state`'s Rust
variants (`Discovered, Active, Closed, Expired`) vs. its lowercase prose usage
everywhere else has the identical unstated-convention problem — worth covering with
the same one-line fix rather than just `ObservationState`.

---

### PIF-004 — body-preview size limit needed in Phase 0.4, defined in Phase 0.6

| | |
|---|---|
| **Status** | Fixed, definition only — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: only the numeric *definition* moves earlier (into `PRIVACY_AND_SECURITY.md`/`DATA_MODEL.md`, referenced by Phase 0.4); the full memory-budget/eviction-policy work explicitly stays in Phase 0.6, which already has its own TODO items for that. No conflict between "fix now" and "Phase 0.6 still does the rest." |
| **Severity** | SHOULD-FIX-BEFORE-CODING (blocks Phase 0.4, not Phase 0.1) |
| **Location** | `docs/[9] TODO.md` (Phase 0.4 vs. Phase 0.6) / `docs/[4] DATA_MODEL.md` (`body_preview`) / `docs/[6] PRIVACY_AND_SECURITY.md` |

**Issue**

`HTTPRequest`/`HTTPResponse.body_preview` is documented as "truncated per
the size limit in `PRIVACY_AND_SECURITY.md`," and that doc says the limit is
"enforced at capture time" — but the actual numeric limit isn't defined until TODO
Phase 0.6, two phases after Phase 0.4 implements the `Redactor` that's supposed to
already be truncating against it.

**Why it matters**

Phase 0.4 either blocks on an undefined constant or someone picks
an arbitrary number Phase 0.6 then has to reconcile/migrate.

**Proposed fix**

Move the size-limit *definition* (not the full memory-budget/
eviction-policy work, which can stay in 0.6) earlier into Phase 0.4, or explicitly
note Phase 0.4 uses a provisional constant revisited in 0.6.

---

### PIF-005 — connection-matching algorithm underspecified

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: load-bearing for the very first Engine logic written in Phase 0.1 (ADR-001's core differentiator); Round 2 independently confirmed the circular citation and supplied a concrete, adoptable algorithm (see cross-check below) — using it near-verbatim rather than re-deriving one. |
| **Severity** | SHOULD-FIX-BEFORE-CODING (Phase 0.1, load-bearing) |
| **Location** | `docs/[4] DATA_MODEL.md` / `docs/[2] DECISIONS.md` ADR-006, ADR-012 / `docs/[9] TODO.md` Phase 0.1 |

**Issue**

The identity-matching heuristic is specified only as policy ("prefer split
over merge") and "deterministic-where-possible, heuristic-where-not" — never which
field combination is the actual match key, or what counts as ambiguous. This is the
product's core differentiator (ADR-001), and the four mandatory tests assume it's
testable.

**Why it matters**

Getting this wrong risks the "false merge" failure mode the docs
call out repeatedly as the worse of the two failure modes — worth pinning down before,
not after, it's load-bearing in test fixtures.

**Proposed fix**

Not full pseudocode, but name the primary composite key candidates
in `DATA_MODEL.md` (e.g. `pid + protocol + local_addr + local_port` as anchor,
`remote_addr + remote_port` as tiebreaker when present) before Phase 0.1's Engine work
starts.

**Round 2 cross-check:** independently rediscovered, and confirmed the two documents
that are supposed to define this (`DATA_MODEL.md`'s `connection_id` note and
`TODO.md` Phase 0.1) actually just cite each other in a circle — neither contains a
mechanism. Round 2 also found there is no `confidence` field anywhere on the socket
side (it exists only on `HostnameObservation`), so "insufficient confidence" as
written isn't expressible against any defined type. Round 2's concrete proposal:
match key = exact-tuple equality on `(pid, protocol, local_addr, local_port,
remote_addr, remote_port)` against the immediately preceding *successful* snapshot;
`state` does not participate in matching; on multiple matches, split (new
connection) rather than merge, per the existing split-over-merge policy; Phase 0.1
needs no numeric confidence score at all — "insufficient confidence" reduces to "not
an exact match." Worth adopting close to verbatim in step C.

---

### PIF-006 — staleness threshold formula undefined; `stale` unreachable in Phase 0.1

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: siding with Round 2's escalation to BLOCKING — Phase 0.1's *required* frontend state list includes `stale`, so "unreachable, harmless" (Round 1's read) undersells it; also unblocks PIF-023's frontend state-model rewrite, which needs a real formula to reference. One paragraph, cheap. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[5] OBSERVATION_CONTRACT.md` (`stale`) / `docs/[9] TODO.md` Phase 0.1 frontend state list / Phase 0.2 |

**Issue**

`stale` "requires knowing the expected polling cadence," but no doc gives
the actual formula relative to the user-configurable interval introduced in Phase
0.2. `stale` also appears in Phase 0.1's frontend state list even though no polling
loop/cadence concept exists yet that phase (manual refresh only) — harmless
(unreachable state) but worth a one-line clarification.

**Why it matters**

Low urgency — doesn't block Phase 0.1 — but should be resolved
before Phase 0.2 actually implements "surface last updated in the UI."

**Proposed fix**

Add the staleness formula (e.g. "stale if `now - last_successful_at
> 2 × polling_interval`") to `DATA_MODEL.md` or `ARCHITECTURE.md` before Phase 0.2
starts; optionally drop `stale` from Phase 0.1's frontend state list until it's
reachable.

**Round 2 cross-check:** independently rediscovered — and rated **BLOCKING**, not
WORTH-NOTING. Round 2's read: Phase 0.1's *required* frontend state list includes
`stale`, and mandatory test 4 asserts a status of "`transient_failure`/`stale`," so
this isn't just unreachable-and-harmless in Phase 0.1 as Round 1 assessed — it's
required by a mandatory test with no formula to satisfy it, in a phase that (per
Round 1's own note) has no polling cadence concept at all yet (manual refresh only).
Round 2's proposed formula: `now - last_successful_at > 3 × configured poll interval`
generally, with a Phase-0.1-specific fallback (`> 30s`) since no configurable
interval exists yet that phase. Flag the severity mismatch (WORTH-NOTING vs.
BLOCKING) for step B — this changes whether PIF-006 can be deferred past Phase 0.1.

---

### Categories checked, no findings (Round 1)

| Category | Result |
|---|---|
| Assumed-vs-verified facts (category 4) | Clean. `docs/[9] TODO.md` Phase 0's first task is the permissions spike, correctly gating the ASSUMED claims in `docs/[7] PERMISSIONS_AND_PLATFORM.md` before any provider code is written. |
| Scope-creep tripwire (category 7) | Clean across all phases including Phase 2 — nothing edges toward modify/replay/inject; export and session-reopen explicitly forbid resend. |
| Phase-boundary leakage (category 6), general | Clean except PIF-006's minor note above — Phase 0.1 doesn't otherwise depend on `Flow`, `ObservationCapabilities`, or a real `TrafficProvider` implementation. |

---

## Round 2 — independent audit, run 2026-09-07 (different model, no access to Round 1)

Run against `docs/` as of the same state Round 1 audited (still-unfixed). The
auditing agent read `[1] AUDIT_PROMPT.md` and all of `docs/`, and was explicitly
instructed not to read this file or `[3] TODO.md` first, to preserve independent
discovery. It reported not having done so. Five of its findings independently
rediscovered PIF-001, PIF-002, PIF-003, PIF-005, and PIF-006 (cross-referenced above,
not duplicated here — see the "Round 2 cross-check" note on each). Two of those five
carried a higher severity than Round 1 assigned (PIF-002: SHOULD-FIX → BLOCKING;
PIF-006: WORTH-NOTING → BLOCKING) — flagged for step B. Everything below is new.

### At a glance (Round 2 new findings)

| ID | Severity | Status | Summary |
|---|---|---|---|
| [PIF-007](#pif-007--no-provider-side-status-type-exists) | BLOCKING | Fixed | No provider-side status type exists; every Phase 0.1 provider trait is unwritable as specified |
| [PIF-008](#pif-008--transient_failure-status-pathway-is-unimplementable-end-to-end) | BLOCKING | Fixed | `transient_failure` status pathway is unimplementable end-to-end |
| [PIF-009](#pif-009--hostnameobservation-provider-owned-type-requires-an-engine-owned-connection_id) | BLOCKING | Fixed | `HostnameObservation` (provider-owned) requires an Engine-owned `connection_id` |
| [PIF-010](#pif-010--trafficeventrequest_idresponse_id-reference-fields-that-dont-exist) | BLOCKING | Fixed | `TrafficEvent.request_id`/`response_id` reference fields that don't exist |
| [PIF-011](#pif-011--trafficeventtype-has-no-connectionexpired-variant) | SHOULD-FIX | Fixed | `TrafficEvent.type` has no `ConnectionExpired` variant |
| [PIF-012](#pif-012--flow-defined-immediately-per-adr-004-vs-not-defined-until-phase-05-per-todomd) | SHOULD-FIX | Fixed | `Flow` "defined immediately" per ADR-004 vs. not defined until Phase 0.5 per `TODO.md` |
| [PIF-013](#pif-013--report-status-vocabulary-drift-six-of-seven-listed) | WORTH-NOTING | Fixed | Report status vocabulary drift (six of seven listed) |
| [PIF-014](#pif-014--trafficevent-table-formattingconnection_id-note-self-contradiction) | WORTH-NOTING | Fixed | `TrafficEvent` table formatting/`connection_id` note self-contradiction |
| [PIF-015](#pif-015--observationcapabilities-panel-vocabulary-mismatch-across-three-docs) | WORTH-NOTING | Fixed | `ObservationCapabilities` panel vocabulary mismatch across three docs |
| [PIF-016](#pif-016--doc-cross-references-missing-the-n--filename-prefix) | WORTH-NOTING | Deferred (Phase 0, standalone commit) | Doc cross-references missing the `[N] ` filename prefix |
| [PIF-017](#pif-017--redactor-constructionownership-ambiguity-vs-engine-only-constructs-domain-state-rule) | SHOULD-FIX | Fixed | Redactor construction/ownership ambiguity vs. "Engine only constructs domain state" rule |
| [PIF-018](#pif-018--capture-time-redaction-boundary-rust-core-vs-python-mitmproxy-addon-undefined) | SHOULD-FIX | Fixed | Capture-time redaction boundary (Rust core vs. Python mitmproxy addon) undefined |
| [PIF-019](#pif-019--absence-for-expired-not-qualified-as-absence-from-a-successful-snapshot) | SHOULD-FIX | Fixed | "Absence" for `expired` not qualified as absence from a *successful* snapshot |
| [PIF-020](#pif-020--discovered--active-transition-never-defined) | SHOULD-FIX | Fixed | `discovered` → `active` transition never defined |
| [PIF-021](#pif-021--field-level-absence-best-effortprovider-dependent-fields-not-covered-by-the-no-inference-rule) | SHOULD-FIX | Fixed | Field-level absence (best-effort/provider-dependent fields) not covered by the no-inference rule |
| [PIF-022](#pif-022--processinfo-has-no-connection-count-field-but-the-ui-and-todomd-require-one) | SHOULD-FIX | Fixed | `ProcessInfo` has no connection-count field, but the UI and `TODO.md` require one |
| [PIF-023](#pif-023--frontend-shared-view-state-model-doesnt-map-onto-the-seven-value-status-vocabulary) | SHOULD-FIX | Fixed | Frontend shared view-state model doesn't map onto the seven-value status vocabulary |
| [PIF-024](#pif-024--last_successful_at-defined-as-set-only-when-stale-contradicts-always-needed-for-ui) | SHOULD-FIX | Fixed | `last_successful_at` defined as "set only when `stale`," contradicts always-needed-for-UI |
| [PIF-025](#pif-025--sensitive-field-starter-list-undefined-for-the-redactor) | SHOULD-FIX | Fixed (doc list only) | Sensitive-field starter list undefined for the Redactor |
| [PIF-026](#pif-026--hostnameobservation-confidence-values-per-source--multi-source-display-rule-undefined) | SHOULD-FIX | Fixed | `HostnameObservation` confidence values per source + multi-source display rule undefined |
| [PIF-027](#pif-027--observationstatusreasonprovider-schema-free-provider-should-arguably-be-an-enum) | WORTH-NOTING | Fixed | `ObservationStatus.reason`/`provider` schema-free; `provider` should arguably be an enum |
| [PIF-028](#pif-028--phase-0-permissions-spike-scoped-too-narrowly) | BLOCKING | Fixed | Phase 0 permissions spike scoped too narrowly (privilege only, not data-shape questions) |
| [PIF-029](#pif-029--macos-tcchardened-runtime-not-addressed-in-permissions_and_platformmd) | SHOULD-FIX | Fixed | macOS TCC/hardened runtime not addressed in `PERMISSIONS_AND_PLATFORM.md` |
| [PIF-030](#pif-030--no-documented-contingency-for-a-negative-phase-0-spike-result) | SHOULD-FIX | Fixed | No documented contingency for a negative Phase 0 spike result |
| [PIF-031](#pif-031--apple-developer-program-membership-assumed-untagged) | WORTH-NOTING | Fixed | Apple Developer Program membership assumed, untagged |
| [PIF-032](#pif-032--exited-process--connection-lifecycle-and-retention-in-output-undefined-for-mandatory-test-1) | SHOULD-FIX | Fixed | Exited-process → connection lifecycle and retention-in-output undefined for mandatory test 1 |
| [PIF-033](#pif-033--redacted-field-marker--show-anywayreveal-command-undefined) | SHOULD-FIX | Fixed | Redacted-field marker + "show anyway"/reveal command undefined |
| [PIF-034](#pif-034--observationcapabilities-phase-assignment-conflict--per-connection-vs-per-provider-ownership) | SHOULD-FIX | Fixed | `ObservationCapabilities` phase assignment conflict + per-connection vs. per-provider ownership |
| [PIF-035](#pif-035--architecturemd-marked-frozen-for-phase-01-while-still-containing-the-above-gaps) | SHOULD-FIX | Fixed | `ARCHITECTURE.md` marked frozen for Phase 0.1 while still containing the above gaps |
| [PIF-036](#pif-036--capture-mechanism-in-path-tls-terminating-proxy--ca-install-not-reconciled-with-read-only-framing) | SHOULD-FIX | Fixed | Capture mechanism (in-path TLS-terminating proxy + CA install) not reconciled with "read-only" framing |
| [PIF-037](#pif-037--phase-2-nefilterdataprovider-read-only-constraint-not-restated) | WORTH-NOTING | Fixed | Phase 2 `NEFilterDataProvider` read-only constraint not restated |

---

### PIF-007 — No provider-side status type exists

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: blocks the first Rust trait signature written in Phase 0.1; without this, Phase 0.1 cannot start as specified. |
| **Severity** | BLOCKING |
| **Location** | `docs/[4] DATA_MODEL.md` (`ObservationStatus`) vs. `docs/[3] ARCHITECTURE.md` (Providers) vs. `docs/[5] OBSERVATION_CONTRACT.md` ("Two different kinds of status") vs. `docs/[9] TODO.md` Phase 0.1 |

**Issue**

`DATA_MODEL.md` heads its status section "`ObservationStatus` (Engine-owned)" and
says it's attached to every domain object *the Engine* emits. But `ARCHITECTURE.md`
requires every provider to "return typed data plus an explicit provider-level
status," and `TODO.md` Phase 0.1 has a checklist item to implement provider statuses.
`OBSERVATION_CONTRACT.md` additionally references a type called `TrafficProviderStatus`
that is defined in no document.

**Why it matters**

The provider trait signature is the first Rust written in Phase 0.1. Changing it
later re-types every provider, every mock in the unit-test layer, and the Engine's
ingest path.

**Proposed fix**

Add a `ProviderStatus` type to `DATA_MODEL.md` (`state: ProviderState { Observed,
Unavailable, PermissionDenied, Unsupported, TransientFailure }`, `observed_at`,
`reason: Option<String>`), explicitly tagged provider-owned. State that the Engine
constructs `ObservationStatus` by wrapping a `ProviderStatus` and may additionally set
`Stale`/`Unmatched`. Rename the phantom `TrafficProviderStatus` reference in
`OBSERVATION_CONTRACT.md` to `ProviderStatus`.

---

### PIF-008 — `transient_failure` status pathway is unimplementable end-to-end

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: blocks mandatory test 4, the specific test ADR-012 added to catch the "every connection just disappeared" bug; the contract's literal current text would cause that exact bug. Not deferrable past Phase 0.1. |
| **Severity** | BLOCKING |
| **Location** | `docs/[4] DATA_MODEL.md` (`SocketSnapshot`) vs. `docs/[5] OBSERVATION_CONTRACT.md` (lines ~34 and ~79, "Applied per layer") vs. `docs/[8] TESTING_STRATEGY.md` (mandatory test 4) |

**Issue**

Three compounding gaps on the same status value: (1) `SocketSnapshot` has only
`timestamp`/`observations` — no field to carry a failure status, so a provider
literally cannot return `transient_failure`; there is no `ProcessSnapshot` type at
all. (2) `OBSERVATION_CONTRACT.md` contradicts itself about whether `transient_failure`
carries data — one line says only `observed`/`stale` do, another says a `null` data
field pairs only with four *other* named statuses (implying `transient_failure` does
carry data) — and mandatory test 4 requires it to carry the last-known data, which
only one of the two contradictory lines supports. (3) The per-layer status lists
(process/socket/DNS) omit `transient_failure` entirely (DNS also omits
`permission_denied`), even though Phase 0.1 tasks and test 4 require providers to
produce it.

**Why it matters**

Mandatory test 4 — added specifically to catch "every connection just disappeared
when the provider hiccuped" — cannot be written against the current types, and an
implementer following the contract's literal text would build the Engine to *drop*
data on a failed poll, i.e. cause the exact bug the test exists to prevent.

**Proposed fix**

Add `status: ProviderStatus` to `SocketSnapshot`; define `ProcessSnapshot` the same
way. In `OBSERVATION_CONTRACT.md`, state plainly: "`observed`, `stale`, and
`transient_failure` carry data alongside the status (`transient_failure` carries the
last known values, unchanged); `unavailable`, `permission_denied`, `unsupported`, and
`unmatched` carry a `reason` and no data." Add `transient_failure` to the
process/socket/DNS per-layer lists and `permission_denied` to DNS's — or replace the
per-layer blocks with "every provider may return any of the five provider statuses;
the notes below only call out which are expected in practice." State mandatory test
4's assertion concretely: for every connection tracked before the failed poll,
`lifecycle_state` and `last_seen` unchanged, `status.state ∈ {transient_failure,
stale}`, `status.last_successful_at` equal to the previous successful snapshot's
timestamp.

---

### PIF-009 — `HostnameObservation` (provider-owned) requires an Engine-owned `connection_id`

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: doesn't block Phase 0.1 (DNS lands Phase 0.2), but this is the identical provider/Engine-construction contradiction ADR-011 and ADR-012 already paid down for sockets and processes — closing it a third time now, while the pattern and the fix are both well-established, is strictly cheaper than a third rediscovery mid-Phase-0.2. |
| **Severity** | BLOCKING |
| **Location** | `docs/[4] DATA_MODEL.md` (`HostnameObservation`) vs. "The core rule this document enforces" vs. `docs/[9] TODO.md` Phase 0.2 |

**Issue**

`HostnameObservation` is marked provider-owned but has a required `connection_id` —
the same Engine-owned session identity the document says "nothing else invents." A
`DNSProvider` doing a reverse lookup has an IP address, not a `connection_id`, and per
`ARCHITECTURE.md`'s dependency rule ("providers never import the Engine") it cannot
legitimately obtain one.

**Why it matters**

This is the identical provider/Engine-construction contradiction ADR-011 fixed for
sockets and ADR-012 fixed for processes — now present a third time, unfixed, for DNS.
It lands at the top of Phase 0.2.

**Proposed fix**

Split it the same way as the other two: `HostnameObservation` (provider-owned:
`queried_addr`, `source`, `hostname`, `confidence`, `observed_at`) and
`ResolvedHostname` (Engine-owned: adds `connection_id`, `status: ObservationStatus`).
Update the entity-relationship diagram and `TODO.md` Phase 0.2's model bullet to the
split names.

---

### PIF-010 — `TrafficEvent.request_id`/`response_id` reference fields that don't exist

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: two required-field additions; blocks Phase 0.2's `TrafficEvent` emission and is persisted from Phase 0.6 onward, so the earlier it lands the fewer places it has to be retrofitted. Also corrects ADR-012's overstatement that this was already fixed. |
| **Severity** | BLOCKING |
| **Location** | `docs/[4] DATA_MODEL.md` (`TrafficEvent`, `HTTPRequest`, `HTTPResponse`) vs. `docs/[2] DECISIONS.md` ADR-012 |

**Issue**

`TrafficEvent.request_id`/`response_id` are meant to reference identity fields that no
type actually defines: `HTTPRequest` has `connection_id` but no `request_id`;
`HTTPResponse` has `request_id` but no `response_id`. A response cannot be joined to
its request. ADR-012 records this pairing as already fixed, overstating the model's
actual state. `Flow` (`requests`/`responses` vectors) has the same unjoinable-pair
problem at Phase 0.5.

**Why it matters**

Request/response pairing underlies the API Traffic view, per-endpoint latency stats,
and the timeline. Adding an id field after sessions are persisted (Phase 0.6) breaks
the on-disk session format.

**Proposed fix**

Add `request_id: String` (required) as the first row of `HTTPRequest`, and
`response_id: String` (required) as the first row of `HTTPResponse`; state both are
Engine-assigned, like `connection_id`.

---

### PIF-011 — `TrafficEvent.type` has no `ConnectionExpired` variant

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: one enum variant; without it Phase 0.2's timeline can only show connections opening, never ending, or an implementer reintroduces the exact `ConnectionClosed`-for-expiry overclaim ADR-011/012 removed. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[4] DATA_MODEL.md` (`TrafficEvent`, "closed vs. expired" rule) vs. `docs/[9] TODO.md` Phase 0.2 |

**Issue**

`TrafficEvent.type` is `{ ConnectionOpened, ConnectionClosed, Request, Response }`,
but `DATA_MODEL.md`/ADR-012 both establish `Closed` as rare-to-unreachable and
`Expired` as what essentially every connection reaches in Phases 0.1–0.2. `TODO.md`
Phase 0.2 says to "emit `TrafficEvent`s for connection-opened/connection-closed" —
with no `ConnectionExpired` variant to emit, a Phase 0.2 timeline either never shows
anything ending, or an implementer emits `ConnectionClosed` for expiry, reintroducing
the overclaim ADR-011/012 removed.

**Why it matters**

The event type is persisted from Phase 0.6 onward and drives the timeline view;
adding a variant later is a migration plus a UI change.

**Proposed fix**

Add `ConnectionExpired` to the enum: `{ ConnectionOpened, ConnectionClosed,
ConnectionExpired, Request, Response }`. Note `ConnectionClosed` requires the same
positive evidence `lifecycle_state = Closed` does. Update `TODO.md` Phase 0.2's bullet
to "connection-opened / connection-closed / connection-expired."

---

### PIF-012 — `Flow` "defined immediately" per ADR-004 vs. not defined until Phase 0.5 per `TODO.md`

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: `TODO.md` just needs to catch up to ADR-004's already-made decision — add the Phase 0.1 "define, don't wire in" bullet and reword Phase 0.5's. Prevents Phase 0.4's HTTP wiring from calcifying onto `NetworkConnection` directly, which is exactly the outcome ADR-004 exists to avoid. **Correction (step D, 2026-09-07):** the `TODO.md` half of this fix landed, but `TESTING_STRATEGY.md`'s "Integration tests" example line — which the original proposed fix explicitly named — was missed; it still read `expected Flow, redacted`, the exact phase-boundary leak this finding exists to prevent. Caught by the Step D re-verification pass, fixed in commit `85249b0`. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[2] DECISIONS.md` ADR-004 vs. `docs/[4] DATA_MODEL.md` (`Flow`) vs. `docs/[9] TODO.md` Phase 0.5 vs. `docs/[8] TESTING_STRATEGY.md` (integration tests) |

**Issue**

ADR-004 says `Flow` "is written as a struct/model immediately"; `TODO.md` has no task
defining it until Phase 0.5 ("Introduce `Flow` model"). `TESTING_STRATEGY.md`'s
integration tests use `Flow` as the expected output of the Phase 0.3/0.4 traffic chain
— a phase before `TODO.md` says the type exists.

**Why it matters**

ADR-004's stated purpose is preventing an HTTP-shaped design from calcifying before
`Flow` exists. If `Flow` genuinely first appears at Phase 0.5, Phase 0.4's HTTP wiring
gets built directly onto `NetworkConnection`, and 0.5 becomes the refactor ADR-004 was
written to avoid rather than an addition.

**Proposed fix**

Add to `TODO.md` Phase 0.1 models: "Define the `Flow` struct per `DATA_MODEL.md` —
defined now, not wired into the Engine or UI until Phase 0.5 (ADR-004)." Reword Phase
0.5's bullet to "Wire the already-defined `Flow` model into the Engine and UI." Change
`TESTING_STRATEGY.md`'s integration-test line to reference `HTTPRequest`/`HTTPResponse`
attached to the right `NetworkConnection` (via `Flow` from Phase 0.5 onward).

---

### PIF-013 — Report status vocabulary drift (six of seven listed)

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: trivial, no reason to leave open. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[1] process-network-inspector-report.md` vs. `docs/[5] OBSERVATION_CONTRACT.md` |

**Issue**

The report enumerates the status vocabulary as six values, omitting
`transient_failure`. Cheap drift in a document the report itself doesn't claim is
authoritative.

**Proposed fix**

Add `transient_failure` to the report's list, or replace the enumeration with a
pointer to `OBSERVATION_CONTRACT.md` so it can't drift again.

---

### PIF-014 — `TrafficEvent` table formatting/`connection_id` note self-contradiction

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: trivial, bundled with the other `TrafficEvent` edits (PIF-010, PIF-011) already touching this table in the same pass. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[4] DATA_MODEL.md` (`TrafficEvent`) |

**Issue**

The table header is `Field / Type / Notes` but every row's third cell is actually a
required-flag, inconsistent with every other table's `Field / Type / Required /
Notes`. Separately, `connection_id: Option<String>` is annotated "populated for every
event type," which contradicts its own optionality.

**Proposed fix**

Normalize to four columns; change the `connection_id` note to "populated for every
event type except traffic that is `unmatched`."

---

### PIF-015 — `ObservationCapabilities` panel vocabulary mismatch across three docs

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: doesn't block anything before Phase 0.3/1.0 (the panel isn't implemented until then), but it's a cheap table edit and `DATA_MODEL.md` is already open for `ObservationCapabilities`-adjacent edits (PIF-034) in this same pass. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[1] process-network-inspector-report.md` ("Observation Capabilities panel") vs. `docs/[4] DATA_MODEL.md` (`ObservationCapabilities`) vs. `docs/[9] TODO.md` Phase 0.3 |

**Issue**

Three different field vocabularies for the same panel: the report's mock lists eight
rows including `remote_addresses` and `raw_packet_data`; the type has seven different
fields, missing those two; `TODO.md` Phase 0.3 uses a fourth, camelCase set.

**Proposed fix**

Make `DATA_MODEL.md`'s table authoritative, add `remote_addresses` and
`raw_packet_data` rows, and change `TODO.md` Phase 0.3's bullet to reference the
`DATA_MODEL.md` fields instead of restating names.

---

### PIF-016 — Doc cross-references missing the `[N] ` filename prefix

| | |
|---|---|
| **Status** | Deferred — to Phase 0, as its own standalone mechanical commit. Decided 2026-09-07. Reason: purely cosmetic/navigational, zero functional risk, touches every file in `docs/` with the same sed-style change — bundling it into step C's substantive edits would bury the real diffs under mechanical noise and make that step harder to review. Must still land before Phase 0.1 starts (it's part of this pre-implementation gate), just sequenced as its own commit, done last, right before Phase 0's project-setup work begins. |
| **Severity** | WORTH-NOTING |
| **Location** | All of `docs/` |

**Issue**

Every internal reference is written as `docs/DATA_MODEL.md`, `PRIVACY_AND_SECURITY.md`,
etc., but actual filenames carry a `[N] ` prefix. Every cross-reference is a broken
path for a link checker, "go to file," or an agent following references.

**Proposed fix**

Either drop the numeric prefixes from filenames (keep ordering via
`[0] READING_ORDER.md`) or update every reference. Mechanical, one-time.

---

### PIF-017 — Redactor construction/ownership ambiguity vs. "Engine only constructs domain state" rule

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: one-sentence amendment to `ARCHITECTURE.md`; needed before that document's Phase 0.1 freeze can honestly be stamped (see PIF-035), and it's exactly the kind of rule ADR-012 consolidated specifically to make checkable in code review. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[3] ARCHITECTURE.md` (dependency rules; Redactor) vs. `docs/[9] TODO.md` Phase 0.4 |

**Issue**

`ARCHITECTURE.md`'s consolidated rule says the Engine is the *only* component
permitted to create or mutate authoritative domain state. But the Redactor —
described as a distinct component — constructs `HTTPRequest`/`HTTPResponse`, which are
Engine-owned. Either the Redactor is an Engine sub-component (unstated) or the rule
has an unstated exception.

**Why it matters**

This is the exact rule ADR-012 consolidated so it would be a single checkable thing
in code review; an ambiguous rule isn't enforceable, on the code path that decides
whether a credential reaches disk.

**Proposed fix**

Amend `ARCHITECTURE.md`'s dependency-rules bullet: "...the only component permitted to
create or mutate authoritative domain state. The `Redactor` and `Session Store` are
Engine-internal components for the purposes of this rule; they're named separately
only because they sit at specific checkpoints, not because they're separate layers."

---

### PIF-018 — Capture-time redaction boundary (Rust core vs. Python mitmproxy addon) undefined

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: doesn't block Phase 0.1, but this sits directly on the path that decides whether a credential ever crosses a process boundary in retrievable form — worth pinning down while the design is still just prose, not an already-built IPC schema. Adopting Round 2's proposed approach (redact in the addon, before the IPC socket; one shared field-name list read by both processes) since it's consistent with the IPC design already planned (local socket + JSON via `serde`) and requires no new mechanism. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[6] PRIVACY_AND_SECURITY.md` ("The two redaction checkpoints") vs. `docs/[3] ARCHITECTURE.md` (Redactor in Rust core) vs. `docs/[9] TODO.md` Phase 0.3/0.4 |

**Issue**

Nothing says which side of the Rust/Python boundary capture-time (tier-1) redaction
runs on. The rule says highly-sensitive fields never exist in raw form "including in
memory" before a `RawHTTPRequest` exists — but capture physically happens in the
Python mitmproxy addon. Taken literally, the sensitive-match list needs a second,
Python implementation; if instead raw values cross the addon→core IPC socket first,
the "never exists in raw form anywhere" guarantee is already violated.

**Why it matters**

Determines the helper's IPC message schema and where redaction unit tests live —
discovering it mid-Phase-0.4 means rewriting the addon, the IPC schema, and the
Redactor's tests, on the path where getting it wrong leaks credentials.

**Proposed fix**

State in `PRIVACY_AND_SECURITY.md`: "Capture-time redaction for the highly-sensitive
tier runs inside the mitmproxy helper addon, before any value crosses the
helper→core IPC socket. The helper and Rust core share the same field-name list, one
data file read by both." Add a matching `TODO.md` Phase 0.3 bullet: the addon applies
tier-1 redaction before emitting; the IPC schema has no field able to carry an
unredacted tier-1 value.

---

### PIF-019 — "Absence" for `expired` not qualified as absence from a *successful* snapshot

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: directly gates mandatory tests 2 and 4, both Phase 0.1/cross-cutting; getting this wrong reproduces the exact bugs those two tests exist to catch. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[4] DATA_MODEL.md` ("closed vs. expired" rule) vs. `docs/[8] TESTING_STRATEGY.md` (mandatory tests 2 and 4) |

**Issue**

"No longer observable" isn't qualified as "absent from a *successful* snapshot," and
no missed-poll count is given. Test 2 requires expiry after disappearance; test 4
requires *no* expiry when the provider itself failed — the distinguishing condition is
inferable from test 4 but never stated as a rule.

**Why it matters**

Getting it wrong produces exactly the two bugs the two mandatory tests exist to
catch; the fix touches the Engine's core diffing loop after the lifecycle state
machine is written.

**Proposed fix**

Amend the `expired` bullet: previously observed and absent from **one subsequent
successful snapshot** (`status.state == observed`). Snapshots carrying
`transient_failure`/`unavailable`/`permission_denied` are not evidence of absence and
must never advance lifecycle; on such a snapshot the Engine leaves `lifecycle_state`
and `last_seen` unchanged and updates only `status`.

---

### PIF-020 — `discovered` → `active` transition never defined

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: these are the two lifecycle states a Phase 0.1 connection actually spends its life in (per PIF-019/the `closed`/`expired` rule, `closed` is rare-to-unreachable and `expired` is terminal) — Phase 0.1 cannot render the connection table correctly without this. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[4] DATA_MODEL.md` (`NetworkConnection.lifecycle_state`) vs. `docs/[9] TODO.md` |

**Issue**

`DATA_MODEL.md` fully specifies `closed` vs. `expired` but says nothing about the two
states a Phase 0.1 connection actually spends its life in. Is `Discovered` the first
snapshot and `Active` the second? Tied to socket state `ESTABLISHED`? Does a `LISTEN`
socket ever reach `Active`? `TODO.md` asserts the lifecycle order without saying what
advances it.

**Why it matters**

Rendered in the UI and drives `ConnectionOpened` event emission in Phase 0.2.

**Proposed fix**

Add: `discovered` = seen in exactly one snapshot so far, identity not yet
corroborated; `active` = matched in at least two consecutive successful snapshots, or
observed once with socket state `ESTABLISHED`; `LISTEN` sockets follow the same rule
(listening is a legitimate active state). `ConnectionOpened` events fire on
`discovered → active`, not on first sight.

---

### PIF-021 — Field-level absence (best-effort/provider-dependent fields) not covered by the no-inference rule

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: Phase 0.1's socket table renders `bytes_sent`/`bytes_received` today; without this rule the frontend is left to invent its own convention for `None`, which is exactly the kind of drift `OBSERVATION_CONTRACT.md`'s "one rule" section exists to prevent. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[4] DATA_MODEL.md` (`bytes_sent`/`bytes_received`, `cpu_percent`, `memory_bytes`, marked best-effort) vs. `docs/[5] OBSERVATION_CONTRACT.md` ("the one rule this document exists to enforce") |

**Issue**

`ObservationStatus` is attached per *object*, not per *field*. A `NetworkConnection`
with `status = observed` and `bytes_sent = None` gives the frontend nothing to reason
from except the no-inference rule, which the design's own optional fields then force
it to violate — no document says what the consumer does with a `None` on an optional
field.

**Why it matters**

`TODO.md`'s definition of done states unknown/denied/stale is never displayed as
empty/zero; the report promises bytes-sent/received as a feature.

**Proposed fix**

Add a "Field-level absence" section to `OBSERVATION_CONTRACT.md`: `None` on an
optional field means this provider doesn't supply this field — the one permitted
exception to the no-inference rule. Render as an explicit "not reported" affordance,
never as `0`/blank. Field-level absence never changes the object's `ObservationStatus`.
Name the governed fields explicitly.

---

### PIF-022 — `ProcessInfo` has no connection-count field, but the UI and `TODO.md` require one

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: the process list is Phase 0.1's first screen; this also settles a real architectural question (whether `get_processes` depends on `SocketProvider`) that's cheaper to answer in a doc than discover while implementing the command. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[4] DATA_MODEL.md` (`ProcessInfo`) vs. `docs/[1] process-network-inspector-report.md` §3 vs. `docs/[9] TODO.md` Phase 0.1 |

**Issue**

Both the report and Phase 0.1's frontend/demo checkpoint specify a process table with
a connection count; `ProcessInfo` has no such field. Undefined: whether `get_processes`
joins socket state (making it depend on `SocketProvider`), whether the count includes
`expired` connections, and what it shows when the socket layer is
`permission_denied` (0 would be exactly the forbidden empty-means-unknown display).

**Why it matters**

Decides whether `get_processes` is a cheap independent command or a joined one — a
real difference in the Engine's state model and per-refresh cost.

**Proposed fix**

Add `active_connection_count: Option<u32>` to `ProcessInfo`: Engine-derived, count of
this PID's connections with `lifecycle_state ∈ {discovered, active}`; `None` when the
socket layer's status for this PID isn't `observed` — rendered as "—," never `0`.
Update `TODO.md` Phase 0.1's frontend bullet to name the field.

---

### PIF-023 — Frontend shared view-state model doesn't map onto the seven-value status vocabulary

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: this model is imported by every Phase 0.1 React view; this is precisely the "cheap now, expensive after the process list/connections table/badge component are built" case this whole pre-implementation pass exists to catch. Depends on PIF-006's staleness formula (also fixed now) to be fully concrete. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[9] TODO.md` Phase 0.1 (shared state model: `loading`/`loaded`/`empty`/`permission_denied`/`error`/`stale`) vs. `docs/[5] OBSERVATION_CONTRACT.md` vs. `docs/[8] TESTING_STRATEGY.md` (frontend tests) |

**Issue**

The Phase 0.1 shared view-state model introduces `empty` — the contract's own
canonical example of forbidden inference — plus `loaded`/`error` (not statuses at
all), while omitting `unavailable`, `unsupported`, `unmatched`, and
`transient_failure`. `TESTING_STRATEGY.md`'s frontend tests require a status badge for
*each* of the seven contract statuses.

**Why it matters**

This model is imported by every React view; correcting it after the process list,
connections table, and badge component are built means rewriting all of them and
their tests.

**Proposed fix**

Rewrite as: `loading` (client-side only, never a backend status) plus the seven
`ObservationStatus` states verbatim (`observed`/`unavailable`/`permission_denied`/
`unsupported`/`transient_failure`/`stale`/`unmatched`). No `empty` state — `observed`
with zero rows renders "no connections observed"; any other status renders that
status. No `error` state — transport failures render as `unavailable`.

---

### PIF-024 — `last_successful_at` defined as "set only when `stale`," contradicts always-needed-for-UI

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: one-word contradiction in a Phase 0.1 type (`ObservationStatus`); as written it silently breaks Phase 0.2's "last updated" UI requirement. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[4] DATA_MODEL.md` (`ObservationStatus.last_successful_at`) vs. `docs/[9] TODO.md` Phase 0.2 |

**Issue**

`DATA_MODEL.md` says the field is "set when `state == stale`"; `TODO.md` Phase 0.2
requires surfacing "last updated" in the UI using this exact field during normal
(non-stale) operation. If it's only ever set once already stale, the UI can't show
"last updated" the rest of the time — the entire point of the affordance.

**Proposed fix**

Change the note to: "always set once any successful observation has occurred for this
object; `None` only before the first success. Required for the 'last updated' display
and the `stale` threshold computation."

---

### PIF-025 — Sensitive-field starter list undefined for the Redactor

| | |
|---|---|
| **Status** | Fixed, doc list only — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: doesn't block Phase 0.1 (Redactor lands Phase 0.4), but on the tier where both a miss and a false positive are unrecoverable, "start from a blank list" is worth closing while it's just a doc edit rather than a decision made under implementation pressure. Only the list moves earlier; building the actual `Redactor` still happens in Phase 0.4. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[6] PRIVACY_AND_SECURITY.md` (data classification: "any field name") vs. `docs/[9] TODO.md` Phase 0.4 (Redactor, "configurable list") |

**Issue**

The starter list of sensitive field names exists in no document. The highly-sensitive
tier — irreversible, no reveal path — is defined by an unbounded phrase rather than a
list, so Phase 0.4 starts from a blank list on the tier where both a miss (value
never captured, unrecoverable) and a false positive (no reveal path, also
unrecoverable) can't be fixed after the fact. Also unspecified: name vs. value-shape
matching, case sensitivity (a Phase 0.4 test assumes it), nested-JSON key matching.

**Proposed fix**

Add a starter list to `PRIVACY_AND_SECURITY.md`: header names (`authorization`,
`proxy-authorization`, `x-api-key`, `api-key`, `x-auth-token`, `x-access-token`,
`authentication`); body/query keys matching case-insensitive substring (`password`,
`passwd`, `secret`, `token`, `api_key`/`apikey`, `access_key`, `private_key`,
`client_secret`, `refresh_token`, `session_id`, `credential`). Tier 2 (reversible
in-session): `cookie`, `set-cookie`, full bodies/query strings. Matching is
case-insensitive substring against the leaf key name only, never the value. List is
user-extendable, never user-shrinkable below this baseline. Point `TODO.md`'s Redactor
bullet at this list.

---

### PIF-026 — `HostnameObservation` confidence values per source + multi-source display rule undefined

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: doesn't block Phase 0.1 (DNS lands Phase 0.2), but it's user-visible display behavior built once — cheap to pin the starter values and the display rule now rather than have Phase 0.2 invent them ad hoc. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[4] DATA_MODEL.md` (`HostnameObservation.confidence`) vs. `docs/[9] TODO.md` Phase 0.2 |

**Issue**

`confidence` has no defined values per source, and no rule says which hostname the
connections view displays when reverse DNS, SNI, and `Host` disagree — the document's
own stated reason for keeping per-source rows.

**Proposed fix**

Add a starter table (`HttpHost = 0.95`, `Sni = 0.90`, `ReverseDns = 0.50`, `0.30` if
the PTR resolves to a shared/CDN-suffixed name) and a display rule: the connections
view shows the highest-confidence observation with source named on hover; when two
sources disagree above 0.85, show both rather than picking one. All observations are
retained regardless of what's displayed.

---

### PIF-027 — `ObservationStatus.reason`/`provider` schema-free; `provider` should arguably be an enum

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: one sentence plus a type change on a Phase 0.1 type (`ObservationStatus`); cheap to close the same pass its other fields are being touched (PIF-024). |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[4] DATA_MODEL.md` (`ObservationStatus.reason`, `.provider`; `CorrelationEvidence.source`; `HostnameObservation.source`) |

**Issue**

`reason` and `provider` are free-form strings with no schema, so every provider
invents its own conventions — low-stakes only if nothing ever parses them, which no
document states. `CorrelationEvidence.source`/`ObservationStatus.provider` are
free-form strings for the same concept `HostnameObservation.source` treats as an enum.

**Proposed fix**

Add: "`reason` is display-and-log only — no code, test, or frontend branch may parse
or switch on its contents; behavior needing to depend on something belongs in
`state`." Make `provider` an enum (`Process | Socket | Dns | Traffic | Engine`)
matching `HostnameObservation.source`'s treatment.

---

### PIF-028 — Phase 0 permissions spike scoped too narrowly

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: this rescopes step F of this very checklist (the permissions spike) — must land in `PERMISSIONS_AND_PLATFORM.md`/`TODO.md` before step F is considered complete, or step F risks returning a green light without answering the questions Phase 0.1's types actually depend on. Bundling with PIF-029 (TCC/hardened runtime) and PIF-030 (negative-result contingency) since all three edit the same "spike" sections. |
| **Severity** | BLOCKING |
| **Location** | `docs/[7] PERMISSIONS_AND_PLATFORM.md` ("First technical spike," "TO VERIFY") vs. `docs/[9] TODO.md` Phase 0 permissions-spike bullet vs. `docs/[4] DATA_MODEL.md` (`SocketObservation`) — also directly affects `[3] TODO.md` step F of this pre-implementation checklist |

**Issue**

The spike, as scoped, only asks the privilege question (what an unprivileged process
can read about other processes' sockets, and what needs elevation). It does not ask
the questions Phase 0.1's *type shape* actually depends on: whether `netstat2`
attributes sockets to PIDs on macOS at all (or `SocketProvider` needs to be
`libproc`-FFI-based from day one — a materially different implementation); whether any
candidate crate supplies per-socket `bytes_sent`/`bytes_received` (which
`SocketObservation` carries and the report promises); whether per-socket `state`
(LISTEN/ESTABLISHED/...) is available at all, which the entire connections view and
demo checkpoint depend on.

**Why it matters**

The spike is explicitly positioned as determining whether Phase 0.1 is trivial or the
project's first real blocker. A spike that clears privilege but leaves data-shape
questions open lets Phase 0.1 start against a `SocketObservation` the platform can't
actually populate.

**Proposed fix**

Replace the `TODO.md` Phase 0 spike bullet and `PERMISSIONS_AND_PLATFORM.md`'s "First
technical spike" section with an explicit checklist, answers written back to the
VERIFIED section: (1) same-user socket enumeration without elevation; (2) other-users'
— exact errno/refusal shape; (3) can each candidate crate/API attribute a socket to a
PID; (4) which of `bytes_sent`/`bytes_received` are obtainable per socket, if any; (5)
is per-socket TCP state obtainable, and does its vocabulary match `lsof`'s. State the
fallback consequences explicitly if (3) or (4) fail.

**Note:** this directly affects step F of `[3] TODO.md` — the permissions spike step
should be scoped to this checklist, not just the privilege question, before it's
considered complete.

---

### PIF-029 — macOS TCC/hardened runtime not addressed in `PERMISSIONS_AND_PLATFORM.md`

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: cheap ASSUMED bullet; the failure mode (a spike that passes under `tauri dev` but not under the signed Phase 1.0 build) would otherwise surface at exactly the point `TODO.md` already warns to catch entitlement issues before they're a release blocker. Bundled with PIF-028/030 (same sections). |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[7] PERMISSIONS_AND_PLATFORM.md` (whole document) |

**Issue**

macOS TCC and the hardened runtime aren't mentioned at all — the document's
permission model is "same-user vs. other-user + App Sandbox," incomplete for a
modern signed macOS app. Reading `executable_path` on other processes and calling
`proc_pidinfo` can involve TCC-mediated prompts; a notarized build runs under the
hardened runtime with different entitlements than `cargo tauri dev`. A spike run
under `tauri dev` could return a green light the signed Phase 1.0 build doesn't
reproduce.

**Why it matters**

Would surface at code-signing time — the exact point `TODO.md` warns to catch
entitlement issues before they're a release blocker — after every provider is written
against the dev build's permissions.

**Proposed fix**

Add an ASSUMED bullet: the process/socket visibility measured by the Phase 0 spike is
identical under the hardened runtime in a signed, notarized build as under `cargo
tauri dev`. TO VERIFY: re-run the spike's checklist once against a `cargo tauri build`
signed binary, not only a dev build.

---

### PIF-030 — No documented contingency for a negative Phase 0 spike result

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: one paragraph; ensures step F actually re-orders Phase 0.1 if same-user socket enumeration turns out to need elevation, instead of that being discovered three tasks into Phase 0.1. Bundled with PIF-028/029 (same sections). |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[7] PERMISSIONS_AND_PLATFORM.md` ("First technical spike") |

**Issue**

The document names the fork in the road (spike determines whether Phase 0.1 is
trivial or the project's first real blocker) and stops there. If same-user socket
enumeration needs elevation, `TODO.md` Phase 0.1's ordering is wrong, and the
`SMAppService` privileged-helper work — currently "not required for Phase 0.1" —
becomes a prerequisite.

**Proposed fix**

Add: "If the spike is negative (same-user enumeration requires elevation), Phase 0.1
is re-ordered: the `SMAppService` design moves from later-priority to a Phase 0.1
prerequisite, and the demo checkpoint is amended to require the authorization prompt.
Do not begin Phase 0.1's provider tasks before recording the spike result here."

---

### PIF-031 — Apple Developer Program membership assumed, untagged

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: one line; doesn't block anything before Phase 1.0 but costs nothing to tag now, and this document's whole purpose is separating verified from assumed. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[7] PERMISSIONS_AND_PLATFORM.md` ("Running the app itself") vs. `docs/[9] TODO.md` Phase 1.0 |

**Issue**

The signed/notarized `.app` DECISION presupposes a paid Apple Developer Program
membership and a working notarization pipeline — an unstated dependency with real
lead time, untagged in a document whose whole purpose is separating verified from
assumed.

**Proposed fix**

Add an ASSUMED bullet: an Apple Developer Program membership is available (or will be
obtained) before Phase 1.0; notarization has real turnaround, and an unavailable
account defers packaging without blocking any earlier phase.

---

### PIF-032 — Exited-process → connection lifecycle and retention-in-output undefined for mandatory test 1

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: directly needed to write mandatory test 1's assertion, Phase 0.1; also gives `closed` its one actually-reachable path in this phase, which the `closed`/`expired` rule (PIF-019) currently leaves it without. **Correction (step D, 2026-09-07):** the `DATA_MODEL.md` half of this fix (process-exit as the `closed` signal) landed, but the second half of the original proposed fix — the `TODO.md` Phase 0.1 bullet stating the Engine retains exited processes/connections in command output — was missed, leaving mandatory test 1 citing a rule that wasn't actually written down anywhere in Phase 0.1. Caught by the Step D re-verification pass, fixed in commit `85249b0`. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[8] TESTING_STRATEGY.md` (mandatory test 1) vs. `docs/[4] DATA_MODEL.md` ("closed vs. expired" rule) — related to PIF-019 |

**Issue**

Test 1's second assertion has no defined answer: when a process exits, do its
connections become `Expired` or `Closed`? Process exit is arguably the one source of
*positive* closure evidence available to polling-only providers (the kernel
definitively closed those sockets), yet the closed/expired rule never considers it.
Also undefined: whether an exited process's `ProcessInfo` and its connections remain
in `get_processes`/`get_connections` output at all in Phase 0.1 (Phase 0.2 says keep
historical observations for the session; Phase 0.1 says nothing) — which determines
whether test 1 can even fetch the object it asserts on.

**Why it matters**

Decides whether `Closed` is dead code or the one reachable path to it — a branch in
the Engine's lifecycle logic and a distinct UI state.

**Proposed fix**

Add a third bullet to the closed/expired rule: process exit is the one positive
closure signal available to polling-only providers — when `ProcessProvider` confirms
a PID no longer exists, that PID's connections transition to `closed`, not `expired`.
Add to `TODO.md` Phase 0.1: the Engine retains exited processes and terminated
connections for the session's lifetime rather than removing them from command output.

---

### PIF-033 — Redacted-field marker + "show anyway"/reveal command undefined

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: doesn't block Phase 0.1 (lands Phase 0.4), but Phase 0's own stated principle is deciding the Tauri capability allowlist scope "from the first commit, not a hardening pass to do at the end" — naming the `reveal_raw` command now means it's designed into that allowlist from the start rather than added as an afterthought. Security-relevant UI path, cheap as a doc-only addition today. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[8] TESTING_STRATEGY.md` (frontend tests) vs. `docs/[4] DATA_MODEL.md` (`HTTPRequest`) vs. `docs/[9] TODO.md` Phase 0.4 ("show anyway") vs. `docs/[3] ARCHITECTURE.md` (frontend never contains redaction logic) |

**Issue**

No typed marker distinguishes a redacted value from a real one (`HTTPRequest.headers`
is a plain `HashMap<String, String>`), so a component can't know a field was redacted
and the mandatory frontend test can't assert on it. "Show anyway" requires an `invoke`
command returning the transient `Raw*` object, which appears in no document — and per
`ARCHITECTURE.md` the frontend "never contains redaction logic" and everything it
displays "is already redacted by the time it arrives," which the reveal path directly
contradicts unless a dedicated command is specified.

**Why it matters**

Security-relevant UI path; retrofitting a marker after sessions are persisted changes
the on-disk format, and retrofitting the reveal command means revisiting the Tauri
capability allowlist Phase 0 insists on getting right from the first commit.

**Proposed fix**

Add `redacted_fields: Vec<String>` to `HTTPRequest`/`HTTPResponse` (field paths whose
values were replaced) and distinct placeholder tokens per tier (`"<redacted:tier1>"` /
`"<redacted:tier2>"`). Amend `ARCHITECTURE.md`'s React bullet: "...except via the
single `reveal_raw(request_id)` command, which returns the in-memory tier-2 `Raw*`
object for the current session only and is separately listed in the Tauri capability
allowlist." Add the command to `TODO.md` Phase 0.4.

---

### PIF-034 — `ObservationCapabilities` phase assignment conflict + per-connection vs. per-provider ownership

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: real four-document conflict, resolved by counting — `ARCHITECTURE.md`, `DATA_MODEL.md`'s intent, and `TODO.md` Phase 1.0 all already point at Phase 0.3; only `TODO.md`'s own Phase 0.2 bullet disagrees with the rest of the set. Moving that one bullet to Phase 0.3 and fixing the per-connection wording in the same edit is cheap and removes the exact conflation ADR-012 already warned about. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[9] TODO.md` Phase 0.2 vs. `docs/[3] ARCHITECTURE.md` (Phase 0.3) vs. `docs/[4] DATA_MODEL.md` ("Not implemented in Phase 0.1") vs. `docs/[9] TODO.md` Phase 1.0 |

**Issue**

`ObservationCapabilities` is assigned to Phase 0.2 by one document and Phase 0.3 by
three others. Phase 0.2's task also asks to track capabilities "per connection" —
the exact provider-vs-connection conflation ADR-012 and `DATA_MODEL.md` warn against;
capabilities are per *provider*, independent of any single connection, and
per-connection variability is `ObservationStatus`'s job.

**Why it matters**

A per-connection capability field would have to be unwound later out of the Engine,
IPC types, and the capabilities panel — exactly what ADR-012's warning was meant to
prevent.

**Proposed fix**

Change the `TODO.md` Phase 0.2 bullet to: add `ObservationCapabilities` **per
provider** (not per connection) as an Engine-aggregated struct via a `get_capabilities()`
command; per-connection variability stays in `ObservationStatus`. Move the bullet to
Phase 0.3 to match the other three documents (or amend those three) — pick one. Add
an ownership line to `DATA_MODEL.md`: Engine-owned, aggregated from each provider's
self-report.

---

### PIF-035 — `ARCHITECTURE.md` marked frozen for Phase 0.1 while still containing the above gaps

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: not a separate content fix — resolved automatically once PIF-001 (envelope), PIF-007 (provider status type), and PIF-017 (Redactor ownership) land in `ARCHITECTURE.md`; this entry just tracks re-stamping the "Frozen for Phase 0.1" line with a date once those three are in, so the freeze is honest when Phase 0.1 actually starts. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[3] ARCHITECTURE.md` (Status: Frozen for Phase 0.1) |

**Issue**

The document that's frozen for Phase 0.1 contains the load-bearing gaps above (no
provider-status type — PIF-007; Redactor/Engine ownership ambiguity — PIF-017; the
error-propagation section describing a `status` field without specifying the
envelope — see PIF-001's Round 2 cross-check). Freezing it in this state means Phase
0.1's first act is departing from a frozen document.

**Why it matters**

"Frozen" is a process commitment; breaking it in week one devalues the freeze for the
rest of the project.

**Proposed fix**

Land the `ProviderStatus`, Redactor-ownership, and payload-envelope edits into
`ARCHITECTURE.md` before stamping the freeze; add a line under Status noting the date
and that it's frozen *after* the Round 2 pre-implementation audit's fixes landed.

---

### PIF-036 — Capture mechanism (in-path TLS-terminating proxy + CA install) not reconciled with "read-only" framing

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: documents an already-made architectural choice (mitmproxy, per ADR-003/009/013) honestly — no behavior or scope change, purely closing a gap between what the report claims and what the chosen mechanism actually does. Consistent with the report's own stated bar ("honest about visibility limits") and with ADR-011's precedent of correcting overclaiming language elsewhere in this same report. |
| **Severity** | SHOULD-FIX-BEFORE-CODING |
| **Location** | `docs/[1] process-network-inspector-report.md` §2 ("explicitly out of scope") vs. `docs/[3] ARCHITECTURE.md` (Stack) vs. `docs/[7] PERMISSIONS_AND_PLATFORM.md` (Traffic capture) |

**Issue**

The chosen capture mechanism (`mitmproxy --mode local:<pid>`) terminates the target's
TLS and re-originates each request to the real server — the app transmits bytes on
the target's behalf and requires installing a CA certificate into the user's trust
store, a real, persistent system change. Nothing in the docs reconciles this with §2's
absolutist "read-only with respect to network traffic" framing, and the CA-trust-store
change is a user-facing security trade-off documented nowhere in
`PRIVACY_AND_SECURITY.md`.

**Why it matters**

The report's stated bar is being honest about visibility limits; this is exactly the
kind of gap a reader (or worse, a user) finds for themselves if it isn't addressed —
best handled now, in the doc, not invented under pressure later (e.g. in a Phase 1.0
README).

**Proposed fix**

Add to the report §2, after the two guarantees: HTTPS observation (Phase 0.3+) uses a
local MITM proxy, which by construction terminates TLS and forwards each request
onward unchanged — the app sits in the path of observed traffic but does not
originate, modify, reorder, replay, or withhold any of it, and forwarding is not a
capability exposed to the user; this requires the user to trust a locally-generated CA
certificate, an explicit, reversible, user-consented step documented in
`PRIVACY_AND_SECURITY.md`. Add a matching bullet there covering CA install location and
removal.

---

### PIF-037 — Phase 2 `NEFilterDataProvider` read-only constraint not restated

| | |
|---|---|
| **Status** | Fixed — decided 2026-09-07, applied in commit `fc4ec5e`. Reason: one guardrail bullet in a Phase 2 section that's optional/distant and may never be built — costs nothing to add now, and closes the one spot in the whole roadmap where the platform primitive itself is a control capability rather than an observation one. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[9] TODO.md` Phase 2 |

**Issue**

`NEFilterDataProvider` exists to return allow/deny verdicts on flows — a filtering
API, one line of code away from "block this connection." Phase 2 names it only as
"following the same `TrafficProvider` interface contract," without restating the
read-only constraint. It's the one place in the roadmap where the platform primitive
itself is a control capability rather than an observation one (everywhere else in the
roadmap stays cleanly on the observation side — no other scope-creep found).

**Proposed fix**

Add to `TODO.md` Phase 2: the extension is observe-only by construction — a
`NEFilterDataProvider` implementation must return an allow verdict on every flow
unconditionally, with no configuration path making that conditional; any deviation
requires reopening `process-network-inspector-report.md` §2 first.

---

### Categories checked, no findings (Round 2)

| Category | Result |
|---|---|
| Scope-creep tripwire (category 7), feature-level | Clean across every phase including Phase 2 — no task edges toward modify/replay/inject. (Two documentation-honesty gaps found and logged as PIF-036, PIF-037; not scope creep in the roadmap itself.) |
| The "who constructs this type" rule (category 2), for `NetworkConnection`, `ProcessInfo`, `Flow`, `TrafficEvent` | Clean — no TODO task hands construction of these four to a provider. (`ObservationStatus` and `HostnameObservation` are not clean — PIF-007, PIF-009.) |

---

## Round 3 — re-verification pass (step D), run 2026-09-07 against commit `fc4ec5e`

Per `[3] TODO.md` step D, this pass checks two things against the post-fix `docs/`:
(1) did all 36 `Fixed` findings actually land correctly, and (2) does a fresh full
7-category pass turn up anything new. Not independent discovery — the agent read
`[2] FINDINGS.md` deliberately, to check each fix against its own proposed text.

**Result:** two of the 36 `Fixed` findings (PIF-012, PIF-032) turned out
half-applied — each had a two-part proposed fix where only one part actually
landed in commit `fc4ec5e`. Both corrected in commit `85249b0`; see the
"Correction (step D...)" notes added to those two entries above. All other 34
`Fixed` findings were individually checked against current doc text and confirmed
landed as described. Two new, low-stakes items surfaced (below). Zero new BLOCKING
findings. PIF-016 confirmed still correctly `Deferred` (not re-flagged).

### At a glance (Round 3 new findings)

| ID | Severity | Status | Summary |
|---|---|---|---|
| [PIF-038](#pif-038--observationcapabilities-had-no-named-provider-owned-self-report-type) | WORTH-NOTING | Fixed | `ObservationCapabilities` had no named provider-owned self-report type |
| [PIF-039](#pif-039--correlationevidencesource-still-a-free-form-string-unlike-hostnameobservationsource) | WORTH-NOTING | Not an issue | `CorrelationEvidence.source` still a free-form string, unlike `HostnameObservation.source` |

### PIF-038 — `ObservationCapabilities` had no named provider-owned self-report type

| | |
|---|---|
| **Status** | Fixed — decided and applied 2026-09-07, commit `85249b0`. Reason: every other Engine-owned type in `DATA_MODEL.md` has a named provider-owned counterpart it's built from (`ProcessObservation`→`ProcessInfo`, `ProviderStatus`→`ObservationStatus`, etc.); `ObservationCapabilities` was the one exception, described only as "aggregated from each provider's self-report" with nothing named for what a provider actually returns. Low-stakes (Phase 0.3, not persisted, largely static per provider) but a one-line fix consistent with an already-established pattern, cheap enough to close immediately rather than leave as a genuine gap. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[4] DATA_MODEL.md` (`ObservationCapabilities`) vs. `docs/[9] TODO.md` Phase 0.3 |

**Issue**

`ObservationCapabilities`'s ownership note said "Engine-owned, aggregated from each
provider's self-report" without naming a type for that self-report — the one
Engine-owned type in this document without a named provider-owned counterpart.

**Why it matters**

Low-stakes on its own (nothing persisted, no cross-document contradiction), but
worth closing for consistency with every other type in the document, which all
follow the same provider-owned/Engine-owned split.

**Fix applied**

Named `ProviderCapabilities` as the provider-owned counterpart, exposed via a
`capabilities() -> ProviderCapabilities` trait method, aggregated by the Engine into
`ObservationCapabilities` on `get_capabilities()`.

---

### PIF-039 — `CorrelationEvidence.source` still a free-form string, unlike `HostnameObservation.source`

| | |
|---|---|
| **Status** | Not an issue — decided 2026-09-07. Reason: `CorrelationEvidence.source` describes which capture *mechanism* produced a piece of evidence (e.g. `"sni-sniff"`), an open-ended, provider-implementation-specific label — not a fixed provider-kind enum like `HostnameObservation.source`'s `{ReverseDns, Sni, HttpHost}`, which enumerates a small, closed set of DNS-signal sources. PIF-027's original finding named both fields as inconsistent, but its proposed fix (and what was actually applied) only converted `ObservationStatus.provider` — a genuinely closed, small set (`{Process, Socket, Dns, Traffic, Engine}`) — to an enum. Re-reading `CorrelationEvidence`'s own definition confirms `source` is meant to stay open-ended prose, same treatment as `ObservationStatus.reason`. No change needed. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[4] DATA_MODEL.md` (`CorrelationEvidence.source`) |

**Issue**

A prior finding (PIF-027) flagged both `ObservationStatus.provider` and
`CorrelationEvidence.source` as free-form strings inconsistent with
`HostnameObservation.source`'s enum treatment. Only the former was converted; this
flags that the latter appears, at a glance, to be an inconsistent leftover.

**Why it's not an issue**

`CorrelationEvidence.source` is documented as "which provider/mechanism produced
this evidence" — an open-ended, implementation-specific label (unlike
`HostnameObservation.source`'s closed three-value set), so a free-form `String` is
the correct type here, not a missed conversion.

---

## Round 4 — re-verification pass, run 2026-09-07 against commit `0566011`

Per `[3] TODO.md` step D's loop: Round 3 found two SHOULD-FIX-level regressions
(half-applied fixes), so the exit condition wasn't met and another pass was run
to confirm those specific corrections landed and nothing else was missed.

**Part 1 (targeted):** confirmed all three Round 3 corrections — PIF-012's
`TESTING_STRATEGY.md` line, PIF-032's `TODO.md` retention bullet, and PIF-038's
`ProviderCapabilities` naming — landed correctly and completely in commit
`85249b0`. No further correction needed on any of the three.

**Part 2 (fresh full pass):** found one new SHOULD-FIX item and two trivial
WORTH-NOTING items, logged below as PIF-040 through PIF-042. All three fixed
immediately, commit `1354e73`. Zero new BLOCKING findings.

### At a glance (Round 4 new findings)

| ID | Severity | Status | Summary |
|---|---|---|---|
| [PIF-040](#pif-040--rawhttprequestrawhttpresponse-had-no-field-table-or-ownership-tag) | SHOULD-FIX-BEFORE-CODING (blocks Phase 0.4) | Fixed | `RawHTTPRequest`/`RawHTTPResponse` had no field table or ownership tag |
| [PIF-041](#pif-041--process-layer-status-list-omitted-stale-unlike-the-socket-layer) | WORTH-NOTING | Fixed | Process layer status list omitted `stale`, unlike the socket layer |
| [PIF-042](#pif-042--mandatory-test-2-used-inconsistent-casing-vs-tests-134) | WORTH-NOTING | Fixed | Mandatory test 2 used inconsistent casing vs. tests 1/3/4 |

### PIF-040 — `RawHTTPRequest`/`RawHTTPResponse` had no field table or ownership tag

| | |
|---|---|
| **Status** | Fixed — decided and applied 2026-09-07, commit `1354e73`. Reason: blocks Phase 0.4's very first TODO bullet ("Define `RawHTTPRequest`/`RawHTTPResponse`... per `docs/DATA_MODEL.md`"), which had nothing to define from — no fields, no ownership tag, unlike every other type in the document. Also a wire-contract question, not just internal: this is the object `reveal_raw(request_id)` serializes for "show anyway." |
| **Severity** | SHOULD-FIX-BEFORE-CODING (blocks Phase 0.4, not Phase 0.1) |
| **Location** | `docs/[4] DATA_MODEL.md` (`RawHTTPRequest`/`RawHTTPResponse`) vs. `docs/[9] TODO.md` Phase 0.4 vs. `docs/[3] ARCHITECTURE.md` (Engine-owned type list) |

**Issue**

Every other named type in `DATA_MODEL.md` has an explicit `(provider-owned)`/
`(Engine-owned)` tag and a field table — including types not implemented until
later phases (`Flow`, `ObservationCapabilities`). `RawHTTPRequest`/`RawHTTPResponse`
had neither: just prose describing categories of content, no ownership tag, and no
answer to whether `TrafficProvider` constructs it directly or the Engine assembles
it from something lower-level.

**Why it matters**

An implementer hits this on Phase 0.4's first bullet with no field list to build
the struct from, and no answer to whether returning it directly from
`TrafficProvider`'s trait method is an exception to the provider/Engine
construction rule or already consistent with it. Guessing wrong means retyping the
`TrafficProvider` trait signature and the `Redactor`'s input type after Phase
0.3/0.4 code already depends on it.

**Fix applied**

Tagged `RawHTTPRequest`/`RawHTTPResponse` provider-owned (returned directly from
`TrafficProvider`, after the mitmproxy addon's tier-1 redaction, consumed — never
constructed — by the Engine's `Redactor`), and gave both a full field table
mirroring `HTTPRequest`/`HTTPResponse` minus the post-redaction-only fields
(`redacted_fields`, `status`, `evidence`). Added both to the "who constructs this
type" rule's provider-owned list at the bottom of `DATA_MODEL.md`.

---

### PIF-041 — Process layer status list omitted `stale`, unlike the socket layer

| | |
|---|---|
| **Status** | Fixed — decided and applied 2026-09-07, commit `1354e73`. Reason: `ProcessInfo.status` is an `ObservationStatus` subject to the same generic staleness formula as everything else; the socket layer's per-layer list explicitly noted `stale` as Engine-added, the process layer's didn't, for no stated reason. Trivial, one line. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[5] OBSERVATION_CONTRACT.md` ("Applied per layer") |

**Fix applied:** added `stale` to the `processes:` line, matching the socket layer's treatment.

---

### PIF-042 — Mandatory test 2 used inconsistent casing vs. tests 1/3/4

| | |
|---|---|
| **Status** | Fixed — decided and applied 2026-09-07, commit `1354e73`. Reason: tests 1, 3, 4 use the capitalized Rust-variant form (`== Closed`, `== Unmatched`) per the document's own stated wire-form/variant-form convention (`DATA_MODEL.md`'s Notation section); test 2 used lowercase. Zero implementation risk either way, but cheap to make consistent. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[8] TESTING_STRATEGY.md` (mandatory test 2) |

**Fix applied:** capitalized to `lifecycle_state == Expired — never Closed`, matching tests 1/3/4's convention.

---

## Round 5 — re-verification pass, run 2026-09-07 against commit `1354e73`

Per `[3] TODO.md` step D's loop: Round 4 found one real SHOULD-FIX item (PIF-040),
so another pass was run. This round found one more real SHOULD-FIX item — and
notably, it's fresh drift introduced by PIF-040's own fix, not a leftover from the
original design: giving `RawHTTPRequest`/`RawHTTPResponse` a field table (to close
PIF-040) mirrored `HTTPRequest`/`HTTPResponse`'s shape onto them without checking
whether post-correlation Engine-assigned identity fields belong on a pre-correlation,
provider-owned type. Both fixed immediately, commit `ea7ac0e`. Zero new BLOCKING.

### At a glance (Round 5 new findings)

| ID | Severity | Status | Summary |
|---|---|---|---|
| [PIF-043](#pif-043--rawhttprequestrawhttpresponse-carried-engine-only-identity-fields-they-could-never-legitimately-hold) | SHOULD-FIX-BEFORE-CODING (blocks Phase 0.4) | Fixed | `RawHTTPRequest`/`RawHTTPResponse` carried Engine-only identity fields they could never legitimately hold |
| [PIF-044](#pif-044--observationcapabilities-panel-mock-was-one-row-short-of-data_modelmds-table-request-vs-response-body) | WORTH-NOTING | Fixed | `ObservationCapabilities` panel mock was one row short of `DATA_MODEL.md`'s table (request vs. response body) |

### PIF-043 — `RawHTTPRequest`/`RawHTTPResponse` carried Engine-only identity fields they could never legitimately hold

| | |
|---|---|
| **Status** | Fixed — decided and applied 2026-09-07, commit `ea7ac0e`. Reason: the fourth occurrence of the exact provider/Engine-construction contradiction ADR-011/012/014 already spent three rounds fixing (sockets, processes, hostnames) — except this one was fresh drift from PIF-040's own fix, not inherited from the original design. Blocks Phase 0.4's first bullet the same way PIF-040 did. |
| **Severity** | SHOULD-FIX-BEFORE-CODING (blocks Phase 0.4, not Phase 0.1) |
| **Location** | `docs/[4] DATA_MODEL.md` (`RawHTTPRequest`/`RawHTTPResponse`, added by PIF-040's fix) vs. this document's own "who constructs this type" rule vs. `docs/[3] ARCHITECTURE.md` (`reveal_raw`) |

**Issue**

`RawHTTPRequest.connection_id` and `RawHTTPResponse.request_id` — added by PIF-040's
fix to mirror `HTTPRequest`/`HTTPResponse`'s shape — are Engine-assigned identity
fields on a type explicitly tagged provider-owned and provider-constructed. Nothing
named which actor sets them or when; the provider can't (it doesn't have Engine
identity), and the Engine "never constructs" `Raw*` per the same section. Separately,
nothing linked a captured raw flow to the `CorrelationEvidence` it should be scored
against, and `reveal_raw(request_id)`'s lookup mechanism was unspecified once the
`Raw*` objects themselves couldn't carry that ID.

**Why it matters**

Same failure mode as PIF-009/PIF-002 (contradictory required-field-vs-ownership),
just freshly introduced. Blocks Phase 0.4: an implementer building `TrafficProvider`,
the `Redactor`, and `reveal_raw` has no correct way to populate these fields.

**Fix applied**

Removed `connection_id`/`request_id` from `RawHTTPRequest`/`RawHTTPResponse`.
Specified `TrafficProvider`'s trait method returns `(RawHTTPRequest,
Option<RawHTTPResponse>, CorrelationEvidence)` — giving the Engine something to
correlate against without the raw type needing its own future identity. Named the
actual `reveal_raw` mechanism: a session-scoped `request_id → (RawHTTPRequest,
Option<RawHTTPResponse>)` map, populated when the Redactor mints `request_id` while
processing a pair — the map is what `reveal_raw` looks up, not a field on `Raw*`.

---

### PIF-044 — `ObservationCapabilities` panel mock was one row short of `DATA_MODEL.md`'s table (request vs. response body)

| | |
|---|---|
| **Status** | Fixed — decided and applied 2026-09-07, commit `ea7ac0e`. Reason: PIF-015's fix added the two missing rows (`remote_addresses`, `raw_packet_data`) but didn't reconcile that `DATA_MODEL.md` has separate `request_body`/`response_body` fields while the mock still has one combined "HTTP body" row. Low-stakes (report already disclaims authority to `DATA_MODEL.md`), but cheap to note as deliberate rather than leave looking like a miss. |
| **Severity** | WORTH-NOTING |
| **Location** | `docs/[1] process-network-inspector-report.md` ("Observation Capabilities panel") vs. `docs/[4] DATA_MODEL.md` (`ObservationCapabilities`) |

**Fix applied:** added a sentence noting the mock's single "HTTP body" row deliberately combines `DATA_MODEL.md`'s two fields for space — a display choice, not a missed field.
