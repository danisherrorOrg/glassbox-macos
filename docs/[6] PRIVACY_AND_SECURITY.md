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

   **Where this actually runs:** capture physically happens inside the Python mitmproxy helper addon (`ARCHITECTURE.md`), not the Rust core, so tier-1 redaction runs **inside that addon**, before any value crosses the helper→core IPC socket. If it ran in the Rust core instead, the raw value would necessarily have existed in the helper's memory and traveled over the IPC socket first — contradicting "never exists in raw form anywhere, including in memory." The helper and the Rust core share the same field-name list (the starter list below), distributed as a single data file both processes read — neither maintains its own separate copy, so the two can't drift apart.

2. **Persistence/export redaction (potentially sensitive tier)** — mandatory, irreversible, applied to everything written to the Session Store or any export file, regardless of what's currently toggled on screen in the live UI. A "show anyway" preference (via the `reveal_raw(request_id)` command — `DATA_MODEL.md`) reveals the in-memory `Raw*` object for display only and must never propagate to disk.

## Starter sensitive-field list (tier 1, capture-time, irreversible)

The list `Phase 0.3`'s addon and `Phase 0.4`'s `Redactor` both start from — matching is case-insensitive substring against the leaf key name only, never the value, and applies to header names and to body/query keys (including nested JSON, matched by leaf key regardless of path depth):

- **Header names:** `authorization`, `proxy-authorization`, `x-api-key`, `api-key`, `x-auth-token`, `x-access-token`, `authentication`
- **Body/query keys:** `password`, `passwd`, `secret`, `token`, `api_key`, `apikey`, `access_key`, `private_key`, `client_secret`, `refresh_token`, `session_id`, `credential`

**Tier 2 (potentially sensitive, reversible in-session):** `cookie`, `set-cookie`, and full request/response bodies and query strings — these get the display-transform/persistence-transform split above, not the irreversible tier-1 treatment.

This list is user-extendable (per `TODO.md` Phase 0.4's "configurable list") but never user-shrinkable below the baseline above — a user can add field names to redact, never remove ones already on this list.

## Tauri/React-specific rules

The FastAPI-era stack (`DECISIONS.md` ADR-009) required binding a server to `127.0.0.1`, locking down CORS, and reconfiguring uvicorn's access logging — all to manage a local network surface that a native app never had. ADR-013 removed that surface rather than continuing to manage it: Tauri's `invoke`/`event` IPC bridge is not a network socket, so there is no port to bind and no CORS policy to restrict. What still applies to this stack specifically:

- Tauri's command allowlist/capabilities configuration (`tauri.conf.json`) should expose only the specific commands the frontend actually needs — not a blanket "allow everything" capability set. This is the Tauri-native equivalent of the old CORS/binding discipline: restrict what the webview is permitted to reach into, even though the mechanism is different.
- The mitmproxy helper's local IPC channel to the Rust core is process-local and not reachable from outside the machine; it carries no user-facing surface and needs no additional locking-down beyond what `PERMISSIONS_AND_PLATFORM.md` already assumes about the helper process itself.
- No captured request/response body or header content goes into `println!`/`log`/application logging at any level — only into the Redactor's own controlled output paths. Rust's own logging crates (`log`, `tracing`) have the same "the transport/framework logs independently of your application code" risk that uvicorn had; audit whatever logging is configured for the Tauri shell and the mitmproxy helper for the same reason the old uvicorn access-log risk was called out.

## Storage & export

- **Sessions are always stored in redacted form. This is an unconditional invariant, not a default with a hypothetical opt-out.** An earlier version of this document left the door open to "raw storage, if ever offered, as an explicit opt-in" — that phrasing is dropped. It sat awkwardly next to the rest of this document's absolute language ("never persisted," "no reveal path"), nobody has asked for a raw-storage feature, and leaving the possibility documented costs nothing to remove and something real to keep: raw in memory, never disk, with no exception clause for a future feature to grow into.
- Potentially-sensitive raw data (`RawHTTPRequest`/`RawHTTPResponse` in `DATA_MODEL.md`) may be retained transiently in memory during a live session only — never persisted, and destroyed (not just dereferenced) when the session ends. See `DATA_MODEL.md`'s lifetime rule for the memory-budget and eviction requirements this implies.
- Highly-sensitive fields are never retained raw anywhere, including in memory — see the classification table above.
- Maximum body-preview size and header/session memory limits are enforced at capture time (`DATA_MODEL.md`'s `body_preview` fields are truncated, not full bodies) — oversized captures are truncated safely rather than held in full. **The size limit itself is 8 KiB (8192 bytes) per `body_preview`**, provisional — this is the number Phase 0.4 actually enforces; Phase 0.6 revisits it only as part of the broader memory-budget/eviction-policy design (retained-object caps, session-level budget), not as a re-litigation of this specific number.
- Export (session as JSON, single flow as text) goes through the exact same mandatory redaction path as storage. There is no export code path that bypasses the Redactor.
- Document where session files live on disk once Phase 0.6 implements storage, so a user can find and delete them without hunting.
- **CA certificate — documented 2026-09-07, confirmed against a real trust step during the Phase 0.3 spike, not just mitmproxy's general documentation:** HTTPS observation requires trusting a locally-generated mitmproxy CA certificate once (see `process-network-inspector-report.md` §2's clarification on the capture mechanism).
  - **Where it lives:** `mitmproxy` generates it on first run, at `~/.mitmproxy/` — `mitmproxy-ca-cert.pem`/`.cer`/`.p12` (the certificate, in three formats) and `mitmproxy-ca.pem`/`.p12` (the certificate *and* private key together — more sensitive than the cert-only files, since it's what would let someone else mint certificates this CA would be trusted for). This project doesn't relocate or duplicate these files; they're exactly where mitmproxy itself puts them, one shared location regardless of which target process a capture session runs against.
  - **How it's trusted:** at the user's login-keychain level (`security add-trusted-cert -r trustRoot -k ~/Library/Keychains/login.keychain-db ~/.mitmproxy/mitmproxy-ca-cert.pem`), **not** the system/admin keychain — this needs the user's own Keychain authorization (Touch ID/password) but not `sudo`, and only affects this one user account, not the whole machine. Confirmed sufficient for TLS interception to actually work (`docs/PERMISSIONS_AND_PLATFORM.md`'s Phase 0.3 spike findings) — no broader trust scope was needed.
  - **How to fully remove it:** two independent steps, both required —
    1. Untrust and delete the certificate from the keychain: `security delete-certificate -c mitmproxy ~/Library/Keychains/login.keychain-db` (removes both the trust setting and the certificate object in one step — this is also directly reversible via Keychain Access.app's GUI: find "mitmproxy" under login keychain → Certificates, delete it).
    2. Delete the files themselves: `rm -rf ~/.mitmproxy` — removes the CA cert/key files above, `mitmproxy-dhparam.pem`, and anything else mitmproxy has cached there. Step 1 alone leaves these files on disk (untrusted, but still present); step 2 alone leaves a dangling trusted-but-file-missing keychain entry. A user wanting to fully undo this needs both, in either order.
  - This project does not (and per "What this document explicitly forbids" below, must never) automate step 1 or 2 as an uninstall action without the user explicitly asking for it — trusting the cert already requires the user's own explicit Keychain approval per step; removing it should be equally deliberate, not a side effect of some other cleanup action.

## What this document explicitly forbids, permanently

No replay, resend, or injection of captured requests, anywhere in the stack, regardless of feature request. No raw-credential "developer mode" that writes unredacted data to disk. No network-facing server of any kind added to this stack without a deliberate, separately-reviewed decision — the whole point of the Tauri IPC boundary (`ARCHITECTURE.md`) is that this app has no such surface today.
