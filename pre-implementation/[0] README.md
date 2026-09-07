# Pre-Implementation — Process Network Inspector

**Purpose:** the design in `docs/` has already been through two internal consistency
passes (`docs/[2] DECISIONS.md` ADR-011, ADR-012). This folder is a *third*, external
kind of check, run before Phase 0.1 starts: is the design actually complete enough to
**code from**, not just internally consistent? The distinction matters because
contradiction-hunting and gap-hunting catch different bugs — see
`[1] AUDIT_PROMPT.md`'s category 3 for why.

The premise: a design change is cheap right now (edit a markdown table) and expensive
once Phase 0.1 is underway (touch code, migrate types, rewrite tests). Everything in
this folder exists to move as many of those changes as possible from "discovered mid-
implementation" to "found now."

## Files, in reading order

1. **`[1] AUDIT_PROMPT.md`** — the reusable prompt. Paste it (with the `docs/` files
   attached) into another AI model to get an independent pass. Re-run it after any
   round of fixes, and whenever a doc changes materially.
2. **`[2] FINDINGS.md`** — the results log. One entry per finding, append-only —
   never delete an entry, only update its `Status`, the same discipline
   `docs/[2] DECISIONS.md` uses and for the same reason: the point is answering "did
   we check this, and what did we decide" months later, not keeping the file short.
3. **`[3] TODO.md`** — the actionable checklist. Work through it in order; it's what
   actually gates the start of `docs/[9] TODO.md` Phase 0.1.

## How this loop works

```
Round 1: run AUDIT_PROMPT.md (self-audit)
        │
        ▼
Round 2: run AUDIT_PROMPT.md against a DIFFERENT model, against the SAME
         (still-unfixed) docs — independent discovery only works before fixes land
        │
        ▼
Log every finding from both rounds in FINDINGS.md, Status = Open
        │
        ▼
Decide per finding: fix now (Status → Fixed) or defer (Status → Deferred, with why)
        │
        ▼
Apply the fixes as doc edits in docs/, then record them as a DECISIONS.md ADR
        │
        ▼
Re-run AUDIT_PROMPT.md against the updated docs (this pass checks the fix landed,
   not independent discovery — either model is fine here)
        │
        ▼
Repeat from Round 2 until a round produces zero new BLOCKING/SHOULD-FIX findings
        │
        ▼
TODO.md's final gate: owner sign-off, then green-light Phase 0.1
```

Cross-model verification matters here specifically because a single model (including
whichever one wrote the original docs) can share blind spots with itself across
review passes. A different model running the same prompt against the same
*(still-unfixed)* docs is a cheap way to catch what round 1 didn't — but only if it
runs before those findings are already fixed. See `[3] TODO.md` for the exact,
current ordering.

## Status at a glance

See `[3] TODO.md` for the live checklist. As of 2026-09-07, steps A–D and F have run:
Round 1 (self-audit) and Round 2 (independent second-model audit) found 37 issues;
Rounds 3–5 (re-verification passes) found 8 more, several of them fresh drift
introduced by earlier rounds' own fixes rather than leftovers from the original
design; step F's empirical permissions spike (run directly on this machine, not a
doc re-read) found 2 more, both stemming from real test results rather than doc
ambiguity. All 46 findings (`PIF-001`–`PIF-046`) are resolved — 44 `Fixed`, 1
`Deferred` (`PIF-016`, a cosmetic cross-reference cleanup, intentionally left for
its own standalone commit), 1 `Not an issue`. **Zero findings are left `Open`, and
none are `BLOCKING`.** The step D re-verification loop was stopped after Round 5 by
an explicit decision — not because a round hit the project's literal "zero new
findings" exit criterion — on the basis that three straight rounds produced zero
new BLOCKING findings and the remaining churn had the shape of diminishing-returns
drift rather than substantive gaps; see `[3] TODO.md` step D's note and
`[2] FINDINGS.md`'s Round 5 section for the full reasoning.

Step F confirmed same-user process/socket visibility needs no elevation on this
machine (no fallback to raw `libproc` FFI required for `SocketProvider`), confirmed
the exact refusal shape for other-users' processes (`EPERM`, errno 1), and found one
real capability gap: per-socket byte counters are not obtainable via this project's
documented provider stack on macOS at all — a platform limit, not an implementation
gap — so the report's Level-3 example was corrected (`PIF-045`). It also surfaced an
implementation-guidance gap for Phase 0.1: `SocketProvider` can't rely on `netstat2`'s
own error type to produce `permission_denied` (the crate silently swallows per-PID
permission failures), and `sysinfo`'s uid isn't reliable enough to make that
same-user/other-user determination itself (`PIF-046`) — both fixed by adding explicit
implementation guidance to `docs/[7] PERMISSIONS_AND_PLATFORM.md` rather than a
contract change, since `OBSERVATION_CONTRACT.md`'s `permission_denied` status was
already correctly specified. Full method and raw results live in
`docs/[7] PERMISSIONS_AND_PLATFORM.md`'s "First technical spike" section.

**Not yet done:** step E's owner sign-off (a human read of `[2] FINDINGS.md`
end-to-end, separate from the checklist having no open boxes) — **this is the only
thing left before `docs/[9] TODO.md` Phase 0.1 begins**, and it's a human step this
agent cannot complete on the owner's behalf.
