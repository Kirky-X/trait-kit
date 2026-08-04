// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Scoped dependency container for per-request instance isolation.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::core::{AutoBuilder, BuildFn};
use crate::error::TraitKitError;
use crate::kit::TypeMap;

use super::kit::LazySlot;

/// Scoped dependency container for per-request instance isolation.
///
/// A `Scope` is a lightweight container that can register a subset of
/// modules and build them independently from the main `Kit`. Each scope
/// creates its own instances — useful for per-request isolation in web
/// servers, where each request gets its own scope with fresh instances.
///
/// Requires the `scope` feature.
#[cfg(feature = "scope")]
pub struct Scope {
    capabilities: TypeMap,
    lazy_slots: RefCell<HashMap<TypeId, LazySlot>>,
}

#[cfg(feature = "scope")]
impl Scope {
    /// Create a new empty scope.
    #[must_use]
    pub fn new() -> Self {
        Scope {
            capabilities: TypeMap::new(),
            lazy_slots: RefCell::new(HashMap::new()),
        }
    }

    /// Register a module factory in this scope.
    ///
    /// The module's `build_fn` is stored but not invoked until `require()`
    /// is called (lazy construction within the scope).
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::AlreadyRegistered` if the module type was
    /// already registered in this scope.
    pub fn register<M: AutoBuilder>(&mut self) -> Result<(), TraitKitError> {
        let type_id = TypeId::of::<M>();
        if self.lazy_slots.borrow().contains_key(&type_id) {
            return Err(TraitKitError::AlreadyRegistered { module: M::NAME });
        }

        let build_fn: BuildFn = Box::new(|kit| {
            let cap = M::build(kit)
                .map_err(|e| -> Box<dyn std::error::Error + Send + 'static> { Box::new(e) })?;
            Ok(Box::new(cap) as Box<dyn std::any::Any>)
        });

        self.lazy_slots.borrow_mut().insert(
            type_id,
            LazySlot {
                builder: Some(build_fn),
                cell: OnceLock::new(),
            },
        );

        Ok(())
    }

    /// Retrieve a module's capability from this scope.
    ///
    /// On first access, the module's build function is invoked with a
    /// temporary empty `Kit` and the result is cached. Subsequent calls
    /// return the cached value.
    ///
    /// Note: scoped modules cannot access parent Kit capabilities or configs.
    /// They must be self-contained.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingCapability` if the module was not
    /// registered in this scope. Returns `TraitKitError::BuildFailed` if
    /// the build function fails.
    pub fn require<M: AutoBuilder>(&self) -> Result<M::Capability, TraitKitError> {
        let type_id = TypeId::of::<M>();

        // Check if already built in this scope
        if let Some(cap) = self
            .capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
        {
            return Ok(cap);
        }

        // Check lazy slots
        if let Some(boxed) = self
            .lazy_slots
            .borrow()
            .get(&type_id)
            .and_then(|slot| slot.cell.get())
        {
            return boxed
                .downcast_ref::<M::Capability>()
                .cloned()
                .ok_or(TraitKitError::MissingCapability { key: M::NAME });
        }

        // First-access construction
        let builder = self
            .lazy_slots
            .borrow_mut()
            .get_mut(&type_id)
            .and_then(|slot| slot.builder.take());

        if let Some(builder) = builder {
            // Create a minimal empty Kit for the build callback.
            let temp_kit = crate::kit::Kit::new();
            let boxed = (builder)(&temp_kit).map_err(|e| TraitKitError::BuildFailed {
                context: M::NAME,
                source: e,
            })?;
            if let Some(slot) = self.lazy_slots.borrow().get(&type_id) {
                let _ = slot.cell.set(boxed);
            }
            return self
                .lazy_slots
                .borrow()
                .get(&type_id)
                .and_then(|slot| slot.cell.get())
                .and_then(|b| b.downcast_ref::<M::Capability>().cloned())
                .ok_or(TraitKitError::MissingCapability { key: M::NAME });
        }

        Err(TraitKitError::MissingCapability { key: M::NAME })
    }

    /// Check if a module type is registered in this scope.
    #[must_use]
    pub fn contains<M: AutoBuilder>(&self) -> bool {
        let type_id = TypeId::of::<M>();
        self.lazy_slots.borrow().contains_key(&type_id)
            || self.capabilities.contains_by_type_id(type_id)
    }
}

#[cfg(feature = "scope")]
impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "scope")]
impl Drop for Scope {
    fn drop(&mut self) {
        // Explicitly clear all stored instances and lazy slots.
        self.capabilities.clear();
        self.lazy_slots.borrow_mut().clear();
    }
}

// ─── AsyncScope ─────────────────────────────────────────────────────────────

#[cfg(all(feature = "scope", feature = "async"))]
mod async_scope {
    use std::any::TypeId;
    use std::collections::HashSet;
    use std::sync::{Arc, RwLock};

    use crate::core::AsyncAutoBuilder;
    use crate::error::TraitKitError;
    use crate::kit::AsyncTypeMap;

    /// Async scoped dependency container (Send + Sync).
    ///
    /// Multi-threaded counterpart to [`super::Scope`]. Uses `Arc<RwLock>` for
    /// interior mutability.
    pub struct AsyncScope {
        capabilities: AsyncTypeMap,
        builders: Arc<RwLock<HashSet<TypeId>>>,
    }

    impl AsyncScope {
        /// Create a new empty async scope.
        #[must_use]
        pub fn new() -> Self {
            AsyncScope {
                capabilities: AsyncTypeMap::new(),
                builders: Arc::new(RwLock::new(HashSet::new())),
            }
        }

        /// Register a module factory in this async scope.
        ///
        /// # Errors
        ///
        /// Returns `TraitKitError::AlreadyRegistered` if the module was
        /// already registered.
        ///
        /// # Panics
        ///
        /// Panics if the internal `RwLock` is poisoned.
        pub fn register<M: AsyncAutoBuilder>(&mut self) -> Result<(), TraitKitError> {
            let type_id = TypeId::of::<M>();
            if self
                .builders
                .read()
                .expect("lock poisoned")
                .contains(&type_id)
            {
                return Err(TraitKitError::AlreadyRegistered { module: M::NAME });
            }

            self.builders
                .write()
                .expect("lock poisoned")
                .insert(type_id);
            Ok(())
        }

        /// Check if a module type is registered in this scope.
        ///
        /// # Panics
        ///
        /// Panics if the internal `RwLock` is poisoned.
        #[must_use]
        pub fn contains<M: AsyncAutoBuilder>(&self) -> bool {
            let type_id = TypeId::of::<M>();
            self.capabilities.contains_by_type_id(type_id)
                || self
                    .builders
                    .read()
                    .expect("lock poisoned")
                    .contains(&type_id)
        }
    }

    impl Default for AsyncScope {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(all(feature = "scope", feature = "async"))]
pub use async_scope::AsyncScope;

#[cfg(all(test, feature = "scope"))]
mod tests {
    use super::*;
    use crate::core::{AutoBuilder, ModuleMeta};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SCOPE_COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug, Clone)]
    struct ScopeCap {
        id: usize,
    }

    #[derive(Debug)]
    struct ScopeTestError;

    impl std::fmt::Display for ScopeTestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "scope error")
        }
    }

    impl std::error::Error for ScopeTestError {}

    struct ScopeModule;

    impl ModuleMeta for ScopeModule {
        const NAME: &'static str = "scope-module";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AutoBuilder for ScopeModule {
        type Capability = Arc<ScopeCap>;
        type Error = ScopeTestError;

        fn build(_kit: &crate::kit::Kit) -> Result<Arc<ScopeCap>, ScopeTestError> {
            let id = SCOPE_COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(ScopeCap { id }))
        }
    }

    #[test]
    fn scope_new_is_empty() {
        let scope = Scope::new();
        assert!(!scope.contains::<ScopeModule>());
    }

    #[test]
    fn scope_register_then_require() {
        SCOPE_COUNTER.store(0, Ordering::SeqCst);
        let mut scope = Scope::new();
        scope.register::<ScopeModule>().expect("register should succeed");
        assert!(scope.contains::<ScopeModule>());

        let cap = scope.require::<ScopeModule>().expect("require should succeed");
        assert_eq!(cap.id, 0);
    }

    #[test]
    fn scope_require_caches_result() {
        SCOPE_COUNTER.store(100, Ordering::SeqCst);
        let mut scope = Scope::new();
        scope.register::<ScopeModule>().expect("register");

        let cap1 = scope.require::<ScopeModule>().expect("require 1");
        let cap2 = scope.require::<ScopeModule>().expect("require 2");
        assert_eq!(cap1.id, cap2.id, "scope should cache the built instance");
        assert_eq!(
            SCOPE_COUNTER.load(Ordering::SeqCst),
            101,
            "builder should be invoked exactly once"
        );
    }

    #[test]
    fn scope_register_duplicate_returns_error() {
        let mut scope = Scope::new();
        scope.register::<ScopeModule>().expect("first register");
        let err = scope.register::<ScopeModule>().unwrap_err();
        assert!(matches!(
            err,
            TraitKitError::AlreadyRegistered {
                module: "scope-module"
            }
        ));
    }

    #[test]
    fn scope_require_unregistered_returns_missing() {
        let scope = Scope::new();
        let err = scope.require::<ScopeModule>().unwrap_err();
        assert!(matches!(
            err,
            TraitKitError::MissingCapability {
                key: "scope-module"
            }
        ));
    }

    #[test]
    fn scope_default_creates_empty() {
        let scope = Scope::default();
        assert!(!scope.contains::<ScopeModule>());
    }

    #[test]
    fn scope_drop_clears_resources() {
        let mut scope = Scope::new();
        scope.register::<ScopeModule>().expect("register");
        assert!(scope.contains::<ScopeModule>());
        drop(scope);
        // After drop, the scope is gone — no panic
    }
}

#[cfg(all(test, feature = "scope", feature = "async"))]
mod async_tests {
    use super::*;
    use crate::core::{AsyncAutoBuilder, ModuleMeta};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    #[derive(Debug, Clone, PartialEq)]
    struct AsyncScopeCap {
        value: i32,
    }

    #[derive(Debug)]
    struct AsyncScopeError;

    impl std::fmt::Display for AsyncScopeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "async scope error")
        }
    }

    impl std::error::Error for AsyncScopeError {}

    struct AsyncScopeModule;

    impl ModuleMeta for AsyncScopeModule {
        const NAME: &'static str = "async-scope-module";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for AsyncScopeModule {
        type Capability = Arc<AsyncScopeCap>;
        type Error = AsyncScopeError;

        fn build<'a>(
            _kit: &'a crate::kit::AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AsyncScopeCap>, AsyncScopeError>> + Send + 'a>>
        {
            Box::pin(async move { Ok(Arc::new(AsyncScopeCap { value: 99 })) })
        }
    }

    #[test]
    fn async_scope_new_is_empty() {
        let scope = AsyncScope::new();
        assert!(!scope.contains::<AsyncScopeModule>());
    }

    #[test]
    fn async_scope_register_then_contains() {
        let mut scope = AsyncScope::new();
        scope.register::<AsyncScopeModule>().expect("register");
        assert!(scope.contains::<AsyncScopeModule>());
    }

    #[test]
    fn async_scope_register_duplicate_returns_error() {
        let mut scope = AsyncScope::new();
        scope.register::<AsyncScopeModule>().expect("first register");
        let err = scope.register::<AsyncScopeModule>().unwrap_err();
        assert!(matches!(
            err,
            TraitKitError::AlreadyRegistered {
                module: "async-scope-module"
            }
        ));
    }

    #[test]
    fn async_scope_default_is_empty() {
        let scope = AsyncScope::default();
        assert!(!scope.contains::<AsyncScopeModule>());
    }
}
