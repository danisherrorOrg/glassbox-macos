//! `DNSProvider` — reverse DNS lookup, per `docs/[9] TODO.md` Phase 0.2
//! ("Implement `DNSProvider` (reverse DNS lookup to start; SNI/Host-header
//! sources land in Phase 0.4 once HTTP exists)"). See `docs/DATA_MODEL.md`'s
//! `HostnameObservation`.

use std::net::IpAddr;

use chrono::Utc;
use dns_lookup::lookup_addr;

use crate::models::{HostnameObservation, HostnameSource, ProviderStatus};

pub trait DNSProvider: Send + Sync {
    /// Resolves one address. Returns the observation alongside the
    /// `ProviderStatus` of the attempt — a hostname lookup is a per-address
    /// call, not a whole-layer snapshot the way `ProcessProvider`/
    /// `SocketProvider` are (`docs/OBSERVATION_CONTRACT.md`'s DNS layer).
    fn resolve(&self, addr: &str) -> (Option<HostnameObservation>, ProviderStatus);

    fn capabilities(&self) -> crate::models::ProviderCapabilities {
        crate::models::ProviderCapabilities {
            dns: Some(crate::models::Availability::Available),
            ..Default::default()
        }
    }
}

pub struct ReverseDnsProvider;

impl DNSProvider for ReverseDnsProvider {
    fn resolve(&self, addr: &str) -> (Option<HostnameObservation>, ProviderStatus) {
        let now = Utc::now();

        let Ok(ip) = addr.parse::<IpAddr>() else {
            return (
                None,
                ProviderStatus::transient_failure(now, format!("not a valid IP address: {addr}")),
            );
        };

        match lookup_addr(&ip) {
            Ok(hostname) if hostname != addr => (
                Some(HostnameObservation {
                    queried_addr: addr.to_string(),
                    source: HostnameSource::ReverseDns,
                    confidence: HostnameSource::ReverseDns.starter_confidence(),
                    hostname,
                    observed_at: now,
                }),
                ProviderStatus::observed(now),
            ),
            // `lookup_addr` falls back to returning the address itself,
            // stringified, when there's no PTR record — that's a real,
            // observed "no hostname" result, not a failure.
            Ok(_) => (None, ProviderStatus::observed(now)),
            Err(e) => (
                None,
                ProviderStatus::transient_failure(now, e.to_string()),
            ),
        }
    }
}
