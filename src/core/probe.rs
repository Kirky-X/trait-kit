// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Service probes: object-safe async liveness/readiness checks with latency
//! capture.
//!
//! Division of labor across the three health abstractions
//! (probe-vs-healthcheck):
//!
//! - [`HealthCheck`](super::health::HealthCheck) — synchronous, module-type
//!   bound, reads cached state on the reporting path.
//! - [`AsyncHealthCheck`](super::health::AsyncHealthCheck) — same signature
//!   as `HealthCheck`, intentionally synchronous; see its docs for the
//!   two-stage probe/report split.
//! - [`ServiceProbe`] — a real async network probe (the framework measures
//!   wall latency around each `probe().await`),
//!   executed only when the caller explicitly awaits
//!   `AsyncKit<Ready>::run_probes()` / `probe_aggregate()`. This is where
//!   network I/O belongs. Bounded execution entries —
//!   `run_probes_with_timeout` / `probe_aggregate_with_timeout` — give each
//!   probe a hard individual limit (a hung probe is recorded unhealthy and
//!   the pass continues), the recommended backstop for the
//!   implement-side-timeout contract.

use std::future::Future;
use std::pin::Pin;

use super::health::HealthStatus;

/// Outcome of one [`ServiceProbe`] execution: health status plus the
/// probe's self-reported internal duration.
#[cfg(feature = "probe")]
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    /// Probe verdict.
    pub status: HealthStatus,
    /// Probe-internal duration as reported by the implementation
    /// (diagnostic only). The framework does not trust this value:
    /// `run_probes()` measures wall latency around the `probe().await`
    /// itself and records **that** measurement in
    /// [`ProbeEntry::latency_ms`], so a faulty or inflated self-report
    /// never reaches the readiness payload.
    pub latency: std::time::Duration,
}

/// Object-safe async service probe.
///
/// Register instances via `AsyncKit::register_probe` (dynamically, any
/// state); execute via `AsyncKit<Ready>::run_probes()` (per-probe records)
/// or `AsyncKit<Ready>::probe_aggregate()` (worst-of verdict). The hand
/// written `Pin<Box<dyn Future>>` dispatch mirrors
/// [`AsyncLifecycle`](super::lifecycle::AsyncLifecycle) — no `async-trait`
/// dependency.
///
/// Probes run sequentially in registration order when
/// `run_probes()`/`probe_aggregate()` awaits them one at a time (no
/// concurrent join, mirroring the shutdown-hook philosophy); callers needing
/// fan-out can spawn their own tasks around [`ServiceProbe::probe`].
#[cfg(feature = "probe")]
pub trait ServiceProbe: Send + Sync {
    /// Run the probe once and report status plus latency.
    ///
    /// Network I/O is expected here (unlike
    /// [`AsyncHealthCheck::check`](super::health::AsyncHealthCheck::check),
    /// which must stay synchronous) — and the implementation **must** keep
    /// a timeout inside the call so a hung dependency cannot stall the
    /// reporting caller indefinitely. Treat this as a contract, not a
    /// suggestion: a probe that never resolves blocks a plain
    /// `run_probes()` pass forever. When implementor compliance cannot be
    /// assumed (third-party probes), enforce the bound at the framework
    /// level with `AsyncKit<Ready>::run_probes_with_timeout(timeout)` /
    /// `probe_aggregate_with_timeout(timeout)` — each probe is then
    /// individually bounded, a hung probe is recorded as unhealthy, and
    /// the pass continues with the remaining probes.
    ///
    /// The future must be cancellation-safe: the bounded variants drop it
    /// on timeout.
    #[allow(
        clippy::type_complexity,
        reason = "Pin<Box<dyn Future + Send>> is the canonical dyn-compatible async dispatch type"
    )]
    fn probe<'a>(&'a self) -> Pin<Box<dyn Future<Output = ProbeOutcome> + Send + 'a>>;
}

/// One named probe's result in a [`ProbeReport`].
#[cfg(feature = "probe")]
#[derive(Debug, Clone)]
#[cfg_attr(feature = "report", derive(serde::Serialize))]
pub struct ProbeEntry {
    /// Probe name given at registration time.
    pub name: &'static str,
    /// Flat status name (`healthy` / `degraded` / `unhealthy`).
    pub status: &'static str,
    /// Detail message for degraded / unhealthy states, if any.
    ///
    /// This string is carried verbatim into the readiness payload
    /// (`to_json()`), so probe implementations must never embed
    /// credentials, connection strings or internal topology in the detail.
    pub detail: Option<String>,
    /// Wall latency in milliseconds, measured by the framework around the
    /// `probe().await` call — not the probe's self-reported
    /// [`ProbeOutcome::latency`]. On a timed-out probe (bounded variants)
    /// this is the elapsed time at cancellation, i.e. close to the timeout.
    pub latency_ms: u64,
}

/// Report of one `AsyncKit<Ready>::run_probes()` pass, ready for a
/// `/health/ready`-style endpoint.
///
/// The overall status is the worst-of across all registered probes
/// (`unhealthy` > `degraded` > `healthy`), computed with the same severity
/// rank as the health aggregate; an empty probe registry is healthy by
/// convention (matching the health aggregate's empty-set semantics) —
/// **except** after the shutdown protocol has cleared the registry, in
/// which case the report is unhealthy with `stopped == true`: an empty
/// registry does not mean "all green", and a shut-down kit must not be
/// reported as serviceable. Empty does not imply healthy; always check
/// `stopped` (or match on `healthy`) before drawing a readiness verdict.
///
/// Probe details are carried verbatim into the JSON payload — keep
/// credentials and internal topology out of [`ProbeEntry::detail`].
#[cfg(feature = "probe")]
#[derive(Debug, Clone)]
#[cfg_attr(feature = "report", derive(serde::Serialize))]
pub struct ProbeReport {
    /// Worst-of overall status name.
    pub overall: &'static str,
    /// Convenience flag: `overall == "healthy"`.
    pub healthy: bool,
    /// Per-probe records in registration (= execution) order.
    pub probes: Vec<ProbeEntry>,
    /// `true` when this report does not come from probe execution at all:
    /// the shutdown protocol has cleared the registry
    /// (`shutdown_async` / `register_shutdown_into`), and the report is
    /// the explicit not-serviceable verdict (`overall == "unhealthy"`,
    /// `probes == []`). Re-registering probes after shutdown resets the
    /// field as soon as a pass runs them again. `false` for every
    /// executed pass, including the healthy-by-convention empty registry.
    pub stopped: bool,
}

#[cfg(all(feature = "probe", feature = "report"))]
impl ProbeReport {
    /// Serialize to a JSON string (the readiness endpoint payload).
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_json` error instead of embedding it in
    /// an otherwise-valid-looking JSON body, so a serialization failure can
    /// never be mistaken for a healthy readiness report.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

#[cfg(all(test, feature = "probe"))]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct StaticProbe {
        status: HealthStatus,
        delay_ms: u64,
    }

    impl ServiceProbe for StaticProbe {
        fn probe<'a>(&'a self) -> Pin<Box<dyn Future<Output = ProbeOutcome> + Send + 'a>> {
            Box::pin(async move {
                if self.delay_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
                }
                ProbeOutcome {
                    status: self.status.clone(),
                    latency: std::time::Duration::from_millis(self.delay_ms),
                }
            })
        }
    }

    #[test]
    fn service_probe_is_object_safe_and_dispatchable() {
        // Object safety: probes are stored as `Arc<dyn ServiceProbe>` by the
        // registry, so the trait must be dyn-compatible and dispatch through
        // the hand-written Pin<Box<...>> signature.
        let probes: Vec<Box<dyn ServiceProbe>> = vec![
            Box::new(StaticProbe {
                status: HealthStatus::Healthy,
                delay_ms: 0,
            }),
            Box::new(StaticProbe {
                status: HealthStatus::degraded("slow replica"),
                delay_ms: 0,
            }),
        ];
        let outcomes: Vec<ProbeOutcome> = probes
            .iter()
            .map(|p| crate::test_helpers::block_on(p.probe()))
            .collect();
        assert!(outcomes[0].status.is_healthy());
        assert_eq!(outcomes[1].status.as_status_name(), "degraded");
    }

    #[test]
    fn probe_outcome_carries_status_and_latency() {
        let outcome = ProbeOutcome {
            status: HealthStatus::unhealthy("connection refused"),
            latency: std::time::Duration::from_millis(42),
        };
        assert_eq!(outcome.status.severity_rank(), 2);
        assert_eq!(outcome.latency.as_millis(), 42);
    }

    #[cfg(feature = "report")]
    #[test]
    fn probe_report_json_round_trips() {
        let report = ProbeReport {
            overall: "degraded",
            healthy: false,
            probes: vec![ProbeEntry {
                name: "db",
                status: "degraded",
                detail: Some("slow replica".into()),
                latency_ms: 12,
            }],
            stopped: false,
        };
        let json = report.to_json().expect("serialize probe report");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(value["overall"], "degraded");
        assert_eq!(value["healthy"], false);
        assert_eq!(value["probes"][0]["name"], "db");
        assert_eq!(value["probes"][0]["latency_ms"], 12);
        assert_eq!(value["stopped"], false);
    }
}
