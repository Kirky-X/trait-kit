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
use super::graph::{DependencyGraph, ModuleEntry};

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
#[allow(
    clippy::type_complexity,
    reason = "Pin<Box<dyn Future + Send>> is the canonical dyn-compatible async dispatch type; mirrors AsyncAutoBuilder::build"
)]
pub(crate) type AsyncBuildFn = Box<
    dyn for<'a> FnOnce(
        &'a AsyncKit,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Box<dyn Any + Send>, Box<dyn std::error::Error + Send>>>
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
                    .map_err(|e| -> Box<dyn std::error::Error + Send> { Box::new(e) })?;
                Ok(Box::new(cap) as Box<dyn Any + Send>)
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
}

impl Default for AsyncKit {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, feature = "async"))]
mod tests {
    use super::AsyncKit;
    use crate::core::error::KitError;
    use crate::core::meta::{AsyncAutoBuilder, ModuleMeta};
    use std::any::TypeId;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

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
}
