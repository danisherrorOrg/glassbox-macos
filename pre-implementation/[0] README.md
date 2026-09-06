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

See `[3] TODO.md` for the live checklist. As of the first pass (Round 1, run
2026-09-06), the design is **not yet cleared** for Phase 0.1 — one BLOCKING finding
(`PIF-001`) is open. See `[2] FINDINGS.md` for detail.
