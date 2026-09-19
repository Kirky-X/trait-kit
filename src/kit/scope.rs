// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Scoped dependency container for per-request instance isolation.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::core::AutoBuilder;
use crate::error::TraitKitError;

use super::kit::{Kit, LazyBuildFn, LazySlot, Ready};

/// Scoped dependency container for per-request instance isolation.
///
/// A `Scope` is a lightweight container that can register a subset of
/// modules and build them independently from the main `Kit`. Each scope
/// creates its own instances — useful for per-request isolation in web
/// servers, where each request gets its own scope with fresh instances.
///
/// # Parent context
///
/// A `Scope` can optionally hold a **read-only** handle to a parent
/// `Kit<Ready>` (created via
/// [`Kit::create_scope_from`](crate::kit::Kit::create_scope_from)). The handle
/// is a `Weak` reference: a scope never keeps its parent alive, which is the
/// cycle guard — a parent that owns scopes cannot be retained by them past
/// its own lifetime. `scope.parent::<M>()` resolves capabilities from the
/// parent without cloning them into the scope; it returns `None` once the
/// parent is gone (or when the module is absent there).
///
/// # Thread safety
///
/// `Scope` uses `RefCell` for interior mutability and is therefore
/// `!Send + !Sync`. It is designed for single-threaded, per-request use.
/// For a thread-safe async counterpart, see [`AsyncScope`].
///
/// Requires the `request-scope` feature.
#[cfg(feature = "request-scope")]
pub struct Scope {
    lazy_slots: RefCell<HashMap<TypeId, LazySlot>>,
    /// Optional parent context: `Weak` on purpose (cycle guard — the scope
    /// never prolongs the parent Kit's lifetime).
    parent: Option<std::rc::Weak<Kit<Ready>>>,
}

#[cfg(feature = "request-scope")]
impl Scope {
    /// Create a new empty scope.
    #[must_use]
    pub fn new() -> Self {
        Scope {
            lazy_slots: RefCell::new(HashMap::new()),
            parent: None,
        }
    }

    /// Create a new empty scope with a parent context.
    ///
    /// Only holds a `Weak` reference to the parent: dropping the parent Kit
    /// invalidates [`parent`](Scope::parent) queries (returns `None`) instead
    /// of panicking or keeping the parent alive — this is the cycle guard.
    #[must_use]
    pub fn with_parent(parent: std::rc::Weak<Kit<Ready>>) -> Self {
        Scope {
            lazy_slots: RefCell::new(HashMap::new()),
            parent: Some(parent),
        }
    }

    /// Read-only query into the parent context.
    ///
    /// Resolves `M`'s capability from the parent `Kit<Ready>` if one exists.
    /// The scope never caches or mutates parent state. Returns `None` when:
    /// the scope has no parent, the parent has been dropped (weak reference
    /// expired — cycle guard), or the parent has no capability for `M`.
    ///
    /// Scoped modules themselves still build against a self-contained empty
    /// `Kit`; use this method to *explicitly* pull in parent singletons.
    #[must_use]
    pub fn parent<M: AutoBuilder>(&self) -> Option<M::Capability> {
        let kit = self.parent.as_ref()?.upgrade()?;
        kit.optional::<M>()
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

        let build_fn: LazyBuildFn = Box::new(|kit| {
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

        // Check lazy slots first (the primary cache path)
        if let Some(boxed) = self
            .lazy_slots
            .borrow()
            .get(&type_id)
            .and_then(|slot| slot.cell.get())
        {
            return boxed.downcast_ref::<M::Capability>().cloned().ok_or(
                TraitKitError::MissingCapability {
                    key: M::NAME.to_string(),
                },
            );
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
            // `LazyBuildFn` is an `Fn` closure: the call only borrows it, so
            // the same builder remains available for the failure path below.
            let boxed = match builder(&temp_kit) {
                Ok(boxed) => boxed,
                Err(e) => {
                    // Restore the builder so the slot stays retryable: a
                    // failed build must not degrade later require() calls
                    // into a permanent MissingCapability.
                    if let Some(slot) = self.lazy_slots.borrow_mut().get_mut(&type_id) {
                        slot.builder = Some(builder);
                    }
                    return Err(TraitKitError::BuildFailed {
                        context: M::NAME.to_string(),
                        source: e,
                    });
                }
            };
            if let Some(slot) = self.lazy_slots.borrow().get(&type_id) {
                // If the cell is already set (e.g. from a re-entrant call),
                // the existing value is kept.
                let _ = slot.cell.set(boxed);
            }
            return self
                .lazy_slots
                .borrow()
                .get(&type_id)
                .and_then(|slot| slot.cell.get())
                .and_then(|b| b.downcast_ref::<M::Capability>().cloned())
                .ok_or(TraitKitError::MissingCapability {
                    key: M::NAME.to_string(),
                });
        }

        Err(TraitKitError::MissingCapability {
            key: M::NAME.to_string(),
        })
    }

    /// Check if a module type is registered in this scope.
    #[must_use]
    pub fn contains<M: AutoBuilder>(&self) -> bool {
        let type_id = TypeId::of::<M>();
        self.lazy_slots.borrow().contains_key(&type_id)
    }
}

#[cfg(feature = "request-scope")]
impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

// NOTE: `Scope` needs no `Drop` impl — `lazy_slots` is a plain
// `RefCell<HashMap>` that frees itself on drop; an explicit `clear()` would
// be redundant.

// ─── AsyncScope ─────────────────────────────────────────────────────────────

#[cfg(all(feature = "request-scope", feature = "async"))]
mod async_scope {
    use std::any::TypeId;

    use crate::core::AsyncAutoBuilder;
    use crate::error::TraitKitError;
    use crate::kit::AsyncTypeMap;

    /// Async scoped dependency container (`Send + Sync`).
    ///
    /// Multi-threaded counterpart to [`super::Scope`]. Uses [`AsyncTypeMap`]
    /// for interior mutability.
    ///
    /// # Design differences from `Scope`
    ///
    /// Unlike the synchronous `Scope` (which lazily builds modules on first
    /// `require()`), `AsyncScope` does **not** store async build functions or
    /// perform lazy construction. This is because `AsyncAutoBuilder::build`
    /// returns a `Future` whose lifetime is bound to the `AsyncKit` reference,
    /// making it impossible to store in the scope. `insert()` is therefore the
    /// single entry point: it stores a pre-built capability, `contains()`
    /// reports exactly what was inserted, and `require()` retrieves it.
    pub struct AsyncScope {
        capabilities: AsyncTypeMap,
    }

    impl AsyncScope {
        /// Create a new empty async scope.
        #[must_use]
        pub fn new() -> Self {
            AsyncScope {
                capabilities: AsyncTypeMap::new(),
            }
        }

        /// Insert a pre-built capability into this scope.
        ///
        /// This is the only way to populate the scope: capabilities are built
        /// externally (e.g. from an `AsyncKit` or a manual async build) and
        /// stored here. The `require()` method retrieves these values.
        /// Inserting the same module type again replaces the stored value.
        ///
        /// # Example
        ///
        /// ```ignore
        /// let cap = MyModule::build(&kit).await?;
        /// scope.insert::<MyModule>(cap);
        /// let retrieved = scope.require::<MyModule>()?;
        /// ```
        pub fn insert<M: AsyncAutoBuilder>(&self, capability: M::Capability)
        where
            M::Capability: Send + Sync + 'static,
        {
            self.capabilities
                .insert_boxed(TypeId::of::<M>(), Box::new(capability));
        }

        /// Retrieve a module's capability from this scope.
        ///
        /// The capability must have been previously inserted via `insert()`.
        /// Async modules cannot be lazily built inside a scope because
        /// `AsyncAutoBuilder::build` requires an `AsyncKit` reference and
        /// returns a future whose lifetime is bound to that reference.
        ///
        /// # Errors
        ///
        /// Returns `TraitKitError::MissingCapability` if the capability was
        /// not inserted into this scope.
        pub fn require<M: AsyncAutoBuilder>(&self) -> Result<M::Capability, TraitKitError>
        where
            M::Capability: Clone + Send + Sync + 'static,
        {
            let type_id = TypeId::of::<M>();
            self.capabilities
                .get_cloned_by_type_id::<M::Capability>(type_id)
                .ok_or(TraitKitError::MissingCapability {
                    key: M::NAME.to_string(),
                })
        }

        /// Check if a capability for this module type was inserted into this
        /// scope.
        ///
        /// `contains` reflects exactly the `insert()` history: `true` if and
        /// only if `insert::<M>()` has been called (and `require::<M>()` will
        /// then succeed).
        #[must_use]
        pub fn contains<M: AsyncAutoBuilder>(&self) -> bool {
            self.capabilities.contains_by_type_id(TypeId::of::<M>())
        }
    }

    impl Default for AsyncScope {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(all(feature = "request-scope", feature = "async"))]
pub use async_scope::AsyncScope;

#[cfg(all(test, feature = "request-scope"))]
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
            let id = SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed);
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
        let mut scope = Scope::new();
        scope
            .register::<ScopeModule>()
            .expect("register should succeed");
        assert!(scope.contains::<ScopeModule>());

        let cap = scope
            .require::<ScopeModule>()
            .expect("require should succeed");
        // 有意义断言：每个 Scope 各自构建出独立实例。id 来自进程内单调
        // 递增的共享计数器（并行测试会并发递增，不能断言绝对值），两个
        // Scope 各 require 一次得到两个 id 互不相同的实例，即证明构建了
        // 真实的独立实例而非占位值。
        let mut scope2 = Scope::new();
        scope2.register::<ScopeModule>().expect("register 2");
        let cap2 = scope2.require::<ScopeModule>().expect("require 2");
        assert_ne!(cap.id, cap2.id, "each scope builds its own instance");
    }

    #[test]
    fn scope_require_caches_result() {
        let mut scope = Scope::new();
        scope.register::<ScopeModule>().expect("register");

        let cap1 = scope.require::<ScopeModule>().expect("require 1");
        let cap2 = scope.require::<ScopeModule>().expect("require 2");
        assert_eq!(cap1.id, cap2.id, "scope should cache the built instance");
        // 两次 require 返回相同 id 即证明只构建了一次（无需检查全局计数器）
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
                ref key
            } if key == "scope-module"
        ));
    }

    /// 构建失败的 builder 必须被放回槽位：第二次 require 重新执行
    /// builder，返回同样的 BuildFailed（错误信息一致），而不是永久
    /// MissingCapability，原始错误也不丢失。
    #[test]
    fn scope_require_build_failure_is_retryable() {
        struct ScopeFailModule;
        impl ModuleMeta for ScopeFailModule {
            const NAME: &'static str = "scope-fail";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for ScopeFailModule {
            type Capability = Arc<ScopeCap>;
            type Error = ScopeTestError;
            fn build(_kit: &crate::kit::Kit) -> Result<Arc<ScopeCap>, ScopeTestError> {
                Err(ScopeTestError)
            }
        }

        let mut scope = Scope::new();
        scope.register::<ScopeFailModule>().expect("register");
        let err1 = scope.require::<ScopeFailModule>().unwrap_err();
        assert!(matches!(err1, TraitKitError::BuildFailed { .. }));

        let err2 = scope.require::<ScopeFailModule>().unwrap_err();
        assert!(
            matches!(err2, TraitKitError::BuildFailed { .. }),
            "第二次 require 应重试 builder 并返回 BuildFailed，got {err2:?}"
        );
        assert!(
            !matches!(err2, TraitKitError::MissingCapability { .. }),
            "第二次 require 不得退化为 MissingCapability"
        );
        assert_eq!(
            err1.to_string(),
            err2.to_string(),
            "两次 require 的错误信息应一致"
        );
    }

    /// 循环依赖的可观测行为：M 的构建内调 `kit.require::<N>()`，N 的
    /// 构建内调 `kit.require::<M>()`。Scope 的构建回调拿到的是临时空
    /// Kit，内层 require 得到 MissingCapability，向上包装为外层
    /// `require` 的 `BuildFailed` —— 外层返回 Err 而非 panic / 死循环。
    /// 注意：这是当前循环依赖的可观测行为；更精确的循环依赖诊断
    /// （如 CycleDetected）是未来工作。
    #[test]
    fn scope_circular_dependency_returns_error_not_panic() {
        struct CycM;
        impl ModuleMeta for CycM {
            const NAME: &'static str = "cyc-m";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for CycM {
            type Capability = Arc<ScopeCap>;
            type Error = ScopeTestError;
            fn build(kit: &crate::kit::Kit) -> Result<Arc<ScopeCap>, ScopeTestError> {
                // 循环的一侧：构建 M 时请求 N（空临时 Kit 上必然失败）。
                kit.require::<CycN>().map_err(|_| ScopeTestError)?;
                Ok(Arc::new(ScopeCap { id: 0 }))
            }
        }

        struct CycN;
        impl ModuleMeta for CycN {
            const NAME: &'static str = "cyc-n";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for CycN {
            type Capability = Arc<ScopeCap>;
            type Error = ScopeTestError;
            fn build(kit: &crate::kit::Kit) -> Result<Arc<ScopeCap>, ScopeTestError> {
                // 循环的另一侧：构建 N 时请求 M，构成完整循环。
                kit.require::<CycM>().map_err(|_| ScopeTestError)?;
                Ok(Arc::new(ScopeCap { id: 0 }))
            }
        }

        let mut scope = Scope::new();
        scope.register::<CycM>().expect("register cyc-m");
        let err = scope.require::<CycM>().unwrap_err();
        assert!(
            matches!(
                err,
                TraitKitError::BuildFailed { ref context, .. } if context == "cyc-m"
            ),
            "外层 require 应返回 BuildFailed（内层为 MissingCapability），got {err:?}"
        );
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

    #[test]
    fn scope_registrations_do_not_leak_across_scopes() {
        // 两个独立 Scope 之间必须完全隔离：在一个 scope 注册/构建的
        // 模块不得在另一个 scope 中可见（per-request 隔离的语义保证）。
        let mut scope_a = Scope::new();
        scope_a.register::<ScopeModule>().expect("register in A");
        assert!(scope_a.contains::<ScopeModule>());

        let mut scope_b = Scope::new();
        // B 未注册该模块：contains 与 require 都必须失败。
        assert!(!scope_b.contains::<ScopeModule>());
        let err = scope_b.require::<ScopeModule>().unwrap_err();
        assert!(matches!(
            err,
            TraitKitError::MissingCapability {
                ref key
            } if key == "scope-module"
        ));

        // A 与 B 各构建出自己的独立实例：id 不同即证明未共享缓存。
        let cap_a = scope_a.require::<ScopeModule>().expect("require A");
        scope_b.register::<ScopeModule>().expect("register in B");
        let cap_b = scope_b.require::<ScopeModule>().expect("require B");
        assert_ne!(cap_a.id, cap_b.id, "scopes must not share instances");
    }

    #[test]
    fn scope_test_error_display() {
        let e = ScopeTestError;
        assert_eq!(format!("{e}"), "scope error");
    }

    #[test]
    fn parent_query_resolves_parent_capability_and_shares_singleton() {
        use std::rc::Rc;
        let mut unbuilt = crate::kit::Kit::new();
        unbuilt.register::<ScopeModule>().expect("register");
        let kit = Rc::new(unbuilt.build().expect("build ok"));

        let scope_a = kit.create_scope_from();
        let scope_b = kit.create_scope_from();

        let from_a = scope_a.parent::<ScopeModule>().expect("parent has module");
        let from_b = scope_b.parent::<ScopeModule>().expect("parent has module");
        // Read-only query returns the parent's shared singleton (same Arc).
        assert!(
            Arc::ptr_eq(&from_a, &from_b),
            "parent query must resolve the parent singleton, not fresh instances"
        );

        // Scope-local instances remain independent from the parent's.
        let mut scoped = Scope::new();
        scoped.register::<ScopeModule>().expect("register scoped");
        let local = scoped.require::<ScopeModule>().expect("require scoped");
        assert!(
            !Arc::ptr_eq(&from_a, &local),
            "scope-local build must not alias the parent singleton"
        );
    }

    #[test]
    fn parent_query_none_without_parent_or_missing_module() {
        use std::rc::Rc;

        let scope = Scope::new();
        assert!(
            scope.parent::<ScopeModule>().is_none(),
            "scope without parent → None"
        );

        let kit = Rc::new(crate::kit::Kit::new().build().expect("build ok"));
        let scope = kit.create_scope_from();
        assert!(
            scope.parent::<ScopeModule>().is_none(),
            "parent exists but module absent → None"
        );
    }

    #[test]
    fn parent_query_is_cycle_guarded_weak_reference() {
        use std::rc::Rc;
        let mut unbuilt = crate::kit::Kit::new();
        unbuilt.register::<ScopeModule>().expect("register");
        let scope;
        {
            let kit = Rc::new(unbuilt.build().expect("build ok"));
            scope = kit.create_scope_from();
            assert!(
                scope.parent::<ScopeModule>().is_some(),
                "parent alive → query resolves"
            );
            // Parent dropped here; the scope must not keep it alive (Weak).
        }
        assert!(
            scope.parent::<ScopeModule>().is_none(),
            "dropped parent → None (weak cycle guard), no panic, no retain"
        );
    }

    #[test]
    fn scope_module_dependencies_empty() {
        let deps = ScopeModule::dependencies();
        assert!(deps.is_empty());
    }
}

#[cfg(all(test, feature = "request-scope", feature = "async"))]
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
    fn async_scope_insert_then_contains() {
        let scope = AsyncScope::new();
        // contains 只反映 insert 后的事实：insert 前 false，insert 后 true，
        // 且此时 require 必然成功（不再存在"注册了但 require 失败"的陷阱）。
        assert!(!scope.contains::<AsyncScopeModule>());
        scope.insert::<AsyncScopeModule>(Arc::new(AsyncScopeCap { value: 7 }));
        assert!(scope.contains::<AsyncScopeModule>());
        let retrieved = scope
            .require::<AsyncScopeModule>()
            .expect("require after insert");
        assert_eq!(retrieved.value, 7);
    }

    #[test]
    fn async_scope_insert_twice_replaces_value() {
        let scope = AsyncScope::new();
        scope.insert::<AsyncScopeModule>(Arc::new(AsyncScopeCap { value: 1 }));
        scope.insert::<AsyncScopeModule>(Arc::new(AsyncScopeCap { value: 2 }));
        assert!(scope.contains::<AsyncScopeModule>());
        let retrieved = scope
            .require::<AsyncScopeModule>()
            .expect("require after re-insert");
        assert_eq!(retrieved.value, 2, "re-insert must replace the value");
    }

    #[test]
    fn async_scope_default_is_empty() {
        let scope = AsyncScope::default();
        assert!(!scope.contains::<AsyncScopeModule>());
    }

    #[test]
    fn async_scope_insert_and_require() {
        let scope = AsyncScope::new();
        let cap = Arc::new(AsyncScopeCap { value: 42 });
        scope.insert::<AsyncScopeModule>(cap.clone());
        let retrieved = scope
            .require::<AsyncScopeModule>()
            .expect("require should succeed");
        assert_eq!(retrieved.value, 42);
    }

    #[test]
    fn async_scope_require_missing_returns_error() {
        let scope = AsyncScope::new();
        let err = scope.require::<AsyncScopeModule>().unwrap_err();
        assert!(matches!(
            err,
            TraitKitError::MissingCapability {
                ref key
            } if key == "async-scope-module"
        ));
    }

    #[test]
    fn async_scope_error_display() {
        let e = AsyncScopeError;
        assert_eq!(format!("{e}"), "async scope error");
    }

    #[test]
    fn async_scope_module_dependencies_empty() {
        let deps = AsyncScopeModule::dependencies();
        assert!(deps.is_empty());
    }
}
