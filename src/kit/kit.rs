// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Kit — the capability and configuration management center.
//!
//! Uses typestate pattern: `Kit` (unbuilt) → `Kit<Ready>` (after `build()`).
//!
//! # Sync/Async 行为对照清单
//!
//! 修改任一侧时同步核对另一侧（async 侧为 `async_kit.rs`）：
//! 1. `build`：模块按拓扑序构建；`on_ready` 回调按拓扑序执行。
//! 2. `register_lifecycle`：幂等——重复注册同一模块为 no-op。
//! 3. lazy require：构建失败后 builder 放回槽位，可重试且错误信息稳定
//!    （lazy 为 sync-only surface，async 侧无对应路径）。
//! 4. `shutdown`：回调 drain（one-shot），二次调用为 no-op；async 清理走
//!    `shutdown_async()`。
//! 5. decorator：按 `decorator_module_to_cap` 映射应用（eager 与 lazy 一致；
//!    eager 未映射时回退模块 `TypeId`，该差异由 e2e DEC-07 冻结）。
//! 6. `factory`：typestate cast + 编译期 size/align 布局断言。

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(feature = "reload")]
use std::rc::Rc;
use std::sync::OnceLock;

use crate::core::{AutoBuilder, BuildFn};
use crate::error::TraitKitError;
use crate::i18n::tr;

#[cfg(feature = "encryption")]
use super::EncryptedBlob;
use super::TypeMap;
use super::{DependencyGraph, GraphError, ModuleEntry};
#[cfg(feature = "toggle")]
use super::toggle::ToggleBackend;

#[cfg(feature = "lifecycle")]
type ShutdownCallback = Box<dyn Fn(&TypeMap)>;
#[cfg(feature = "lifecycle")]
type ReadyCallback = Box<dyn Fn(&Kit<Ready>) -> Result<(), TraitKitError>>;
#[cfg(feature = "health")]
type HealthCheckerFn = Box<dyn Fn(&TypeMap) -> crate::core::health::HealthStatus>;
#[cfg(feature = "observer")]
type ObserverRef = std::sync::Arc<dyn crate::core::observer::BuildObserver>;
/// Per-module snapshot of the registered build observers, consumed by the
/// `notify_*` helpers in `build_eager_modules()`. Both cfg arms keep the
/// same (function) signature so the eager-build loop body can be written
/// exactly once: under `observer` it is a cloned `Vec` (cheap `Arc` clones,
/// one clone per built module); without the feature it is a zero-sized
/// placeholder that the optimizer erases entirely.
#[cfg(feature = "observer")]
type ObserverList = Vec<ObserverRef>;
#[cfg(not(feature = "observer"))]
type ObserverList = [(); 0];
#[cfg(feature = "decorator")]
type DecoratorFn = Box<dyn Fn(Box<dyn Any>) -> Box<dyn Any>>;

/// HKDF key-derivation version label bound into every per-field key.
/// Bumping this rotates all encrypted configs without changing master keys.
#[cfg(feature = "encryption")]
const KEY_DERIVATION_VERSION: &str = "v1";

/// Derive a per-field encryption key, mapping HKDF failures to `TraitKitError`.
#[cfg(feature = "encryption")]
pub(crate) fn derive_kit_field_key(
    master_key: &[u8],
    path: &'static str,
    context: &'static str,
) -> Result<[u8; 32], TraitKitError> {
    super::config::derive_field_key(master_key, path, KEY_DERIVATION_VERSION).map_err(|e| {
        TraitKitError::BuildFailed {
            context: context.to_string(),
            source: Box::new(e),
        }
    })
}

/// 用易失写清零密钥材料，防止编译器把普通的 `for b in buf { *b = 0 }`
/// 当作死存储（dead-store elimination）优化掉。
///
/// 注意：`ptr::write_volatile` 本身是 `unsafe` 函数（与任务描述相反），
/// 因此这里显式 `#[allow(unsafe_code)]` 并给出 SAFETY 论证，模式与
/// `require()` / `factory()` 中的 typestate cast 一致。
#[cfg(feature = "encryption")]
#[allow(unsafe_code)]
pub(crate) fn zeroize_bytes(buf: &mut [u8]) {
    for byte in buf.iter_mut() {
        // SAFETY: `byte` is a `&mut u8` derived from a valid, aligned and
        // exclusively-borrowed `&mut [u8]`, so a volatile write of `0u8`
        // cannot cause UB. The volatile semantics are exactly what stops
        // LLVM from eliding the wipe.
        unsafe { std::ptr::write_volatile(byte, 0u8) };
    }
    std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
}

/// Marker type for the unbuilt state.
pub struct Unbuilt;

/// Marker type for the ready (built) state.
pub struct Ready;

/// Type alias for reload subscriber callbacks (single-threaded, `!Sync`).
#[cfg(feature = "reload")]
type SubscriberMap = RefCell<HashMap<TypeId, Vec<Rc<dyn Fn()>>>>;

/// Type alias for the encrypted config store (single-threaded, `!Sync`).
#[cfg(feature = "encryption")]
type EncryptedConfigMap = RefCell<HashMap<TypeId, EncryptedBlob>>;

/// Type-erased lazy build closure stored in `LazySlot`.
///
/// Unlike `BuildFn` (a `Box<dyn FnOnce>`, consumed by the call), an `Fn`
/// closure can be invoked through a shared borrow — so when a lazy build
/// fails, the very same closure can be put back into its slot and the slot
/// stays retryable instead of degrading into a permanent `MissingCapability`.
pub(crate) type LazyBuildFn = Box<
    dyn Fn(&Kit) -> Result<Box<dyn Any>, Box<dyn std::error::Error + Send + 'static>>,
>;

/// A lazy construction slot: holds a `build_fn` and a `OnceLock` cache cell.
/// The builder is invoked on first access; the result is cached in the
/// `OnceLock` for subsequent accesses. After construction, `builder` is
/// `None` (consumed) and `cell` holds the built capability. If the build
/// fails, the builder is put back so the slot can be retried.
///
/// Shared between `Kit` and `Scope` to avoid struct duplication.
pub(crate) struct LazySlot {
    pub(crate) builder: Option<LazyBuildFn>,
    pub(crate) cell: OnceLock<Box<dyn Any>>,
}

// ─── Feature-gated field groups ────────────────────────────────────────────
//
// 按 feature 聚合的字段子结构：新增 feature 时扩展对应子结构即可，不要在
// `Kit` 上平铺新字段（否则字段声明、`new()`、`build()` 搬移等多个位点都要
// 逐字段散弹式修改）。各子结构内部字段类型与聚合前完全一致，外部行为零
// 变化；空态构造集中在 `Kit::new()`，直接复用各子结构的 `derive(Default)`
// （`RefCell<T: Default>` 等均可派生）。

/// Fields gated behind the `interface` feature.
#[cfg(feature = "interface")]
#[derive(Default)]
struct InterfaceFields {
    interface_builders: RefCell<HashMap<TypeId, (&'static str, BuildFn)>>,
}

/// Fields gated behind the `reload` feature.
#[cfg(feature = "reload")]
#[derive(Default)]
struct ReloadFields {
    subscribers: SubscriberMap,
}

/// Fields gated behind the `encryption` feature.
#[cfg(feature = "encryption")]
#[derive(Default)]
struct EncryptionFields {
    encrypted_configs: EncryptedConfigMap,
}

/// Fields gated behind the `confers` feature.
#[cfg(feature = "confers")]
#[derive(Default)]
struct ConfersFields {
    config_snapshots: RefCell<HashMap<TypeId, Box<dyn Any>>>,
    /// Shared field overlay for cross-type config inheritance.
    /// Values are `serde_json::Value` to preserve type information.
    shared_fields: RefCell<serde_json::Map<String, serde_json::Value>>,
}

/// Fields gated behind the `toggle` feature.
#[cfg(feature = "toggle")]
struct ToggleFields {
    backend: RefCell<super::toggle::ToggleBackendType>,
}

/// Fields gated behind the `lifecycle` feature.
#[cfg(feature = "lifecycle")]
#[derive(Default)]
struct LifecycleFields {
    shutdown_callbacks: RefCell<Vec<(TypeId, ShutdownCallback)>>,
    ready_callbacks: RefCell<Vec<(TypeId, ReadyCallback)>>,
}

/// Fields gated behind the `health` feature.
#[cfg(feature = "health")]
#[derive(Default)]
struct HealthFields {
    health_checkers: RefCell<HashMap<TypeId, (/* module_name */ &'static str, HealthCheckerFn)>>,
}

/// Fields gated behind the `observer` feature.
#[cfg(feature = "observer")]
#[derive(Default)]
struct ObserverFields {
    observers: RefCell<Vec<ObserverRef>>,
}

/// Fields for observation ports (always present, no feature gate).
#[derive(Default)]
struct PortsFields {
    metrics_port: RefCell<super::ports::OptionalMetricsPort>,
    log_port: RefCell<super::ports::OptionalLogPort>,
    event_bus: RefCell<super::events::OptionalEventBus>,
}

/// Fields gated behind the `decorator` feature.
#[cfg(feature = "decorator")]
#[derive(Default)]
struct DecoratorFields {
    decorators: RefCell<HashMap<TypeId, Vec<DecoratorFn>>>,
    /// Maps module `TypeId` → capability `TypeId` for decorator lookup in
    /// `build_eager_modules()` (where only module `TypeId`s from the
    /// dependency graph are available).
    decorator_module_to_cap: RefCell<HashMap<TypeId, TypeId>>,
}

/// Fields gated behind the `i18n` feature (T207).
#[cfg(feature = "i18n")]
#[derive(Default)]
struct I18nFields {
    /// Module-owned FTL fragments collected at registration: `(locale, source)`.
    module_ftl: RefCell<Vec<(&'static str, &'static str)>>,
    /// Kit-local overlay catalog merged at `build()` from the fragments that
    /// match the active locale (`None` before build or with no fragments).
    module_catalog: RefCell<Option<crate::i18n::MessageCatalog>>,
}

/// The capability and configuration management center.
///
/// # Thread safety
///
/// `Kit` contains `RefCell` fields (see the `SubscriberMap` /
/// `EncryptedConfigMap` type-alias comments) and is therefore not `Sync` —
/// it cannot be shared across threads (e.g. via `Arc`). Its interior
/// mutability also means many `&self` methods have side effects
/// (`override_module`, `set_config`, `subscribe`, `decorate`,
/// `enable_toggle`, ...).
pub struct Kit<S = Unbuilt> {
    builders: RefCell<HashMap<TypeId, BuildFn>>,
    /// Override map for test injection: `TypeId` of module → pre-built capability.
    /// Populated by `override_module` / `override_module_strict`; consumed by `build()`.
    overrides: RefCell<HashMap<TypeId, Box<dyn Any>>>,
    /// Lazy builders (Unbuilt state): modules registered via `register_lazy`.
    /// Transferred to `lazy_slots` during `build()`. Stored as re-invocable
    /// `Fn` closures (`LazyBuildFn`) so a failed lazy build can be retried.
    lazy_builders: RefCell<HashMap<TypeId, LazyBuildFn>>,
    /// Lazy slots (Ready state): `build_fn` + `OnceLock` cache. Populated by
    /// `build()` from `lazy_builders`. Consumed by `require()` on first access.
    lazy_slots: RefCell<HashMap<TypeId, LazySlot>>,
    /// Multi-binding builders (Unbuilt state): modules registered via
    /// `register_multi`. Keyed by `TypeId::of::<M::Capability>()` (not the
    /// module type) so multiple module types with the same capability type
    /// aggregate into one Vec. Built into `multi_capabilities` during
    /// `build()` by T011.
    multi_builders: RefCell<HashMap<TypeId, Vec<BuildFn>>>,
    /// Multi-binding capabilities (Ready state): built results from
    /// `multi_builders`. Keyed by `TypeId::of::<M::Capability>()`.
    /// Populated by `build()`; consumed by `require_all()`.
    multi_capabilities: RefCell<HashMap<TypeId, Vec<Box<dyn Any>>>>,
    /// Interface builders (Unbuilt state): modules registered via
    /// `register_as`. Keyed by `TypeId::of::<M::Interface>()` (not the
    /// module type) so `resolve::<I>()` retrieves by interface type.
    /// Built into `capabilities` during `build()` (T015). Values carry the
    /// owning module's name so duplicate-interface errors can name the
    /// module that already occupies the interface.
    #[cfg(feature = "interface")]
    interface: InterfaceFields,
    graph: DependencyGraph,
    configs: TypeMap,
    capabilities: TypeMap,
    #[cfg(feature = "reload")]
    reload: ReloadFields,
    #[cfg(feature = "encryption")]
    encryption: EncryptionFields,
    #[cfg(feature = "confers")]
    confers: ConfersFields,
    #[cfg(feature = "toggle")]
    toggle: ToggleFields,
    #[cfg(feature = "lifecycle")]
    lifecycle: LifecycleFields,
    #[cfg(feature = "health")]
    health: HealthFields,
    #[cfg(feature = "observer")]
    observer: ObserverFields,
    #[cfg(feature = "decorator")]
    decorator: DecoratorFields,
    #[cfg(feature = "i18n")]
    i18n: I18nFields,
    #[cfg(feature = "report")]
    report: super::report::ReportFields,
    ports: PortsFields,
    _state: std::marker::PhantomData<S>,
}

impl Kit {
    /// Create a new empty Kit.
    #[must_use]
    pub fn new() -> Self {
        Kit {
            builders: RefCell::new(HashMap::new()),
            overrides: RefCell::new(HashMap::new()),
            lazy_builders: RefCell::new(HashMap::new()),
            lazy_slots: RefCell::new(HashMap::new()),
            multi_builders: RefCell::new(HashMap::new()),
            multi_capabilities: RefCell::new(HashMap::new()),
            #[cfg(feature = "interface")]
            interface: InterfaceFields::default(),
            graph: DependencyGraph::new(),
            configs: TypeMap::new(),
            capabilities: TypeMap::new(),
            #[cfg(feature = "reload")]
            reload: ReloadFields::default(),
            #[cfg(feature = "encryption")]
            encryption: EncryptionFields::default(),
            #[cfg(feature = "confers")]
            confers: ConfersFields::default(),
            #[cfg(feature = "toggle")]
            toggle: ToggleFields {
                backend: RefCell::new(super::toggle::ToggleBackendType::new()),
            },
            #[cfg(feature = "lifecycle")]
            lifecycle: LifecycleFields::default(),
            #[cfg(feature = "health")]
            health: HealthFields::default(),
            #[cfg(feature = "observer")]
            observer: ObserverFields::default(),
            #[cfg(feature = "decorator")]
            decorator: DecoratorFields::default(),
            #[cfg(feature = "i18n")]
            i18n: I18nFields::default(),
            #[cfg(feature = "report")]
            report: super::report::ReportFields::default(),
            ports: PortsFields::default(),
            _state: std::marker::PhantomData,
        }
    }

    /// Register a module for construction.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::AlreadyRegistered` if a module with the same `TypeId` was already registered.
    pub fn register<M: AutoBuilder>(&mut self) -> Result<(), TraitKitError> {
        let entry = ModuleEntry {
            type_id: TypeId::of::<M>(),
            name: M::NAME,
            dependencies: M::dependencies().iter().map(|(n, id)| (*n, *id)).collect(),
        };

        self.graph
            .add(entry)
            .map_err(|name| TraitKitError::AlreadyRegistered { module: name })?;

        let build_fn: BuildFn = Box::new(|kit| {
            let capability = M::build(kit)
                .map_err(|e| -> Box<dyn std::error::Error + Send + 'static> { Box::new(e) })?;
            Ok(Box::new(capability) as Box<dyn Any>)
        });

        self.builders
            .borrow_mut()
            .insert(TypeId::of::<M>(), build_fn);
        self.record_module_i18n::<M>();
        Ok(())
    }

    /// Register a module for lazy construction.
    ///
    /// The module is added to the dependency graph (for validation) but its
    /// `build_fn` is **not** invoked during `build()`. Instead, the `build_fn`
    /// is stored in `lazy_builders` and transferred to `Kit<Ready>.lazy_slots`
    /// during `build()`. The capability is constructed on first `require()`
    /// call and cached via `OnceLock` for subsequent accesses.
    ///
    /// This is useful for modules that are expensive to build or may never
    /// be needed in a particular run.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::AlreadyRegistered` if the module was already
    /// registered (via `register` or `register_lazy`).
    /// Returns `TraitKitError::DependencyMissing` if a dependency is not registered.
    pub fn register_lazy<M: AutoBuilder>(&mut self) -> Result<(), TraitKitError>
    where
        M::Capability: Clone + 'static,
    {
        let entry = ModuleEntry {
            type_id: TypeId::of::<M>(),
            name: M::NAME,
            dependencies: M::dependencies().iter().map(|(n, id)| (*n, *id)).collect(),
        };

        self.graph
            .add(entry)
            .map_err(|name| TraitKitError::AlreadyRegistered { module: name })?;

        let build_fn: LazyBuildFn = Box::new(|kit| {
            let capability = M::build(kit)
                .map_err(|e| -> Box<dyn std::error::Error + Send + 'static> { Box::new(e) })?;
            Ok(Box::new(capability) as Box<dyn Any>)
        });

        self.lazy_builders
            .borrow_mut()
            .insert(TypeId::of::<M>(), build_fn);
        self.record_module_i18n::<M>();
        Ok(())
    }

    /// Register a module for multi-binding construction.
    ///
    /// Multiple module types that share the same `M::Capability` type can be
    /// registered via `register_multi`; their `build_fns` are appended to a
    /// `Vec` keyed by `TypeId::of::<M::Capability>()` (the capability type,
    /// not the module type). The Vec preserves registration order.
    ///
    /// The module is also added to the dependency graph for validation, so
    /// `M` must be distinct from any previously registered module (via
    /// `register`, `register_lazy`, or `register_multi`). Two registrations
    /// of the same module type `M` will return `AlreadyRegistered`.
    ///
    /// During `build()`, all multi-binding builders are invoked and the
    /// results are stored in `multi_capabilities` (T011). Use `require_all`
    /// to retrieve the ordered Vec of capabilities.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::AlreadyRegistered` if `M` was already registered
    /// (via any `register*` method). Dependency validation is deferred to
    /// `build()` (via `graph.validate()`).
    pub fn register_multi<M: AutoBuilder>(&mut self) -> Result<(), TraitKitError>
    where
        M::Capability: Clone + 'static,
    {
        let entry = ModuleEntry {
            type_id: TypeId::of::<M>(),
            name: M::NAME,
            dependencies: M::dependencies().iter().map(|(n, id)| (*n, *id)).collect(),
        };

        self.graph
            .add(entry)
            .map_err(|name| TraitKitError::AlreadyRegistered { module: name })?;

        let build_fn: BuildFn = Box::new(|kit| {
            let capability = M::build(kit)
                .map_err(|e| -> Box<dyn std::error::Error + Send + 'static> { Box::new(e) })?;
            Ok(Box::new(capability) as Box<dyn Any>)
        });

        // Aggregate by capability type so require_all::<M>() returns all
        // implementations of the same capability type.
        let cap_id = TypeId::of::<M::Capability>();
        self.multi_builders
            .borrow_mut()
            .entry(cap_id)
            .or_default()
            .push(build_fn);
        self.record_module_i18n::<M>();
        Ok(())
    }

    /// Register a module for interface-based construction.
    ///
    /// Unlike `register`, this method stores the `build_fn` keyed by
    /// `TypeId::of::<M::Interface>()` (the interface type, not the module
    /// type). The module's `into_interface` method converts the concrete
    /// capability into `Arc<M::Interface>` during `build()`, enabling
    /// type-erased retrieval via `resolve::<I>()`.
    ///
    /// Only one implementation per interface type is allowed. For multiple
    /// implementations of the same capability type, use `register_multi`
    /// instead.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::AlreadyRegistered` if the interface type was
    /// already registered via `register_as`, or if the module type `M` was
    /// already registered via any `register*` method.
    ///
    /// # Panics
    ///
    /// With the `decorator` feature, panics at build time if a registered
    /// decorator's internal `downcast` fails due to a type mismatch (should
    /// never happen when `decorate::<M>()` is used with the same module).
    #[cfg(feature = "interface")]
    pub fn register_as<M>(&mut self) -> Result<(), TraitKitError>
    where
        M: crate::core::InterfaceBuilder,
    {
        let interface_id = TypeId::of::<M::Interface>();

        // One implementation per interface type. Report the module that
        // already occupies the interface, not the rejected newcomer.
        if let Some((owner, _)) = self.interface.interface_builders.borrow().get(&interface_id) {
            return Err(TraitKitError::AlreadyRegistered { module: owner });
        }

        let entry = ModuleEntry {
            type_id: TypeId::of::<M>(),
            name: M::NAME,
            dependencies: M::dependencies().iter().map(|(n, id)| (*n, *id)).collect(),
        };

        self.graph
            .add(entry)
            .map_err(|name| TraitKitError::AlreadyRegistered { module: name })?;

        let build_fn: BuildFn = Box::new(|kit| {
            let cap = M::build(kit)
                .map_err(|e| -> Box<dyn std::error::Error + Send + 'static> { Box::new(e) })?;
            // Apply decorators (keyed by capability TypeId) before the
            // interface conversion, mirroring the eager / lazy / multi paths
            // so `decorate::<M>()` covers all four build paths.
            #[cfg(feature = "decorator")]
            let cap = {
                let boxed = kit.apply_decorators(
                    kit.decorator.decorator_module_to_cap
                        .borrow()
                        .get(&TypeId::of::<M>())
                        .copied()
                        .unwrap_or_else(TypeId::of::<M::Capability>),
                    Box::new(cap),
                );
                *boxed
                    .downcast::<M::Capability>()
                    .expect("decorator type mismatch")
            };
            let iface: std::sync::Arc<M::Interface> = M::into_interface(cap);
            Ok(Box::new(iface) as Box<dyn Any>)
        });

        self.interface.interface_builders
            .borrow_mut()
            .insert(interface_id, (M::NAME, build_fn));
        Ok(())
    }

    /// Override a module's capability with a pre-built value, skipping `build_fn`.
    ///
    /// Used for test injection: inject a mock capability without running the
    /// module's build function. Completely skips dependency checking (pure
    /// unit testing). The module does **not** need to be registered via
    /// `register()` first — the override is keyed by `TypeId::of::<M>()`.
    ///
    /// If `build()` is called later, the override is consumed and the
    /// original `build_fn` (if any) is never invoked for this module.
    pub fn override_module<M: AutoBuilder>(&self, capability: M::Capability)
    where
        M::Capability: 'static,
    {
        self.overrides
            .borrow_mut()
            .insert(TypeId::of::<M>(), Box::new(capability));
        #[cfg(feature = "report")]
        self.report.push_override_record(super::report::OverrideRecord {
            module: M::NAME,
            source: "override_module",
        });
    }

    /// Override a module's capability with a pre-built value, but still
    /// verify that the module's declared dependencies are registered in the
    /// dependency graph.
    ///
    /// Unlike `override_module`, this method requires `&mut self` (exclusive
    /// access) and checks `M::dependencies()` against the graph. If any
    /// dependency is not registered, returns `TraitKitError::DependencyMissing`.
    ///
    /// The module does **not** need to be registered via `register()` first.
    /// Only the dependencies must be present.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::DependencyMissing` if any of `M::dependencies()`
    /// is not registered in the graph.
    pub fn override_module_strict<M: AutoBuilder>(
        &mut self,
        capability: M::Capability,
    ) -> Result<(), TraitKitError>
    where
        M::Capability: 'static,
    {
        for (dep_name, dep_id) in M::dependencies() {
            if self.graph.name_of(*dep_id).is_none() {
                return Err(TraitKitError::DependencyMissing {
                    module: M::NAME,
                    missing: dep_name,
                });
            }
        }
        self.overrides
            .borrow_mut()
            .insert(TypeId::of::<M>(), Box::new(capability));
        #[cfg(feature = "report")]
        self.report.push_override_record(super::report::OverrideRecord {
            module: M::NAME,
            source: "override_module_strict",
        });
        Ok(())
    }

    /// Set a configuration value.
    pub fn set_config<C: Clone + 'static>(&self, config: C) {
        self.configs.insert(config);
    }

    /// Load a configuration via its `Configurable` implementation and store it.
    ///
    /// Requires the `confers` feature. The type must implement `Configurable`,
    /// typically by delegating to `confers::Config`'s derived `load_sync()`.
    /// The loaded value overrides any prior `set_config` of the same type.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if `Configurable::load` fails.
    #[cfg(feature = "confers")]
    pub fn load_config<C: super::Configurable>(&self) -> Result<(), TraitKitError> {
        let config = C::load().map_err(|e| TraitKitError::BuildFailed {
            context: "load_config".into(),
            source: e,
        })?;
        self.set_config(config);
        Ok(())
    }

    /// Load a configuration and validate it before storing.
    ///
    /// Requires the `confers` feature. Calls `C::load()`, then
    /// `C::validate()`. The configuration is only stored if validation passes.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if loading or validation fails.
    /// On validation failure, the error source is a `ValidationError` containing
    /// all failure reasons, and the configuration is not stored.
    #[cfg(feature = "confers")]
    pub fn load_and_validate<C>(&self) -> Result<(), TraitKitError>
    where
        C: super::Configurable + super::Validatable,
    {
        let config = C::load().map_err(|e| TraitKitError::BuildFailed {
            context: "load_and_validate".into(),
            source: e,
        })?;
        match config.validate() {
            Ok(()) => {
                self.set_config(config);
                Ok(())
            }
            Err(errors) => Err(TraitKitError::BuildFailed {
                context: "load_and_validate".into(),
                source: Box::new(super::ValidationError { errors }),
            }),
        }
    }

    /// Snapshot the current configuration of type `C`.
    ///
    /// Requires the `confers` feature. Clones the current config and stores
    /// it as a snapshot. Returns `false` if no config of type `C` is present.
    /// Subsequent snapshots of the same type overwrite the previous one.
    #[cfg(feature = "confers")]
    pub fn snapshot_config<C: Clone + 'static>(&self) -> bool {
        if let Some(config) = self.configs.get_cloned::<C>() {
            self.confers.config_snapshots
                .borrow_mut()
                .insert(TypeId::of::<C>(), Box::new(config));
            true
        } else {
            false
        }
    }

    /// Restore a configuration from its snapshot.
    ///
    /// Requires the `confers` feature. Clones the snapshot back into the
    /// configs `TypeMap`, overwriting the current value.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingConfig` if no snapshot exists for `C`.
    #[cfg(feature = "confers")]
    pub fn restore_config<C: Clone + 'static>(&self) -> Result<(), TraitKitError> {
        let snapshots = self.confers.config_snapshots.borrow();
        let boxed =
            snapshots
                .get(&TypeId::of::<C>())
                .ok_or_else(|| TraitKitError::MissingConfig {
                    key: format!("{} (snapshot)", std::any::type_name::<C>()),
                })?;
        let config =
            boxed
                .downcast_ref::<C>()
                .cloned()
                .ok_or_else(|| TraitKitError::MissingConfig {
                    key: format!("{} (snapshot downcast)", std::any::type_name::<C>()),
                })?;
        drop(snapshots);
        self.set_config(config);
        Ok(())
    }

    /// Check if a snapshot exists for configuration type `C`.
    #[cfg(feature = "confers")]
    pub fn has_snapshot<C: 'static>(&self) -> bool {
        self.confers.config_snapshots
            .borrow()
            .contains_key(&TypeId::of::<C>())
    }

    /// Load a configuration with variable interpolation.
    ///
    /// Requires the `confers` feature. Calls `C::load()`, serializes to JSON,
    /// replaces `${VAR}` and `${VAR:-default}` patterns in string values using
    /// the provided `vars` map, then deserializes back and stores the result.
    ///
    /// `C` must implement `serde::Serialize` and `serde::de::DeserializeOwned`
    /// in addition to `Configurable`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if loading, serialization,
    /// or deserialization fails.
    #[cfg(feature = "confers")]
    pub fn load_config_with<C, S: std::hash::BuildHasher>(
        &self,
        vars: &std::collections::HashMap<String, String, S>,
    ) -> Result<(), TraitKitError>
    where
        C: super::Configurable + serde::Serialize + serde::de::DeserializeOwned,
    {
        let config = C::load().map_err(|e| TraitKitError::BuildFailed {
            context: "load_config_with".into(),
            source: e,
        })?;
        let mut json_value =
            serde_json::to_value(&config).map_err(|e| TraitKitError::BuildFailed {
                context: "load_config_with (serialize)".into(),
                source: Box::new(e),
            })?;
        super::config::interpolate_json_value(&mut json_value, vars);
        let interpolated: C =
            serde_json::from_value(json_value).map_err(|e| TraitKitError::BuildFailed {
                context: "load_config_with (deserialize)".into(),
                source: Box::new(e),
            })?;
        self.set_config(interpolated);
        Ok(())
    }

    /// Validate the dependency graph and build all modules in topological order.
    ///
    /// After this call, all capabilities are available via `require()`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::DependencyMissing` if a registered module depends on an unregistered module.
    /// Returns `TraitKitError::CycleDetected` if a dependency cycle is found.
    /// Returns `TraitKitError::MissingCapability` if a build function is missing for a sorted module.
    /// Returns `TraitKitError::BuildFailed` if a module's `build` callback returns an error.
    pub fn build(self) -> Result<Kit<Ready>, TraitKitError> {
        let sorted = match self.graph.validate() {
            Ok(sorted) => sorted,
            Err(GraphError::DependencyMissing { module, missing }) => {
                return Err(TraitKitError::DependencyMissing { module, missing });
            }
            Err(GraphError::CycleDetected { cycle }) => {
                return Err(TraitKitError::CycleDetected { cycle });
            }
        };

        // Report: capture validated topological order (module names) and the
        // overall build start time. Zero code exists without `report`.
        #[cfg(feature = "report")]
        self.report
            .set_topo_order(sorted.iter().map(|id| self.module_name(*id)).collect());
        #[cfg(feature = "report")]
        let report_build_start = std::time::Instant::now();

        // Phase 1: Build eager modules (overrides + build_fn in topo order)
        self.build_eager_modules(&sorted)?;

        // Phase 2: Transfer lazy builders to lazy slots
        self.transfer_lazy_builders();

        // Phase 3: Build multi-binding modules
        self.build_multi_bindings()?;

        // Phase 4: Build interface modules
        #[cfg(feature = "interface")]
        self.build_interface_modules()?;

        // Extract ready_callbacks before moving self
        #[cfg(feature = "lifecycle")]
        let ready_callbacks: Vec<(TypeId, ReadyCallback)> =
            { self.lifecycle.ready_callbacks.borrow_mut().drain(..).collect() };
        #[cfg(feature = "lifecycle")]
        let shutdown_callbacks: Vec<(TypeId, ShutdownCallback)> =
            { self.lifecycle.shutdown_callbacks.borrow_mut().drain(..).collect() };
        #[cfg(feature = "lifecycle")]
        // Stable topological sort so `shutdown()`'s reverse iteration really
        // runs in reverse topological order: dependents (consumers) shut down
        // before the modules they depend on. Callbacks whose module is absent
        // from the graph sort last (relative order preserved via stable sort).
        let shutdown_callbacks: Vec<(TypeId, ShutdownCallback)> = {
            let topo_index: std::collections::HashMap<TypeId, usize> =
                sorted.iter().enumerate().map(|(i, id)| (*id, i)).collect();
            let mut callbacks = shutdown_callbacks;
            callbacks
                .sort_by_key(|(type_id, _)| topo_index.get(type_id).copied().unwrap_or(usize::MAX));
            callbacks
        };

        // Structural build time stops here (lifecycle on_ready callbacks are
        // excluded from the report's total).
        #[cfg(feature = "report")]
        self.report
            .set_total_elapsed_us(report_build_start.elapsed().as_micros() as u64);

        // T207: merge module-owned FTL fragments matching the active locale
        // into the kit-local overlay catalog (must happen before `self.i18n`
        // is moved into the ready Kit below).
        #[cfg(feature = "i18n")]
        {
            let locale = crate::i18n::I18nManager::init().locale_tag().to_lowercase();
            let want_zh = locale.starts_with("zh");
            let fragments = self.i18n.module_ftl.borrow();
            let merged = fragments
                .iter()
                .filter(|(loc, _)| loc.to_lowercase().starts_with("zh") == want_zh)
                .map(|(_, ftl)| *ftl)
                .collect::<Vec<_>>()
                .join("\n");
            drop(fragments);
            *self.i18n.module_catalog.borrow_mut() = if merged.is_empty() {
                None
            } else {
                Some(crate::i18n::MessageCatalog::parse(&merged))
            };
        }

        let kit = Kit {
            builders: self.builders,
            overrides: self.overrides,
            lazy_builders: self.lazy_builders,
            lazy_slots: self.lazy_slots,
            multi_builders: self.multi_builders,
            multi_capabilities: self.multi_capabilities,
            #[cfg(feature = "interface")]
            interface: self.interface,
            graph: self.graph,
            configs: self.configs,
            capabilities: self.capabilities,
            #[cfg(feature = "reload")]
            reload: self.reload,
            #[cfg(feature = "encryption")]
            encryption: self.encryption,
            #[cfg(feature = "confers")]
            confers: self.confers,
            #[cfg(feature = "toggle")]
            toggle: self.toggle,
            #[cfg(feature = "lifecycle")]
            lifecycle: LifecycleFields {
                shutdown_callbacks: RefCell::new(shutdown_callbacks),
                ready_callbacks: RefCell::new(Vec::new()),
            },
            #[cfg(feature = "health")]
            health: self.health,
            #[cfg(feature = "observer")]
            observer: self.observer,
            #[cfg(feature = "decorator")]
            decorator: self.decorator,
            #[cfg(feature = "i18n")]
            i18n: self.i18n,
            #[cfg(feature = "report")]
            report: self.report,
            ports: self.ports,
            _state: std::marker::PhantomData,
        };

        // Call lifecycle on_ready callbacks in topological order
        #[cfg(feature = "lifecycle")]
        {
            let mut callbacks: std::collections::HashMap<TypeId, ReadyCallback> =
                ready_callbacks.into_iter().collect();
            for type_id in &sorted {
                if let Some(callback) = callbacks.remove(type_id) {
                    callback(&kit)?;
                }
            }
        }

        Ok(kit)
    }

    /// Phase 1: Build eager modules in topological order.
    ///
    /// For each module in the sorted list:
    /// 1. Check overrides first (skip `build_fn` if override exists)
    /// 2. Skip lazy-registered modules (deferred to first `require()`)
    /// 3. Invoke the `build_fn` for regular modules
    /// 4. Insert remaining unregistered overrides after the loop
    fn build_eager_modules(&self, sorted: &[TypeId]) -> Result<(), TraitKitError> {
        // T208: per-module timestamps are only taken when a bus is injected.
        let event_bus_present = self.ports.event_bus.borrow().is_some();
        for type_id in sorted {
            let module_name = self.module_name(*type_id);

            // Report: dependency names for this module (cheap Vec, report only).
            #[cfg(feature = "report")]
            let report_deps = self.graph.dependency_names(*type_id);

            // [Override] Priority 1: check overrides map first.
            if let Some(boxed) = self.overrides.borrow_mut().remove(type_id) {
                self.capabilities.insert_boxed(*type_id, boxed);
                #[cfg(feature = "report")]
                self.report.push_overridden(module_name, report_deps);
                continue;
            }

            // [Lazy] Skip lazy-registered modules — deferred to first require().
            if self.lazy_builders.borrow().contains_key(type_id) {
                #[cfg(feature = "report")]
                self.report.push_lazy(module_name, report_deps);
                continue;
            }

            // [Build] Priority 2: invoke the registered build_fn.
            let Some(build_fn) = self.builders.borrow_mut().remove(type_id) else {
                continue;
            };

            // Observer: notify build start. The observer list is cloned once
            // per module (cheap `Arc` clones) and held locally for the whole
            // flow; every observer-vs-no-observer difference is confined to
            // the cfg-gated `observers_snapshot` / `notify_*` helpers below,
            // so this loop body exists exactly once for both configurations.
            let observers = self.observers_snapshot();
            #[cfg(feature = "observer")]
            let start_instant = std::time::Instant::now();
            #[cfg(feature = "report")]
            let report_start = std::time::Instant::now();
            let event_start = event_bus_present.then(std::time::Instant::now);
            Self::notify_module_start(&observers, module_name);

            match (build_fn)(self) {
                Ok(boxed) => {
                    // `elapsed` is taken before decorators run, matching the
                    // historical observer-only behavior.
                    #[cfg(feature = "observer")]
                    let elapsed = start_instant.elapsed();
                    #[cfg(feature = "decorator")]
                    let boxed = {
                        let cap_type_id = self
                            .decorator
                            .decorator_module_to_cap
                            .borrow()
                            .get(type_id)
                            .copied()
                            .unwrap_or(*type_id);
                        self.apply_decorators(cap_type_id, boxed)
                    };
                    self.capabilities.insert_boxed(*type_id, boxed);
                    #[cfg(feature = "observer")]
                    Self::notify_module_built(&observers, module_name, elapsed);
                    #[cfg(feature = "report")]
                    self.report.push_built(
                        module_name,
                        report_start.elapsed().as_micros() as u64,
                        report_deps,
                    );
                    if let Some(event_start) = event_start {
                        let elapsed_us = event_start.elapsed().as_micros() as u64;
                        self.publish_event(super::events::KitEvent::ModuleBuilt {
                            module: module_name,
                            elapsed_us,
                        });
                    }
                }
                Err(e) => {
                    let err = TraitKitError::BuildFailed {
                        context: module_name.to_string(),
                        source: e,
                    };
                    Self::notify_module_error(&observers, module_name, &err);
                    return Err(err);
                }
            }
        }

        // Handle modules that were overridden but NOT registered.
        let remaining: Vec<(TypeId, Box<dyn Any>)> = self.overrides.borrow_mut().drain().collect();
        for (type_id, boxed) in remaining {
            self.capabilities.insert_boxed(type_id, boxed);
        }
        Ok(())
    }

    // Observer notification helpers for `build_eager_modules`. Both cfg arms
    // share identical signatures so the eager-build loop body can be written
    // once; the `not(observer)` arms are zero-sized no-ops the optimizer
    // erases (no `observers` field or `BuildObserver` reference is compiled
    // outside the `observer` feature).

    /// Snapshot of the observers for one module build: one `Vec` clone
    /// (cheap `Arc` clones) per built module, instead of re-borrowing the
    /// `RefCell` around every notification.
    #[cfg(feature = "observer")]
    fn observers_snapshot(&self) -> ObserverList {
        self.observer.observers.borrow().clone()
    }

    #[cfg(not(feature = "observer"))]
    #[allow(clippy::unused_self)] // signature parity with the observer arm
    fn observers_snapshot(&self) -> ObserverList {
        []
    }

    #[cfg(feature = "observer")]
    fn notify_module_start(list: &ObserverList, name: &'static str) {
        for obs in list {
            obs.on_module_start(name);
        }
    }

    #[cfg(not(feature = "observer"))]
    #[allow(clippy::trivially_copy_pass_by_ref)] // signature parity
    fn notify_module_start(_list: &ObserverList, _name: &'static str) {}

    #[cfg(feature = "observer")]
    fn notify_module_built(list: &ObserverList, name: &'static str, elapsed: std::time::Duration) {
        for obs in list {
            obs.on_module_built(name, elapsed);
        }
    }

    // Not called without `observer` (its only call site is observer-gated),
    // kept for signature parity with the observer arm.
    #[cfg(not(feature = "observer"))]
    #[allow(dead_code)]
    #[allow(clippy::trivially_copy_pass_by_ref)] // signature parity
    fn notify_module_built(
        _list: &ObserverList,
        _name: &'static str,
        _elapsed: std::time::Duration,
    ) {
    }

    #[cfg(feature = "observer")]
    fn notify_module_error(list: &ObserverList, name: &'static str, err: &TraitKitError) {
        for obs in list {
            obs.on_build_error(name, err);
        }
    }

    #[cfg(not(feature = "observer"))]
    #[allow(clippy::trivially_copy_pass_by_ref)] // signature parity
    fn notify_module_error(_list: &ObserverList, _name: &'static str, _err: &TraitKitError) {}

    /// Phase 2: Transfer lazy builders to lazy slots for first-access construction.
    fn transfer_lazy_builders(&self) {
        let lazy: Vec<(TypeId, LazyBuildFn)> = self.lazy_builders.borrow_mut().drain().collect();
        self.lazy_slots.borrow_mut().reserve(lazy.len());
        for (type_id, builder) in lazy {
            self.lazy_slots.borrow_mut().insert(
                type_id,
                LazySlot {
                    builder: Some(builder),
                    cell: OnceLock::new(),
                },
            );
        }
    }

    /// Phase 3: Build all multi-binding modules.
    fn build_multi_bindings(&self) -> Result<(), TraitKitError> {
        let multi: Vec<(TypeId, Vec<BuildFn>)> = self.multi_builders.borrow_mut().drain().collect();
        for (cap_id, build_fns) in multi {
            let mut vec = Vec::with_capacity(build_fns.len());
            for build_fn in build_fns {
                let boxed = (build_fn)(self).map_err(|e| TraitKitError::BuildFailed {
                    context: tr("trait-kit-diag-multi-binding", &[]),
                    source: e,
                })?;
                #[cfg(feature = "decorator")]
                let boxed = self.apply_decorators(cap_id, boxed);
                vec.push(boxed);
            }
            self.multi_capabilities.borrow_mut().insert(cap_id, vec);
        }
        Ok(())
    }

    /// Phase 4: Build all interface-registered modules.
    ///
    /// Decorators are applied inside the registered `build_fn` (keyed by the
    /// module's capability `TypeId`, before `into_interface`) — the same
    /// mechanism as the eager / lazy / multi paths. A lookup by interface
    /// `TypeId` here can never match: `decorate` keys by `M::Capability`,
    /// whose `TypeId` always differs from the unsized `dyn Interface`.
    #[cfg(feature = "interface")]
    fn build_interface_modules(&self) -> Result<(), TraitKitError> {
        let interfaces: Vec<(TypeId, (&'static str, BuildFn))> =
            self.interface.interface_builders.borrow_mut().drain().collect();
        for (interface_id, (_owner, build_fn)) in interfaces {
            let boxed = (build_fn)(self).map_err(|e| TraitKitError::BuildFailed {
                context: tr("trait-kit-diag-interface", &[]),
                source: e,
            })?;
            self.capabilities.insert_boxed(interface_id, boxed);
        }
        Ok(())
    }

    fn module_name(&self, type_id: TypeId) -> &'static str {
        self.graph.name_of(type_id).unwrap_or("<unknown>")
    }

    // ─── Lifecycle ─────────────────────────────────────────────────────

    /// Register lifecycle hooks for a previously registered module.
    ///
    /// The module must have been registered via `register::<M>()` first.
    /// This stores `on_ready` and `on_shutdown` callbacks that are invoked
    /// during `build()` and `shutdown()` respectively. Idempotent:
    /// registering the same module twice is a no-op.
    ///
    /// Requires the `lifecycle` feature.
    #[cfg(feature = "lifecycle")]
    pub fn register_lifecycle<M>(&mut self)
    where
        M: crate::core::lifecycle::Lifecycle + 'static,
        M::Capability: 'static,
    {
        // Idempotent: re-registering the same module must not duplicate its
        // on_shutdown callbacks (`shutdown()` walks the Vec — duplicates
        // would run twice).
        let module_type_id = TypeId::of::<M>();
        if self
            .lifecycle
            .shutdown_callbacks
            .borrow()
            .iter()
            .any(|(id, _)| *id == module_type_id)
        {
            return;
        }

        // Store shutdown callback
        let shutdown_cb: ShutdownCallback = Box::new(|caps: &TypeMap| {
            let type_id = TypeId::of::<M>();
            if let Some((_guard, cap_ref)) = caps.get_ref_by_type_id::<M::Capability>(type_id) {
                M::on_shutdown(cap_ref);
            }
        });
        self.lifecycle.shutdown_callbacks
            .borrow_mut()
            .push((TypeId::of::<M>(), shutdown_cb));

        // Store ready callback
        let ready_cb: ReadyCallback = Box::new(|kit: &Kit<Ready>| {
            M::on_ready(kit).map_err(|e| TraitKitError::LifecycleFailed {
                context: M::NAME.to_string(),
                source: Box::new(e),
            })
        });
        self.lifecycle.ready_callbacks
            .borrow_mut()
            .push((TypeId::of::<M>(), ready_cb));
    }

    // ─── Health Check ──────────────────────────────────────────────────

    /// Register a health checker for a previously registered module.
    ///
    /// The module must have been registered via `register::<M>()` first.
    /// Use `health_check::<M>()` or `health_report()` on `Kit<Ready>` to query.
    ///
    /// Requires the `health` feature.
    #[cfg(feature = "health")]
    pub fn register_health_check<M>(&mut self)
    where
        M: crate::core::health::HealthCheck + 'static,
        M::Capability: 'static,
    {
        let checker: HealthCheckerFn = Box::new(|caps: &TypeMap| {
            let type_id = TypeId::of::<M>();
            match caps.get_ref_by_type_id::<M::Capability>(type_id) {
                Some((_guard, cap_ref)) => M::check(cap_ref),
                None => crate::core::health::HealthStatus::Unhealthy {
                    detail: "capability not found".to_string(),
                },
            }
        });
        self.health.health_checkers
            .borrow_mut()
            .insert(TypeId::of::<M>(), (M::NAME, checker));
    }

    // ─── Conditional Registration ───────────────────────────────────────

    /// Conditionally register a module based on a runtime predicate.
    ///
    /// The predicate receives the current `Kit` (for inspecting configs
    /// or other state). Returns `true` if the module was actually registered.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::AlreadyRegistered` if the predicate returns
    /// `true` but the module was already registered.
    pub fn register_if<M: AutoBuilder>(
        &mut self,
        predicate: impl FnOnce(&Kit) -> bool,
    ) -> Result<bool, TraitKitError> {
        if predicate(self) {
            self.register::<M>()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ─── Feature Toggle ────────────────────────────────────────────────

    /// Enable or disable a boolean feature toggle.
    ///
    /// Requires the `toggle` feature. Delegates to the toggle backend
    /// (confers registry when `confers` feature is enabled, memory otherwise).
    #[cfg(feature = "toggle")]
    pub fn enable_toggle(&self, key: impl Into<String>, enabled: bool) {
        let key = key.into();
        self.toggle
            .backend
            .borrow_mut()
            .set(key, super::toggle::ToggleValue::Bool(enabled));
    }

    /// Check if a feature toggle is enabled.
    ///
    /// Returns `false` for unknown keys or non-boolean toggle values.
    #[cfg(feature = "toggle")]
    pub fn is_toggle_enabled(&self, key: &str) -> bool {
        matches!(
            self.toggle.backend.borrow().get(key),
            Some(super::toggle::ToggleValue::Bool(true))
        )
    }

    /// Set a typed toggle value.
    ///
    /// Accepts any [`ToggleValue`](super::toggle::ToggleValue) variant
    /// (Bool/Int/Float/Str). For boolean-only toggles, prefer `enable_toggle`.
    #[cfg(feature = "toggle")]
    pub fn set_toggle(&self, key: impl Into<String>, value: super::toggle::ToggleValue) {
        self.toggle.backend.borrow_mut().set(key.into(), value);
    }

    /// Get a typed toggle value.
    ///
    /// Returns `None` if the key does not exist.
    #[cfg(feature = "toggle")]
    pub fn get_toggle(&self, key: &str) -> Option<super::toggle::ToggleValue> {
        self.toggle.backend.borrow().get(key)
    }

    /// Remove a toggle. Returns the previous value if it existed.
    #[cfg(feature = "toggle")]
    pub fn remove_toggle(&self, key: &str) -> Option<super::toggle::ToggleValue> {
        self.toggle.backend.borrow_mut().remove(key)
    }

    /// List all toggles as `(key, value)` pairs.
    #[cfg(feature = "toggle")]
    pub fn list_toggles(&self) -> Vec<(String, super::toggle::ToggleValue)> {
        self.toggle.backend.borrow().list()
    }

    /// Conditionally register a module based on a feature toggle.
    ///
    /// Requires the `toggle` feature. Delegates to `register_if` with a
    /// predicate that checks `is_toggle_enabled(key)`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::AlreadyRegistered` if the toggle is enabled
    /// but the module was already registered.
    #[cfg(feature = "toggle")]
    pub fn register_if_toggle<M: AutoBuilder>(&mut self, key: &str) -> Result<bool, TraitKitError> {
        let enabled = self.is_toggle_enabled(key);
        if enabled {
            self.register::<M>()?;
        }
        Ok(enabled)
    }

    // ─── Observability ─────────────────────────────────────────────────

    /// Collect `M`'s FTL fragments into the kit-local i18n store (T207).
    /// Zero code without the `i18n` feature (call sites are cfg-gated too).
    #[cfg(feature = "i18n")]
    fn record_module_i18n<M: crate::core::ModuleMeta>(&self) {
        self.i18n
            .module_ftl
            .borrow_mut()
            .extend(M::i18n_ftl().iter().copied());
    }

    #[cfg(not(feature = "i18n"))]
    #[allow(clippy::unused_self, dead_code)] // signature parity with the i18n arm
    fn record_module_i18n<M: crate::core::ModuleMeta>(&self) {}

    /// Register a build observer that receives callbacks during `build()`.
    ///
    /// Requires the `observer` feature.
    #[cfg(feature = "observer")]
    pub fn with_observer(
        &mut self,
        observer: std::sync::Arc<dyn crate::core::observer::BuildObserver>,
    ) {
        self.observer.observers.borrow_mut().push(observer);
    }

    // ─── Observation Ports ─────────────────────────────────────────────

    /// Inject a [`MetricsPort`] for recording counters/gauges/histograms.
    ///
    /// The port is stored as `Option<Arc<dyn MetricsPort>>`. Pass `None` to
    /// explicitly disable metrics, or omit this call entirely (default is `None`).
    pub fn with_metrics_port(
        &mut self,
        port: impl Into<super::ports::OptionalMetricsPort>,
    ) {
        *self.ports.metrics_port.borrow_mut() = port.into();
    }

    /// Inject a [`LogPort`] for structured log recording.
    ///
    /// The port is stored as `Option<Arc<dyn LogPort>>`. Pass `None` to
    /// explicitly disable logging, or omit this call entirely (default is `None`).
    pub fn with_log_port(
        &mut self,
        port: impl Into<super::ports::OptionalLogPort>,
    ) {
        *self.ports.log_port.borrow_mut() = port.into();
    }

    // ─── Decorator ─────────────────────────────────────────────────────

    /// Register a decorator for a module's capability.
    ///
    /// The decorator is applied after the module's capability is built,
    /// wrapping or enhancing the original value. Multiple decorators can
    /// be registered for the same module; they are applied in registration
    /// order.
    ///
    /// Requires the `decorator` feature.
    ///
    /// # Panics
    ///
    /// Panics at runtime if the internal `downcast` fails due to a type
    /// mismatch (should never happen when used correctly).
    #[cfg(feature = "decorator")]
    pub fn decorate<M: AutoBuilder>(
        &self,
        decorator: impl Fn(M::Capability) -> M::Capability + 'static,
    ) where
        M::Capability: 'static,
    {
        let wrapper: DecoratorFn = Box::new(move |boxed_cap| {
            let cap = boxed_cap
                .downcast::<M::Capability>()
                .expect("decorator type mismatch");
            let decorated = decorator(*cap);
            Box::new(decorated) as Box<dyn Any>
        });
        self.decorator.decorators
            .borrow_mut()
            .entry(TypeId::of::<M::Capability>())
            .or_default()
            .push(wrapper);
        // Record module TypeId → capability TypeId mapping so
        // `build_eager_modules()` can look up decorators by module TypeId.
        self.decorator.decorator_module_to_cap
            .borrow_mut()
            .insert(TypeId::of::<M>(), TypeId::of::<M::Capability>());
    }
}

impl<S> Kit<S> {
    /// Apply registered decorators for a capability (keyed by capability `TypeId`).
    #[cfg(feature = "decorator")]
    fn apply_decorators(&self, cap_type_id: TypeId, boxed: Box<dyn Any>) -> Box<dyn Any> {
        let decorators = self.decorator.decorators.borrow();
        let Some(dec_list) = decorators.get(&cap_type_id) else {
            return boxed;
        };
        let mut current = boxed;
        for dec in dec_list {
            current = dec(current);
        }
        current
    }

    // ─── Event bus (T208, available on both Kit states) ────────────────

    /// Inject an [`EventBus`](super::events::EventBus) that receives runtime
    /// lifecycle events: module builds, health samples, config changes.
    ///
    /// Default is `None` (= no-op): publishing costs one `Option` check.
    pub fn with_event_bus(&mut self, bus: impl Into<super::events::OptionalEventBus>) {
        *self.ports.event_bus.borrow_mut() = bus.into();
    }

    /// Retrieve the injected event bus, if any (T208).
    #[must_use]
    pub fn event_bus(&self) -> super::events::OptionalEventBus {
        self.ports.event_bus.borrow().clone()
    }

    /// Publish `event` to the injected bus (no-op when absent). Internal
    /// helper keeping the `Option` check in exactly one place.
    fn publish_event(&self, event: super::events::KitEvent) {
        if let Some(bus) = self.ports.event_bus.borrow().as_ref() {
            bus.publish(event);
        }
    }

    /// Retrieve a capability by its module type.
    ///
    /// Available on both `Kit<Unbuilt>` (inside `AutoBuilder::build` callbacks)
    /// and `Kit<Ready>` (after `build()` completes).
    ///
    /// On `Kit<Ready>`, if the module was registered via `register_lazy`,
    /// the first `require()` call triggers lazy construction: the stored
    /// `build_fn` is invoked, the result is cached in a `OnceLock` cell,
    /// and subsequent calls return a clone from the cache without re-running
    /// the builder.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingCapability` if the module has not been built.
    /// Returns `TraitKitError::BuildFailed` if a lazy module's `build_fn` fails.
    pub fn require<M: AutoBuilder>(&self) -> Result<M::Capability, TraitKitError> {
        let type_id = TypeId::of::<M>();

        // 1. Eager capabilities (already-built modules + overrides)
        if let Some(cap) = self
            .capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
        {
            return Ok(cap);
        }

        // 1b. T210: a capability box exists under this module's TypeId but the
        // downcast to `M::Capability` failed → precise TypeMismatch error.
        if self.capabilities.contains_by_type_id(type_id) {
            return Err(TraitKitError::CapabilityTypeMismatch {
                key: M::NAME.to_string(),
            });
        }

        // 2. Lazy slots — check OnceLock cache (previously-built lazy modules)
        if let Some(cached) = Self::get_lazy_cached::<M>(self, type_id) {
            return Ok(cached);
        }

        // 2b. T210: a lazy-slot cache value exists but the downcast failed.
        if let Some(slot) = self.lazy_slots.borrow().get(&type_id) {
            if slot.cell.get().is_some() {
                return Err(TraitKitError::CapabilityTypeMismatch {
                    key: M::NAME.to_string(),
                });
            }
        }

        // 3. Lazy slots — first-access construction (cell empty, builder exists)
        // Take the builder out to release the RefCell borrow before calling it,
        // allowing the builder to re-enter require() for its own dependencies.
        let builder = self
            .lazy_slots
            .borrow_mut()
            .get_mut(&type_id)
            .and_then(|slot| slot.builder.take());

        if let Some(builder) = builder {
            // SAFETY: `Kit<S>` has the same memory layout as `Kit<Unbuilt>`
            // because `S` only appears in `PhantomData<S>` (zero-sized, same
            // representation as `()`). `BuildFn` expects `&Kit<Unbuilt>`; we
            // hold `&Kit<S>`. The cast is sound for any `S` since the field
            // layout is identical. In practice, this code path is only reached
            // on `Kit<Ready>` (lazy_slots is only populated after `build()`),
            // but the cast is valid regardless.
            //
            // Compile-time layout assertion: if any field depending on `S` is
            // added to `Kit`, this will fail at compile time, catching the
            // unsoundness before runtime. (size + align, mirroring the
            // `factory` cast and `async_kit.rs`.)
            const _: () = assert!(
                std::mem::size_of::<Kit<Ready>>() == std::mem::size_of::<Kit>(),
                "Kit layout changed; unsafe cast is no longer sound"
            );
            const _: () = assert!(
                std::mem::align_of::<Kit<Ready>>() == std::mem::align_of::<Kit>(),
                "Kit layout changed; unsafe cast is no longer sound"
            );
            #[allow(unsafe_code)]
            let kit_ref: &Kit = unsafe { &*std::ptr::from_ref(self).cast::<Kit>() };
            // `LazyBuildFn` is an `Fn` closure: the call only borrows it, so
            // the same builder remains available for the failure path below.
            let boxed = match builder(kit_ref) {
                Ok(boxed) => boxed,
                Err(e) => {
                    // Restore the builder so the slot stays retryable: dropping
                    // it here would degrade every later require() into a
                    // permanent MissingCapability and lose the original error.
                    if let Some(slot) = self.lazy_slots.borrow_mut().get_mut(&type_id) {
                        slot.builder = Some(builder);
                    }
                    return Err(TraitKitError::BuildFailed {
                        context: M::NAME.to_string(),
                        source: e,
                    });
                }
            };
            // Apply decorators only when THIS module was decorated: resolve
            // the capability TypeId through the module→capability mapping
            // recorded by `decorate()`. Falling back to
            // `TypeId::of::<M::Capability>()` alone would over-apply the
            // decorators to any other module that merely shares the same
            // capability type. (The eager path keeps its unmapped fallback
            // by module TypeId — its observable behavior, including the
            // documented downcast-mismatch panic, is frozen by e2e DEC-07.)
            #[cfg(feature = "decorator")]
            let boxed = {
                let mapped_cap = self.decorator.decorator_module_to_cap.borrow().get(&type_id).copied();
                match mapped_cap {
                    Some(cap_type_id) => self.apply_decorators(cap_type_id, boxed),
                    None => boxed,
                }
            };
            // Cache in OnceLock for future require() / require_ref() calls
            if let Some(slot) = self.lazy_slots.borrow().get(&type_id) {
                let _ = slot.cell.set(boxed);
            }
            return Self::get_lazy_cached::<M>(self, type_id).ok_or(
                TraitKitError::MissingCapability {
                    key: M::NAME.to_string(),
                },
            );
        }

        // 4. Not found
        Err(TraitKitError::MissingCapability {
            key: M::NAME.to_string(),
        })
    }

    /// Extracted helper: retrieve a cached lazy-slot value without rebuilding.
    /// Consolidates the duplicate lazy-cache lookup pattern in `require()`.
    fn get_lazy_cached<M: AutoBuilder>(&self, type_id: TypeId) -> Option<M::Capability> {
        self.lazy_slots
            .borrow()
            .get(&type_id)
            .and_then(|slot| slot.cell.get())
            .and_then(|b| b.downcast_ref::<M::Capability>().cloned())
    }

    /// Retrieve all capabilities registered via `register_multi` for the
    /// given module type, in registration order.
    ///
    /// Available on both `Kit<Unbuilt>` and `Kit<Ready>`, but
    /// `multi_capabilities` is only populated after `build()`. Calling
    /// `require_all` before `build()` returns `MissingCapability`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingCapability` if no multi-binding
    /// capabilities were registered for `M::Capability`.
    pub fn require_all<M: AutoBuilder>(&self) -> Result<Vec<M::Capability>, TraitKitError>
    where
        M::Capability: Clone + 'static,
    {
        let cap_id = TypeId::of::<M::Capability>();
        let multi = self.multi_capabilities.borrow();
        let vec = multi.get(&cap_id).ok_or(TraitKitError::MissingCapability {
            key: M::NAME.to_string(),
        })?;

        let mut result = Vec::with_capacity(vec.len());
        for boxed in vec {
            let cap = boxed.downcast_ref::<M::Capability>().cloned().ok_or(
                TraitKitError::CapabilityTypeMismatch {
                    key: M::NAME.to_string(),
                },
            )?;
            result.push(cap);
        }
        Ok(result)
    }

    /// Get a configuration value.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingConfig` if no value of type `C` was set.
    pub fn config<C: Clone + 'static>(&self) -> Result<C, TraitKitError> {
        self.configs
            .get_cloned::<C>()
            .ok_or(TraitKitError::MissingConfig {
                key: std::any::type_name::<C>().to_string(),
            })
    }

    /// Subscribe a callback to be invoked when config of type `C` is reloaded.
    ///
    /// Requires the `reload` feature. The callback receives no
    /// arguments; use `Kit::config::<C>()` inside it to read the new value.
    /// Callbacks are stored in a `RefCell` (single-threaded, `!Sync`).
    ///
    /// Layer 2 of the inheritance system: cargo feature chain
    /// `reload` → `confers`.
    #[cfg(feature = "reload")]
    pub fn subscribe<C: 'static>(&self, callback: impl Fn() + 'static) {
        let callback: Rc<dyn Fn()> = Rc::new(callback);
        self.reload.subscribers
            .borrow_mut()
            .entry(TypeId::of::<C>())
            .or_default()
            .push(callback);
    }

    /// Reload a configuration via its `Configurable` implementation and
    /// notify all subscribers of type `C`.
    ///
    /// Requires the `reload` feature. Calls `C::load()`, stores
    /// the result via `set_config`, then invokes every `subscribe::<C>`
    /// callback. Errors from `load()` are mapped to `TraitKitError::BuildFailed`.
    ///
    /// # Panics
    ///
    /// The new config is stored *before* invoking callbacks. If a callback
    /// panics, the config has already been updated but remaining subscribers
    /// in the chain are skipped (panic unwinds through `reload_config`).
    /// Use `std::panic::catch_unwind` inside callbacks if you need to
    /// guarantee notification of all subscribers.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if `Configurable::load` fails.
    #[cfg(feature = "reload")]
    pub fn reload_config<C: super::Configurable>(&self) -> Result<(), TraitKitError> {
        let config = C::load().map_err(|e| TraitKitError::BuildFailed {
            context: "reload_config".into(),
            source: e,
        })?;
        self.configs.insert(config);
        // Clone individual Rc pointers (ref-count increment only) with
        // pre-allocated Vec to avoid a full `.cloned()` pass.
        let callbacks: Vec<Rc<dyn Fn()>> = match self.reload.subscribers.borrow().get(&TypeId::of::<C>()) {
            Some(subs) => subs.iter().map(Rc::clone).collect(),
            None => Vec::new(),
        };
        for cb in &callbacks {
            cb();
        }
        Ok(())
    }

    /// Resolve a capability by its interface type.
    ///
    /// Retrieves an `Arc<I>` previously stored via `register_as<M>()`.
    /// The interface type `I` must be `?Sized + 'static` (e.g.,
    /// `dyn Logger`).
    ///
    /// Available on both `Kit<Unbuilt>` (inside `InterfaceBuilder::build`
    /// callbacks) and `Kit<Ready>` (after `build()` completes).
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingCapability` if the interface has not
    /// been registered or built.
    #[cfg(feature = "interface")]
    pub fn resolve<I>(&self) -> Result<std::sync::Arc<I>, TraitKitError>
    where
        I: ?Sized + 'static,
    {
        let interface_id = TypeId::of::<I>();
        self.capabilities
            .get_cloned_by_type_id::<std::sync::Arc<I>>(interface_id)
            .ok_or(TraitKitError::MissingCapability {
                key: "interface".into(),
            })
    }
}

impl Kit {
    /// Encrypt and store a configuration value.
    ///
    /// Requires the `encryption` feature. Serializes `value` to JSON,
    /// derives a per-field key from `master_key` and `C::PATH` via HKDF, then
    /// encrypts with XChaCha20-Poly1305. The resulting nonce + ciphertext is
    /// stored in `encrypted_configs`, separate from the plaintext `TypeMap`.
    ///
    /// Layer 3 of the inheritance system: the encryption key is bound to
    /// `ModuleConfig::PATH`, so the same master key produces different field
    /// keys for different modules.
    ///
    /// # Security
    ///
    /// `master_key` is a caller-owned `&[u8]`: this method cannot wipe the
    /// caller's buffer, so zeroing it remains the caller's responsibility.
    /// The internally derived per-field `field_key` and the serialized
    /// `plaintext` are zeroed with a volatile write as soon as they are no
    /// longer needed.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if serialization, key derivation, or
    /// encryption fails.
    #[cfg(feature = "encryption")]
    pub fn set_encrypted<C>(&self, value: &C, master_key: &[u8]) -> Result<(), TraitKitError>
    where
        C: super::ModuleConfig + serde::Serialize,
    {
        use super::XChaCha20Crypto;

        // XChaCha20-Poly1305 requires a 256-bit (32-byte) key; HKDF needs
        // a reasonably sized input key material. Reject short keys early.
        if master_key.len() < 16 {
            return Err(TraitKitError::BuildFailed {
                context: "set_encrypted".into(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "master_key must be at least 16 bytes, got {}",
                        master_key.len()
                    ),
                )),
            });
        }

        // Derive before serializing: derivation does not depend on the
        // plaintext, so a derivation failure never leaves a generated
        // plaintext buffer behind.
        let mut field_key = derive_kit_field_key(master_key, C::PATH, "set_encrypted")?;

        let mut plaintext = match serde_json::to_vec(value) {
            Ok(vec) => vec,
            Err(e) => {
                // No plaintext exists on this path, but the derived key must
                // not outlive the call either.
                zeroize_bytes(&mut field_key);
                return Err(TraitKitError::BuildFailed {
                    context: "set_encrypted".into(),
                    source: Box::new(e),
                });
            }
        };

        // Compute the encryption Result first, then wipe both buffers before
        // propagating success or failure, so the plaintext serialization and
        // the derived key never outlive this call.
        let encrypted = XChaCha20Crypto::new().encrypt(&plaintext, &field_key);
        zeroize_bytes(&mut field_key);
        zeroize_bytes(&mut plaintext);
        let (nonce, ciphertext) = encrypted.map_err(|e| TraitKitError::BuildFailed {
            context: "set_encrypted".into(),
            source: Box::new(e),
        })?;

        self.encryption.encrypted_configs
            .borrow_mut()
            .insert(TypeId::of::<C>(), EncryptedBlob::new(nonce, ciphertext));
        Ok(())
    }

    /// Check if an encrypted config of type `C` is registered.
    #[cfg(feature = "encryption")]
    pub fn contains_encrypted<C: super::ModuleConfig>(&self) -> bool {
        self.encryption.encrypted_configs
            .borrow()
            .contains_key(&TypeId::of::<C>())
    }

    /// Load a configuration via `Configurable::load`, falling back to
    /// `ModuleConfig::default_value` if loading fails.
    ///
    /// Requires the `confers` feature. Stores the resulting value
    /// via `set_config`, overriding any prior value of the same type.
    ///
    /// # Returns
    ///
    /// `true` if `C::load()` succeeded, `false` if the default was used.
    /// The return value lets callers detect fallback without inspecting the
    /// stored value.
    ///
    /// # Errors
    ///
    /// Currently never returns an error, but the `Result` is reserved for
    /// future use (e.g. validation of the default value).
    #[cfg(feature = "confers")]
    pub fn load_config_or_default<C>(&self) -> Result<bool, TraitKitError>
    where
        C: super::Configurable + super::ModuleConfig,
    {
        match C::load() {
            Ok(value) => {
                self.set_config(value);
                Ok(true)
            }
            Err(_e) => {
                self.set_config(C::default_value());
                Ok(false)
            }
        }
    }
}

// ─── Config inheritance (confers feature) ─────────────────────────────────

impl<S> Kit<S> {
    /// Populate the config `TypeMap` with `C::default_value()` if no value of
    /// type `C` is present.
    ///
    /// Returns `true` if the default was populated, `false` if a value already
    /// existed (the existing value is not overridden).
    ///
    /// Requires the `confers` feature.
    #[cfg(feature = "confers")]
    pub fn populate_defaults<C: super::ModuleConfig>(&self) -> bool {
        if self.configs.contains::<C>() {
            return false;
        }
        self.configs.insert(C::default_value());
        true
    }

    /// Apply a compile-time safe field-level override to the config of type `C`.
    ///
    /// Reads the current config, applies non-`None` fields from `ovr`, and
    /// writes the result back. If no config of type `C` exists, this is a
    /// no-op (does not panic).
    ///
    /// Requires the `confers` feature.
    #[cfg(feature = "confers")]
    // 有意按值接收 Override(调用方构造临时对象后转移所有权,API 人体工学优先);
    // 改为 `&C::Override` 属公共 API 破坏性变更,不在本次配置统一范围内。
    #[allow(clippy::needless_pass_by_value)]
    pub fn merge_config<C: super::ConfigInherit>(&self, ovr: C::Override) {
        if let Ok(mut current) = self.config::<C>() {
            current.apply_override(&ovr);
            self.configs.insert(current);
        }
    }

    /// Extract shared fields from config `C` into the Kit's shared overlay.
    ///
    /// Calls `C::extract_shared()` and merges the result into
    /// `self.confers.shared_fields`. New values override same-named keys from
    /// previous extractions. If no config of type `C` exists, this is a
    /// no-op.
    ///
    /// Requires the `confers` feature.
    #[cfg(feature = "confers")]
    pub fn extract_shared<C: super::SharedConfig>(&self) {
        if let Ok(config) = self.config::<C>() {
            let fields = config.extract_shared();
            self.confers.shared_fields.borrow_mut().extend(fields);
        }
    }

    /// Inject shared fields from the Kit's overlay into config `C`.
    ///
    /// Reads the current shared overlay and calls `C::inject_shared()`.
    /// The updated config is written back to the `TypeMap`. If no config of
    /// type `C` exists, this is a no-op.
    ///
    /// Requires the `confers` feature.
    #[cfg(feature = "confers")]
    pub fn inject_shared<C: super::SharedConfig>(&self) {
        if let Ok(mut config) = self.config::<C>() {
            let overlay = self.confers.shared_fields.borrow().clone();
            config.inject_shared(&overlay);
            self.configs.insert(config);
        }
    }
}

impl Kit<Ready> {
    /// Retrieve an optional capability. Returns `None` if not built.
    pub fn optional<M: AutoBuilder>(&self) -> Option<M::Capability> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
    }

    /// Retrieve a capability by reference, avoiding `Clone`.
    ///
    /// Unlike `require()`, this returns a `Ref` borrowing the stored value
    /// directly, with no clone overhead. The `Ref` holds a read lock on the
    /// interior `RefCell` — while it is alive, calling `reload_config` or
    /// any mutating method will panic (`borrow_mut` conflict). Keep the
    /// `Ref` lifetime short.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingCapability` if the module has not been built.
    pub fn require_ref<M: AutoBuilder>(
        &self,
    ) -> Result<std::cell::Ref<'_, M::Capability>, TraitKitError>
    where
        M::Capability: 'static,
    {
        use std::cell::Ref;

        let type_id = TypeId::of::<M>();
        if !self.capabilities.contains_by_type_id(type_id) {
            return Err(TraitKitError::MissingCapability {
                key: M::NAME.to_string(),
            });
        }
        Ref::filter_map(self.capabilities.inner_ref(), |map| {
            map.get(&type_id)
                .and_then(|b| b.downcast_ref::<M::Capability>())
        })
        .map_err(|_| TraitKitError::MissingCapability {
            key: M::NAME.to_string(),
        })
    }

    /// Check if a capability has been built.
    pub fn contains<M: AutoBuilder>(&self) -> bool {
        self.capabilities.contains_by_type_id(TypeId::of::<M>())
    }

    /// Check if a config is registered.
    pub fn contains_config<C: Clone + 'static>(&self) -> bool {
        self.configs.contains::<C>()
    }

    // ─── Feature Toggle (Ready state) ──────────────────────────────────

    /// Check if a feature toggle is enabled (available after build).
    #[cfg(feature = "toggle")]
    pub fn is_toggle_enabled(&self, key: &str) -> bool {
        matches!(
            self.toggle.backend.borrow().get(key),
            Some(super::toggle::ToggleValue::Bool(true))
        )
    }

    /// Enable or disable a feature toggle (available after build).
    #[cfg(feature = "toggle")]
    pub fn enable_toggle(&self, key: impl Into<String>, enabled: bool) {
        let key = key.into();
        self.toggle
            .backend
            .borrow_mut()
            .set(key, super::toggle::ToggleValue::Bool(enabled));
    }

    /// Get a typed toggle value (available after build).
    #[cfg(feature = "toggle")]
    pub fn get_toggle(&self, key: &str) -> Option<super::toggle::ToggleValue> {
        self.toggle.backend.borrow().get(key)
    }

    /// Set a typed toggle value (available after build).
    #[cfg(feature = "toggle")]
    pub fn set_toggle(&self, key: impl Into<String>, value: super::toggle::ToggleValue) {
        self.toggle.backend.borrow_mut().set(key.into(), value);
    }

    /// List all toggles (available after build).
    #[cfg(feature = "toggle")]
    pub fn list_toggles(&self) -> Vec<(String, super::toggle::ToggleValue)> {
        self.toggle.backend.borrow().list()
    }

    // ─── Lifecycle: shutdown ───────────────────────────────────────────

    /// Shut down all lifecycle modules in reverse topological order.
    ///
    /// Calls `on_shutdown` for each module registered via `register_lifecycle`.
    /// A failed shutdown does not prevent other modules from shutting down
    /// (a panicking `on_shutdown` is isolated; the remaining callbacks still
    /// run). If any `on_ready` callback fails during `build()`, the
    /// constructed `Kit<Ready>` is discarded as a whole and no `on_shutdown`
    /// callback runs (sync and async behave identically).
    ///
    /// Requires the `lifecycle` feature.
    #[cfg(feature = "lifecycle")]
    pub fn shutdown(&self) {
        let callbacks: Vec<(TypeId, ShutdownCallback)> =
            self.lifecycle.shutdown_callbacks.borrow_mut().drain(..).collect();
        // Reverse order: last built → first shut down
        for (_type_id, callback) in callbacks.iter().rev() {
            // Contract: a failed shutdown does not block other modules.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                callback(&self.capabilities);
            }));
        }
    }

    // ─── Health Check ──────────────────────────────────────────────────

    /// Check the health of a specific module.
    ///
    /// Requires the `health` feature and the module to have been registered
    /// via `register_health_check::<M>()`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingConfig` if no health checker is
    /// registered for `M`.
    #[cfg(feature = "health")]
    pub fn health_check<M: crate::core::health::HealthCheck>(
        &self,
    ) -> Result<crate::core::health::HealthStatus, TraitKitError> {
        let type_id = TypeId::of::<M>();
        let checkers = self.health.health_checkers.borrow();
        let (_name, checker) = checkers.get(&type_id).ok_or(TraitKitError::MissingConfig {
            key: M::NAME.to_string(),
        })?;
        Ok(checker(&self.capabilities))
    }

    /// Generate a health report for all registered health checkers.
    ///
    /// Returns a list of `(module_name, HealthStatus)` pairs.
    ///
    /// Requires the `health` feature.
    #[cfg(feature = "health")]
    pub fn health_report(&self) -> Vec<(&'static str, crate::core::health::HealthStatus)> {
        let checkers = self.health.health_checkers.borrow();
        let report: Vec<(&'static str, crate::core::health::HealthStatus)> = checkers
            .values()
            .map(|(name, checker)| (*name, checker(&self.capabilities)))
            .collect();
        drop(checkers);
        // T208: publish each sampled status to the injected event bus.
        if self.ports.event_bus.borrow().is_some() {
            for (name, status) in &report {
                self.publish_event(super::events::KitEvent::HealthChanged {
                    module: name,
                    status: status.as_status_name(),
                    detail: status.detail().map(str::to_owned),
                });
            }
        }
        report
    }

    /// Aggregate the health of all registered checkers into a structured
    /// [`HealthAggregate`] (T205): worst-of overall status plus per-module
    /// entries, ready for a `/healthz` endpoint.
    ///
    /// Requires the `health` and `report` features. Serialize with
    /// [`HealthAggregate::to_json`](crate::core::health::HealthAggregate::to_json)
    /// or `Kit::health_json()`.
    #[cfg(all(feature = "health", feature = "report"))]
    pub fn health_aggregate(&self) -> crate::core::health::HealthAggregate {
        use crate::core::health::{HealthAggregate, HealthModuleEntry};

        let report = self.health_report();
        let mut worst_rank = 0u8;
        let modules = report
            .into_iter()
            .map(|(name, status)| {
                worst_rank = worst_rank.max(status.severity_rank());
                HealthModuleEntry {
                    module: name,
                    status: status.as_status_name(),
                    detail: status.detail().map(str::to_owned),
                }
            })
            .collect::<Vec<_>>();
        let status = match worst_rank {
            0 => "healthy",
            1 => "degraded",
            _ => "unhealthy",
        };
        HealthAggregate {
            status,
            healthy: worst_rank == 0,
            modules,
        }
    }

    /// Aggregate health JSON string (T205) — the direct `/healthz` payload.
    ///
    /// See [`Kit::health_aggregate`] for the structured form. Requires the
    /// `health` and `report` features.
    #[cfg(all(feature = "health", feature = "report"))]
    pub fn health_json(&self) -> String {
        self.health_aggregate().to_json()
    }

    // ─── Factory Pattern ───────────────────────────────────────────────

    /// Create a factory closure that produces new instances on each call.
    ///
    /// Unlike `require()` which returns the singleton built during `build()`,
    /// the factory invokes `M::build()` on every call, producing a fresh
    /// instance each time.
    ///
    pub fn factory<M: AutoBuilder>(
        &self,
    ) -> impl Fn() -> Result<M::Capability, TraitKitError> + '_ {
        move || {
            // Compile-time layout assertion (mirrors `require()`): the cast is
            // only sound while `S` appears solely in `PhantomData<S>`.
            const _: () = assert!(
                std::mem::size_of::<Kit<Ready>>() == std::mem::size_of::<Kit>(),
                "Kit layout changed; unsafe cast is no longer sound"
            );
            const _: () = assert!(
                std::mem::align_of::<Kit<Ready>>() == std::mem::align_of::<Kit>(),
                "Kit layout changed; unsafe cast is no longer sound"
            );
            // SAFETY: Kit<Ready> and Kit<Unbuilt> have identical memory layout
            // (S only appears in PhantomData<S>). BuildFn expects &Kit<Unbuilt>.
            #[allow(unsafe_code)]
            let kit_ref: &Kit = unsafe { &*std::ptr::from_ref::<Kit<Ready>>(self).cast::<Kit>() };
            M::build(kit_ref).map_err(|e| TraitKitError::BuildFailed {
                context: M::NAME.to_string(),
                source: Box::new(e),
            })
        }
    }

    // ─── Scope ─────────────────────────────────────────────────────────

    /// Create a new empty scope for per-request instance isolation.
    ///
    /// Requires the `scope` feature.
    #[cfg(feature = "scope")]
    #[must_use]
    pub fn create_scope(&self) -> super::scope::Scope {
        super::scope::Scope::new()
    }

    /// Create a scope bound to this Kit as its parent context (T206).
    ///
    /// The scope can resolve this Kit's capabilities read-only via
    /// `scope.parent::<M>()` (per-request modules plus shared parent
    /// singletons). The scope holds a `Weak` back-reference only, so dropping
    /// the parent invalidates parent queries instead of creating a retain
    /// cycle — cycle protection by construction.
    ///
    /// Requires the `scope` feature. `Kit` is `!Sync`, so the `Rc` here is a
    /// single-threaded owner — consistent with the sync thread model.
    #[cfg(feature = "scope")]
    #[must_use]
    pub fn create_scope_from(self: &std::rc::Rc<Self>) -> super::scope::Scope {
        super::scope::Scope::with_parent(std::rc::Rc::downgrade(self))
    }

    // ─── i18n: module-owned translations (T207) ────────────────────────

    /// Raw module-owned FTL fragments collected at registration time
    /// (`(locale, ftl_source)` pairs, registration order).
    ///
    /// Requires the `i18n` feature.
    #[cfg(feature = "i18n")]
    #[must_use]
    pub fn i18n_module_ftl(&self) -> Vec<(&'static str, &'static str)> {
        self.i18n.module_ftl.borrow().clone()
    }

    /// Translate a message preferring the kit-local module overlay (T207).
    ///
    /// Lookup order: (1) the overlay catalog merged at `build()` from module
    /// fragments matching the active locale, (2) the global `tr()` catalog.
    /// Before `build()` — or when no module contributed fragments — this is
    /// exactly the global `tr()`.
    ///
    /// Requires the `i18n` feature.
    #[cfg(feature = "i18n")]
    #[must_use]
    pub fn module_tr(&self, message_id: &str, args: &[(&str, &str)]) -> String {
        if let Some(catalog) = self.i18n.module_catalog.borrow().as_ref() {
            let translated = catalog.translate(message_id, args);
            if translated != message_id {
                return translated;
            }
        }
        crate::i18n::tr(message_id, args)
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

    /// Structured, machine-readable build report (T202).
    ///
    /// Companion to the human-oriented `graph_dot()` / `graph_mermaid()`
    /// exports: module list with build states (built / lazy / overridden),
    /// validated topological order, per-module construction time, override
    /// sources, and the total structural build time.
    ///
    /// Requires the `report` feature. Serialize with
    /// [`BuildReport::to_json`](crate::kit::report::BuildReport::to_json).
    #[cfg(feature = "report")]
    #[must_use]
    pub fn build_report(&self) -> super::report::BuildReport {
        self.report.snapshot()
    }

    /// Retrieve and decrypt a configuration value.
    ///
    /// Requires the `encryption` feature. Looks up the encrypted
    /// blob for type `C`, derives the per-field key from `master_key` and
    /// `C::PATH`, decrypts with XChaCha20-Poly1305, then deserializes from
    /// JSON. The `master_key` must match the one passed to `set_encrypted`.
    ///
    /// # Security
    ///
    /// `master_key` is a caller-owned `&[u8]`: this method cannot wipe the
    /// caller's buffer, so zeroing it remains the caller's responsibility.
    /// The internally derived per-field `field_key` is zeroed with a volatile
    /// write as soon as decryption is done; the decrypted intermediate JSON
    /// buffer is zeroed right after deserialization, and only the parsed `C`
    /// is returned to the caller.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingConfig` if no encrypted blob for `C` exists.
    /// Returns `TraitKitError::BuildFailed` if key derivation, decryption, or
    /// deserialization fails (e.g. wrong master key, tampered ciphertext).
    #[cfg(feature = "encryption")]
    pub fn get_encrypted<C>(&self, master_key: &[u8]) -> Result<C, TraitKitError>
    where
        C: super::ModuleConfig + serde::de::DeserializeOwned,
    {
        use super::XChaCha20Crypto;

        if master_key.len() < 16 {
            return Err(TraitKitError::BuildFailed {
                context: "get_encrypted".into(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "master_key must be at least 16 bytes, got {}",
                        master_key.len()
                    ),
                )),
            });
        }

        let blob = self
            .encryption
            .encrypted_configs
            .borrow()
            .get(&TypeId::of::<C>())
            .cloned()
            .ok_or(TraitKitError::MissingConfig {
                key: std::any::type_name::<C>().to_string(),
            })?;

        let mut field_key = derive_kit_field_key(master_key, C::PATH, "get_encrypted")?;

        // Compute the decryption Result first, then wipe the derived key
        // before propagating success or failure. The intermediate plaintext
        // is wiped right after parsing; only the deserialized `C` escapes
        // this call.
        let decrypted = XChaCha20Crypto::new().decrypt(blob.nonce(), blob.ciphertext(), &field_key);
        zeroize_bytes(&mut field_key);
        let mut plaintext = decrypted.map_err(|e| TraitKitError::BuildFailed {
            context: "get_encrypted".into(),
            source: Box::new(e),
        })?;

        let parsed: Result<C, _> = serde_json::from_slice(&plaintext);
        zeroize_bytes(&mut plaintext);
        parsed.map_err(|e| TraitKitError::BuildFailed {
            context: "get_encrypted".into(),
            source: Box::new(e),
        })
    }

    // ─── Observation Port Accessors ────────────────────────────────────

    /// Retrieve the injected [`MetricsPort`], if any.
    #[must_use]
    pub fn metrics_port(&self) -> super::ports::OptionalMetricsPort {
        self.ports.metrics_port.borrow().clone()
    }

    /// Retrieve the injected [`LogPort`], if any.
    #[must_use]
    pub fn log_port(&self) -> super::ports::OptionalLogPort {
        self.ports.log_port.borrow().clone()
    }
}

impl Default for Kit {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Kit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kit<Unbuilt>")
            .field("modules", &self.graph.entries().len())
            .field("configs", &self.configs.len())
            .finish()
    }
}

impl std::fmt::Debug for Kit<Ready> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kit<Ready>")
            .field("modules", &self.graph.entries().len())
            .field("configs", &self.configs.len())
            .finish()
    }
}

#[cfg(test)]
#[path = "kit_tests.rs"]
mod kit_tests;

#[cfg(all(test, feature = "i18n"))]
mod i18n_module_tests {
    use crate::core::{AutoBuilder, ModuleMeta};
    use crate::kit::Kit;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct I18nCap;

    #[derive(Debug)]
    struct I18nTestError;

    impl std::fmt::Display for I18nTestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "i18n test error")
        }
    }
    impl std::error::Error for I18nTestError {}

    /// Module carrying its own FTL fragment for both locale buckets.
    struct I18nGreetingModule;
    impl ModuleMeta for I18nGreetingModule {
        const NAME: &'static str = "i18n-greeting";
        fn i18n_ftl() -> &'static [(&'static str, &'static str)] {
            &[
                (
                    "zh-CN",
                    "greet-hello = 你好，{ $name }！\ngreet-only = 模块私有消息",
                ),
                ("en-US", "greet-hello = Hello, { $name }!\ngreet-only = module-private message"),
            ]
        }
    }
    impl AutoBuilder for I18nGreetingModule {
        type Capability = Arc<I18nCap>;
        type Error = I18nTestError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(I18nCap))
        }
    }

    /// Second module contributing an additional fragment.
    struct I18nFarewellModule;
    impl ModuleMeta for I18nFarewellModule {
        const NAME: &'static str = "i18n-farewell";
        fn i18n_ftl() -> &'static [(&'static str, &'static str)] {
            &[("en-US", "greet-bye = Goodbye")]
        }
    }
    impl AutoBuilder for I18nFarewellModule {
        type Capability = Arc<I18nCap>;
        type Error = I18nTestError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(I18nCap))
        }
    }

    #[test]
    fn module_ftl_fragments_collected_at_registration() {
        let mut kit = Kit::new();
        kit.register::<I18nGreetingModule>().expect("register");
        kit.register_lazy::<I18nFarewellModule>().expect("register");
        let ready = kit.build().expect("build ok");

        let fragments = ready.i18n_module_ftl();
        assert_eq!(fragments.len(), 3, "1 zh + 2 en fragments");
        assert!(fragments.iter().any(|(l, _)| *l == "zh-CN"));
        assert!(fragments.iter().any(|(l, _)| *l == "en-US"));
    }

    #[test]
    fn module_tr_resolves_module_private_keys_via_overlay() {
        let mut kit = Kit::new();
        kit.register::<I18nGreetingModule>().expect("register");
        let ready = kit.build().expect("build ok");

        // Placeholder substitution through the merged overlay.
        let hello = ready.module_tr("greet-hello", &[("name", "Kit")]);
        assert!(
            hello.contains("Kit") && hello != "greet-hello",
            "module overlay must resolve its own key: {hello}"
        );
    }

    #[test]
    fn module_tr_falls_back_to_global_catalog() {
        let mut kit = Kit::new();
        kit.register::<I18nGreetingModule>().expect("register");
        let ready = kit.build().expect("build ok");

        // Built-in catalog key resolved through the fallback path.
        let msg = ready.module_tr(
            "trait-kit-error-missing-capability",
            &[("key", "some-key")],
        );
        assert!(
            msg.contains("some-key") && msg != "trait-kit-error-missing-capability",
            "fallback must reach the global catalog: {msg}"
        );
        // Unknown key everywhere → returned verbatim.
        assert_eq!(ready.module_tr("no-such-key-anywhere", &[]), "no-such-key-anywhere");
    }

    #[test]
    fn default_i18n_ftl_is_empty() {
        struct Plain;
        impl ModuleMeta for Plain {
            const NAME: &'static str = "plain";
        }
        assert!(<Plain as ModuleMeta>::i18n_ftl().is_empty());
    }
}

#[cfg(test)]
mod event_bus_tests {
    use crate::core::{AutoBuilder, ModuleMeta};
    use crate::kit::events::{KitEvent, MemoryEventBus};
    use crate::kit::Kit;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    struct BusCap;

    #[derive(Debug)]
    struct BusError;

    impl std::fmt::Display for BusError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "bus test error")
        }
    }
    impl std::error::Error for BusError {}

    struct BusLeaf;
    impl ModuleMeta for BusLeaf {
        const NAME: &'static str = "bus-leaf";
    }
    impl AutoBuilder for BusLeaf {
        type Capability = Arc<BusCap>;
        type Error = BusError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(BusCap))
        }
    }

    struct BusTop;
    impl ModuleMeta for BusTop {
        const NAME: &'static str = "bus-top";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            static DEPS: &[(&str, std::any::TypeId)] =
                &[(<BusLeaf as ModuleMeta>::NAME, std::any::TypeId::of::<BusLeaf>())];
            DEPS
        }
    }
    impl AutoBuilder for BusTop {
        type Capability = Arc<BusCap>;
        type Error = BusError;
        fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
            kit.require::<BusLeaf>().map_err(|_| BusError)?;
            Ok(Arc::new(BusCap))
        }
    }

    #[test]
    fn module_built_events_published_in_build_order() {
        let bus = Arc::new(MemoryEventBus::new());
        let seen: Arc<Mutex<Vec<(&'static str, u64)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        bus.subscribe(move |event| {
            if let KitEvent::ModuleBuilt { module, elapsed_us } = event {
                sink.lock().unwrap().push((*module, *elapsed_us));
            }
        });

        let mut kit = Kit::new();
        kit.with_event_bus(Some(Arc::clone(&bus) as Arc<dyn crate::kit::events::EventBus>));
        kit.register::<BusTop>().expect("register top");
        kit.register::<BusLeaf>().expect("register leaf");
        let ready = kit.build().expect("build ok");

        let log = seen.lock().unwrap();
        assert_eq!(
            log.len(),
            2,
            "both modules published ModuleBuilt (topo order)"
        );
        assert_eq!(log[0].0, "bus-leaf", "leaf built first (topological)");
        assert_eq!(log[1].0, "bus-top");
        // Ready kit keeps the injected bus accessible.
        assert!(ready.event_bus().is_some());
    }

    #[test]
    fn build_without_bus_publishes_nothing_and_succeeds() {
        let mut kit = Kit::new();
        kit.register::<BusLeaf>().expect("register");
        let ready = kit.build().expect("build ok without event bus");
        assert!(ready.event_bus().is_none(), "default bus is None (no-op)");
    }

    #[cfg(feature = "health")]
    #[test]
    fn health_report_publishes_health_changed_events() {
        use crate::core::HealthCheck;
        use crate::core::HealthStatus;

        struct BusSick;
        impl ModuleMeta for BusSick {
            const NAME: &'static str = "bus-sick";
        }
        impl AutoBuilder for BusSick {
            type Capability = Arc<BusCap>;
            type Error = BusError;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                Ok(Arc::new(BusCap))
            }
        }
        impl HealthCheck for BusSick {
            fn check(_cap: &Self::Capability) -> HealthStatus {
                HealthStatus::unhealthy("simulated failure")
            }
        }

        let bus = Arc::new(MemoryEventBus::new());
        let seen: Arc<Mutex<Vec<(&'static str, &'static str)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        bus.subscribe(move |event| {
            if let KitEvent::HealthChanged { module, status, .. } = event {
                sink.lock().unwrap().push((*module, *status));
            }
        });

        let mut kit = Kit::new();
        kit.with_event_bus(Some(Arc::clone(&bus) as Arc<dyn crate::kit::events::EventBus>));
        kit.register::<BusSick>().expect("register");
        kit.register_health_check::<BusSick>();
        let ready = kit.build().expect("build ok");
        let report = ready.health_report();
        let _ = report;

        let log = seen.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0], ("bus-sick", "unhealthy"));
    }
}

#[cfg(test)]
mod require_error_kind_tests {
    use crate::core::{AutoBuilder, ModuleMeta};
    use crate::error::ErrorKind;
    use crate::kit::Kit;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct StringCap(String);

    #[derive(Debug, Clone)]
    struct OtherCap;

    #[derive(Debug)]
    struct KindError;

    impl std::fmt::Display for KindError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "kind error")
        }
    }
    impl std::error::Error for KindError {}

    struct KindModule;
    impl ModuleMeta for KindModule {
        const NAME: &'static str = "kind-module";
    }
    impl AutoBuilder for KindModule {
        type Capability = Arc<StringCap>;
        type Error = KindError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(StringCap("cap".into())))
        }
    }

    #[test]
    fn missing_capability_classifies_as_missing() {
        let kit = Kit::new().build().expect("build ok");
        let err = kit.require::<KindModule>().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Missing);
        assert!(matches!(err, crate::TraitKitError::MissingCapability { .. }));
    }

    #[test]
    fn wrong_type_capability_classifies_as_type_mismatch() {
        let mut kit = Kit::new();
        kit.register::<KindModule>().expect("register");
        let ready = kit.build().expect("build ok");
        // Simulate a broken invariant (capability box of a different type under
        // the module's TypeId) via the crate-internal TypeMap so the T210
        // detection path in `require` is exercised. Insert AFTER build, since
        // build would otherwise overwrite the box with the real capability.
        ready.capabilities.insert_boxed(
            std::any::TypeId::of::<KindModule>(),
            Box::new(Arc::new(OtherCap)),
        );

        let err = ready.require::<KindModule>().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::TypeMismatch, "got: {err:?}");
        assert!(
            matches!(err, crate::TraitKitError::CapabilityTypeMismatch { .. }),
            "wrong-type capability must surface TypeMismatch, got {err:?}"
        );
    }

    #[test]
    fn kind_covers_all_variants() {
        let missing = crate::TraitKitError::MissingCapability { key: "k".into() };
        let cfg = crate::TraitKitError::MissingConfig { key: "c".into() };
        let mismatch = crate::TraitKitError::CapabilityTypeMismatch { key: "k".into() };
        let build = crate::TraitKitError::BuildFailed {
            context: "m".into(),
            source: Box::new(std::io::Error::other("x")),
        };
        let reg = crate::TraitKitError::AlreadyRegistered { module: "m" };
        assert_eq!(missing.kind(), ErrorKind::Missing);
        assert_eq!(cfg.kind(), ErrorKind::Missing);
        assert_eq!(mismatch.kind(), ErrorKind::TypeMismatch);
        assert_eq!(build.kind(), ErrorKind::InitFailed);
        assert_eq!(reg.kind(), ErrorKind::Other);
    }
}
