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
