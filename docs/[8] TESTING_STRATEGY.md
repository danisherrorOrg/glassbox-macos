# Testing Strategy — Process Network Inspector

## The pyramid

```
Unit tests (cargo test)
      ↓
Provider tests (mocked system-call boundary)
      ↓
Observation Engine tests (correlation logic, no real OS calls)
      ↓
Integration tests (NetworkTestTarget → full core pipeline)
      ↓
Frontend component tests (React Testing Library)
      ↓
End-to-end pass (before calling any version 1.0)
```

## Unit tests

Standard `cargo test`, one module per provider and per Engine responsibility (lifecycle diffing, correlation matching, redaction). Mock the system-call boundary — the `sysinfo`/`netstat2`/`libproc` calls, the mitmproxy helper's IPC — behind the same provider traits `ARCHITECTURE.md` defines, so tests don't depend on real running processes or real network traffic, and run identically in CI or offline. A mock provider implementation (returning fixture `SocketObservation`/`ProcessObservation` values) is the Rust-native equivalent of mocking `subprocess`/`psutil` calls directly.

## Provider tests

Each provider is tested against fixture data resembling real `sysinfo`/`libproc`/mitmproxy output, asserting it parses into the correct `serde`-derived types and returns the correct `OBSERVATION_CONTRACT.md` status for edge cases: empty output, malformed output, a permission-denied error message, a timeout.

## Observation Engine tests

The highest-value tests in this project, because this is where the product's actual differentiator (correlation) lives. Feed the Engine synthetic sequences of `SocketSnapshot`s and traffic events, and assert: connection identity survives across snapshots (including the reused-local-port and rapid close/reopen cases from `TODO.md`), lifecycle transitions fire at the right points, a traffic event with no matching connection ends up `unmatched` rather than silently dropped or wrongly attached, and redaction has actually been applied before anything reaches the Session Store — not just before it reaches the API response.

## `NetworkTestTarget`

A small deterministic Python script, not a full application, that generates on demand:

```
a plain TCP connection
a short-lived connection (opens and closes quickly)
a long-lived connection (stays open)
several simultaneous connections
a local connection (localhost)
an HTTP request (once Phase 0.3 exists)
an HTTPS request
a large response body (to exercise truncation limits)
a slow response (to exercise timing/timeline)
requests carrying intentionally fake sensitive-looking fields (password, token, Authorization header) for redaction testing
```

Every integration test in this project should be written against `NetworkTestTarget`, not against Chrome, Slack, or any other real application — same principle the source learning path opened with: verify against traffic you already understand before trusting a tool on anything else.

## Integration tests

Exercise the full chain end to end within the Rust core, without a webview:

```
NetworkTestTarget → SocketProvider → Observation Engine → expected NetworkConnection
NetworkTestTarget (HTTP) → TrafficProvider → Engine correlation → expected Flow, redacted
```

This is the layer that unit tests per provider cannot substitute for — a provider can be individually correct while the Engine still mis-correlates its output.

**Three of these are mandatory, not optional, because they directly protect the architecture's least obvious assumptions:**

1. **Process termination.** Start `NetworkTestTarget`, confirm its connections are observed, terminate it, and assert the Engine marks it `exited` rather than continuing to show its last-known connections as live.
2. **Polling gap → `expired`, not `closed`.** Simulate a connection disappearing between two snapshots with no positive evidence of closure, and assert the Engine produces `lifecycle_state = expired` — never `closed` — per the rule in `DATA_MODEL.md`. This is the test that would catch a naive `if not in snapshot: closed` implementation.
3. **Correlation ambiguity.** Feed the Engine `CorrelationEvidence` that doesn't confidently match any known `NetworkConnection` (or matches more than one equally well), and assert the result is `unmatched` — never a guessed `connection_id`, and never a silently dropped flow.
4. **Provider failure must not manufacture state.** Simulate `SocketProvider` returning `transient_failure` for one polling cycle while connections from the previous successful snapshot are still tracked, and assert none of those connections are marked `expired` or `closed` as a result — the correct outcome is the connections' status reflecting the provider failure (`transient_failure`/`stale`), unchanged lifecycle otherwise. Without this test, a single failed poll could be misread by the diffing logic as "every connection just disappeared," which is a distinct and nastier bug than the polling-gap case above: that one is about one connection legitimately vanishing, this one is about the provider itself failing to report anything at all.

## Frontend tests

React Testing Library for component-level behavior: a connection row renders the right status badge for each `OBSERVATION_CONTRACT.md` status, a redacted field never renders its raw value even if the mock `invoke` response includes one by mistake, filter inputs produce the expected query params.

## End-to-end regression pass (before any 1.0 tag)

A deliberate, scoped walkthrough — not a demand for CI infrastructure at this project's size — covering the whole chain: process discovery → socket observation → lifecycle tracking → DNS correlation → HTTP observation → redaction → session storage → session reopening. The point is confirming the phases still compose correctly together, not just that each one's own demo checkpoint passed in isolation.
