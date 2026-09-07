//! `ProviderStatus`/`ObservationStatus` — the two-kinds-of-status split from
//! `docs/OBSERVATION_CONTRACT.md`. A provider constructs `ProviderStatus`;
//! only the Engine constructs `ObservationStatus`.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// The five provider-determinable statuses. A provider never produces
/// `Stale` or `Unmatched` — those are Engine-derived (see `ObservationState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderState {
    Observed,
    Unavailable,
    PermissionDenied,
    Unsupported,
    TransientFailure,
}

/// What a provider can legitimately assert about its own call, in isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub state: ProviderState,
    pub observed_at: DateTime<Utc>,
    pub reason: Option<String>,
}

impl ProviderStatus {
    pub fn observed(now: DateTime<Utc>) -> Self {
        Self {
            state: ProviderState::Observed,
            observed_at: now,
            reason: None,
        }
    }

    pub fn transient_failure(now: DateTime<Utc>, reason: impl Into<String>) -> Self {
        Self {
            state: ProviderState::TransientFailure,
            observed_at: now,
            reason: Some(reason.into()),
        }
    }
}

/// The full seven-value vocabulary a consumer of the API ever sees. Only the
/// Engine may set `Stale`/`Unmatched` — everything else is copied up from a
/// `ProviderStatus.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationState {
    Observed,
    Unavailable,
    PermissionDenied,
    Unsupported,
    TransientFailure,
    Stale,
    Unmatched,
}

/// Which provider/layer a status originates from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Process,
    Socket,
    Dns,
    Traffic,
    Engine,
}

/// Attached to every domain object the Engine emits. Constructed by the
/// Engine by wrapping a `ProviderStatus` and optionally promoting to `Stale`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationStatus {
    pub state: ObservationState,
    pub observed_at: DateTime<Utc>,
    pub last_successful_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub provider: Option<Provider>,
}

/// Phase 0.1's flat staleness threshold (`OBSERVATION_CONTRACT.md`) — used
/// until live monitoring (Phase 0.2) has ever configured a real poll
/// interval for this session.
pub const PHASE_0_1_STALE_THRESHOLD_SECONDS: i64 = 30;

/// `OBSERVATION_CONTRACT.md`'s staleness formula once a poll interval is
/// configured: `now - last_successful_at > 3 × the configured poll interval`.
pub fn polling_stale_threshold(poll_interval_ms: u64) -> Duration {
    Duration::milliseconds(3 * poll_interval_ms as i64)
}

impl ObservationStatus {
    /// Wraps a fresh `ProviderStatus` into an `ObservationStatus`, given the
    /// object's previous status (if any) to carry forward `last_successful_at`
    /// and to decide whether a non-`Observed` result has aged into `Stale`.
    /// `stale_threshold` is `docs/OBSERVATION_CONTRACT.md`'s formula: a flat
    /// 30s in Phase 0.1, `3 × poll interval` once live monitoring has
    /// configured one (`polling_stale_threshold`).
    ///
    /// This is the one place that implements `OBSERVATION_CONTRACT.md`'s
    /// staleness rule and satisfies `TESTING_STRATEGY.md`'s 4th mandatory
    /// test: a failed poll never manufactures fresh data, and only ages a
    /// carried-forward failure into `Stale` once `last_successful_at` is
    /// older than that threshold.
    pub fn from_provider(
        provider: &ProviderStatus,
        previous: Option<&ObservationStatus>,
        source: Provider,
        stale_threshold: Duration,
    ) -> Self {
        let now = provider.observed_at;
        let previous_success_at = previous.and_then(|p| p.last_successful_at);

        if provider.state == ProviderState::Observed {
            return Self {
                state: ObservationState::Observed,
                observed_at: now,
                last_successful_at: Some(now),
                reason: None,
                provider: Some(source),
            };
        }

        let aged_past_threshold = previous_success_at
            .map(|t| now - t > stale_threshold)
            .unwrap_or(false);

        let state = if aged_past_threshold {
            ObservationState::Stale
        } else {
            match provider.state {
                ProviderState::Unavailable => ObservationState::Unavailable,
                ProviderState::PermissionDenied => ObservationState::PermissionDenied,
                ProviderState::Unsupported => ObservationState::Unsupported,
                ProviderState::TransientFailure => ObservationState::TransientFailure,
                ProviderState::Observed => unreachable!("handled above"),
            }
        };

        Self {
            state,
            observed_at: now,
            last_successful_at: previous_success_at,
            reason: provider.reason.clone(),
            provider: Some(source),
        }
    }

    /// A status with no prior data and no successful observation yet —
    /// used when a whole-layer failure means there is nothing to carry
    /// forward (e.g. `permission_denied` on a call that's never succeeded).
    pub fn denied(now: DateTime<Utc>, reason: impl Into<String>, source: Provider) -> Self {
        Self {
            state: ObservationState::PermissionDenied,
            observed_at: now,
            last_successful_at: None,
            reason: Some(reason.into()),
            provider: Some(source),
        }
    }
}
