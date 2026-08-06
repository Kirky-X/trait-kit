// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! `AsyncKit` — the async capability and configuration management center.
//!
//! Typestate `AsyncKit<Unbuilt>` → `AsyncKit<Ready>` with `Arc<RwLock>`
//! interior mutability (multi-threaded, `Send + Sync`). Mirrors the
//! synchronous [`super::kit::Kit`] but swaps `RefCell` for `RwLock` and
//! stores async build functions returning `Pin<Box<dyn Future + Send>>`.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use crate::core::AsyncAutoBuilder;
use crate::error::TraitKitError;

use super::AsyncTypeMap;

use super::{DependencyGraph, GraphError, ModuleEntry};

#[cfg(feature = "lifecycle")]
type AsyncShutdownCallback = Box<dyn Fn(&AsyncTypeMap) + Send + Sync>;
#[cfg(feature = "lifecycle")]
type AsyncReadyCallback = Box<
    dyn for<'a> Fn(
            &'a AsyncKit<Ready>,
        ) -> Pin<Box<dyn Future<Output = Result<(), TraitKitError>> + Send + 'a>>
        + Send
        + Sync,
>;
#[cfg(feature = "health")]
type AsyncHealthCheckerFn =
    Box<dyn Fn(&AsyncTypeMap) -> crate::core::health::HealthStatus + Send + Sync>;
#[cfg(feature = "observer")]
type AsyncObserverRef = Arc<dyn crate::core::observer::BuildObserver>;
#[cfg(feature = "decorator")]
type AsyncDecoratorFn =
    Box<dyn Fn(Box<dyn Any + Send + Sync>) -> Box<dyn Any + Send + Sync> + Send + Sync>;

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
/// The error variant is `Box<dyn Error + Send + 'static>` to match
/// `TraitKitError::BuildFailed::source` (which is `Send` so that `TraitKitError: Send`
/// and `tokio::spawn(async move { kit.build().await })` compiles on a
/// multi-threaded runtime). The future is `Send` because both
/// `Box<dyn Any + Send + Sync>` and `Box<dyn Error + Send + 'static>` are
/// `Send`.
#[allow(
    clippy::type_complexity,
    reason = "Pin<Box<dyn Future + Send>> is the canonical dyn-compatible async dispatch type; mirrors AsyncAutoBuilder::build"
)]
pub(crate) type AsyncBuildFn = Box<
    dyn for<'a> FnOnce(
            &'a AsyncKit,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            Box<dyn Any + Send + Sync>,
                            Box<dyn std::error::Error + Send + 'static>,
                        >,
                    > + Send
                    + 'a,
            >,
        > + Send
        + Sync,
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
    capabilities: AsyncTypeMap,
    #[cfg(feature = "lifecycle")]
    shutdown_callbacks: Arc<RwLock<Vec<(TypeId, AsyncShutdownCallback)>>>,
    #[cfg(feature = "lifecycle")]
    ready_callbacks: Arc<RwLock<Vec<(TypeId, AsyncReadyCallback)>>>,
    #[cfg(feature = "health")]
    health_checkers: Arc<RwLock<HashMap<TypeId, (&'static str, AsyncHealthCheckerFn)>>>,
    #[cfg(feature = "observer")]
    observers: Arc<RwLock<Vec<AsyncObserverRef>>>,
    #[cfg(feature = "decorator")]
    decorators: Arc<RwLock<HashMap<TypeId, Vec<AsyncDecoratorFn>>>>,
    #[cfg(feature = "decorator")]
    decorator_module_to_cap: Arc<RwLock<HashMap<TypeId, TypeId>>>,
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
            #[cfg(feature = "lifecycle")]
            shutdown_callbacks: Arc::new(RwLock::new(Vec::new())),
            #[cfg(feature = "lifecycle")]
            ready_callbacks: Arc::new(RwLock::new(Vec::new())),
            #[cfg(feature = "health")]
            health_checkers: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "observer")]
            observers: Arc::new(RwLock::new(Vec::new())),
            #[cfg(feature = "decorator")]
            decorators: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "decorator")]
            decorator_module_to_cap: Arc::new(RwLock::new(HashMap::new())),
            _state: PhantomData,
        }
    }

    /// Register a module for async construction.
    ///
    /// The module's [`AsyncAutoBuilder::build`] is stored as a type-erased
    /// `AsyncBuildFn` and invoked during `build()`. Registration order does
    /// not matter — `build()` resolves the construction order via the
    /// dependency graph's topological sort.
    ///
    /// # Errors
    ///
    /// Returns [`TraitKitError::AlreadyRegistered`] if a module with the same
    /// `TypeId` was already registered.
    ///
    /// # Panics
    ///
    /// Panics if the `builders` [`RwLock`] is poisoned (a worker thread
    /// panicked while holding the write lock). Lock poisoning indicates a
    /// logic bug in the async build pipeline and should fail loudly.
    #[must_use = "ignoring the Result may hide AlreadyRegistered errors"]
    pub fn register<M: AsyncAutoBuilder>(&mut self) -> Result<(), TraitKitError> {
        let entry = ModuleEntry {
            type_id: TypeId::of::<M>(),
            name: M::NAME,
            dependencies: M::dependencies().iter().map(|(n, id)| (*n, *id)).collect(),
        };

        self.graph
            .add(entry)
            .map_err(|name| TraitKitError::AlreadyRegistered { module: name })?;

        let build_fn: AsyncBuildFn = Box::new(|kit| {
            Box::pin(async move {
                let cap = M::build(kit)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + 'static> { Box::new(e) })?;
                Ok(Box::new(cap) as Box<dyn Any + Send + Sync>)
            })
        });
        self.builders
            .write()
            .expect(
                "AsyncKit builders lock poisoned: another thread panicked while holding the lock",
            )
            .insert(TypeId::of::<M>(), build_fn);
        Ok(())
    }

    /// Set a configuration value.
    ///
    /// Overwrites any prior value of the same type. Configs are read during
    /// `build()` via [`AsyncKit::config`] inside module `build` callbacks.
    ///
    /// # Panics
    ///
    /// Panics if the `configs` [`RwLock`] is poisoned (a worker thread
    /// panicked while holding the write lock). See [`register`](Self::register)
    /// for context on lock poisoning.
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
    /// - [`TraitKitError::DependencyMissing`] if a registered module declares a
    ///   dependency that was never registered.
    /// - [`TraitKitError::CycleDetected`] if the dependency graph contains a cycle.
    /// - [`TraitKitError::MissingCapability`] if a topologically-sorted module has
    ///   no stored build function (internal invariant violation).
    /// - [`TraitKitError::BuildFailed`] if a module's `build` callback returns `Err`.
    ///
    /// # Panics
    ///
    /// Panics if the `builders` [`RwLock`] is poisoned (a worker thread
    /// panicked while holding the write lock). Lock poisoning indicates a
    /// logic bug in the async build pipeline and should fail loudly.
    #[must_use = "ignoring the built kit loses all capabilities and lifecycle callbacks"]
    #[allow(clippy::too_many_lines)]
    pub async fn build(self) -> Result<AsyncKit<Ready>, TraitKitError> {
        // 1. Validate the dependency graph: missing-dep check + Kahn topo sort.
        let sorted = match self.graph.validate() {
            Ok(sorted) => sorted,
            Err(GraphError::DependencyMissing { module, missing }) => {
                return Err(TraitKitError::DependencyMissing { module, missing });
            }
            Err(GraphError::CycleDetected { cycle }) => {
                return Err(TraitKitError::CycleDetected { cycle });
            }
        };

        // 2. Extract all builders from the Arc<RwLock<…>> in a single
        //    write-lock acquisition (instead of one lock per module in the
        //    loop). The drain empties the map inside the RwLock; the Arc
        //    itself remains held by the struct for subsequent operations.
        let mut builders: HashMap<TypeId, AsyncBuildFn> = {
            let mut guard = self
                .builders
                .write()
                .expect("AsyncKit builders lock poisoned");
            guard.drain().collect()
        };

        // 3. Invoke each module's AsyncBuildFn in topological order.
        for type_id in &sorted {
            let module_name = self.graph.name_of(*type_id).unwrap_or("<unknown>");
            let build_fn =
                builders
                    .remove(type_id)
                    .ok_or_else(|| TraitKitError::MissingCapability {
                        key: module_name.to_string(),
                    })?;

            // Observer: notify build start
            #[cfg(feature = "observer")]
            let start_instant = std::time::Instant::now();
            #[cfg(feature = "observer")]
            {
                let observers = self.observers.read().expect("lock poisoned");
                for obs in observers.iter() {
                    obs.on_module_start(module_name);
                }
            }

            // `build_fn(&self)` returns `Pin<Box<dyn Future + Send + 'a>>`
            // where `'a` is tied to the borrow of `self`. Awaiting consumes
            // the future, releasing the borrow before the next statement.
            let fut = build_fn(&self);
            match fut.await {
                Ok(boxed) => {
                    // Apply decorators (keyed by capability TypeId)
                    #[cfg(feature = "decorator")]
                    let boxed = {
                        let cap_type_id = self
                            .decorator_module_to_cap
                            .read()
                            .expect("lock poisoned")
                            .get(type_id)
                            .copied()
                            .unwrap_or(*type_id);
                        self.apply_decorators(cap_type_id, boxed)
                    };
                    self.capabilities.insert_boxed(*type_id, boxed);
                    #[cfg(feature = "observer")]
                    {
                        let elapsed = start_instant.elapsed();
                        let observers = self.observers.read().expect("lock poisoned");
                        for obs in observers.iter() {
                            obs.on_module_built(module_name, elapsed);
                        }
                    }
                }
                Err(e) => {
                    let err = TraitKitError::BuildFailed {
                        context: module_name.to_string(),
                        source: e,
                    };
                    #[cfg(feature = "observer")]
                    {
                        let observers = self.observers.read().expect("lock poisoned");
                        for obs in observers.iter() {
                            obs.on_build_error(module_name, &err);
                        }
                    }
                    return Err(err);
                }
            }
        }

        // 4. Transition to Ready: reuse all containers, swap the state marker.
        //    `builders` was drained (not moved) above; the empty map is reused.
        #[cfg(feature = "lifecycle")]
        let ready_callbacks: Vec<(TypeId, AsyncReadyCallback)> = {
            self.ready_callbacks
                .write()
                .expect("lock poisoned")
                .drain(..)
                .collect()
        };

        let kit = AsyncKit {
            builders: self.builders,
            graph: self.graph,
            configs: self.configs,
            capabilities: self.capabilities,
            #[cfg(feature = "lifecycle")]
            shutdown_callbacks: self.shutdown_callbacks,
            #[cfg(feature = "lifecycle")]
            ready_callbacks: Arc::new(RwLock::new(Vec::new())),
            #[cfg(feature = "health")]
            health_checkers: self.health_checkers,
            #[cfg(feature = "observer")]
            observers: self.observers,
            #[cfg(feature = "decorator")]
            decorators: self.decorators,
            #[cfg(feature = "decorator")]
            decorator_module_to_cap: self.decorator_module_to_cap,
            _state: PhantomData::<Ready>,
        };

        // Call lifecycle on_ready callbacks in topological order
        #[cfg(feature = "lifecycle")]
        {
            for (_type_id, callback) in &ready_callbacks {
                callback(&kit).await?;
            }
        }

        Ok(kit)
    }

    /// Look up a module's diagnostic name by `TypeId` (mirrors `Kit::module_name`).
    #[allow(dead_code, reason = "used in tests and available for diagnostics")]
    fn module_name(&self, type_id: TypeId) -> &'static str {
        self.graph.name_of(type_id).unwrap_or("<unknown>")
    }

    // ─── Lifecycle ─────────────────────────────────────────────────────

    /// Register lifecycle hooks for an async module.
    ///
    /// Requires the `lifecycle` feature.
    ///
    /// # Limitations
    ///
    /// `AsyncLifecycle::on_ready` is fully supported and called during `build()`.
    /// However, `AsyncLifecycle::on_shutdown` is **not** called by the synchronous
    /// `shutdown()` method, because sync callbacks cannot await async futures.
    /// Async cleanup logic in `on_shutdown` will be silently skipped.
    ///
    /// For async shutdown, use `AsyncKit::shutdown_async()` (when available) or
    /// manually invoke `M::on_shutdown(&cap)` for each module that needs async cleanup.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "lifecycle")]
    pub fn register_lifecycle<M>(&mut self)
    where
        M: crate::core::lifecycle::AsyncLifecycle + 'static,
        M::Capability: Send + Sync + 'static,
    {
        let shutdown_cb: AsyncShutdownCallback = Box::new(|caps: &AsyncTypeMap| {
            let type_id = TypeId::of::<M>();
            if let Some((_guard, cap_ref)) = caps.read_by_type_id::<M::Capability>(type_id) {
                // NOTE: AsyncLifecycle::on_shutdown returns a Future and cannot
                // be called from a sync closure. The async shutdown is intentionally
                // skipped here — users must invoke it manually or via an async
                // shutdown method. See register_lifecycle() docs for details.
                let _ = cap_ref;
            }
        });
        self.shutdown_callbacks
            .write()
            .expect("lock poisoned")
            .push((TypeId::of::<M>(), shutdown_cb));

        let ready_cb: AsyncReadyCallback = Box::new(|kit: &AsyncKit<Ready>| {
            let fut = M::on_ready(kit);
            Box::pin(async move {
                fut.await.map_err(|e| TraitKitError::LifecycleFailed {
                    context: M::NAME.to_string(),
                    source: Box::new(e),
                })
            })
        });
        self.ready_callbacks
            .write()
            .expect("lock poisoned")
            .push((TypeId::of::<M>(), ready_cb));
    }

    // ─── Health Check ──────────────────────────────────────────────────

    /// Register a health checker for an async module.
    ///
    /// Requires the `health` feature.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "health")]
    pub fn register_health_check<M>(&mut self)
    where
        M: crate::core::health::AsyncHealthCheck + 'static,
        M::Capability: Send + Sync + 'static,
    {
        let checker: AsyncHealthCheckerFn = Box::new(|caps: &AsyncTypeMap| {
            let type_id = TypeId::of::<M>();
            match caps.read_by_type_id::<M::Capability>(type_id) {
                Some((_guard, cap_ref)) => M::check(cap_ref),
                None => crate::core::health::HealthStatus::Unhealthy {
                    detail: "capability not found".to_string(),
                },
            }
        });
        self.health_checkers
            .write()
            .expect("lock poisoned")
            .insert(TypeId::of::<M>(), (M::NAME, checker));
    }

    // ─── Conditional Registration ───────────────────────────────────────

    /// Conditionally register an async module based on a runtime predicate.
    ///
    /// # Errors
    ///
    /// Returns [`TraitKitError::AlreadyRegistered`] if the module was already registered.
    #[must_use = "ignoring the Result may hide registration failures"]
    pub fn register_if<M: AsyncAutoBuilder>(
        &mut self,
        predicate: impl FnOnce(&AsyncKit) -> bool,
    ) -> Result<bool, TraitKitError> {
        if predicate(self) {
            self.register::<M>()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ─── Observability ─────────────────────────────────────────────────

    /// Register a build observer for the async build pipeline.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "observer")]
    pub fn with_observer(&mut self, observer: Arc<dyn crate::core::observer::BuildObserver>) {
        self.observers
            .write()
            .expect("lock poisoned")
            .push(observer);
    }

    // ─── Decorator ─────────────────────────────────────────────────────

    /// Register a decorator for an async module's capability.
    ///
    /// Requires the `decorator` feature.
    ///
    /// # Panics
    ///
    /// Panics at runtime if the internal `downcast` fails due to a type
    /// mismatch (should never happen when used correctly), or if the
    /// internal `RwLock` is poisoned.
    #[cfg(feature = "decorator")]
    pub fn decorate<M: AsyncAutoBuilder>(
        &self,
        decorator: impl Fn(M::Capability) -> M::Capability + Send + Sync + 'static,
    ) where
        M::Capability: Send + Sync + 'static,
    {
        let wrapper: AsyncDecoratorFn = Box::new(move |boxed_cap| {
            let cap = boxed_cap
                .downcast::<M::Capability>()
                .expect("decorator type mismatch");
            let decorated = decorator(*cap);
            Box::new(decorated) as Box<dyn Any + Send + Sync>
        });
        self.decorators
            .write()
            .expect("lock poisoned")
            .entry(TypeId::of::<M>())
            .or_default()
            .push(wrapper);
        // Record module TypeId → capability TypeId mapping so
        // `build()` can look up decorators by module TypeId.
        self.decorator_module_to_cap
            .write()
            .expect("lock poisoned")
            .insert(TypeId::of::<M>(), TypeId::of::<M::Capability>());
    }
}

impl<S> AsyncKit<S> {
    /// Apply registered decorators for a capability (keyed by capability `TypeId`).
    #[cfg(feature = "decorator")]
    fn apply_decorators(
        &self,
        cap_type_id: TypeId,
        boxed: Box<dyn Any + Send + Sync>,
    ) -> Box<dyn Any + Send + Sync> {
        let decorators = self.decorators.read().expect("lock poisoned");
        let Some(dec_list) = decorators.get(&cap_type_id) else {
            return boxed;
        };
        let mut current = boxed;
        for dec in dec_list {
            current = dec(current);
        }
        current
    }

    /// Get a configuration value.
    ///
    /// Available on both `AsyncKit<Unbuilt>` (inside `AsyncAutoBuilder::build`
    /// callbacks) and `AsyncKit<Ready>` (after `build()` completes).
    ///
    /// # Errors
    ///
    /// Returns [`TraitKitError::MissingConfig`] if no value of type `C` was set.
    ///
    /// # Panics
    ///
    /// Panics if the `configs` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    pub fn config<C: Clone + Send + Sync + 'static>(&self) -> Result<C, TraitKitError> {
        self.configs
            .get_cloned::<C>()
            .ok_or(TraitKitError::MissingConfig {
                key: std::any::type_name::<C>().to_string(),
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
    /// Returns [`TraitKitError::MissingCapability`] if the module has not been
    /// built yet (its `TypeId` is absent from the capabilities map).
    ///
    /// # Panics
    ///
    /// Panics if the `capabilities` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    pub fn require<M: AsyncAutoBuilder>(&self) -> Result<M::Capability, TraitKitError> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
            .ok_or(TraitKitError::MissingCapability {
                key: M::NAME.to_string(),
            })
    }
}

impl AsyncKit<Ready> {
    /// Retrieve an optional capability. Returns `None` if the module has not
    /// been built (its `TypeId` is absent from the capabilities map).
    ///
    /// # Panics
    ///
    /// Panics if the `capabilities` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    #[must_use]
    pub fn optional<M: AsyncAutoBuilder>(&self) -> Option<M::Capability> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
    }

    /// Check if a capability has been built (its `TypeId` is present in the
    /// capabilities map).
    ///
    /// # Panics
    ///
    /// Panics if the `capabilities` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    #[must_use]
    pub fn contains<M: AsyncAutoBuilder>(&self) -> bool {
        self.capabilities.contains_by_type_id(TypeId::of::<M>())
    }

    /// Check if a config of type `C` has been registered.
    ///
    /// # Panics
    ///
    /// Panics if the `configs` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    #[must_use]
    pub fn contains_config<C: Clone + Send + Sync + 'static>(&self) -> bool {
        self.configs.contains::<C>()
    }

    // ─── Lifecycle: shutdown ───────────────────────────────────────────

    /// Shut down all lifecycle modules in reverse topological order.
    ///
    /// Requires the `lifecycle` feature.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "lifecycle")]
    pub fn shutdown(&self) {
        let callbacks: Vec<(TypeId, AsyncShutdownCallback)> = {
            self.shutdown_callbacks
                .write()
                .expect("lock poisoned")
                .drain(..)
                .collect()
        };
        // Reverse order
        for (_type_id, callback) in callbacks.iter().rev() {
            callback(&self.capabilities);
        }
    }

    // ─── Health Check ──────────────────────────────────────────────────

    /// Check the health of a specific async module.
    ///
    /// Requires the `health` feature.
    ///
    /// # Errors
    ///
    /// Returns [`TraitKitError::MissingConfig`] if no health checker is registered
    /// for the given module.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "health")]
    pub fn health_check<M: crate::core::health::AsyncHealthCheck>(
        &self,
    ) -> Result<crate::core::health::HealthStatus, TraitKitError> {
        let type_id = TypeId::of::<M>();
        let checkers = self.health_checkers.read().expect("lock poisoned");
        let (_name, checker) = checkers.get(&type_id).ok_or(TraitKitError::MissingConfig {
            key: M::NAME.to_string(),
        })?;
        Ok(checker(&self.capabilities))
    }

    /// Generate a health report for all registered async health checkers.
    ///
    /// Requires the `health` feature.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "health")]
    #[must_use]
    pub fn health_report(&self) -> Vec<(&'static str, crate::core::health::HealthStatus)> {
        let checkers = self.health_checkers.read().expect("lock poisoned");
        checkers
            .values()
            .map(|(name, checker)| (*name, checker(&self.capabilities)))
            .collect()
    }

    // ─── Factory Pattern ───────────────────────────────────────────────

    /// Create a factory closure that produces new async instances on each call.
    ///
    /// The returned closure is `Send + Sync`, suitable for use in multi-threaded
    /// async runtimes.
    ///
    #[allow(clippy::type_complexity)]
    pub fn factory<M: AsyncAutoBuilder>(
        &self,
    ) -> impl Fn() -> Pin<Box<dyn Future<Output = Result<M::Capability, TraitKitError>> + Send>>
    + Send
    + Sync
    + '_ {
        // Store self's address as usize so the closure is Send+Sync.
        // SAFETY: AsyncKit<Ready> and AsyncKit<Unbuilt> have identical layout.
        // The pointer remains valid for the lifetime bound `'_`.
        //
        // Compile-time layout assertion: if any field depending on `S` is
        // added to `AsyncKit`, this will fail at compile time.
        const _: () = assert!(
            std::mem::size_of::<AsyncKit<Ready>>() == std::mem::size_of::<AsyncKit>(),
            "AsyncKit layout changed; unsafe cast is no longer sound"
        );
        let addr: usize = std::ptr::from_ref::<AsyncKit<Ready>>(self) as usize;

        move || {
            #[allow(unsafe_code)]
            let kit_ref: &AsyncKit = unsafe { &*(addr as *const AsyncKit) };
            let fut = M::build(kit_ref);
            Box::pin(async move {
                fut.await.map_err(|e| TraitKitError::BuildFailed {
                    context: M::NAME.to_string(),
                    source: Box::new(e),
                })
            })
                as Pin<Box<dyn Future<Output = Result<M::Capability, TraitKitError>> + Send>>
        }
    }

    // ─── Scope ─────────────────────────────────────────────────────────

    /// Create a new empty async scope.
    ///
    /// Requires the `scope` feature.
    #[cfg(feature = "scope")]
    #[must_use]
    pub fn create_scope(&self) -> super::scope::AsyncScope {
        super::scope::AsyncScope::new()
    }

    // ─── Graph Visualization ───────────────────────────────────────────

    /// Export the dependency graph as a Graphviz DOT string.
    #[must_use]
    pub fn graph_dot(&self) -> String {
        self.graph.to_dot()
    }

    /// Export the dependency graph as a Mermaid flowchart string.
    #[must_use]
    pub fn graph_mermaid(&self) -> String {
        self.graph.to_mermaid()
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
    use crate::core::{AsyncAutoBuilder, ModuleMeta};
    use crate::error::TraitKitError;
    use crate::test_helpers::{MockError, block_on};
    use std::any::TypeId;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone, PartialEq)]
    struct MockCap {
        value: i32,
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(MockCap { value: 42 })) })
        }
    }

    // --- T008 mock modules for build() tests ---

    /// Build callback returns `Err`, exercising `TraitKitError::BuildFailed`.
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Err(MockError::Failed("intentional build failure".to_string())) })
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
    /// dependency to trigger `TraitKitError::DependencyMissing`.
    struct MissingDep;

    /// Declares a dependency on `MissingDep` (unregistered) to trigger
    /// `TraitKitError::DependencyMissing` during `graph.validate()`.
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
            matches!(
                err,
                TraitKitError::AlreadyRegistered {
                    module: "mock-module"
                }
            ),
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
        let err = kit
            .config::<u64>()
            .expect_err("missing config should error");
        assert!(
            matches!(err, TraitKitError::MissingConfig { .. }),
            "expected MissingConfig, got {err:?}"
        );
    }

    #[test]
    fn async_kit_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AsyncKit>();
    }

    // --- MED-002: Send-ness assertions for TraitKitError and build() result ---

    /// Verifies HIGH-001: `TraitKitError` is `Send` (so it can cross
    /// `tokio::spawn` boundaries). Before HIGH-001, `TraitKitError::BuildFailed::source`
    /// was `Box<dyn Error>` (without `+ Send`), which made the entire enum
    /// `!Send` and blocked `tokio::spawn(async move { kit.build().await })`.
    #[test]
    fn kit_error_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<TraitKitError>();
    }

    /// Verifies HIGH-001: `AsyncKit::build()`'s return type is `Send`, so the
    /// spawned future's output satisfies `tokio::spawn`'s `Send` requirement
    /// on a multi-threaded runtime.
    #[test]
    fn async_kit_build_result_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Result<AsyncKit<Ready>, TraitKitError>>();
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
        kit.register::<MockModule>().expect("register module A");
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
        let err =
            block_on(kit.build()).expect_err("build should fail when a dependency is unregistered");
        assert!(
            matches!(
                err,
                TraitKitError::DependencyMissing {
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
        kit.register::<MockCycleA>().expect("register cycle A");
        kit.register::<MockCycleB>().expect("register cycle B");
        let err = block_on(kit.build()).expect_err("build should fail on cyclic dependency graph");
        assert!(
            matches!(err, TraitKitError::CycleDetected { .. }),
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
        let err =
            block_on(kit.build()).expect_err("build should fail when module build returns Err");
        match &err {
            TraitKitError::BuildFailed { context, .. } => {
                assert_eq!(
                    context.as_str(),
                    "mock-err-module",
                    "expected BuildFailed for mock-err-module, got {err:?}"
                );
            }
            _ => panic!("expected BuildFailed for mock-err-module, got {err:?}"),
        }
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
            matches!(err, TraitKitError::MissingCapability { ref key } if key == "mock-module"),
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
    // `From<TraitKitError> for MockError` lets `?` convert require errors
    // (matches the production pattern in design.md where DbNexusModule
    // uses `kit.require::<OxcacheModule>()?` with `OxcacheError: From<TraitKitError>`).

    impl From<TraitKitError> for MockError {
        fn from(e: TraitKitError) -> Self {
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
            static DEPS: &[(&str, TypeId)] = &[("mock-chain-b", TypeId::of::<MockChainBModule>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockChainAModule {
        type Capability = Arc<ChainAcap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
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

    /// R-004 #3: Missing dependency → `TraitKitError::DependencyMissing`.
    /// Register only `MockAModule` (declares dep on `MockBModule`); `MockBModule`
    /// is intentionally unregistered. `graph.validate()` must reject before
    /// any `build_fn` runs.
    #[test]
    fn async_kit_di_missing_dependency_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockAModule>()
            .expect("register A only (B missing)");
        let err =
            block_on(kit.build()).expect_err("build must fail when declared dep is unregistered");
        assert!(
            matches!(
                err,
                TraitKitError::DependencyMissing {
                    module: "mock-a",
                    missing: "mock-b"
                }
            ),
            "expected DependencyMissing {{ module: \"mock-a\", missing: \"mock-b\" }}, got {err:?}"
        );
    }

    /// R-004 #4: 3-node cycle A→B→C→A → `TraitKitError::CycleDetected`.
    /// Distinct from the 2-node cycle test (T008) — exercises DFS cycle
    /// extraction on a longer ring.
    #[test]
    fn async_kit_di_three_node_cycle_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockCycleA3>().expect("register cycle A3");
        kit.register::<MockCycleB3>().expect("register cycle B3");
        kit.register::<MockCycleC3>().expect("register cycle C3");
        let err = block_on(kit.build()).expect_err("build must fail on 3-node cycle");
        assert!(
            matches!(err, TraitKitError::CycleDetected { .. }),
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
        kit.register::<MockChainBModule>()
            .expect("register chain-B");
        kit.register::<MockChainAModule>()
            .expect("register chain-A");
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

    /// Trigger `From<TraitKitError> for MockError` by having a module
    /// require an unregistered module during build.
    #[test]
    fn async_from_trait_kit_error_for_mock_error() {
        struct RequireMissingModule;
        impl ModuleMeta for RequireMissingModule {
            const NAME: &'static str = "require-missing";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AsyncAutoBuilder for RequireMissingModule {
            type Capability = Arc<()>;
            type Error = MockError;
            fn build<'a>(
                kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
                Box::pin(async move {
                    // This require will fail — MockBModule is not registered.
                    // The `?` triggers From<TraitKitError> for MockError.
                    let _b: Arc<Bcap> = kit.require::<MockBModule>()?;
                    Ok(Arc::new(()))
                })
            }
        }

        let mut kit = AsyncKit::new();
        kit.register::<RequireMissingModule>().unwrap();
        let result = block_on(kit.build());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TraitKitError::BuildFailed { .. }
        ));
    }

    // Direct build calls for error-path fixtures to cover their build fns.

    #[test]
    fn async_mock_missing_dep_module_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockMissingDepModule::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_a_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleA::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_b_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleB::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_a3_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleA3::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_b3_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleB3::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_c3_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleC3::build(&kit));
        assert!(result.is_ok());
    }
}

// ─── Feature-gated async integration tests ────────────────────────────────

#[cfg(all(test, feature = "lifecycle"))]
mod async_lifecycle_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::core::lifecycle::AsyncLifecycle;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ASYNC_LC_READY: AtomicUsize = AtomicUsize::new(0);

    struct AsyncLcModule;
    impl ModuleMeta for AsyncLcModule {
        const NAME: &'static str = "async-lc";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncLcModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }
    impl AsyncLifecycle for AsyncLcModule {
        fn on_ready<'a>(
            _kit: &'a AsyncKit<Ready>,
        ) -> Pin<Box<dyn Future<Output = Result<(), MockError>> + Send + 'a>> {
            Box::pin(async {
                ASYNC_LC_READY.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    #[test]
    fn async_lifecycle_on_ready_called() {
        let before = ASYNC_LC_READY.load(Ordering::SeqCst);
        let mut kit = AsyncKit::new();
        kit.register::<AsyncLcModule>().unwrap();
        kit.register_lifecycle::<AsyncLcModule>();
        let _built = block_on(kit.build()).unwrap();
        let after = ASYNC_LC_READY.load(Ordering::SeqCst);
        assert!(
            after > before,
            "on_ready should have been called at least once: before={before}, after={after}"
        );
    }

    #[test]
    fn async_shutdown_does_not_panic() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncLcModule>().unwrap();
        kit.register_lifecycle::<AsyncLcModule>();
        let built = block_on(kit.build()).unwrap();
        built.shutdown();
    }
}

#[cfg(all(test, feature = "health"))]
mod async_health_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::core::health::{AsyncHealthCheck, HealthStatus};
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct AsyncHcCap {
        val: i32,
    }

    struct AsyncHcModule;
    impl ModuleMeta for AsyncHcModule {
        const NAME: &'static str = "async-hc";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncHcModule {
        type Capability = Arc<AsyncHcCap>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AsyncHcCap>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(AsyncHcCap { val: 42 })) })
        }
    }
    impl AsyncHealthCheck for AsyncHcModule {
        fn check(cap: &Arc<AsyncHcCap>) -> HealthStatus {
            if cap.val > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "zero".into(),
                }
            }
        }
    }

    #[test]
    fn async_health_check_queryable() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncHcModule>().unwrap();
        kit.register_health_check::<AsyncHcModule>();
        let built = block_on(kit.build()).unwrap();
        let status = built.health_check::<AsyncHcModule>().unwrap();
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn async_health_report_returns_all() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncHcModule>().unwrap();
        kit.register_health_check::<AsyncHcModule>();
        let built = block_on(kit.build()).unwrap();
        let report = built.health_report();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].0, "async-hc");
    }

    #[test]
    fn async_health_check_unregistered_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncHcModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let err = built.health_check::<AsyncHcModule>().unwrap_err();
        assert!(matches!(err, TraitKitError::MissingConfig { .. }));
    }
}

#[cfg(test)]
mod async_observability_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::core::observer::BuildObserver;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct AsyncObs {
        start: Arc<AtomicUsize>,
        built: Arc<AtomicUsize>,
    }
    impl BuildObserver for AsyncObs {
        fn on_module_start(&self, _: &'static str) {
            self.start.fetch_add(1, Ordering::SeqCst);
        }
        fn on_module_built(&self, _: &'static str, _: Duration) {
            self.built.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct AsyncObsModule;
    impl ModuleMeta for AsyncObsModule {
        const NAME: &'static str = "async-obs";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncObsModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_observer_callbacks_fired() {
        let start = Arc::new(AtomicUsize::new(0));
        let built_count = Arc::new(AtomicUsize::new(0));
        let obs = Arc::new(AsyncObs {
            start: Arc::clone(&start),
            built: Arc::clone(&built_count),
        });
        let mut kit = AsyncKit::new();
        kit.with_observer(obs);
        kit.register::<AsyncObsModule>().unwrap();
        block_on(kit.build()).unwrap();
        assert_eq!(start.load(Ordering::SeqCst), 1);
        assert_eq!(built_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn async_observer_on_build_error_called() {
        struct AsyncFailObs {
            errors: Arc<AtomicUsize>,
        }
        impl BuildObserver for AsyncFailObs {
            fn on_build_error(&self, _: &'static str, _: &TraitKitError) {
                self.errors.fetch_add(1, Ordering::SeqCst);
            }
        }

        struct AsyncFailBuildModule;
        impl ModuleMeta for AsyncFailBuildModule {
            const NAME: &'static str = "async-fail-build";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AsyncAutoBuilder for AsyncFailBuildModule {
            type Capability = Arc<()>;
            type Error = MockError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
                Box::pin(async move { Err(MockError::Failed("intentional".into())) })
            }
        }

        let errors = Arc::new(AtomicUsize::new(0));
        let obs = Arc::new(AsyncFailObs {
            errors: Arc::clone(&errors),
        });
        let mut kit = AsyncKit::new();
        kit.with_observer(obs);
        kit.register::<AsyncFailBuildModule>().unwrap();
        let result = block_on(kit.build());
        assert!(result.is_err());
        assert_eq!(
            errors.load(Ordering::SeqCst),
            1,
            "on_build_error should fire"
        );
    }
}

#[cfg(test)]
mod async_factory_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ASYNC_FACTORY_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct AsyncFactoryModule;
    impl ModuleMeta for AsyncFactoryModule {
        const NAME: &'static str = "async-factory";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncFactoryModule {
        type Capability = Arc<AtomicUsize>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AtomicUsize>, MockError>> + Send + 'a>>
        {
            Box::pin(async move {
                let n = ASYNC_FACTORY_COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(AtomicUsize::new(n)))
            })
        }
    }

    #[test]
    fn async_factory_creates_new_instances() {
        ASYNC_FACTORY_COUNT.store(0, Ordering::SeqCst);
        let mut kit = AsyncKit::new();
        kit.register::<AsyncFactoryModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let factory = built.factory::<AsyncFactoryModule>();
        let cap1 = block_on(factory());
        let cap2 = block_on(factory());
        assert!(cap1.is_ok());
        assert!(cap2.is_ok());
        assert_ne!(
            cap1.unwrap().load(Ordering::SeqCst),
            cap2.unwrap().load(Ordering::SeqCst)
        );
    }
}

#[cfg(all(test, feature = "scope"))]
mod async_scope_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    struct AsyncScopeMockModule;
    impl ModuleMeta for AsyncScopeMockModule {
        const NAME: &'static str = "async-scope-mock";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncScopeMockModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_create_scope_returns_empty() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncScopeMockModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let scope = built.create_scope();
        assert!(!scope.contains::<AsyncScopeMockModule>());
    }
}

#[cfg(test)]
mod async_conditional_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    struct AsyncCondMockModule;
    impl ModuleMeta for AsyncCondMockModule {
        const NAME: &'static str = "async-cond-mock";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncCondMockModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_register_if_true() {
        let mut kit = AsyncKit::new();
        let registered = kit.register_if::<AsyncCondMockModule>(|_| true).unwrap();
        assert!(registered);
        let built = block_on(kit.build()).unwrap();
        assert!(built.contains::<AsyncCondMockModule>());
    }

    #[test]
    fn async_register_if_false() {
        let mut kit = AsyncKit::new();
        let registered = kit.register_if::<AsyncCondMockModule>(|_| false).unwrap();
        assert!(!registered);
        let built = block_on(kit.build()).unwrap();
        assert!(!built.contains::<AsyncCondMockModule>());
    }
}

#[cfg(all(test, feature = "decorator"))]
mod async_decorator_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct AsyncDecCap {
        val: String,
    }

    struct AsyncDecModule;
    impl ModuleMeta for AsyncDecModule {
        const NAME: &'static str = "async-dec";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncDecModule {
        type Capability = Arc<AsyncDecCap>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AsyncDecCap>, MockError>> + Send + 'a>>
        {
            Box::pin(async move { Ok(Arc::new(AsyncDecCap { val: "base".into() })) })
        }
    }

    #[test]
    fn async_decorate_registers() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncDecModule>().unwrap();
        kit.decorate::<AsyncDecModule>(|cap| {
            Arc::new(AsyncDecCap {
                val: format!("{}+wrapped", cap.val),
            })
        });
        let built = block_on(kit.build()).unwrap();
        let cap = built.require::<AsyncDecModule>().unwrap();
        assert!(!cap.val.is_empty());
    }
}

// ─── AsyncKit<Ready> surface tests ─────────────────────────────────────

#[cfg(all(test, feature = "async"))]
mod async_ready_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    struct AsyncReadyMockModule;
    impl ModuleMeta for AsyncReadyMockModule {
        const NAME: &'static str = "async-ready-mock";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncReadyMockModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_debug_unbuilt_format() {
        let kit = AsyncKit::new();
        let debug = format!("{kit:?}");
        assert!(debug.contains("AsyncKit<Unbuilt>"));
    }

    #[test]
    fn async_debug_ready_format() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncReadyMockModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let debug = format!("{built:?}");
        assert!(debug.contains("AsyncKit<Ready>"));
    }

    #[test]
    fn async_default_creates_empty() {
        let kit = AsyncKit::default();
        let built = block_on(kit.build()).unwrap();
        assert_eq!(built.graph.entries().len(), 0);
    }

    #[test]
    fn async_graph_dot_works() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncReadyMockModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let dot = built.graph_dot();
        assert!(dot.contains("digraph"));
    }

    #[test]
    fn async_graph_mermaid_works() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncReadyMockModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let mermaid = built.graph_mermaid();
        assert!(mermaid.contains("graph TD"));
    }

    #[test]
    fn async_config_missing_returns_error() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).unwrap();
        let err = built.config::<i32>().unwrap_err();
        assert!(matches!(err, TraitKitError::MissingConfig { .. }));
    }

    #[test]
    fn async_build_missing_dep_returns_error() {
        struct AsyncNeedsDep;
        impl ModuleMeta for AsyncNeedsDep {
            const NAME: &'static str = "async-needs-dep";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] = &[("dep", TypeId::of::<AsyncReadyMockModule>())];
                DEPS
            }
        }
        impl AsyncAutoBuilder for AsyncNeedsDep {
            type Capability = Arc<()>;
            type Error = MockError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
                Box::pin(async move { Ok(Arc::new(())) })
            }
        }

        let mut kit = AsyncKit::new();
        kit.register::<AsyncNeedsDep>().unwrap();
        let result = block_on(kit.build());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TraitKitError::DependencyMissing { .. }
        ));
    }

    #[test]
    fn async_optional_returns_none_for_unbuilt() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).unwrap();
        assert!(built.optional::<AsyncReadyMockModule>().is_none());
    }
}
