# Pre-Implementation Audit Prompt

Reusable prompt for getting an independent AI model to audit the `docs/` design set
before (or between rounds of) implementation. Paste the block below verbatim, attach
all 9 files from `docs/` (the model needs the actual content — it has no filesystem
access), and log whatever comes back into `[2] FINDINGS.md`.

Re-run this after applying fixes from a previous round, and again any time a doc in
`docs/` changes materially. Stop iterating once a round produces zero new
BLOCKING/SHOULD-FIX findings.

---

```
You are auditing a fully-designed but not-yet-implemented software project before it enters
Phase 0.1. Your job is NOT to redesign anything, propose alternative architectures, suggest new
features, or second-guess settled trade-offs (e.g. the stack choice, the provider pattern). This
project has already been through two rounds of exactly that kind of review (see DECISIONS.md
ADR-011 and ADR-012) — treat those as evidence of what "done well" looks like, and hold this pass
to the same bar or higher.

Your only goal: find anything that will cause a mid-implementation design change — the expensive
kind, where fixing it means touching code already written — versus a cheap doc edit today. I'm
attaching 9 markdown documents (read them in this order, it's designed intentionally):
[0] READING_ORDER.md, [1] process-network-inspector-report.md, [2] DECISIONS.md,
[3] ARCHITECTURE.md, [4] DATA_MODEL.md, [5] OBSERVATION_CONTRACT.md,
[6] PRIVACY_AND_SECURITY.md, [7] PERMISSIONS_AND_PLATFORM.md, [8] TESTING_STRATEGY.md,
[9] TODO.md.

Run these checks, in order, and report findings for each — don't skip a category just because
nothing turns up; say so explicitly:

1. CROSS-DOCUMENT TYPE CONSISTENCY
   Every named type/field that appears in more than one document (e.g. NetworkConnection,
   SocketObservation, ObservationStatus, CorrelationEvidence) must have identical field names,
   types, and ownership (provider-owned vs. Engine-owned) everywhere it's mentioned. Flag any
   drift — including a field present in DATA_MODEL.md's table but silently assumed different in
   prose elsewhere, or a status value used in OBSERVATION_CONTRACT.md that ARCHITECTURE.md or
   TODO.md describes differently.

2. THE "WHO CONSTRUCTS THIS TYPE" RULE
   The core invariant is "providers report observations, the Engine creates domain state," with
   an explicit rule that a provider must never construct an Engine-owned type. Trace every
   Engine-owned type (NetworkConnection, ProcessInfo, Flow, TrafficEvent, ObservationStatus) back
   through every phase in TODO.md and confirm no task, as written, would require a provider
   implementation to construct or mutate one directly. Flag any TODO item that's ambiguous about
   which layer does the work.

3. UNRESOLVED JUDGMENT CALLS — the category the prior two review passes may have under-covered
   For each Phase 0.1 and 0.2 TODO checklist item, ask: "if I sat down to implement this literally
   right now, using only what's in these 9 documents, is there a decision I'd have to make that
   the docs don't actually answer?" Concrete examples of the kind of gap to hunt for (don't limit
   yourself to these):
   - The connection-identity matching heuristic is described as "deterministic-where-possible,
     heuristic-where-not" — is the actual matching algorithm (which fields, what confidence
     threshold, what happens on a tie) specified anywhere, or only the *policy* ("prefer split
     over merge") without the mechanism?
   - ObservationStatus.reason is "human-readable" with no schema — will every provider need to
     invent its own conventions, and does that matter for the frontend or tests?
   - The redaction "configurable list" of sensitive field names — is the actual starter list
     defined anywhere, or does Phase 0.4 start from a blank list with no guidance?
   - Any place a table says "best-effort" or "provider-dependent" without saying what the
     consumer (Engine, frontend) should do when the value is absent.
   Flag every such gap as either (a) fine to leave as an implementation-time decision because it's
   genuinely low-stakes and reversible, or (b) risky enough to pin down in the docs now, with a
   one-line reason why.

4. ASSUMED VS. VERIFIED FACTS THAT DESIGN DEPENDS ON
   PERMISSIONS_AND_PLATFORM.md tags claims VERIFIED/ASSUMED/DECISION. For every ASSUMED claim,
   trace forward: which later documents or TODO phases would need to change if that assumption
   turns out false once the Phase 0 spike runs? Flag any case where a downstream design decision
   (in ARCHITECTURE.md, DATA_MODEL.md, or a TODO phase ordering) is load-bearing on an assumption
   that isn't flagged as risky enough to spike first, or where the spike's findings wouldn't
   actually be sufficient to unblock the next phase.

5. TESTABILITY CHECK
   TESTING_STRATEGY.md names four mandatory integration tests. For each one, confirm the current
   DATA_MODEL.md/OBSERVATION_CONTRACT.md types actually contain enough information to write an
   assertion for it (e.g. can you literally state, in terms of the defined fields, what "the
   Engine marks it exited" or "status reflects transient_failure, lifecycle unchanged" means as a
   check on a concrete object). Flag any mandatory test that's currently only checkable in prose,
   not against a typed field.

6. PHASE BOUNDARY LEAKAGE
   ARCHITECTURE.md is marked "frozen for Phase 0.1." Confirm Phase 0.1's TODO items are actually
   satisfiable using only Phase 0.1-relevant types and rules, without silently depending on a
   concept the docs place in a later phase (Flow/Phase 0.5, ObservationCapabilities/Phase 0.3,
   TrafficProvider/Phase 0.3+). Flag any such leakage.

7. SCOPE-CREEP TRIPWIRE
   Re-check the "explicitly out of scope" list in process-network-inspector-report.md Section 2
   against every phase in TODO.md, including Phase 2. Flag anything, however small, that edges
   toward modify/replay/inject.

OUTPUT FORMAT
For each finding: [Severity: BLOCKING / SHOULD-FIX-BEFORE-CODING / WORTH-NOTING] —
[Document + section] — [the issue in 1-2 sentences] — [why it's expensive if caught mid-
implementation instead of now] — [a concrete proposed fix, phrased as a documentation edit, not
a re-architecture]. End with a one-paragraph verdict: is this design set actually safe to freeze
and start Phase 0.1 against, or is there at least one BLOCKING item first?
```
