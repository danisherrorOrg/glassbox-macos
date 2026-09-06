# Privacy & Security — Process Network Inspector

This project's entire premise is observing other processes' potentially sensitive traffic. This document is the single source of truth for what's captured, what's kept, and what's exposed — the TODO's individual checklist items (redact before persistence, no sensitive data in logs, size limits) all derive from the rules here, not the other way around.

## Data classification

| Class | Examples | Handling |
|---|---|---|
| Public-ish | process name, PID, remote IP | shown freely |
| Potentially sensitive | URLs, query parameters, request/response bodies, cookies, non-auth headers | redacted by default, revealable via "show anyway" for the current session only |
| Highly sensitive | `Authorization`, API keys, passwords, tokens, credentials in any field name | always redacted, including in "show anyway" mode — never revealed in the UI at all |

The highly-sensitive tier is intentionally stricter than "redact by default" — a header or body field matching the sensitive-field heuristics (see `DATA_MODEL.md`/`Redactor`) should not have a reveal path in the UI, precisely because heuristic matching isn't perfectly reliable and the failure mode of over-redacting is far cheaper than the failure mode of exposing a credential.

## The pipeline rule

```
Capture → Redact → Normalize → Display / Store / Export
```

Never:

```
Capture → Store raw → Redact only in the UI
```

This is not a style preference — it's the difference between a session file on disk containing your actual API tokens versus not. `PRIVACY_AND_SECURITY.md` is the authority if any future code path is tempted to store raw data "just for now, we'll redact on read": don't.

## The two redaction checkpoints (see `ARCHITECTURE.md`)

1. **Display-time redaction** — reversible within a running session via an explicit "show anyway" action, for the *potentially sensitive* tier only.
2. **Persistence/export redaction** — mandatory, irreversible, applied to everything written to the Session Store or any export file, regardless of what's currently toggled on screen. A "show anyway" preference must never propagate to disk.

## FastAPI/React-specific rules (new since the stack pivot — a native app didn't need these)

- Bind the FastAPI server to `127.0.0.1` only; never `0.0.0.0`.
- Restrict CORS to the frontend's own origin explicitly — don't use a wildcard.
- Disable or reconfigure uvicorn's default access logging so that request URLs (which can carry query-string secrets) aren't written to a plaintext log file by the web framework itself, bypassing the Redactor entirely. This is a real, specific risk this stack introduces that a native app never had: the transport layer has its own logging behavior independent of application code.
- No captured request/response body or header content goes into `print()`/application logging at any log level — only into the Redactor's own controlled output paths.

## Storage & export

- Sessions are stored **redacted-only by default**; raw storage, if ever offered, is an explicit, separately-gated opt-in.
- Maximum body-preview size and header/session memory limits are enforced at capture time (`DATA_MODEL.md`'s `body_preview` fields are truncated, not full bodies) — oversized captures are truncated safely rather than held in full.
- Export (session as JSON, single flow as text) goes through the exact same mandatory redaction path as storage. There is no export code path that bypasses the Redactor.
- Document where session files live on disk once Phase 0.6 implements storage, so a user can find and delete them without hunting.

## What this document explicitly forbids, permanently

No replay, resend, or injection of captured requests, anywhere in the stack, regardless of feature request. No raw-credential "developer mode" that writes unredacted data to disk. No FastAPI server reachable from anything other than localhost without a deliberate, separately-reviewed decision to change that.
