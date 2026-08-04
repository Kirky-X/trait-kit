// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Health check traits for module runtime status reporting.

/// Runtime health status of a module.
#[cfg(feature = "health")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// Module is operating normally.
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
/// Requires both `health` and `async` features.
#[cfg(all(feature = "health", feature = "async"))]
pub trait AsyncHealthCheck: crate::core::AsyncAutoBuilder {
    /// Check the health of the async module given its built capability.
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

    #[test]
    fn health_status_is_healthy() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(!HealthStatus::Degraded {
            detail: String::new()
        }
        .is_healthy());
        assert!(!HealthStatus::Unhealthy {
            detail: String::new()
        }
        .is_healthy());
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
}
