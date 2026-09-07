//! `TrafficProvider` — bare trait stub. Per `docs/[9] TODO.md` Phase 0.1:
//! don't design the final shape now, the Phase 0.3 mitmproxy spike will
//! reveal real constraints a premature interface would likely get wrong.
//! See `docs/DATA_MODEL.md`'s `RawHTTPRequest`/`RawHTTPResponse`/
//! `CorrelationEvidence` for the eventual output shape this trait will
//! produce.

#[allow(dead_code)]
pub trait TrafficProvider: Send + Sync {}
