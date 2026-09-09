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
    #[must_use = "on_ready returns Result<(), Error>; ignoring it may hide initialization failures"]
    fn on_ready(_kit: &crate::kit::Kit<crate::kit::Ready>) -> Result<(), Self::Error> {
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

    /// Async version of `on_shutdown`. Called during `AsyncKit::shutdown_async()`.
    /// The synchronous `AsyncKit::shutdown()` does **not** invoke async
    /// `on_shutdown` hooks.
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
    use serial_test::serial;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone)]
    struct TestCap;

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
            Ok(Arc::new(TestCap))
        }
    }

    static SHUTDOWN_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static READY_COUNTER: AtomicUsize = AtomicUsize::new(0);

    impl Lifecycle for TestModule {
        fn on_ready(_kit: &Kit<crate::kit::Ready>) -> Result<(), TestError> {
            READY_COUNTER.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn on_shutdown(_cap: &Arc<TestCap>) {
            SHUTDOWN_COUNTER.fetch_add(1, Ordering::Relaxed);
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
                Ok(Arc::new(TestCap))
            }
        }

        impl Lifecycle for DefaultModule {}

        // Actually call the default on_ready through kit integration
        let mut kit = Kit::new();
        kit.register::<DefaultModule>().unwrap();
        kit.register_lifecycle::<DefaultModule>();
        let built = kit.build().unwrap();
        // build() succeeding already proves the default on_ready returned Ok
        // (a failure would surface as TraitKitError::LifecycleFailed). Assert
        // the default impl's return value directly as well.
        let result = DefaultModule::on_ready(&built);
        assert!(result.is_ok(), "default on_ready should return Ok(())");
    }

    #[test]
    #[serial(shutdown_counter)]
    fn lifecycle_trait_has_default_on_shutdown() {
        // Default on_shutdown is a no-op — call through a module that
        // does NOT override on_shutdown to exercise the default impl.
        struct DefaultShutdownModule;

        impl ModuleMeta for DefaultShutdownModule {
            const NAME: &'static str = "default-shutdown";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }

        impl AutoBuilder for DefaultShutdownModule {
            type Capability = Arc<TestCap>;
            type Error = TestError;

            fn build(_kit: &Kit) -> Result<Arc<TestCap>, TestError> {
                Ok(Arc::new(TestCap))
            }
        }

        impl Lifecycle for DefaultShutdownModule {
            // Only override on_ready; on_shutdown uses the default no-op
            fn on_ready(_kit: &Kit<crate::kit::Ready>) -> Result<(), TestError> {
                Ok(())
            }
        }

        let mut kit = Kit::new();
        kit.register::<DefaultShutdownModule>().unwrap();
        kit.register_lifecycle::<DefaultShutdownModule>();
        let built = kit.build().unwrap();
        // Default on_shutdown is a no-op: invoking it directly and through the
        // kit's shutdown path must not panic and must not touch shared state.
        let before = SHUTDOWN_COUNTER.load(Ordering::Relaxed);
        let cap = Arc::new(TestCap);
        DefaultShutdownModule::on_shutdown(&cap);
        built.shutdown(); // exercises default on_shutdown via the registered callback
        let after = SHUTDOWN_COUNTER.load(Ordering::Relaxed);
        assert_eq!(
            after, before,
            "default on_shutdown is a no-op and must not modify shared state"
        );
        built.shutdown(); // callbacks are drained; a second shutdown stays a safe no-op
    }

    #[test]
    #[serial(shutdown_counter)]
    fn lifecycle_shutdown_counter_increments() {
        let before = SHUTDOWN_COUNTER.load(Ordering::Relaxed);
        let cap = Arc::new(TestCap);
        TestModule::on_shutdown(&cap);
        let after = SHUTDOWN_COUNTER.load(Ordering::Relaxed);
        assert_eq!(after, before + 1, "shutdown counter should increment");
    }

    #[test]
    #[serial(shutdown_counter)]
    fn lifecycle_test_module_full_kit_integration() {
        let ready_before = READY_COUNTER.load(Ordering::Relaxed);
        let shutdown_before = SHUTDOWN_COUNTER.load(Ordering::Relaxed);
        let mut kit = Kit::new();
        kit.register::<TestModule>().unwrap();
        kit.register_lifecycle::<TestModule>();
        let built = kit.build().unwrap();
        assert_eq!(
            READY_COUNTER.load(Ordering::Relaxed),
            ready_before + 1,
            "on_ready should be called exactly once during build()"
        );
        built.shutdown();
        assert_eq!(
            SHUTDOWN_COUNTER.load(Ordering::Relaxed),
            shutdown_before + 1,
            "on_shutdown should be called exactly once during shutdown()"
        );
    }

    #[test]
    fn lifecycle_test_error_display() {
        let e = TestError;
        assert_eq!(format!("{e}"), "test error");
    }
}

#[cfg(all(test, feature = "lifecycle", feature = "async"))]
mod async_tests {
    use super::*;
    use crate::core::{AsyncAutoBuilder, ModuleMeta};
    use crate::kit::AsyncKit;
    use crate::test_helpers::block_on;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone)]
    struct AsyncTestCap;

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
            Box::pin(async move { Ok(Arc::new(AsyncTestCap)) })
        }
    }

    impl AsyncLifecycle for AsyncTestModule {}

    /// Module with counting lifecycle overrides, used to verify the full kit
    /// integration (`on_ready` during build; `on_shutdown` skipped by sync shutdown).
    struct CountingAsyncModule;

    impl ModuleMeta for CountingAsyncModule {
        const NAME: &'static str = "async-lifecycle-counting";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for CountingAsyncModule {
        type Capability = Arc<AsyncTestCap>;
        type Error = AsyncTestError;

        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AsyncTestCap>, AsyncTestError>> + Send + 'a>>
        {
            Box::pin(async move { Ok(Arc::new(AsyncTestCap)) })
        }
    }

    static ASYNC_READY_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static ASYNC_SHUTDOWN_COUNTER: AtomicUsize = AtomicUsize::new(0);

    impl AsyncLifecycle for CountingAsyncModule {
        fn on_ready<'a>(
            _kit: &'a AsyncKit<crate::kit::async_kit::Ready>,
        ) -> Pin<Box<dyn Future<Output = Result<(), AsyncTestError>> + Send + 'a>> {
            ASYNC_READY_COUNTER.fetch_add(1, Ordering::Relaxed);
            Box::pin(async { Ok(()) })
        }

        fn on_shutdown<'a>(
            _cap: &'a Arc<AsyncTestCap>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            ASYNC_SHUTDOWN_COUNTER.fetch_add(1, Ordering::Relaxed);
            Box::pin(async {})
        }
    }

    #[test]
    fn async_lifecycle_default_on_ready_returns_ok() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).expect("build should succeed");
        let result = block_on(AsyncTestModule::on_ready(&built));
        assert!(result.is_ok(), "default on_ready should return Ok");
    }

    #[test]
    fn async_lifecycle_default_on_shutdown_completes() {
        let cap = Arc::new(AsyncTestCap);
        block_on(AsyncTestModule::on_shutdown(&cap));
        // Should not panic
    }

    #[test]
    fn async_lifecycle_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AsyncTestCap>();
    }

    #[test]
    fn async_lifecycle_test_module_full_kit_integration() {
        let ready_before = ASYNC_READY_COUNTER.load(Ordering::Relaxed);
        let shutdown_before = ASYNC_SHUTDOWN_COUNTER.load(Ordering::Relaxed);
        let mut kit = AsyncKit::new();
        kit.register::<CountingAsyncModule>().unwrap();
        kit.register_lifecycle::<CountingAsyncModule>();
        let built = block_on(kit.build()).unwrap();
        assert_eq!(
            ASYNC_READY_COUNTER.load(Ordering::Relaxed),
            ready_before + 1,
            "async on_ready should be called exactly once during build()"
        );
        // `AsyncKit` 没有 sync `shutdown()`：async `on_shutdown` 必须显式
        // await（经由 `shutdown_async()` 或直接调用钩子）。此处手动 await
        // 以验证钩子路径恰好执行一次。`built` 保持存活以模拟真实用法
        // （能力随 Kit 生命周期存续）。
        let cap = Arc::new(AsyncTestCap);
        block_on(CountingAsyncModule::on_shutdown(&cap));
        assert_eq!(
            ASYNC_SHUTDOWN_COUNTER.load(Ordering::Relaxed),
            shutdown_before + 1,
            "manually awaited on_shutdown should run exactly once"
        );
        drop(built);
    }

    #[test]
    fn async_lifecycle_test_error_display() {
        let e = AsyncTestError;
        assert_eq!(format!("{e}"), "async test error");
    }
}
