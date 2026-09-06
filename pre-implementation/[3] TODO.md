# Pre-Implementation TODO

This is what actually gates the start of `docs/[9] TODO.md` Phase 0.1. Work through
it in order. Every item that references a `PIF-###` finding should be checked off by
updating that finding's `Status` in `[2] FINDINGS.md`, not just ticking the box here —
the box here tracks that the *step* happened, `FINDINGS.md` tracks the *decision*.

**Ordering note:** cross-model verification (A) comes *before* applying fixes (C),
not after. A second model can only independently confirm a Round 1 finding if it
reviews the docs before that finding gets fixed — reviewing already-fixed docs only
tells you the fix worked, not that the original finding was real. Independent
discovery and post-fix regression-checking are different signals; don't collapse them
into one step.

## A — Cross-model verification (run against the *current*, unfixed docs)

- [ ] Run `[1] AUDIT_PROMPT.md` (with all of `docs/` attached, as they stand right
      now — do not pre-apply any Round 1 fixes first) against at least one AI model
      other than the one that produced Round 1.
- [ ] Log every finding into `[2] FINDINGS.md` as "Round 2," same format as Round 1.
      If a Round 2 finding overlaps an existing `PIF-###`, note the overlap in that
      entry rather than creating a duplicate — and note explicitly that it was an
      *independent* rediscovery, which is a stronger signal than Round 1 alone.
- [ ] If Round 2 surfaces something genuinely new, give it its own `PIF-###`.

## B — Reconcile and decide

- [ ] For every `Open` finding (Round 1 + Round 2 combined), decide: fix now, defer
      to a later phase (say which, and why), or not an issue (say why) — and update
      `Status` in `[2] FINDINGS.md` accordingly *before* touching any doc in
      `docs/`. Deciding first keeps the fix step below purely mechanical.

## C — Apply fixes

Current known items as of Round 1 (Round 2 may add more — check `[2] FINDINGS.md`
for the live list before starting this step):

- [ ] **PIF-001** (BLOCKING) — add `ObservationStatus` to `ProcessInfo` in
      `DATA_MODEL.md`, and define the top-level envelope for list-returning Tauri
      commands (`get_processes`, `get_connections(pid)`).
- [ ] **PIF-005** — name the connection-matching composite key candidates in
      `DATA_MODEL.md`.
- [ ] **PIF-003** — write the actual `ObservationState` Rust enum + serde attribute
      in `DATA_MODEL.md`.
- [ ] **PIF-002** — give the correlation `unmatched` outcome a home (`TrafficEvent
      .status` or a new `CorrelationResult` type) — needed before Phase 0.3, not
      Phase 0.1, but cheap to do now while the design is already open.
- [ ] **PIF-004** — move the body-preview size-limit *definition* earlier (out of
      Phase 0.6, into Phase 0.4), or explicitly mark Phase 0.4's constant as
      provisional.
- [ ] **PIF-006** — define the staleness formula relative to the configurable
      polling interval.
- [ ] Any additional items decided "fix now" in step B.
- [ ] **Record an ADR** in `docs/[2] DECISIONS.md` summarizing what changed and why,
      in the same format as ADR-011/ADR-012 ("cross-document consistency pass") —
      this is the same kind of fix those two entries document, and the project's own
      convention is that this history doesn't go unrecorded just because the fix
      originated from an external audit rather than an internal one.

## D — Re-verification pass

- [ ] After step C, re-run `[1] AUDIT_PROMPT.md` once more against the updated
      `docs/` (either model is fine here — this pass is checking "did the fixes
      actually land and not introduce anything new," not independent discovery).
- [ ] Repeat A–D until a round produces zero new BLOCKING/SHOULD-FIX findings.

## E — Final gate before Phase 0.1

- [ ] All BLOCKING findings in `[2] FINDINGS.md` are `Fixed` (none left `Open`).
- [ ] All SHOULD-FIX findings are either `Fixed` or explicitly `Deferred` with a
      reason and a phase they're deferred *to*.
- [ ] The ADR from step C is committed in `docs/[2] DECISIONS.md`.
- [ ] **Owner sign-off:** you've actually read `[2] FINDINGS.md` end to end and are
      comfortable freezing the design on these terms — not just "the checklist has
      no open boxes."
- [ ] `[0] README.md`'s "Status at a glance" is updated to reflect the cleared state.
- [ ] Only then: start `docs/[9] TODO.md` Phase 0 (project setup) and Phase 0.1.

## F — Permissions spike (empirical, not a doc audit)

Steps A–E only clear *design* risk — contradictions and gaps in the docs themselves.
This step is different in kind: it resolves the `ASSUMED` claims in
`docs/[7] PERMISSIONS_AND_PLATFORM.md` that no amount of doc review can verify,
because they're facts about this machine's actual OS/privilege behavior. Unlike A–E,
this step has no dependency on doc content — it's a throwaway probe script, not real
provider code — so it can run **in parallel** with A–E if you want to save calendar
time. The only hard constraint is that it must complete, and its findings must be
folded back in (last bullet below) if they turn out to matter, before any real
`Provider` code in `docs/[9] TODO.md` Phase 0.1.

- [ ] Set up just enough of the toolchain to run a throwaway probe (`cargo`, Tauri
      CLI) — doesn't need the real project scaffold yet.
- [ ] Confirm what an unprivileged Rust process can read about **other same-user**
      processes' sockets via `sysinfo`/`netstat2`, and what falls back to `libproc`
      FFI being required.
- [ ] Confirm what happens for **other users'** processes — expected to need root;
      confirm it actually does, and exactly what error/empty-result shape the
      refusal takes (this affects the `permission_denied` provider-status mapping in
      `docs/[5] OBSERVATION_CONTRACT.md`).
- [ ] Write the findings back into `docs/[7] PERMISSIONS_AND_PLATFORM.md`, flipping
      the relevant claims from `ASSUMED` to `VERIFIED` (or correcting them if reality
      disagrees).
- [ ] If any finding invalidates a downstream design decision (per the trace done in
      category 4 of `[1] AUDIT_PROMPT.md`), log it as a new `PIF-###` in
      `[2] FINDINGS.md` and loop back through B–E before proceeding.
- [ ] Only after this step *and* E: begin `docs/[9] TODO.md` Phase 0's actual project
      setup and Phase 0.1's provider implementation.

---

**Current state (2026-09-06):** Step A not started. Round 1's six findings
(`PIF-001` through `PIF-006`) are logged in `[2] FINDINGS.md`, all `Open`. Do not
apply fixes before running Step A against at least one other model.
