// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! `AsyncKit` — the async capability and configuration management center.
//!
//! Phase 1b full implementation: typestate `AsyncKit<Unbuilt>` →
//! `AsyncKit<Ready>` with `Arc<RwLock>` interior mutability (multi-threaded,
//! `Send + Sync`). Mirrors the synchronous [`super::kit::Kit`] but swaps
//! `RefCell` for `RwLock` and stores async build functions returning
//! `Pin<Box<dyn Future + Send>>`.
//!
//! This module implements the `Unbuilt` surface (`new` / `register` /
//! `set_config` / `config`). The `build()` / `require` / `optional` /
//! `contains` / `contains_config` methods land in subsequent Phase 1b tasks.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use crate::core::error::KitError;
use crate::core::meta::AsyncAutoBuilder;

use super::async_typemap::AsyncTypeMap;
use super::graph::{DependencyGraph, GraphError, ModuleEntry};

/// Marker type for the unbuilt state.
pub struct Unbuilt;

/// Marker type for the ready (built) state.
pub struct Ready;

/// Type-erased async build function.
///
/// Stored in the dependency graph and called during `AsyncKit::build()` to
/// produce a boxed capability. The returned future borrows the kit for
/// lifetime `'a` (higher-rank), allowing build callbacks to read configs /
/// require dependencies from the kit during async construction without forcing
/// a `'static` capture.
///
/// The future yields `Box<dyn Any + Send + Sync>` (not just `+ Send`) because
/// `AsyncTypeMap::insert_boxed` requires `Send + Sync` storage and the
/// capability trait bound `AsyncAutoBuilder::Capability: Send + Sync + 'static`
/// guarantees both.
///
/// The error variant is `Box<dyn Error>` (without `+ Send`) to match
/// `KitError::BuildFailed::source`. The future is still `Send` because the
/// error is only constructed in the early-return path of `?` and never held
/// across an `.await` — the only await point is `M::build(kit).await`, whose
/// `M::Error: Send` bound is enforced by the trait.
#[allow(
    clippy::type_complexity,
    reason = "Pin<Box<dyn Future + Send>> is the canonical dyn-compatible async dispatch type; mirrors AsyncAutoBuilder::build"
)]
pub(crate) type AsyncBuildFn = Box<
    dyn for<'a> FnOnce(
        &'a AsyncKit,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Box<dyn Any + Send + Sync>, Box<dyn std::error::Error>>>
                + Send
                + 'a,
        >,
    > + Send + Sync,
>;

/// The async capability and configuration management center.
///
/// Multi-threaded (`Send + Sync`) counterpart to [`super::kit::Kit`]. Uses
/// `Arc<RwLock<...>>` for interior mutability (safe to share across threads,
/// poisoning-aware). Async module construction happens in `build()`.
pub struct AsyncKit<S = Unbuilt> {
    builders: Arc<RwLock<HashMap<TypeId, AsyncBuildFn>>>,
    graph: DependencyGraph,
    configs: AsyncTypeMap,
    #[allow(dead_code, reason = "Phase 1b T008+T009 adds build()/require() that read this field")]
    capabilities: AsyncTypeMap,
    _state: PhantomData<S>,
}

impl AsyncKit {
    /// Create a new empty `AsyncKit<Unbuilt>`.
    ///
    /// All containers (`builders`, `graph`, `configs`, `capabilities`) start
    /// empty; register modules and configs before calling `build()`.
    #[must_use]
    pub fn new() -> Self {
        AsyncKit {
            builders: Arc::new(RwLock::new(HashMap::new())),
            graph: DependencyGraph::new(),
            configs: AsyncTypeMap::new(),
            capabilities: AsyncTypeMap::new(),
            _state: PhantomData,
        }
    }

    /// Register a module for async construction.
    ///
    /// The module's [`AsyncAutoBuilder::build`] is stored as a type-erased
    /// [`AsyncBuildFn`] and invoked during `build()`. Registration order does
    /// not matter — `build()` resolves the construction order via the
    /// dependency graph's topological sort.
    ///
    /// # Errors
    ///
    /// Returns [`KitError::AlreadyRegistered`] if a module with the same
    /// `TypeId` was already registered.
    ///
    /// # Panics
    ///
    /// Panics if the `builders` [`RwLock`] is poisoned (a worker thread
    /// panicked while holding the write lock). Lock poisoning indicates a
    /// logic bug in the async build pipeline and should fail loudly.
    pub fn register<M: AsyncAutoBuilder>(&mut self) -> Result<(), KitError> {
        let entry = ModuleEntry {
            type_id: TypeId::of::<M>(),
            name: M::NAME,
            dependencies: M::dependencies().iter().map(|(n, id)| (*n, *id)).collect(),
        };

        self.graph
            .add(entry)
            .map_err(|name| KitError::AlreadyRegistered { module: name })?;

        let build_fn: AsyncBuildFn = Box::new(|kit| {
            Box::pin(async move {
                let cap = M::build(kit)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
                Ok(Box::new(cap) as Box<dyn Any + Send + Sync>)
            })
        });
        self.builders
            .write()
            .expect("AsyncKit builders lock poisoned: another thread panicked while holding the lock")
            .insert(TypeId::of::<M>(), build_fn);
        Ok(())
    }

    /// Set a configuration value.
    ///
    /// Overwrites any prior value of the same type. Configs are read during
    /// `build()` via [`AsyncKit::config`] inside module `build` callbacks.
    pub fn set_config<C: Clone + Send + Sync + 'static>(&self, config: C) {
        self.configs.insert(config);
    }

    /// Validate the dependency graph and build all modules in topological
    /// order, returning an `AsyncKit<Ready>` whose capabilities are available
    /// via `require` / `optional`.
    ///
    /// Async because each module's [`AsyncAutoBuilder::build`] returns a
    /// future. Modules are constructed one at a time in dependency order;
    /// the build callback receives a `&AsyncKit` reference and may read
    /// configs (and, once prior modules are built, capabilities) from it.
    ///
    /// # Cross-Module Dependency Injection
    ///
    /// Because modules are constructed in topological order and each
    /// capability is inserted into the shared [`AsyncTypeMap`] immediately
    /// after its `build` future resolves, a module's `build` callback may
    /// call `kit.require::<DepModule>()?` to pull in the capability of any
    /// already-built dependency. This is the canonical DI pattern and works
    /// transitively (A→B→C chains). The `require` method lives in
    /// `impl<S> AsyncKit<S>` so it is available on `&AsyncKit<Unbuilt>`
    /// during `build()` as well as on `&AsyncKit<Ready>` afterwards.
    ///
    /// ```text
    /// // Inside an AsyncAutoBuilder::build callback:
    /// let dep_cap = kit.require::<DepModule>()?;  // dep was built earlier
    /// ```
    ///
    /// The kit's `capabilities` map is backed by `Arc<RwLock<...>>`, so a
    /// write is visible to subsequent `require` calls without additional
    /// synchronization. The build callback must not hold a write guard
    /// across `.await` (the build pipeline never does this).
    ///
    /// # Errors
    ///
    /// - [`KitError::DependencyMissing`] if a registered module declares a
    ///   dependency that was never registered.
    /// - [`KitError::CycleDetected`] if the dependency graph contains a cycle.
    /// - [`KitError::MissingCapability`] if a topologically-sorted module has
    ///   no stored build function (internal invariant violation).
    /// - [`KitError::BuildFailed`] if a module's `build` callback returns `Err`.
    ///
    /// # Panics
    ///
    /// Panics if the `builders` [`RwLock`] is poisoned (a worker thread
    /// panicked while holding the write lock). Lock poisoning indicates a
    /// logic bug in the async build pipeline and should fail loudly.
    pub async fn build(self) -> Result<AsyncKit<Ready>, KitError> {
        // 1. Validate the dependency graph: missing-dep check + Kahn topo sort.
        let sorted = match self.graph.validate() {
            Ok(sorted) => sorted,
            Err(GraphError::DependencyMissing { module, missing }) => {
                return Err(KitError::DependencyMissing { module, missing });
            }
            Err(GraphError::CycleDetected { cycle }) => {
                return Err(KitError::CycleDetected { cycle });
            }
        };

        // 2. Invoke each module's AsyncBuildFn in topological order.
        //    The build_fn borrows `&self` for lifetime `'a` (HRTB); we await
        //    the returned future immediately so the borrow releases before the
        //    next iteration. The write guard on `builders` is dropped at the
        //    end of the `remove` statement — before `.await` — to avoid
        //    holding the lock across a suspension point (which would block
        //    other readers and risk deadlock).
        for type_id in &sorted {
            let build_fn = self
                .builders
                .write()
                .expect("AsyncKit builders lock poisoned: another thread panicked while holding the lock")
                .remove(type_id)
                .ok_or_else(|| KitError::MissingCapability {
                    key: self.module_name(*type_id),
                })?;
            // Write guard dropped here (end of statement).

            let module_name = self.module_name(*type_id);

            // `build_fn(&self)` returns `Pin<Box<dyn Future + Send + 'a>>`
            // where `'a` is tied to the borrow of `self`. Awaiting consumes
            // the future, releasing the borrow before the next statement.
            let fut = build_fn(&self);
            match fut.await {
                Ok(boxed) => self.capabilities.insert_boxed(*type_id, boxed),
                Err(e) => {
                    return Err(KitError::BuildFailed {
                        context: module_name,
                        source: e,
                    });
                }
            }
        }

        // 3. Transition to Ready: reuse all containers, swap the state marker.
        Ok(AsyncKit {
            builders: self.builders,
            graph: self.graph,
            configs: self.configs,
            capabilities: self.capabilities,
            _state: PhantomData::<Ready>,
        })
    }

    /// Look up a module's diagnostic name by `TypeId` (mirrors `Kit::module_name`).
    fn module_name(&self, type_id: TypeId) -> &'static str {
        self.graph.name_of(type_id).unwrap_or("<unknown>")
    }
}

impl<S> AsyncKit<S> {
    /// Get a configuration value.
    ///
    /// Available on both `AsyncKit<Unbuilt>` (inside `AsyncAutoBuilder::build`
    /// callbacks) and `AsyncKit<Ready>` (after `build()` completes).
    ///
    /// # Errors
    ///
    /// Returns [`KitError::MissingConfig`] if no value of type `C` was set.
    pub fn config<C: Clone + Send + Sync + 'static>(&self) -> Result<C, KitError> {
        self.configs
            .get_cloned::<C>()
            .ok_or(KitError::MissingConfig {
                key: std::any::type_name::<C>(),
            })
    }

    /// Retrieve a capability by its module type.
    ///
    /// Available on both `AsyncKit<Unbuilt>` (inside `AsyncAutoBuilder::build`
    /// callbacks, for cross-module dependency injection during `build()`) and
    /// `AsyncKit<Ready>` (after `build()` completes).
    ///
    /// # Errors
    ///
    /// Returns [`KitError::MissingCapability`] if the module has not been
    /// built yet (its `TypeId` is absent from the capabilities map).
    pub fn require<M: AsyncAutoBuilder>(&self) -> Result<M::Capability, KitError> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
            .ok_or(KitError::MissingCapability { key: M::NAME })
    }
}

impl AsyncKit<Ready> {
    /// Retrieve an optional capability. Returns `None` if the module has not
    /// been built (its `TypeId` is absent from the capabilities map).
    #[must_use]
    pub fn optional<M: AsyncAutoBuilder>(&self) -> Option<M::Capability> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
    }

    /// Check if a capability has been built (its `TypeId` is present in the
    /// capabilities map).
    #[must_use]
    pub fn contains<M: AsyncAutoBuilder>(&self) -> bool {
        self.capabilities.contains_by_type_id(TypeId::of::<M>())
    }

    /// Check if a config of type `C` has been registered.
    #[must_use]
    pub fn contains_config<C: Clone + Send + Sync + 'static>(&self) -> bool {
        self.configs.contains::<C>()
    }
}

impl Default for AsyncKit {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AsyncKit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncKit<Unbuilt>")
            .field("modules", &self.graph.entries().len())
            .field("configs", &self.configs.len())
            .finish()
    }
}

impl std::fmt::Debug for AsyncKit<Ready> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncKit<Ready>")
            .field("modules", &self.graph.entries().len())
            .field("configs", &self.configs.len())
            .finish()
    }
}

#[cfg(all(test, feature = "async"))]
mod tests {
    use super::{AsyncKit, Ready};
    use crate::core::error::KitError;
    use crate::core::meta::{AsyncAutoBuilder, ModuleMeta};
    use std::any::TypeId;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::{self, Poll};

    /// Minimal single-threaded `Future` executor for tests (no extra deps).
    ///
    /// Mirrors the helper in `core::meta::async_tests`. Uses `Waker::noop()`
    /// (stable since 1.85) because the `async` feature stays dep-free.
    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = task::Waker::noop();
        #[allow(clippy::needless_borrow, reason = "Context::from_waker takes &Waker")]
        let mut cx = task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct MockCap {
        value: i32,
    }

    #[derive(Debug, thiserror::Error)]
    #[allow(dead_code, reason = "mock error type verifies trait signature only")]
    enum MockError {
        #[error("mock build failed: {0}")]
        Failed(String),
    }

    struct MockModule;

    impl ModuleMeta for MockModule {
        const NAME: &'static str = "mock-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockModule {
        type Capability = Arc<MockCap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(MockCap { value: 42 })) })
        }
    }

    // --- T008 mock modules for build() tests ---

    /// Build callback returns `Err`, exercising `KitError::BuildFailed`.
    struct MockErrModule;

    impl ModuleMeta for MockErrModule {
        const NAME: &'static str = "mock-err-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockErrModule {
        type Capability = Arc<MockCap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move {
                Err(MockError::Failed("intentional build failure".to_string()))
            })
        }
    }

    /// Build callback reads an `Arc<AtomicUsize>` config and increments it,
    /// proving the async body actually executed.
    struct MockCounterModule;

    impl ModuleMeta for MockCounterModule {
        const NAME: &'static str = "mock-counter-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockCounterModule {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            Box::pin(async move {
                let counter = kit
                    .config::<Arc<AtomicUsize>>()
                    .map_err(|e| MockError::Failed(e.to_string()))?;
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(()))
            })
        }
    }

    /// Phantom module that is never registered; used as a declared-but-missing
    /// dependency to trigger `KitError::DependencyMissing`.
    struct MissingDep;

    /// Declares a dependency on `MissingDep` (unregistered) to trigger
    /// `KitError::DependencyMissing` during `graph.validate()`.
    struct MockMissingDepModule;

    impl ModuleMeta for MockMissingDepModule {
        const NAME: &'static str = "mock-missing-dep-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("missing-dep", TypeId::of::<MissingDep>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockMissingDepModule {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    /// First half of a 2-node dependency cycle.
    struct MockCycleA;

    impl ModuleMeta for MockCycleA {
        const NAME: &'static str = "mock-cycle-a";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-b", TypeId::of::<MockCycleB>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleA {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    /// Second half of a 2-node dependency cycle.
    struct MockCycleB;

    impl ModuleMeta for MockCycleB {
        const NAME: &'static str = "mock-cycle-b";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-a", TypeId::of::<MockCycleA>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleB {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_kit_new_returns_unbuilt_state() {
        let kit = AsyncKit::new();
        assert!(kit.builders.read().expect("lock poisoned").is_empty());
        assert!(kit.graph.entries().is_empty());
        assert_eq!(kit.configs.len(), 0);
        assert_eq!(kit.capabilities.len(), 0);
    }

    #[test]
    fn async_kit_register_stores_builder() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        assert_eq!(kit.builders.read().expect("lock poisoned").len(), 1);
        assert_eq!(kit.graph.entries().len(), 1);
    }

    #[test]
    fn async_kit_register_duplicate_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("first register should succeed");
        let err = kit
            .register::<MockModule>()
            .expect_err("duplicate register should error");
        assert!(
            matches!(err, KitError::AlreadyRegistered { module: "mock-module" }),
            "expected AlreadyRegistered, got {err:?}"
        );
    }

    #[test]
    fn async_kit_set_config_stores_value() {
        let kit = AsyncKit::new();
        kit.set_config(42i32);
        assert_eq!(kit.config::<i32>().expect("config should exist"), 42);
    }

    #[test]
    fn async_kit_set_config_overwrite() {
        let kit = AsyncKit::new();
        kit.set_config(1i32);
        kit.set_config(2i32);
        assert_eq!(kit.config::<i32>().expect("config should exist"), 2);
    }

    #[test]
    fn async_kit_config_missing_returns_error() {
        let kit = AsyncKit::new();
        let err = kit.config::<u64>().expect_err("missing config should error");
        assert!(
            matches!(err, KitError::MissingConfig { .. }),
            "expected MissingConfig, got {err:?}"
        );
    }

    #[test]
    fn async_kit_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AsyncKit>();
    }

    // --- T008 tests for AsyncKit::build() ---

    #[test]
    fn async_kit_build_returns_ready_state() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built: AsyncKit<Ready> = block_on(kit.build()).expect("build should succeed");
        // Type assertion via let binding: built must be AsyncKit<Ready>.
        let _ = built;
    }

    #[test]
    fn async_kit_build_constructs_capability() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built = block_on(kit.build()).expect("build should succeed");
        let cap = built
            .capabilities
            .get_cloned_by_type_id::<Arc<MockCap>>(TypeId::of::<MockModule>())
            .expect("capability should be stored after build");
        assert_eq!(cap.value, 42);
    }

    #[test]
    fn async_kit_build_multiple_modules_in_topo_order() {
        let mut kit = AsyncKit::new();
        kit.set_config(Arc::new(AtomicUsize::new(0)));
        kit.register::<MockModule>()
            .expect("register module A");
        kit.register::<MockCounterModule>()
            .expect("register module B");
        let built = block_on(kit.build()).expect("build should succeed");
        assert_eq!(
            built.capabilities.len(),
            2,
            "capabilities should contain both modules"
        );
    }

    #[test]
    fn async_kit_build_missing_dependency_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockMissingDepModule>()
            .expect("register should succeed (declares missing dep)");
        let err = block_on(kit.build())
            .expect_err("build should fail when a dependency is unregistered");
        assert!(
            matches!(
                err,
                KitError::DependencyMissing {
                    module: "mock-missing-dep-module",
                    missing: "missing-dep"
                }
            ),
            "expected DependencyMissing, got {err:?}"
        );
    }

    #[test]
    fn async_kit_build_cycle_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockCycleA>()
            .expect("register cycle A");
        kit.register::<MockCycleB>()
            .expect("register cycle B");
        let err = block_on(kit.build())
            .expect_err("build should fail on cyclic dependency graph");
        assert!(
            matches!(err, KitError::CycleDetected { .. }),
            "expected CycleDetected, got {err:?}"
        );
    }

    #[test]
    fn async_kit_build_calls_async_build_fn() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut kit = AsyncKit::new();
        kit.set_config(Arc::clone(&counter));
        kit.register::<MockCounterModule>()
            .expect("register should succeed");
        let _built = block_on(kit.build()).expect("build should succeed");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "async build callback should have executed exactly once"
        );
    }

    #[test]
    fn async_kit_build_propagates_build_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockErrModule>()
            .expect("register should succeed");
        let err = block_on(kit.build())
            .expect_err("build should fail when module build returns Err");
        assert!(
            matches!(
                err,
                KitError::BuildFailed {
                    context: "mock-err-module",
                    ..
                }
            ),
            "expected BuildFailed for mock-err-module, got {err:?}"
        );
    }

    // --- T010 tests for AsyncKit<Ready> retrieval API (require/optional/contains/contains_config) ---

    #[test]
    fn async_kit_ready_require_returns_capability() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built = block_on(kit.build()).expect("build should succeed");
        let cap = built
            .require::<MockModule>()
            .expect("require on built module should succeed");
        assert_eq!(cap.value, 42);
    }

    #[test]
    fn async_kit_ready_require_missing_returns_error() {
        // Empty kit: MockModule is never registered/built, so its TypeId is
        // absent from the capabilities map. `require` must return MissingCapability.
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).expect("empty build should succeed");
        let err = built
            .require::<MockModule>()
            .expect_err("require on unbuilt module should error");
        assert!(
            matches!(err, KitError::MissingCapability { key: "mock-module" }),
            "expected MissingCapability for mock-module, got {err:?}"
        );
    }

    #[test]
    fn async_kit_ready_optional_returns_some_for_built() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built = block_on(kit.build()).expect("build should succeed");
        let cap = built
            .optional::<MockModule>()
            .expect("optional on built module should return Some");
        assert_eq!(cap.value, 42);
    }

    #[test]
    fn async_kit_ready_optional_returns_none_for_unbuilt() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).expect("empty build should succeed");
        assert!(
            built.optional::<MockModule>().is_none(),
            "optional on unbuilt module should return None"
        );
    }

    #[test]
    fn async_kit_ready_contains_returns_true_for_built() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built = block_on(kit.build()).expect("build should succeed");
        assert!(
            built.contains::<MockModule>(),
            "contains should return true for built module"
        );
    }

    #[test]
    fn async_kit_ready_contains_returns_false_for_unbuilt() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).expect("empty build should succeed");
        assert!(
            !built.contains::<MockModule>(),
            "contains should return false for unbuilt module"
        );
    }

    #[test]
    fn async_kit_ready_contains_config_returns_true() {
        let kit = AsyncKit::new();
        kit.set_config(42i32);
        let built = block_on(kit.build()).expect("build should succeed");
        assert!(
            built.contains_config::<i32>(),
            "contains_config should return true for stored i32 config"
        );
    }

    #[test]
    fn async_kit_ready_contains_config_returns_false() {
        let kit = AsyncKit::new();
        kit.set_config(42i32);
        let built = block_on(kit.build()).expect("build should succeed");
        assert!(
            !built.contains_config::<u64>(),
            "contains_config should return false for absent u64 config"
        );
    }

    // === T012 mocks: cross-module dependency injection (R-004) ===
    //
    // MockBModule: no deps, cap = Arc<Bcap{n:42}>.
    // MockAModule: declares dep on MockBModule; build() calls
    //   `kit.require::<MockBModule>()?` and embeds B's n into A's cap.
    //   This is the canonical DI pattern from design.md Decision 3.
    // MockCModule / MockChainBModule / MockChainAModule: transitive
    //   A→B→C chain; each build callback calls require on its direct dep.
    // MockCycleA3/B3/C3: 3-node cycle A→B→C→A for cycle detection.
    //
    // `From<KitError> for MockError` lets `?` convert require errors
    // (matches the production pattern in design.md where DbNexusModule
    // uses `kit.require::<OxcacheModule>()?` with `OxcacheError: From<KitError>`).

    impl From<KitError> for MockError {
        fn from(e: KitError) -> Self {
            MockError::Failed(e.to_string())
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Bcap {
        n: i32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Acap {
        b_val: i32,
    }

    struct MockBModule;

    impl ModuleMeta for MockBModule {
        const NAME: &'static str = "mock-b";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockBModule {
        type Capability = Arc<Bcap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(Bcap { n: 42 })) })
        }
    }

    struct MockAModule;

    impl ModuleMeta for MockAModule {
        const NAME: &'static str = "mock-a";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-b", TypeId::of::<MockBModule>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockAModule {
        type Capability = Arc<Acap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            Box::pin(async move {
                // DI happens here: pull B's cap from the kit during A's build.
                let b_cap: Arc<Bcap> = kit.require::<MockBModule>()?;
                Ok(Arc::new(Acap { b_val: b_cap.n }))
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Ccap {
        v: i32,
        build_order: usize,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct ChainBcap {
        c_val: i32,
        build_order: usize,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct ChainAcap {
        b_val: i32,
        build_order: usize,
    }

    struct MockCModule;

    impl ModuleMeta for MockCModule {
        const NAME: &'static str = "mock-c";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockCModule {
        type Capability = Arc<Ccap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            Box::pin(async move {
                let counter = kit.config::<Arc<AtomicUsize>>()?;
                let order = counter.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(Ccap {
                    v: 100,
                    build_order: order + 1,
                }))
            })
        }
    }

    struct MockChainBModule;

    impl ModuleMeta for MockChainBModule {
        const NAME: &'static str = "mock-chain-b";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-c", TypeId::of::<MockCModule>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockChainBModule {
        type Capability = Arc<ChainBcap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            Box::pin(async move {
                // DI: pull C's cap during B's build.
                let c_cap: Arc<Ccap> = kit.require::<MockCModule>()?;
                let counter = kit.config::<Arc<AtomicUsize>>()?;
                let order = counter.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(ChainBcap {
                    c_val: c_cap.v,
                    build_order: order + 1,
                }))
            })
        }
    }

    struct MockChainAModule;

    impl ModuleMeta for MockChainAModule {
        const NAME: &'static str = "mock-chain-a";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] =
                &[("mock-chain-b", TypeId::of::<MockChainBModule>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockChainAModule {
        type Capability = Arc<ChainAcap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            Box::pin(async move {
                // DI: pull chain-B's cap during A's build (transitive).
                let b_cap: Arc<ChainBcap> = kit.require::<MockChainBModule>()?;
                let counter = kit.config::<Arc<AtomicUsize>>()?;
                let order = counter.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(ChainAcap {
                    b_val: b_cap.c_val,
                    build_order: order + 1,
                }))
            })
        }
    }

    // 3-node cycle: MockCycleA3 → MockCycleB3 → MockCycleC3 → MockCycleA3.
    // Build callbacks are trivial because graph.validate() rejects the cycle
    // before any build_fn is invoked.
    struct MockCycleA3;

    impl ModuleMeta for MockCycleA3 {
        const NAME: &'static str = "mock-cycle-a3";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-b3", TypeId::of::<MockCycleB3>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleA3 {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    struct MockCycleB3;

    impl ModuleMeta for MockCycleB3 {
        const NAME: &'static str = "mock-cycle-b3";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-c3", TypeId::of::<MockCycleC3>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleB3 {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    struct MockCycleC3;

    impl ModuleMeta for MockCycleC3 {
        const NAME: &'static str = "mock-cycle-c3";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-a3", TypeId::of::<MockCycleA3>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleC3 {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    // --- T012 tests: cross-module dependency injection (R-004) ---

    /// R-004 #1: A declares dep on B; B is built before A (topo order).
    /// A's cap embeds B's n=42, proving B was ready when A's build ran.
    #[test]
    fn async_kit_di_dependency_built_before_dependent() {
        let mut kit = AsyncKit::new();
        kit.register::<MockBModule>().expect("register B");
        kit.register::<MockAModule>().expect("register A");
        let built = block_on(kit.build()).expect("build should succeed");
        let a_cap = built
            .require::<MockAModule>()
            .expect("A's cap should be built");
        assert_eq!(
            a_cap.b_val, 42,
            "A's cap must contain B's n=42 — proves B built before A"
        );
    }

    /// R-004 #2: A's build callback calls `kit.require::<MockBModule>()`
    /// and receives B's capability. Both caps are retrievable post-build.
    #[test]
    fn async_kit_di_require_returns_dependency_capability() {
        let mut kit = AsyncKit::new();
        kit.register::<MockBModule>().expect("register B");
        kit.register::<MockAModule>().expect("register A");
        let built = block_on(kit.build()).expect("build should succeed");
        let b_cap = built.require::<MockBModule>().expect("B's cap");
        let a_cap = built.require::<MockAModule>().expect("A's cap");
        assert_eq!(b_cap.n, 42);
        assert_eq!(
            a_cap.b_val, 42,
            "A's cap must contain B's n=42 — require worked inside build callback"
        );
    }

    /// R-004 #3: Missing dependency → `KitError::DependencyMissing`.
    /// Register only `MockAModule` (declares dep on `MockBModule`); `MockBModule`
    /// is intentionally unregistered. `graph.validate()` must reject before
    /// any `build_fn` runs.
    #[test]
    fn async_kit_di_missing_dependency_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockAModule>()
            .expect("register A only (B missing)");
        let err = block_on(kit.build())
            .expect_err("build must fail when declared dep is unregistered");
        assert!(
            matches!(
                err,
                KitError::DependencyMissing {
                    module: "mock-a",
                    missing: "mock-b"
                }
            ),
            "expected DependencyMissing {{ module: \"mock-a\", missing: \"mock-b\" }}, got {err:?}"
        );
    }

    /// R-004 #4: 3-node cycle A→B→C→A → `KitError::CycleDetected`.
    /// Distinct from the 2-node cycle test (T008) — exercises DFS cycle
    /// extraction on a longer ring.
    #[test]
    fn async_kit_di_three_node_cycle_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockCycleA3>().expect("register cycle A3");
        kit.register::<MockCycleB3>().expect("register cycle B3");
        kit.register::<MockCycleC3>().expect("register cycle C3");
        let err = block_on(kit.build())
            .expect_err("build must fail on 3-node cycle");
        assert!(
            matches!(err, KitError::CycleDetected { .. }),
            "expected CycleDetected for 3-node cycle, got {err:?}"
        );
    }

    /// R-004 #5: Transitive chain A→B→C. C built first (order=1), B second
    /// (order=2), A third (order=3). A's `require::<MockChainBModule>()`
    /// succeeds, B's `require::<MockCModule>()` succeeds. A's cap contains
    /// C's v=100 transitively — proves DI propagates through the chain.
    #[test]
    fn async_kit_di_transitive_dependency_chain() {
        let mut kit = AsyncKit::new();
        kit.set_config(Arc::new(AtomicUsize::new(0)));
        kit.register::<MockCModule>().expect("register C");
        kit.register::<MockChainBModule>().expect("register chain-B");
        kit.register::<MockChainAModule>().expect("register chain-A");
        let built = block_on(kit.build()).expect("build should succeed");

        let c_cap = built.require::<MockCModule>().expect("C's cap");
        let b_cap = built.require::<MockChainBModule>().expect("chain-B's cap");
        let a_cap = built.require::<MockChainAModule>().expect("chain-A's cap");

        // Topological order: C=1, B=2, A=3.
        assert_eq!(c_cap.build_order, 1, "C should be built first");
        assert_eq!(b_cap.build_order, 2, "B should be built second");
        assert_eq!(a_cap.build_order, 3, "A should be built third");

        // DI propagation: A.b_val ← B.c_val ← C.v.
        assert_eq!(c_cap.v, 100);
        assert_eq!(
            b_cap.c_val, 100,
            "B's cap must contain C's v=100 — require::<MockCModule>() worked in B's build"
        );
        assert_eq!(
            a_cap.b_val, 100,
            "A's cap must transitively contain C's v=100 — transitive DI worked"
        );
    }
}
