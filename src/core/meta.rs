// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Core traits for module declaration and dependency management.

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;

/// Metadata trait for module registration.
pub trait ModuleMeta: 'static {
    /// The diagnostic name of this module.
    const NAME: &'static str;

    /// Returns (name, `TypeId`) pairs for modules this module depends on.
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)];
}

/// Builder trait for module construction.
///
/// Implemented by the user for each module.
pub trait AutoBuilder: ModuleMeta {
    /// The capability type this module provides. Must be Clone.
    type Capability: Clone + 'static;

    /// The error type returned on build failure.
    type Error: std::error::Error + 'static;

    /// Build the module's capability using the provided Kit.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if the module fails to build.
    fn build(kit: &crate::kit::Kit) -> Result<Self::Capability, Self::Error>;
}

/// Async builder trait for module construction in async context.
///
/// Async counterpart of [`AutoBuilder`]. Implement this for modules requiring
/// async initialization (database pools, HTTP clients, cache backends).
///
/// The `build` method returns a `Pin<Box<dyn Future + Send>>` rather than using
/// native `async fn` in trait so that the trait can be type-erased through the
/// `AsyncBuildFn` stored in `AsyncKit`'s dependency graph (Phase 1b). Rust
/// 1.91 supports `async fn` in trait (stable since 1.75), but `dyn`-compatible
/// dispatch still requires the explicit `Pin<Box>` indirection.
///
/// Compared to [`AutoBuilder`], the associated types tighten bounds:
/// - `Capability: Clone + Send + Sync + 'static` (cross-thread sharing).
/// - `Error: std::error::Error + Send + 'static` (cross-thread error propagation).
///
/// Requires the `async` feature on the crate.
#[cfg(feature = "async")]
pub trait AsyncAutoBuilder: ModuleMeta {
    /// The capability type this module provides. Must be `Clone + Send + Sync`.
    type Capability: Clone + Send + Sync + 'static;

    /// The error type returned on build failure. Must be `Send + 'static`.
    type Error: std::error::Error + Send + 'static;

    /// Build the module's capability using the provided `AsyncKit`.
    ///
    /// The returned future borrows the kit for lifetime `'a`, allowing
    /// the build callback to read configs / require dependencies from the kit
    /// during async construction.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if the module fails to build.
    #[allow(
        clippy::type_complexity,
        reason = "Pin<Box<dyn Future + Send>> is the canonical dyn-compatible async trait dispatch type"
    )]
    fn build<'a>(
        kit: &'a crate::kit::AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>;
}

/// Type-erased build function stored in the dependency graph.
///
/// Takes `&Kit<Unbuilt>` (same memory layout as `&Kit<Ready>`)
/// because during the build phase we only have the unbuilt Kit.
pub(crate) type BuildFn =
    Box<dyn FnOnce(&crate::kit::Kit) -> Result<Box<dyn std::any::Any>, Box<dyn std::error::Error>>>;

#[cfg(all(test, feature = "async"))]
mod async_tests {
    use super::*;
    use crate::kit::async_kit::AsyncKit;
    use crate::kit::async_typemap::AsyncTypeMap;
    use crate::kit::graph::DependencyGraph;
    use std::future::Future;
    use std::marker::PhantomData;
    use std::pin::Pin;
    use std::sync::{Arc, RwLock};
    use std::task::{self, Poll, Wake};

    /// Minimal single-threaded `Future` executor for tests (no extra deps).
    ///
    /// Safe: relies on `Arc<W: Wake>` → `Waker` conversion (stable since 1.51).
    /// Used because the `async` feature deliberately stays dep-free.
    fn block_on<F: Future>(future: F) -> F::Output {
        let waker: task::Waker = Arc::new(NoopWaker).into();
        let mut cx = task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct NoopWaker;
    impl Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
    }

    /// Build an `AsyncKit<Unbuilt>` directly from struct fields — Phase 1a stub
    /// has no `new()` constructor yet.
    fn make_stub_kit() -> AsyncKit {
        use std::any::TypeId;
        use std::collections::HashMap;
        AsyncKit {
            builders: Arc::new(RwLock::new(HashMap::<TypeId, ()>::new())),
            graph: DependencyGraph::new(),
            configs: AsyncTypeMap::new(),
            capabilities: AsyncTypeMap::new(),
            _state: PhantomData,
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct LoggerCapability {
        name: String,
    }

    #[derive(Debug, thiserror::Error)]
    #[allow(dead_code, reason = "mock error type verifies trait signature only")]
    enum MockError {
        #[error("mock build failed: {0}")]
        Failed(String),
    }

    struct MockLoggerModule;

    impl ModuleMeta for MockLoggerModule {
        const NAME: &'static str = "mock-logger";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockLoggerModule {
        type Capability = Arc<LoggerCapability>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move {
                Ok(Arc::new(LoggerCapability {
                    name: "mock".to_string(),
                }))
            })
        }
    }

    #[test]
    fn async_auto_builder_returns_pin_box_future() {
        let kit = make_stub_kit();
        let fut = MockLoggerModule::build(&kit);
        let result = block_on(fut);
        assert!(result.is_ok());
        let cap = result.expect("build future returned Ok");
        assert_eq!(cap.name, "mock");
    }

    #[test]
    fn async_auto_builder_capability_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LoggerCapability>();
        assert_send_sync::<Arc<LoggerCapability>>();
    }

    #[test]
    fn async_auto_builder_error_is_send_static() {
        fn assert_send_static<T: Send + 'static>() {}
        assert_send_static::<MockError>();
    }
}
