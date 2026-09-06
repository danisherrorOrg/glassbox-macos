# Privacy & Security — Process Network Inspector

This project's entire premise is observing other processes' potentially sensitive traffic. This document is the single source of truth for what's captured, what's kept, and what's exposed — the TODO's individual checklist items (redact before persistence, no sensitive data in logs, size limits) all derive from the rules here, not the other way around.

## Data classification

| Class | Examples | Handling |
|---|---|---|
| Public-ish | process name, PID, remote IP | shown freely |
| Potentially sensitive | URLs, query parameters, request/response bodies, cookies, non-auth headers | held raw, transiently, in memory only (`RawHTTPRequest`/`RawHTTPResponse` in `DATA_MODEL.md`); redacted by default in the persisted/redacted view, revealable via "show anyway" for the current session only |
| Highly sensitive | `Authorization`, API keys, passwords, tokens, credentials in any field name | redacted irreversibly **at capture time** — never exists in raw form anywhere, including in memory. There is no reveal path, because there is nothing left to reveal. |

**This resolves an earlier inconsistency between this document and `DATA_MODEL.md`/`ARCHITECTURE.md`:** an earlier draft said headers were "redacted before the object exists" while also describing a "show anyway" path that implied a raw value survived somewhere to be revealed. Those can't both be true for the same field. The fix is the two-tier split above — only the *potentially sensitive* tier gets the raw-transient/redacted-persistent treatment; the *highly sensitive* tier skips the raw-in-memory step entirely and is redacted the moment it's captured, with no reveal path at all. The failure mode of over-redacting a credential is far cheaper than the failure mode of exposing one, so the tier where heuristic matching could plausibly be wrong is exactly the tier that gets the irreversible treatment.

## The pipeline rule

```
Capture → Redact → Normalize → Display / Store / Export
```

Never:

```
Capture → Store raw → Redact only in the UI
```

This is not a style preference — it's the difference between a session file on disk containing your actual API tokens versus not. `PRIVACY_AND_SECURITY.md` is the authority if any future code path is tempted to store raw data "just for now, we'll redact on read": don't.

## The two redaction checkpoints (see `ARCHITECTURE.md` and `DATA_MODEL.md`)

1. **Capture-time redaction (highly sensitive only)** — irreversible, applied before a `RawHTTPRequest`/`RawHTTPResponse` object even exists for these fields. Nothing downstream — display, storage, export, "show anyway" — can ever see these values, because they were never captured in retrievable form.
2. **Persistence/export redaction (potentially sensitive tier)** — mandatory, irreversible, applied to everything written to the Session Store or any export file, regardless of what's currently toggled on screen in the live UI. A "show anyway" preference reveals the in-memory `Raw*` object for display only and must never propagate to disk.

## FastAPI/React-specific rules (new since the stack pivot — a native app didn't need these)

- Bind the FastAPI server to `127.0.0.1` only; never `0.0.0.0`.
- Restrict CORS to the frontend's own origin explicitly — don't use a wildcard.
- Disable or reconfigure uvicorn's default access logging so that request URLs (which can carry query-string secrets) aren't written to a plaintext log file by the web framework itself, bypassing the Redactor entirely. This is a real, specific risk this stack introduces that a native app never had: the transport layer has its own logging behavior independent of application code.
- No captured request/response body or header content goes into `print()`/application logging at any log level — only into the Redactor's own controlled output paths.

## Storage & export

- **Sessions are always stored in redacted form. This is an unconditional invariant, not a default with a hypothetical opt-out.** An earlier version of this document left the door open to "raw storage, if ever offered, as an explicit opt-in" — that phrasing is dropped. It sat awkwardly next to the rest of this document's absolute language ("never persisted," "no reveal path"), nobody has asked for a raw-storage feature, and leaving the possibility documented costs nothing to remove and something real to keep: raw in memory, never disk, with no exception clause for a future feature to grow into.
- Potentially-sensitive raw data (`RawHTTPRequest`/`RawHTTPResponse` in `DATA_MODEL.md`) may be retained transiently in memory during a live session only — never persisted, and destroyed (not just dereferenced) when the session ends. See `DATA_MODEL.md`'s lifetime rule for the memory-budget and eviction requirements this implies.
- Highly-sensitive fields are never retained raw anywhere, including in memory — see the classification table above.
- Maximum body-preview size and header/session memory limits are enforced at capture time (`DATA_MODEL.md`'s `body_preview` fields are truncated, not full bodies) — oversized captures are truncated safely rather than held in full.
- Export (session as JSON, single flow as text) goes through the exact same mandatory redaction path as storage. There is no export code path that bypasses the Redactor.
- Document where session files live on disk once Phase 0.6 implements storage, so a user can find and delete them without hunting.

## What this document explicitly forbids, permanently

No replay, resend, or injection of captured requests, anywhere in the stack, regardless of feature request. No raw-credential "developer mode" that writes unredacted data to disk. No FastAPI server reachable from anything other than localhost without a deliberate, separately-reviewed decision to change that.
