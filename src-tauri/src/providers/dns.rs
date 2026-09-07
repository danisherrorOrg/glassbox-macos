//! `DNSProvider` — bare trait stub. Per `docs/[9] TODO.md` Phase 0.1: don't
//! design the final shape now, Phase 0.2's DNS/hostname correlation work
//! will reveal real constraints a premature interface would likely get
//! wrong. See `docs/DATA_MODEL.md`'s `HostnameObservation` for the eventual
//! output shape this trait will produce.

#[allow(dead_code)]
pub trait DNSProvider: Send + Sync {}
