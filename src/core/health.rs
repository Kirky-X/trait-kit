// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Health check traits for module runtime status reporting.

/// Runtime health status of a module.
#[cfg(feature = "health")]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HealthStatus {
    /// Module is operating normally.
    #[default]
    Healthy,
    /// Module is functional but degraded (e.g. high latency, partial failure).
    Degraded {
        /// Human-readable detail about the degradation.
        detail: String,
    },
    /// Module is non-functional.
    Unhealthy {
        /// Human-readable detail about the failure.
        detail: String,
    },
}

#[cfg(feature = "health")]
impl HealthStatus {
    /// Returns `true` if the status is `Healthy`.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    /// Create a `Degraded` status with the given detail message.
    #[must_use]
    pub fn degraded<D: Into<String>>(detail: D) -> Self {
        Self::Degraded {
            detail: detail.into(),
        }
    }

    /// Create an `Unhealthy` status with the given detail message.
    #[must_use]
    pub fn unhealthy<D: Into<String>>(detail: D) -> Self {
        Self::Unhealthy {
            detail: detail.into(),
        }
    }

    /// Flat, lowercase status name (`"healthy"` / `"degraded"` /
    /// `"unhealthy"`) for JSON and Prometheus-style exports.
    #[must_use]
    pub fn as_status_name(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded { .. } => "degraded",
            Self::Unhealthy { .. } => "unhealthy",
        }
    }

    /// The human-readable detail for `Degraded` / `Unhealthy` states.
    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Healthy => None,
            Self::Degraded { detail } | Self::Unhealthy { detail } => Some(detail.as_str()),
        }
    }

    /// Severity rank (0 = healthy, 1 = degraded, 2 = unhealthy) used for
    /// worst-of aggregation.
    #[must_use]
    pub fn severity_rank(&self) -> u8 {
        match self {
            Self::Healthy => 0,
            Self::Degraded { .. } => 1,
            Self::Unhealthy { .. } => 2,
        }
    }
}

/// Per-module health entry of a [`HealthAggregate`] JSON export.
#[cfg(all(feature = "health", feature = "report"))]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthModuleEntry {
    /// Module name.
    pub module: &'static str,
    /// Flat status name (`healthy` / `degraded` / `unhealthy`).
    pub status: &'static str,
    /// Detail message for degraded / unhealthy states, if any.
    pub detail: Option<String>,
}

/// Aggregated health of the whole Kit, ready for a `/healthz` endpoint.
///
/// The overall status is the worst-of across all registered health checkers
/// (`unhealthy` > `degraded` > `healthy`); an empty checker set is healthy by
/// convention.
#[cfg(all(feature = "health", feature = "report"))]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthAggregate {
    /// Worst-of overall status name.
    pub status: &'static str,
    /// Convenience flag: `status == "healthy"`.
    pub healthy: bool,
    /// Per-module statuses.
    pub modules: Vec<HealthModuleEntry>,
}

#[cfg(all(feature = "health", feature = "report"))]
impl HealthAggregate {
    /// Serialize to a JSON string (the `/healthz` payload).
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|e| format!("{{\"error\":\"serialize failed: {e}\"}}"))
    }
}

/// One ring-buffer sample of a module's health.
///
/// Produced by `Kit<Ready>::record_health_history()`; queried via
/// `health_history()`. Requires the `health` feature.
#[cfg(feature = "health")]
#[derive(Debug, Clone)]
pub struct HealthSample {
    /// When the sample was taken (monotonic clock).
    pub sampled_at: std::time::Instant,
    /// Module name.
    pub module: &'static str,
    /// Sampled status.
    pub status: HealthStatus,
}

/// Synchronous health check for a module.
///
/// Implement this trait on a module type to enable health reporting.
/// The `check` method receives a reference to the module's built capability
/// and returns a [`HealthStatus`].
///
/// Requires the `health` feature.
#[cfg(feature = "health")]
pub trait HealthCheck: crate::core::AutoBuilder {
    /// Check the health of the module given its built capability.
    ///
    /// Returns `Healthy`, `Degraded`, or `Unhealthy` depending on the
    /// module's runtime state.
    fn check(cap: &Self::Capability) -> HealthStatus;
}

/// Async health check for a module in async context.
///
/// The `check` method is **intentionally synchronous** — it has exactly the
/// same signature as the sync [`HealthCheck::check`] and is not an async
/// operation. This trait exists to give modules built in the
/// [`AsyncAutoBuilder`](crate::core::AsyncAutoBuilder) context (i.e. modules
/// whose `build` is async) a health-check attachment point for
/// `AsyncKit::register_health_check()` / `AsyncKit::health_check()`; it does
/// **not** provide asynchronous checking.
///
/// Requires both `health` and `async` features.
#[cfg(all(feature = "health", feature = "async"))]
pub trait AsyncHealthCheck: crate::core::AsyncAutoBuilder {
    /// Check the health of the async module given its built capability.
    ///
    /// Intentionally synchronous: see the trait docs for why this is not an
    /// async method.
    fn check(cap: &Self::Capability) -> HealthStatus;
}

#[cfg(all(test, feature = "health"))]
mod tests {
    use super::*;
    use crate::core::{AutoBuilder, ModuleMeta};
    use crate::kit::Kit;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct TestCap {
        value: i32,
    }

    #[derive(Debug)]
    struct TestError;

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "test error")
        }
    }

    impl std::error::Error for TestError {}

    struct TestModule;

    impl ModuleMeta for TestModule {
        const NAME: &'static str = "test-health";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AutoBuilder for TestModule {
        type Capability = Arc<TestCap>;
        type Error = TestError;

        fn build(_kit: &Kit) -> Result<Arc<TestCap>, TestError> {
            Ok(Arc::new(TestCap { value: 42 }))
        }
    }

    impl HealthCheck for TestModule {
        fn check(cap: &Arc<TestCap>) -> HealthStatus {
            if cap.value > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "value is zero".to_string(),
                }
            }
        }
    }

    /// Second module whose build produces a zero value, so the end-to-end
    /// `build()` → `health_check()` path can be exercised for `Unhealthy`.
    struct ZeroTestModule;

    impl ModuleMeta for ZeroTestModule {
        const NAME: &'static str = "zero-health";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AutoBuilder for ZeroTestModule {
        type Capability = Arc<TestCap>;
        type Error = TestError;

        fn build(_kit: &Kit) -> Result<Arc<TestCap>, TestError> {
            Ok(Arc::new(TestCap { value: 0 }))
        }
    }

    impl HealthCheck for ZeroTestModule {
        fn check(cap: &Arc<TestCap>) -> HealthStatus {
            if cap.value > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "value is zero".to_string(),
                }
            }
        }
    }

    #[test]
    fn health_status_is_healthy() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(
            !HealthStatus::Degraded {
                detail: String::new()
            }
            .is_healthy()
        );
        assert!(
            !HealthStatus::Unhealthy {
                detail: String::new()
            }
            .is_healthy()
        );
    }

    #[test]
    fn health_check_returns_healthy_for_positive_value() {
        let cap = Arc::new(TestCap { value: 42 });
        let status = TestModule::check(&cap);
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn health_check_returns_unhealthy_for_zero_value() {
        let cap = Arc::new(TestCap { value: 0 });
        let status = TestModule::check(&cap);
        assert!(matches!(status, HealthStatus::Unhealthy { .. }));
    }

    #[test]
    fn health_status_clone_and_eq() {
        let s = HealthStatus::Degraded {
            detail: "slow".to_string(),
        };
        let s2 = s.clone();
        assert_eq!(s, s2);
    }

    #[test]
    fn health_status_debug_format() {
        let s = HealthStatus::Healthy;
        let debug = format!("{s:?}");
        assert!(debug.contains("Healthy"));

        let s2 = HealthStatus::Unhealthy {
            detail: "down".to_string(),
        };
        let debug2 = format!("{s2:?}");
        assert!(debug2.contains("Unhealthy"));
        assert!(debug2.contains("down"));
    }

    #[test]
    fn health_status_ne_eq() {
        let healthy = HealthStatus::Healthy;
        let degraded = HealthStatus::Degraded {
            detail: "slow".to_string(),
        };
        let unhealthy = HealthStatus::Unhealthy {
            detail: "down".to_string(),
        };
        assert_ne!(healthy, degraded);
        assert_ne!(healthy, unhealthy);
        assert_ne!(degraded, unhealthy);
    }

    #[test]
    fn health_test_module_build_and_check() {
        let mut kit = Kit::new();
        kit.register::<TestModule>().unwrap();
        kit.register_health_check::<TestModule>();
        let built = kit.build().unwrap();
        let status = built.health_check::<TestModule>().unwrap();
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn health_zero_test_module_build_and_check_unhealthy() {
        // End-to-end unhealthy path: build produces value: 0, and the kit's
        // health_check() must report Unhealthy for it.
        let mut kit = Kit::new();
        kit.register::<ZeroTestModule>().unwrap();
        kit.register_health_check::<ZeroTestModule>();
        let built = kit.build().unwrap();
        let status = built.health_check::<ZeroTestModule>().unwrap();
        assert!(
            matches!(status, HealthStatus::Unhealthy { ref detail } if detail == "value is zero"),
            "build→health_check end-to-end should report Unhealthy, got {status:?}"
        );
    }

    #[test]
    fn health_test_error_display() {
        let e = TestError;
        assert_eq!(format!("{e}"), "test error");
    }
}

#[cfg(all(test, feature = "health", feature = "async"))]
mod async_tests {
    use super::*;
    use crate::core::{AsyncAutoBuilder, ModuleMeta};
    use crate::kit::AsyncKit;
    use crate::test_helpers::block_on;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct AsyncHealthCap {
        value: i32,
    }

    #[derive(Debug)]
    struct AsyncHealthError;

    impl std::fmt::Display for AsyncHealthError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "async health error")
        }
    }

    impl std::error::Error for AsyncHealthError {}

    struct AsyncHealthModule;

    impl ModuleMeta for AsyncHealthModule {
        const NAME: &'static str = "async-health";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for AsyncHealthModule {
        type Capability = Arc<AsyncHealthCap>;
        type Error = AsyncHealthError;

        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AsyncHealthCap>, AsyncHealthError>> + Send + 'a>>
        {
            Box::pin(async move { Ok(Arc::new(AsyncHealthCap { value: 42 })) })
        }
    }

    impl AsyncHealthCheck for AsyncHealthModule {
        fn check(cap: &Arc<AsyncHealthCap>) -> HealthStatus {
            if cap.value > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "value is zero".to_string(),
                }
            }
        }
    }

    #[test]
    fn async_health_check_returns_healthy() {
        let cap = Arc::new(AsyncHealthCap { value: 42 });
        let status = AsyncHealthModule::check(&cap);
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn async_health_check_returns_unhealthy() {
        let cap = Arc::new(AsyncHealthCap { value: 0 });
        let status = AsyncHealthModule::check(&cap);
        assert!(matches!(status, HealthStatus::Unhealthy { .. }));
    }

    #[test]
    fn async_health_test_module_build_and_check() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncHealthModule>().unwrap();
        kit.register_health_check::<AsyncHealthModule>();
        let built = block_on(kit.build()).unwrap();
        let status = built.health_check::<AsyncHealthModule>().unwrap();
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn async_health_test_error_display() {
        let e = AsyncHealthError;
        assert_eq!(format!("{e}"), "async health error");
    }
}

#[cfg(all(test, feature = "health", feature = "report"))]
mod aggregate_tests {
    use super::*;
    use crate::core::{AutoBuilder, ModuleMeta};
    use crate::kit::Kit;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct AggCap;

    #[derive(Debug)]
    struct AggError;

    impl std::fmt::Display for AggError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "agg error")
        }
    }
    impl std::error::Error for AggError {}

    struct AggHealthy;
    impl ModuleMeta for AggHealthy {
        const NAME: &'static str = "agg-healthy";
    }
    impl AutoBuilder for AggHealthy {
        type Capability = Arc<AggCap>;
        type Error = AggError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AggCap))
        }
    }
    impl HealthCheck for AggHealthy {
        fn check(_cap: &Self::Capability) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    struct AggDegraded;
    impl ModuleMeta for AggDegraded {
        const NAME: &'static str = "agg-degraded";
    }
    impl AutoBuilder for AggDegraded {
        type Capability = Arc<AggCap>;
        type Error = AggError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AggCap))
        }
    }
    impl HealthCheck for AggDegraded {
        fn check(_cap: &Self::Capability) -> HealthStatus {
            HealthStatus::degraded("slow queries")
        }
    }

    #[test]
    fn status_name_and_detail_are_flat() {
        assert_eq!(HealthStatus::Healthy.as_status_name(), "healthy");
        assert_eq!(HealthStatus::Healthy.detail(), None);
        let deg = HealthStatus::degraded("partial");
        assert_eq!(deg.as_status_name(), "degraded");
        assert_eq!(deg.detail(), Some("partial"));
        assert!(HealthStatus::unhealthy("down").severity_rank() > deg.severity_rank());
    }

    #[test]
    fn health_aggregate_worst_of_and_json() {
        let mut kit = Kit::new();
        kit.register::<AggHealthy>().unwrap();
        kit.register::<AggDegraded>().unwrap();
        kit.register_health_check::<AggHealthy>();
        kit.register_health_check::<AggDegraded>();
        let ready = kit.build().unwrap();

        let agg = ready.health_aggregate();
        assert_eq!(agg.status, "degraded", "worst-of across modules");
        assert!(!agg.healthy);
        let degraded = agg
            .modules
            .iter()
            .find(|m| m.module == "agg-degraded")
            .expect("entry");
        assert_eq!(degraded.status, "degraded");
        assert_eq!(degraded.detail.as_deref(), Some("slow queries"));

        let json = ready.health_json();
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(value["status"], "degraded");
        assert_eq!(value["healthy"], false);
        assert_eq!(value["modules"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn health_aggregate_empty_is_healthy() {
        let kit = Kit::new().build().expect("build ok");
        let agg = kit.health_aggregate();
        assert_eq!(agg.status, "healthy");
        assert!(agg.healthy);
        assert!(agg.modules.is_empty());
        assert!(kit.health_json().contains("\"healthy\""));
    }
}

#[cfg(all(test, feature = "health"))]
mod history_tests {
    use super::*;
    use crate::core::{AutoBuilder, ModuleMeta};
    use crate::kit::Kit;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct HistCap;

    #[derive(Debug)]
    struct HistError;
    impl std::fmt::Display for HistError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "hist error")
        }
    }
    impl std::error::Error for HistError {}

    struct HistModule;
    impl ModuleMeta for HistModule {
        const NAME: &'static str = "hist-module";
    }
    impl AutoBuilder for HistModule {
        type Capability = Arc<HistCap>;
        type Error = HistError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(HistCap))
        }
    }
    impl HealthCheck for HistModule {
        fn check(_cap: &Self::Capability) -> HealthStatus {
            HealthStatus::degraded("sampled")
        }
    }

    #[test]
    fn history_ring_respects_capacity_and_order() {
        let mut kit = Kit::new();
        kit.register::<HistModule>().expect("register");
        kit.register_health_check::<HistModule>();
        let ready = kit.build().expect("build ok");

        ready.set_health_history_capacity(3);
        for _ in 0..5 {
            ready.record_health_history();
        }

        let history = ready.health_history();
        assert_eq!(history.len(), 3, "ring retains only the newest samples");
        assert!(
            history.windows(2).all(|w| w[0].sampled_at <= w[1].sampled_at),
            "history is oldest-first"
        );
        assert_eq!(history[0].module, "hist-module");
        assert_eq!(history[0].status, HealthStatus::degraded("sampled"));
    }

    #[test]
    fn empty_history_before_sampling() {
        let kit = Kit::new().build().expect("build ok");
        assert!(kit.health_history().is_empty());
        kit.record_health_history();
        assert!(kit.health_history().is_empty(), "no checkers → no samples");
    }
}
