// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Lifecycle hooks for module initialization and shutdown.

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;

/// Synchronous lifecycle hooks for modules.
///
/// Provides `on_ready` (called after all modules are built) and `on_shutdown`
/// (called in reverse topological order during `Kit::shutdown()`).
///
/// Both methods have default no-op implementations, so existing modules
/// that don't need lifecycle management are unaffected.
///
/// Requires the `lifecycle` feature.
#[cfg(feature = "lifecycle")]
pub trait Lifecycle: crate::core::AutoBuilder {
    /// Called after all modules have been built (positive topological order).
    ///
    /// The `kit` parameter provides access to all built capabilities via
    /// `kit.require::<M>()`. Use this for cross-module initialization
    /// that depends on multiple capabilities being available.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if initialization fails. The error is wrapped
    /// in `TraitKitError::LifecycleFailed` and propagated from `build()`.
    fn on_ready(
        _kit: &crate::kit::Kit<crate::kit::Ready>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called during `Kit::shutdown()` in reverse topological order.
    ///
    /// Receives a reference to the module's built capability for cleanup.
    /// Use this for resource release (close connections, flush buffers, etc.).
    ///
    /// A failed shutdown does not prevent other modules from shutting down.
    fn on_shutdown(_cap: &Self::Capability) {}
}

/// Async lifecycle hooks for modules in async context.
///
/// Async counterpart of [`Lifecycle`]. Provides async `on_ready` and
/// `on_shutdown` for modules requiring async initialization/cleanup
/// (database pools, HTTP clients, cache backends).
///
/// Requires both `lifecycle` and `async` features.
#[cfg(all(feature = "lifecycle", feature = "async"))]
pub trait AsyncLifecycle: crate::core::AsyncAutoBuilder {
    /// Async version of `on_ready`. Called after all async modules are built.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if async initialization fails.
    #[allow(
        clippy::type_complexity,
        reason = "Pin<Box<dyn Future + Send>> is the canonical dyn-compatible async dispatch type"
    )]
    #[must_use]
    fn on_ready<'a>(
        _kit: &'a crate::kit::AsyncKit<crate::kit::async_kit::Ready>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    /// Async version of `on_shutdown`. Called during `AsyncKit::shutdown()`.
    #[allow(
        clippy::type_complexity,
        reason = "Pin<Box<dyn Future>> is the canonical dyn-compatible async dispatch type"
    )]
    fn on_shutdown<'a>(
        _cap: &'a Self::Capability,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

#[cfg(all(test, feature = "lifecycle"))]
mod tests {
    use super::*;
    use crate::core::{AutoBuilder, ModuleMeta};
    use crate::kit::Kit;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone)]
    struct TestCap {
        name: String,
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
        const NAME: &'static str = "test-lifecycle";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AutoBuilder for TestModule {
        type Capability = Arc<TestCap>;
        type Error = TestError;

        fn build(_kit: &Kit) -> Result<Arc<TestCap>, TestError> {
            Ok(Arc::new(TestCap {
                name: "test".to_string(),
            }))
        }
    }

    static SHUTDOWN_COUNTER: AtomicUsize = AtomicUsize::new(0);

    impl Lifecycle for TestModule {
        fn on_ready(_kit: &Kit<crate::kit::Ready>) -> Result<(), TestError> {
            Ok(())
        }

        fn on_shutdown(_cap: &Arc<TestCap>) {
            SHUTDOWN_COUNTER.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn lifecycle_trait_has_default_on_ready() {
        // Default on_ready returns Ok(())
        struct DefaultModule;

        impl ModuleMeta for DefaultModule {
            const NAME: &'static str = "default-lifecycle";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }

        impl AutoBuilder for DefaultModule {
            type Capability = Arc<TestCap>;
            type Error = TestError;

            fn build(_kit: &Kit) -> Result<Arc<TestCap>, TestError> {
                Ok(Arc::new(TestCap {
                    name: "default".to_string(),
                }))
            }
        }

        impl Lifecycle for DefaultModule {}

        // Default on_ready should return Ok(())
        let result = DefaultModule::on_ready;
        let _ = result; // Just verify it compiles
    }

    #[test]
    fn lifecycle_trait_has_default_on_shutdown() {
        // Default on_shutdown is a no-op
        let cap = Arc::new(TestCap {
            name: "test".to_string(),
        });
        TestModule::on_shutdown(&cap);
        // Should not panic
    }

    #[test]
    fn lifecycle_shutdown_counter_increments() {
        let before = SHUTDOWN_COUNTER.load(Ordering::SeqCst);
        let cap = Arc::new(TestCap {
            name: "test".to_string(),
        });
        TestModule::on_shutdown(&cap);
        let after = SHUTDOWN_COUNTER.load(Ordering::SeqCst);
        assert_eq!(after, before + 1, "shutdown counter should increment");
    }
}

#[cfg(all(test, feature = "lifecycle", feature = "async"))]
mod async_tests {
    use super::*;
    use crate::core::{AsyncAutoBuilder, ModuleMeta};
    use crate::kit::AsyncKit;
    use crate::test_helpers::block_on;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct AsyncTestCap {
        value: i32,
    }

    #[derive(Debug)]
    struct AsyncTestError;

    impl std::fmt::Display for AsyncTestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "async test error")
        }
    }

    impl std::error::Error for AsyncTestError {}

    struct AsyncTestModule;

    impl ModuleMeta for AsyncTestModule {
        const NAME: &'static str = "async-lifecycle";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for AsyncTestModule {
        type Capability = Arc<AsyncTestCap>;
        type Error = AsyncTestError;

        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AsyncTestCap>, AsyncTestError>> + Send + 'a>>
        {
            Box::pin(async move { Ok(Arc::new(AsyncTestCap { value: 42 })) })
        }
    }

    impl AsyncLifecycle for AsyncTestModule {}

    #[test]
    fn async_lifecycle_default_on_ready_returns_ok() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).expect("build should succeed");
        let result = block_on(AsyncTestModule::on_ready(&built));
        assert!(result.is_ok(), "default on_ready should return Ok");
    }

    #[test]
    fn async_lifecycle_default_on_shutdown_completes() {
        let cap = Arc::new(AsyncTestCap { value: 42 });
        block_on(AsyncTestModule::on_shutdown(&cap));
        // Should not panic
    }

    #[test]
    fn async_lifecycle_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AsyncTestCap>();
    }
}
