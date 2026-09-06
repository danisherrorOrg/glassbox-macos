# Reading Order — Process Network Inspector Docs

Three passes, each with a different job. Reading the same nine files in the same order three times is less useful than changing what you're looking for each time.

---

## Pass 1 — the shape of the thing (skim, don't memorize)

1. **`process-network-inspector-report.md`** — what this is and why: the product principle, the four-level visibility model, the core workflow. This is the lens everything else gets read through.
2. **`DECISIONS.md`** — skim only this time, don't dwell on the technical detail of each ADR yet. The point of reading it this early is the *story*: why Swift got dropped for FastAPI/React (ADR-009), why that got revised again to Tauri/Rust before any code was written (ADR-013), and that the architecture went through two rounds of real contradictions getting caught and fixed (ADR-011, ADR-012). That context makes Pass 2 land better — you'll recognize *why* a type is shaped the way it is instead of just memorizing that it is.
3. **`ARCHITECTURE.md`** — the system's shape: the four layers, who's allowed to depend on whom, where state lives, where redaction happens. Read for the shape, not every rule yet.

## Pass 2 — the technical core (this is the one to slow down on)

4. **`DATA_MODEL.md`** — the longest and most load-bearing doc. This is where "providers report, the Engine decides" becomes actual types (`SocketObservation` vs. `NetworkConnection`, `ProcessObservation` vs. `ProcessInfo`), where `closed`/`expired` and the false-merge-vs-false-split rule live, and where the raw-vs-redacted split for traffic is defined. Go slow here.
5. **`OBSERVATION_CONTRACT.md`** — builds directly on `DATA_MODEL.md`'s `ObservationStatus` type; read right after so the provider-status/Engine-derived-status split is still fresh.
6. **`PRIVACY_AND_SECURITY.md`** — builds on the `Raw*`/redacted types from `DATA_MODEL.md`; the two-tier redaction model will make more sense having just read what those types actually are.
7. **`PERMISSIONS_AND_PLATFORM.md`** — shorter, largely self-contained; explains why some of Phase 0.1's tasks are spikes rather than certainties.
8. **`DECISIONS.md`** again — full read this time, not a skim. Every ADR will actually click now that you've seen the current shape of `DATA_MODEL.md` and `ARCHITECTURE.md` that they explain.
9. **`TESTING_STRATEGY.md`** — read last in this pass, since its four mandatory tests are direct call-backs to specific rules in `DATA_MODEL.md` (closed/expired, correlation, provider-failure isolation) — it reads as "oh, that's testing the thing I just read," not as new information.

## Pass 3 — build readiness, right before you start coding

10. **`TODO.md`** alone, start to finish. By now every cross-reference it makes back to the other docs should be familiar rather than something you have to go look up. Read it specifically looking for anything that still feels surprising or underspecified — that's the signal something needs a quick re-check in the relevant doc before Phase 0.1, rather than being discovered mid-implementation.

If a third full pass feels like too much once you're through 1 and 2, Pass 3 alone is the minimum worth doing before writing code — it's the one that actually maps onto what you're about to do.
