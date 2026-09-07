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

- [x] Run `[1] AUDIT_PROMPT.md` (with all of `docs/` attached, as they stand right
      now — do not pre-apply any Round 1 fixes first) against at least one AI model
      other than the one that produced Round 1. Done 2026-09-07: ran against Opus 5
      (Round 1 was Sonnet 5, same session that wrote the docs), via a fresh agent
      with no access to `[2] FINDINGS.md`/this file, to preserve independence.
- [x] Log every finding into `[2] FINDINGS.md` as "Round 2," same format as Round 1.
      If a Round 2 finding overlaps an existing `PIF-###`, note the overlap in that
      entry rather than creating a duplicate — and note explicitly that it was an
      *independent* rediscovery, which is a stronger signal than Round 1 alone. Done:
      5 of Round 2's findings independently rediscovered PIF-001/002/003/005/006
      (cross-referenced in place); two of those five (PIF-002, PIF-006) came back at
      a higher severity than Round 1 assigned — flagged for step B.
- [x] If Round 2 surfaces something genuinely new, give it its own `PIF-###`. Done:
      31 new findings logged, PIF-007 through PIF-037 (9 BLOCKING, 15 SHOULD-FIX, 7
      WORTH-NOTING — severity counts across the *new* IDs only; see `[2] FINDINGS.md`
      for the full at-a-glance table).

## B — Reconcile and decide

- [x] For every `Open` finding (Round 1 + Round 2 combined), decide: fix now, defer
      to a later phase (say which, and why), or not an issue (say why) — and update
      `Status` in `[2] FINDINGS.md` accordingly *before* touching any doc in
      `docs/`. Deciding first keeps the fix step below purely mechanical. Done
      2026-09-07, against the actual `docs/` content (not just the finding
      summaries) to confirm each one before deciding: 36 of 37 findings are
      `Fix now`; PIF-016 (missing `[N] ` prefix in cross-references) is `Deferred`
      to its own standalone mechanical commit in Phase 0, since it's purely
      cosmetic and bundling it into step C's substantive edits would bury the real
      diffs. No finding came back `Not an issue`. The two Round 1/Round 2 severity
      disagreements (PIF-002, PIF-006) are both resolved as `Fix now` regardless of
      exactly which severity label is correct.

## C — Apply fixes

**Done 2026-09-07, commit `fc4ec5e`.** This bullet list predates Round 2 and named
only Round 1's six items; the actual scope ended up being all 36 findings step B
marked `Fix now` (see `[2] FINDINGS.md`'s Status column, which was the live,
authoritative list by the time this step ran, per the note left in step B above) —
kept below in its original form for the historical record of what was originally
scoped, not as the actual done-list.

- [x] **PIF-001** (BLOCKING) — add `ObservationStatus` to `ProcessInfo` in
      `DATA_MODEL.md`, and define the top-level envelope for list-returning Tauri
      commands (`get_processes`, `get_connections(pid)`).
- [x] **PIF-005** — name the connection-matching composite key candidates in
      `DATA_MODEL.md`.
- [x] **PIF-003** — write the actual `ObservationState` Rust enum + serde attribute
      in `DATA_MODEL.md`.
- [x] **PIF-002** — give the correlation `unmatched` outcome a home (`TrafficEvent
      .status` or a new `CorrelationResult` type) — needed before Phase 0.3, not
      Phase 0.1, but cheap to do now while the design is already open.
- [x] **PIF-004** — move the body-preview size-limit *definition* earlier (out of
      Phase 0.6, into Phase 0.4), or explicitly mark Phase 0.4's constant as
      provisional.
- [x] **PIF-006** — define the staleness formula relative to the configurable
      polling interval.
- [x] Any additional items decided "fix now" in step B. Done: all 30 Round-2-only
      `Fix now` items (`PIF-007`–`PIF-037` minus the deferred `PIF-016`) applied
      across `docs/[1]` through `docs/[9]` in the same commit.
- [x] **Record an ADR** in `docs/[2] DECISIONS.md` summarizing what changed and why,
      in the same format as ADR-011/ADR-012 ("cross-document consistency pass") —
      this is the same kind of fix those two entries document, and the project's own
      convention is that this history doesn't go unrecorded just because the fix
      originated from an external audit rather than an internal one. Done: ADR-014.

## D — Re-verification pass

- [x] After step C, re-run `[1] AUDIT_PROMPT.md` once more against the updated
      `docs/` (either model is fine here — this pass is checking "did the fixes
      actually land and not introduce anything new," not independent discovery).
      Done 2026-09-07 (Round 3 in `[2] FINDINGS.md`): found two of the 36 `Fixed`
      findings (PIF-012, PIF-032) were only half-applied in commit `fc4ec5e` — each
      had a two-part proposed fix where one part was missed. Both corrected in
      commit `85249b0`, along with one new low-stakes item (PIF-038, fixed same
      commit) and one confirmed non-issue (PIF-039). Zero new BLOCKING findings.
- [x] Repeat A–D until a round produces zero new BLOCKING/SHOULD-FIX findings.
      **Stopped short of this literal exit condition by explicit decision,
      2026-09-07, after Round 5** — not because a round actually hit zero. Round 4
      found one real SHOULD-FIX item (PIF-040) plus two trivial ones; Round 5 found
      one more real SHOULD-FIX item (PIF-043) — notably, fresh drift introduced by
      Round 4's own fix, not a leftover from the original design — plus one trivial
      item. All fixed (commits `1354e73`, `ea7ac0e`). At that point, three straight
      re-verification rounds (3, 4, 5) had produced zero new BLOCKING findings, and
      the remaining churn had the shape of diminishing-returns drift (each fix
      pass risked introducing a new small inconsistency elsewhere) rather than
      substantive design gaps. Asked the project owner whether to run Round 6, do
      a lighter self-check, or stop — chose to stop and proceed to step E. This is
      a real deviation from the loop's stated exit condition, recorded here rather
      than silently treated as satisfied; if a design gap surfaces during Phase
      0.1 implementation that a Round 6 would plausibly have caught, that's the
      cost of this decision.

## E — Final gate before Phase 0.1

- [x] All BLOCKING findings in `[2] FINDINGS.md` are `Fixed` (none left `Open`).
      Verified 2026-09-07: every `BLOCKING`-severity entry (`PIF-001`, `PIF-007`,
      `PIF-008`, `PIF-009`, `PIF-010`, `PIF-028`, plus PIF-002/006's Round-2-rated
      BLOCKING severity) shows `Status: Fixed`.
- [x] All SHOULD-FIX findings are either `Fixed` or explicitly `Deferred` with a
      reason and a phase they're deferred *to*. Verified 2026-09-07: 42 `Fixed`,
      1 `Deferred` (`PIF-016`, to a standalone Phase 0 commit, reason recorded),
      1 `Not an issue` (`PIF-039`, reason recorded). Zero left `Open`.
- [x] The ADR from step C is committed in `docs/[2] DECISIONS.md`. ADR-014,
      commit `fc4ec5e`.
- [ ] **Owner sign-off:** you've actually read `[2] FINDINGS.md` end to end and are
      comfortable freezing the design on these terms — not just "the checklist has
      no open boxes." **This is a human step — not something an agent can do on
      the owner's behalf.** In particular, worth deliberately re-reading: the step
      D loop-stop decision above (a real deviation from the stated exit condition,
      not a technicality), and PIF-002/PIF-006's severity note (Round 2 rated them
      BLOCKING where Round 1 didn't — both were fixed regardless, but the
      disagreement itself is worth the owner's own read, not just this agent's
      resolution of it).
- [x] `[0] README.md`'s "Status at a glance" is updated to reflect the cleared
      state (including the step D loop-stop decision and step F's outstanding
      status), 2026-09-07.
- [ ] Only then: start `docs/[9] TODO.md` Phase 0 (project setup) and Phase 0.1.
      Blocked on the owner sign-off above, and on step F (below) per its own
      hard constraint.

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

**Current state (2026-09-07):** Steps A, B, and C complete. All 36 `Fix now`
findings are applied to `docs/` and committed (`fc4ec5e`), recorded as ADR-014
(`docs/[2] DECISIONS.md`), and each finding in `[2] FINDINGS.md` is marked `Fixed`
with that commit cited. PIF-016 remains `Deferred` to its own standalone mechanical
commit (not yet done — do it before step E's final gate, since it's still part of
this pre-implementation gate). **Step D (re-verification pass) is next:** re-run
`[1] AUDIT_PROMPT.md` against the now-updated `docs/` to confirm the fixes actually
landed and didn't introduce anything new.
