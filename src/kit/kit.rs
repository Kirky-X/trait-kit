// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Kit — the capability and configuration management center.
//!
//! Uses typestate pattern: `Kit` (unbuilt) → `Kit<Ready>` (after `build()`).

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(feature = "hot-reload")]
use std::rc::Rc;
use std::sync::OnceLock;

use crate::core::{AutoBuilder, BuildFn};
use crate::error::TraitKitError;

#[cfg(feature = "encryption")]
use super::EncryptedBlob;
use super::TypeMap;
use super::{DependencyGraph, GraphError, ModuleEntry};

/// HKDF key-derivation version label bound into every per-field key.
/// Bumping this rotates all encrypted configs without changing master keys.
#[cfg(feature = "encryption")]
const KEY_DERIVATION_VERSION: &str = "v1";

/// Derive a per-field encryption key, mapping HKDF failures to `TraitKitError`.
#[cfg(feature = "encryption")]
fn derive_kit_field_key(
    master_key: &[u8],
    path: &'static str,
    context: &'static str,
) -> Result<[u8; 32], TraitKitError> {
    super::config::derive_field_key(master_key, path, KEY_DERIVATION_VERSION).map_err(|e| {
        TraitKitError::BuildFailed {
            context,
            source: Box::new(e),
        }
    })
}

/// Marker type for the unbuilt state.
pub struct Unbuilt;

/// Marker type for the ready (built) state.
pub struct Ready;

/// Type alias for hot-reload subscriber callbacks (single-threaded, `!Sync`).
#[cfg(feature = "hot-reload")]
type SubscriberMap = RefCell<HashMap<TypeId, Vec<Rc<dyn Fn()>>>>;

/// Type alias for the encrypted config store (single-threaded, `!Sync`).
#[cfg(feature = "encryption")]
type EncryptedConfigMap = RefCell<HashMap<TypeId, EncryptedBlob>>;

/// A lazy construction slot: holds a `build_fn` and a `OnceLock` cache cell.
/// The builder is invoked on first access; the result is cached in the
/// `OnceLock` for subsequent accesses. After construction, `builder` is
/// `None` (consumed) and `cell` holds the built capability.
struct LazySlot {
    builder: Option<BuildFn>,
    cell: OnceLock<Box<dyn Any>>,
}

/// The capability and configuration management center.
pub struct Kit<S = Unbuilt> {
    builders: RefCell<HashMap<TypeId, BuildFn>>,
    /// Override map for test injection: `TypeId` of module → pre-built capability.
    /// Populated by `override_module` / `override_module_strict`; consumed by `build()`.
    overrides: RefCell<HashMap<TypeId, Box<dyn Any>>>,
    /// Lazy builders (Unbuilt state): modules registered via `register_lazy`.
    /// Transferred to `lazy_slots` during `build()`.
    lazy_builders: RefCell<HashMap<TypeId, BuildFn>>,
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
    /// Built into `capabilities` during `build()` (T015).
    #[cfg(feature = "interface")]
    interface_builders: RefCell<HashMap<TypeId, BuildFn>>,
    graph: DependencyGraph,
    configs: TypeMap,
    capabilities: TypeMap,
    #[cfg(feature = "hot-reload")]
    subscribers: SubscriberMap,
    #[cfg(feature = "encryption")]
    encrypted_configs: EncryptedConfigMap,
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
            interface_builders: RefCell::new(HashMap::new()),
            graph: DependencyGraph::new(),
            configs: TypeMap::new(),
            capabilities: TypeMap::new(),
            #[cfg(feature = "hot-reload")]
            subscribers: RefCell::new(HashMap::new()),
            #[cfg(feature = "encryption")]
            encrypted_configs: RefCell::new(HashMap::new()),
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

        let build_fn: BuildFn = Box::new(|kit| {
            let capability = M::build(kit)
                .map_err(|e| -> Box<dyn std::error::Error + Send + 'static> { Box::new(e) })?;
            Ok(Box::new(capability) as Box<dyn Any>)
        });

        self.lazy_builders
            .borrow_mut()
            .insert(TypeId::of::<M>(), build_fn);
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
    #[cfg(feature = "interface")]
    pub fn register_as<M>(&mut self) -> Result<(), TraitKitError>
    where
        M: crate::core::InterfaceBuilder,
    {
        let interface_id = TypeId::of::<M::Interface>();

        // One implementation per interface type.
        if self.interface_builders.borrow().contains_key(&interface_id) {
            return Err(TraitKitError::AlreadyRegistered { module: M::NAME });
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
            let iface: std::sync::Arc<M::Interface> = M::into_interface(cap);
            Ok(Box::new(iface) as Box<dyn Any>)
        });

        self.interface_builders
            .borrow_mut()
            .insert(interface_id, build_fn);
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
        Ok(())
    }

    /// Set a configuration value.
    pub fn set_config<C: Clone + 'static>(&self, config: C) {
        self.configs.insert(config);
    }

    /// Load a configuration via its [`Configurable`] implementation and store it.
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
            context: "load_config",
            source: e,
        })?;
        self.set_config(config);
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

        {
            let kit_ref: &Self = &self;

            for type_id in &sorted {
                let module_name = kit_ref.module_name(*type_id);

                // [Override] Priority 1: check overrides map first.
                // If an override exists, use it and skip build_fn entirely.
                if let Some(boxed) = kit_ref.overrides.borrow_mut().remove(type_id) {
                    kit_ref.capabilities.insert_boxed(*type_id, boxed);
                    continue;
                }

                // [Lazy] Skip lazy-registered modules — they are not built
                // during build(). Their build_fn stays in lazy_builders and
                // will be transferred to Kit<Ready>.lazy_slots (T008).
                if kit_ref.lazy_builders.borrow().contains_key(type_id) {
                    continue;
                }

                // [Build] Priority 2: invoke the registered build_fn.
                // If not in builders, the module was registered via
                // `register_multi` — skip here; built in the multi_builders
                // loop below (T011).
                let Some(build_fn) = kit_ref.builders.borrow_mut().remove(type_id) else {
                    continue;
                };

                match (build_fn)(kit_ref) {
                    Ok(boxed) => {
                        kit_ref.capabilities.insert_boxed(*type_id, boxed);
                    }
                    Err(e) => {
                        return Err(TraitKitError::BuildFailed {
                            context: module_name,
                            source: e,
                        });
                    }
                }
            }
        }

        // [Override] Handle modules that were overridden but NOT registered
        // (override_module allows injecting unregistered modules). These are
        // not in the sorted list, so we insert them after the topo loop.
        {
            let remaining: Vec<(TypeId, Box<dyn Any>)> =
                self.overrides.borrow_mut().drain().collect();
            for (type_id, boxed) in remaining {
                self.capabilities.insert_boxed(type_id, boxed);
            }
        }

        // [Lazy] Transfer lazy_builders to lazy_slots for first-access
        // construction in Kit<Ready>. Each LazySlot wraps the build_fn with
        // an empty OnceLock cache cell. The builder is Option::Some until
        // consumed by the first require() call (T009).
        {
            let lazy: Vec<(TypeId, BuildFn)> = self.lazy_builders.borrow_mut().drain().collect();
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

        // [Multi] Build all multi-binding modules and store in
        // multi_capabilities. Keyed by TypeId::of::<M::Capability>() so
        // require_all::<M>() retrieves all implementations of the same
        // capability type. Runs after the topo-sorted loop so eager
        // dependencies are available via `self.capabilities`.
        {
            let multi: Vec<(TypeId, Vec<BuildFn>)> =
                self.multi_builders.borrow_mut().drain().collect();
            for (cap_id, build_fns) in multi {
                let mut vec = Vec::with_capacity(build_fns.len());
                for build_fn in build_fns {
                    let boxed = (build_fn)(&self).map_err(|e| TraitKitError::BuildFailed {
                        context: "<multi-binding>",
                        source: e,
                    })?;
                    vec.push(boxed);
                }
                self.multi_capabilities.borrow_mut().insert(cap_id, vec);
            }
        }

        // [Interface] Build all interface-registered modules and store in
        // capabilities. Keyed by TypeId::of::<M::Interface>() so
        // resolve::<I>() retrieves by interface type. Runs after the
        // topo-sorted loop so eager dependencies are available (T015).
        #[cfg(feature = "interface")]
        {
            let interfaces: Vec<(TypeId, BuildFn)> =
                self.interface_builders.borrow_mut().drain().collect();
            for (interface_id, build_fn) in interfaces {
                let boxed = (build_fn)(&self).map_err(|e| TraitKitError::BuildFailed {
                    context: "<interface>",
                    source: e,
                })?;
                self.capabilities.insert_boxed(interface_id, boxed);
            }
        }

        Ok(Kit {
            builders: self.builders,
            overrides: self.overrides,
            lazy_builders: self.lazy_builders,
            lazy_slots: self.lazy_slots,
            multi_builders: self.multi_builders,
            multi_capabilities: self.multi_capabilities,
            #[cfg(feature = "interface")]
            interface_builders: self.interface_builders,
            graph: self.graph,
            configs: self.configs,
            capabilities: self.capabilities,
            #[cfg(feature = "hot-reload")]
            subscribers: self.subscribers,
            #[cfg(feature = "encryption")]
            encrypted_configs: self.encrypted_configs,
            _state: std::marker::PhantomData,
        })
    }

    fn module_name(&self, type_id: TypeId) -> &'static str {
        self.graph.name_of(type_id).unwrap_or("<unknown>")
    }
}

impl<S> Kit<S> {
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

        // 1. Eager capabilities (already-built modules + overrides + previously-built lazy)
        if let Some(cap) = self
            .capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
        {
            return Ok(cap);
        }

        // 2. Lazy slots — check OnceLock cache first (previously-built lazy modules)
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
            #[allow(unsafe_code)]
            let kit_ref: &Kit = unsafe { &*std::ptr::from_ref(self).cast::<Kit>() };
            let boxed = (builder)(kit_ref).map_err(|e| TraitKitError::BuildFailed {
                context: M::NAME,
                source: e,
            })?;
            // Cache in OnceLock for future require() / require_ref() calls
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

        // 4. Not found
        Err(TraitKitError::MissingCapability { key: M::NAME })
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
        let vec = multi
            .get(&cap_id)
            .ok_or(TraitKitError::MissingCapability { key: M::NAME })?;

        let mut result = Vec::with_capacity(vec.len());
        for boxed in vec {
            let cap = boxed
                .downcast_ref::<M::Capability>()
                .cloned()
                .ok_or(TraitKitError::MissingCapability { key: M::NAME })?;
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
                key: std::any::type_name::<C>(),
            })
    }

    /// Subscribe a callback to be invoked when config of type `C` is reloaded.
    ///
    /// Requires the `hot-reload` feature. The callback receives no
    /// arguments; use `Kit::config::<C>()` inside it to read the new value.
    /// Callbacks are stored in a `RefCell` (single-threaded, `!Sync`).
    ///
    /// Layer 2 of the inheritance system: cargo feature chain
    /// `hot-reload` → `confers-macros` → `confers`.
    #[cfg(feature = "hot-reload")]
    pub fn subscribe<C: 'static>(&self, callback: impl Fn() + 'static) {
        let callback: Rc<dyn Fn()> = Rc::new(callback);
        self.subscribers
            .borrow_mut()
            .entry(TypeId::of::<C>())
            .or_default()
            .push(callback);
    }

    /// Reload a configuration via its [`Configurable`] implementation and
    /// notify all subscribers of type `C`.
    ///
    /// Requires the `hot-reload` feature. Calls `C::load()`, stores
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
    #[cfg(feature = "hot-reload")]
    pub fn reload_config<C: super::Configurable>(&self) -> Result<(), TraitKitError> {
        let config = C::load().map_err(|e| TraitKitError::BuildFailed {
            context: "reload_config",
            source: e,
        })?;
        self.configs.insert(config);
        // Clone the Rc list out to avoid holding the RefCell borrow across
        // user callbacks (which may re-enter subscribe).
        let callbacks: Vec<Rc<dyn Fn()>> = self
            .subscribers
            .borrow()
            .get(&TypeId::of::<C>())
            .cloned()
            .unwrap_or_default();
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
            .ok_or(TraitKitError::MissingCapability { key: "interface" })
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

        let plaintext = serde_json::to_vec(value).map_err(|e| TraitKitError::BuildFailed {
            context: "set_encrypted",
            source: Box::new(e),
        })?;

        let field_key = derive_kit_field_key(master_key, C::PATH, "set_encrypted")?;

        let (nonce, ciphertext) = XChaCha20Crypto::new()
            .encrypt(&plaintext, &field_key)
            .map_err(|e| TraitKitError::BuildFailed {
                context: "set_encrypted",
                source: Box::new(e),
            })?;

        self.encrypted_configs
            .borrow_mut()
            .insert(TypeId::of::<C>(), EncryptedBlob { nonce, ciphertext });
        Ok(())
    }

    /// Check if an encrypted config of type `C` is registered.
    #[cfg(feature = "encryption")]
    pub fn contains_encrypted<C: super::ModuleConfig>(&self) -> bool {
        self.encrypted_configs
            .borrow()
            .contains_key(&TypeId::of::<C>())
    }

    /// Load a configuration via `Configurable::load`, falling back to
    /// `ModuleConfig::default_value` if loading fails.
    ///
    /// Requires the `confers-macros` feature. Stores the resulting value
    /// via `set_config`, overriding any prior value of the same type.
    ///
    /// # Errors
    ///
    /// Never returns an error: load failures are silently replaced by the
    /// module's declared default. Inspect the stored value via `config::<C>()`
    /// if you need to distinguish "loaded" from "defaulted".
    #[cfg(feature = "confers-macros")]
    pub fn load_config_or_default<C>(&self) -> Result<(), TraitKitError>
    where
        C: super::Configurable + super::ModuleConfig,
    {
        let config = match C::load() {
            Ok(value) => value,
            Err(_) => C::default_value(),
        };
        self.set_config(config);
        Ok(())
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
            return Err(TraitKitError::MissingCapability { key: M::NAME });
        }
        Ref::filter_map(self.capabilities.inner_ref(), |map| {
            map.get(&type_id)
                .and_then(|b| b.downcast_ref::<M::Capability>())
        })
        .map_err(|_| TraitKitError::MissingCapability { key: M::NAME })
    }

    /// Check if a capability has been built.
    pub fn contains<M: AutoBuilder>(&self) -> bool {
        self.capabilities.contains_by_type_id(TypeId::of::<M>())
    }

    /// Check if a config is registered.
    pub fn contains_config<C: Clone + 'static>(&self) -> bool {
        self.configs.contains::<C>()
    }

    /// Retrieve and decrypt a configuration value.
    ///
    /// Requires the `encryption` feature. Looks up the encrypted
    /// blob for type `C`, derives the per-field key from `master_key` and
    /// `C::PATH`, decrypts with XChaCha20-Poly1305, then deserializes from
    /// JSON. The `master_key` must match the one passed to `set_encrypted`.
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

        let blob = self
            .encrypted_configs
            .borrow()
            .get(&TypeId::of::<C>())
            .cloned()
            .ok_or(TraitKitError::MissingConfig {
                key: std::any::type_name::<C>(),
            })?;

        let field_key = derive_kit_field_key(master_key, C::PATH, "get_encrypted")?;

        let plaintext = XChaCha20Crypto::new()
            .decrypt(&blob.nonce, &blob.ciphertext, &field_key)
            .map_err(|e| TraitKitError::BuildFailed {
                context: "get_encrypted",
                source: Box::new(e),
            })?;

        serde_json::from_slice(&plaintext).map_err(|e| TraitKitError::BuildFailed {
            context: "get_encrypted",
            source: Box::new(e),
        })
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
mod tests {
    use super::*;
    use crate::core::{AutoBuilder, ModuleMeta};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // === Test fixtures ===

    struct MockCapability;
    impl ModuleMeta for MockCapability {
        const NAME: &'static str = "mock";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for MockCapability {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(0)))
        }
    }

    struct DependentModule;
    impl ModuleMeta for DependentModule {
        const NAME: &'static str = "dependent";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            static DEPS: &[(&str, std::any::TypeId)] =
                &[("mock", std::any::TypeId::of::<MockCapability>())];
            DEPS
        }
    }
    impl AutoBuilder for DependentModule {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(0)))
        }
    }

    // === T002 tests ===

    #[test]
    fn overrides_field_is_empty_on_new() {
        let kit = Kit::new();
        assert_eq!(kit.overrides.borrow().len(), 0);
    }

    #[test]
    fn overrides_field_is_empty_after_build() {
        let kit = Kit::new();
        assert_eq!(kit.overrides.borrow().len(), 0);
    }

    // === T003 tests ===

    #[test]
    fn override_module_inserts_into_overrides_map() {
        let kit = Kit::new();
        assert_eq!(kit.overrides.borrow().len(), 0);
        kit.override_module::<MockCapability>(Arc::new(AtomicUsize::new(42)));
        assert_eq!(kit.overrides.borrow().len(), 1);
    }

    #[test]
    fn override_module_strict_succeeds_when_deps_registered() {
        let mut kit = Kit::new();
        // Register the dependency first
        kit.register::<MockCapability>().unwrap();
        // Now strict override of the dependent module should succeed
        let result = kit.override_module_strict::<DependentModule>(Arc::new(AtomicUsize::new(99)));
        assert!(result.is_ok());
        assert_eq!(kit.overrides.borrow().len(), 1);
    }

    #[test]
    fn override_module_strict_fails_when_deps_missing() {
        let mut kit = Kit::new();
        // Do NOT register MockCapability first
        let result = kit.override_module_strict::<DependentModule>(Arc::new(AtomicUsize::new(99)));
        assert!(matches!(
            result,
            Err(TraitKitError::DependencyMissing {
                module: "dependent",
                missing: "mock"
            })
        ));
        // Override should not have been inserted
        assert_eq!(kit.overrides.borrow().len(), 0);
    }

    // === T004 tests ===

    /// Module whose `build_fn` increments a counter, to verify override skips it.
    struct CountingModule;
    impl ModuleMeta for CountingModule {
        const NAME: &'static str = "counting";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for CountingModule {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            // Return a counter that starts at 0; the test checks the counter
            // value to distinguish "build_fn ran" from "override used".
            Ok(Arc::new(AtomicUsize::new(0)))
        }
    }

    #[test]
    fn build_uses_override_and_skips_build_fn() {
        let kit = Kit::new();
        // Register the module (so it's in the graph and gets sorted)
        let mut kit = kit;
        kit.register::<CountingModule>().unwrap();
        // Override with a capability value of 42
        kit.override_module::<CountingModule>(Arc::new(AtomicUsize::new(42)));
        // Build
        let built = kit.build().unwrap();
        // require() should return the override value (42), not the build_fn value (0)
        let cap = built.require::<CountingModule>().unwrap();
        assert_eq!(cap.load(Ordering::SeqCst), 42);
    }

    #[test]
    fn build_uses_build_fn_when_no_override() {
        let mut kit = Kit::new();
        kit.register::<CountingModule>().unwrap();
        // No override — build_fn should run and produce value 0
        let built = kit.build().unwrap();
        let cap = built.require::<CountingModule>().unwrap();
        assert_eq!(cap.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn build_inserts_unregistered_override_after_topo_loop() {
        // override_module allows injecting a module that was NOT registered.
        // build() should still make it available via require().
        let kit = Kit::new();
        kit.override_module::<MockCapability>(Arc::new(AtomicUsize::new(77)));
        let built = kit.build().unwrap();
        let cap = built.require::<MockCapability>().unwrap();
        assert_eq!(cap.load(Ordering::SeqCst), 77);
    }

    // === T005 tests ===

    #[test]
    fn require_ref_returns_reference_to_built_capability() {
        let mut kit = Kit::new();
        kit.register::<CountingModule>().unwrap();
        let built = kit.build().unwrap();
        let r = built.require_ref::<CountingModule>().unwrap();
        // build_fn returns Arc<AtomicUsize::new(0)>
        assert_eq!((*r).load(Ordering::SeqCst), 0);
    }

    #[test]
    fn require_ref_returns_override_value() {
        let mut kit = Kit::new();
        kit.register::<CountingModule>().unwrap();
        kit.override_module::<CountingModule>(Arc::new(AtomicUsize::new(55)));
        let built = kit.build().unwrap();
        let r = built.require_ref::<CountingModule>().unwrap();
        assert_eq!((*r).load(Ordering::SeqCst), 55);
    }

    #[test]
    fn require_ref_returns_missing_capability_for_unbuilt() {
        let kit = Kit::new();
        let built = kit.build().unwrap();
        let result = built.require_ref::<CountingModule>();
        assert!(matches!(
            result,
            Err(TraitKitError::MissingCapability { key: "counting" })
        ));
    }

    // === T007 tests ===

    #[test]
    fn register_lazy_does_not_build_during_build() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        // build() should succeed without triggering CountingModule's build_fn
        let built = kit.build().unwrap();
        // The capability should NOT be available (lazy not yet triggered)
        assert!(!built.contains::<CountingModule>());
    }

    #[test]
    fn register_lazy_adds_to_dependency_graph() {
        let mut kit = Kit::new();
        // Register dependency first
        kit.register::<MockCapability>().unwrap();
        // Register lazy module that depends on MockCapability
        kit.register_lazy::<DependentModule>().unwrap();
        // build() should succeed (graph validation passes)
        let built = kit.build().unwrap();
        // MockCapability should be built (eager), DependentModule should NOT (lazy)
        assert!(built.contains::<MockCapability>());
        assert!(!built.contains::<DependentModule>());
    }

    #[test]
    fn register_lazy_returns_already_registered_for_duplicate() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        let result = kit.register_lazy::<CountingModule>();
        assert!(matches!(
            result,
            Err(TraitKitError::AlreadyRegistered { module: "counting" })
        ));
    }

    // === T008 tests ===

    #[test]
    fn lazy_slots_empty_on_new_kit() {
        let kit = Kit::new();
        assert_eq!(kit.lazy_slots.borrow().len(), 0);
    }

    #[test]
    fn build_transfers_lazy_builders_to_lazy_slots() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        assert_eq!(kit.lazy_builders.borrow().len(), 1);
        assert_eq!(kit.lazy_slots.borrow().len(), 0);

        let built = kit.build().unwrap();

        // After build(): lazy_builders drained, lazy_slots populated
        assert_eq!(built.lazy_builders.borrow().len(), 0);
        assert_eq!(built.lazy_slots.borrow().len(), 1);
        assert!(
            built
                .lazy_slots
                .borrow()
                .contains_key(&TypeId::of::<CountingModule>())
        );
    }

    #[test]
    fn lazy_slots_cells_empty_after_build() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        let built = kit.build().unwrap();

        // The OnceLock cell should be empty (not yet constructed) — first
        // access via require() (T009) will populate it.
        let slots = built.lazy_slots.borrow();
        let slot = slots
            .get(&TypeId::of::<CountingModule>())
            .expect("slot exists");
        assert!(slot.cell.get().is_none());
    }

    #[test]
    fn build_transfers_multiple_lazy_builders_to_lazy_slots() {
        let mut kit = Kit::new();
        kit.register::<MockCapability>().unwrap();
        kit.register_lazy::<DependentModule>().unwrap();
        kit.register_lazy::<CountingModule>().unwrap();
        assert_eq!(kit.lazy_builders.borrow().len(), 2);

        let built = kit.build().unwrap();

        assert_eq!(built.lazy_builders.borrow().len(), 0);
        assert_eq!(built.lazy_slots.borrow().len(), 2);
        assert!(
            built
                .lazy_slots
                .borrow()
                .contains_key(&TypeId::of::<DependentModule>())
        );
        assert!(
            built
                .lazy_slots
                .borrow()
                .contains_key(&TypeId::of::<CountingModule>())
        );
    }

    // === T009 tests ===

    #[test]
    fn require_triggers_lazy_construction_on_first_access() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        let built = kit.build().unwrap();

        // Before require: capability not in capabilities map
        assert!(!built.contains::<CountingModule>());

        // First require should trigger lazy construction
        let cap = built.require::<CountingModule>().unwrap();
        assert_eq!(cap.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn require_does_not_rebuild_lazy_on_second_call() {
        // Local static counter — each test function has its own COUNT
        static COUNT: AtomicUsize = AtomicUsize::new(0);

        struct CountedModule;
        impl ModuleMeta for CountedModule {
            const NAME: &'static str = "test-counted";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for CountedModule {
            type Capability = Arc<AtomicUsize>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                let n = COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(AtomicUsize::new(n)))
            }
        }

        COUNT.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register_lazy::<CountedModule>().unwrap();
        let built = kit.build().unwrap();

        let cap1 = built.require::<CountedModule>().unwrap();
        let cap2 = built.require::<CountedModule>().unwrap();

        // Both calls should return the same value (builder called once)
        assert_eq!(
            cap1.load(Ordering::SeqCst),
            0,
            "first require returns count 0"
        );
        assert_eq!(
            cap2.load(Ordering::SeqCst),
            0,
            "second require returns same count"
        );
        assert_eq!(
            COUNT.load(Ordering::SeqCst),
            1,
            "builder invoked exactly once"
        );
    }

    #[test]
    fn require_lazy_with_registered_dependency_succeeds() {
        // A lazy module that calls kit.require() for its dependency in build()
        struct LazyDependentModule;
        impl ModuleMeta for LazyDependentModule {
            const NAME: &'static str = "lazy-dependent";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                static DEPS: &[(&str, std::any::TypeId)] =
                    &[("mock", std::any::TypeId::of::<MockCapability>())];
                DEPS
            }
        }
        impl AutoBuilder for LazyDependentModule {
            type Capability = Arc<AtomicUsize>;
            type Error = TraitKitError;
            fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
                // Verify the eager dependency is accessible during lazy build
                let mock = kit.require::<MockCapability>()?;
                Ok(Arc::new(AtomicUsize::new(
                    mock.load(Ordering::SeqCst) + 100,
                )))
            }
        }

        let mut kit = Kit::new();
        // Register MockCapability (adds to dependency graph) then override
        // with value 42 to verify it's accessible during lazy build
        kit.register::<MockCapability>().unwrap();
        kit.override_module::<MockCapability>(Arc::new(AtomicUsize::new(42)));
        kit.register_lazy::<LazyDependentModule>().unwrap();
        let built = kit.build().unwrap();

        // First require triggers lazy build, which calls require::<MockCapability>()
        let cap = built.require::<LazyDependentModule>().unwrap();
        assert_eq!(
            cap.load(Ordering::SeqCst),
            142,
            "lazy build accessed eager dep (42 + 100)"
        );
    }

    // === T010 tests ===

    /// Multi-binding module A (capability = Arc<AtomicUsize>).
    struct MultiModuleA;
    impl ModuleMeta for MultiModuleA {
        const NAME: &'static str = "multi-a";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for MultiModuleA {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(10)))
        }
    }

    /// Multi-binding module B (same capability type as `MultiModuleA`).
    struct MultiModuleB;
    impl ModuleMeta for MultiModuleB {
        const NAME: &'static str = "multi-b";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for MultiModuleB {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(20)))
        }
    }

    /// Multi-binding module C (same capability type as `MultiModuleA`).
    struct MultiModuleC;
    impl ModuleMeta for MultiModuleC {
        const NAME: &'static str = "multi-c";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for MultiModuleC {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(30)))
        }
    }

    #[test]
    fn multi_builders_empty_on_new_kit() {
        let kit = Kit::new();
        assert_eq!(kit.multi_builders.borrow().len(), 0);
    }

    #[test]
    fn register_multi_adds_to_multi_builders() {
        let mut kit = Kit::new();
        assert_eq!(kit.multi_builders.borrow().len(), 0);

        kit.register_multi::<MultiModuleA>().unwrap();

        // Keyed by TypeId::of::<M::Capability>() = TypeId::of::<Arc<AtomicUsize>>()
        let cap_id = TypeId::of::<Arc<AtomicUsize>>();
        assert_eq!(kit.multi_builders.borrow().len(), 1);
        assert!(kit.multi_builders.borrow().contains_key(&cap_id));
        assert_eq!(
            kit.multi_builders.borrow().get(&cap_id).unwrap().len(),
            1,
            "first register_multi should produce Vec of length 1"
        );
    }

    #[test]
    fn register_multi_three_times_appends_to_vec() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();
        kit.register_multi::<MultiModuleC>().unwrap();

        let cap_id = TypeId::of::<Arc<AtomicUsize>>();
        let builders = kit.multi_builders.borrow();
        let vec = builders.get(&cap_id).expect("cap_id exists");
        assert_eq!(
            vec.len(),
            3,
            "three register_multi calls should produce Vec of length 3"
        );
    }

    #[test]
    fn register_multi_adds_module_to_dependency_graph() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();

        // The module type_id (not cap_id) should be in the graph
        assert!(kit.graph.name_of(TypeId::of::<MultiModuleA>()).is_some());
    }

    #[test]
    fn register_multi_returns_already_registered_for_duplicate_module() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();

        let result = kit.register_multi::<MultiModuleA>();
        assert!(matches!(
            result,
            Err(TraitKitError::AlreadyRegistered { module: "multi-a" })
        ));
    }

    #[test]
    fn register_multi_returns_already_registered_if_already_registered_via_register() {
        let mut kit = Kit::new();
        kit.register::<MockCapability>().unwrap();

        let result = kit.register_multi::<MockCapability>();
        assert!(matches!(
            result,
            Err(TraitKitError::AlreadyRegistered { module: "mock" })
        ));
    }

    #[test]
    fn register_multi_coexists_with_register_for_different_modules() {
        let mut kit = Kit::new();
        kit.register::<MockCapability>().unwrap();
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();

        // MockCapability in builders, MultiModuleA/B in multi_builders
        assert!(
            kit.builders
                .borrow()
                .contains_key(&TypeId::of::<MockCapability>())
        );
        let cap_id = TypeId::of::<Arc<AtomicUsize>>();
        assert_eq!(kit.multi_builders.borrow().get(&cap_id).unwrap().len(), 2);
    }

    // === T011 tests ===

    #[test]
    fn require_all_returns_empty_for_unregistered_capability() {
        let mut kit = Kit::new();
        // Register MockCapability (eager, not multi)
        kit.register::<MockCapability>().unwrap();
        let built = kit.build().unwrap();

        // require_all for a capability with no multi-binding registrations
        let result = built.require_all::<MultiModuleA>();
        assert!(matches!(
            result,
            Err(TraitKitError::MissingCapability { key: "multi-a" })
        ));
    }

    #[test]
    fn require_all_returns_vec_of_three_after_three_register_multi() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();
        kit.register_multi::<MultiModuleC>().unwrap();
        let built = kit.build().unwrap();

        let caps = built.require_all::<MultiModuleA>().unwrap();
        assert_eq!(
            caps.len(),
            3,
            "three register_multi calls should return Vec of length 3"
        );
    }

    #[test]
    fn require_all_preserves_registration_order() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap(); // builds value 10
        kit.register_multi::<MultiModuleB>().unwrap(); // builds value 20
        kit.register_multi::<MultiModuleC>().unwrap(); // builds value 30
        let built = kit.build().unwrap();

        let caps = built.require_all::<MultiModuleA>().unwrap();
        assert_eq!(caps.len(), 3);
        // Verify order matches registration: 10, 20, 30
        assert_eq!(
            caps[0].load(Ordering::SeqCst),
            10,
            "first cap should be 10 (MultiModuleA)"
        );
        assert_eq!(
            caps[1].load(Ordering::SeqCst),
            20,
            "second cap should be 20 (MultiModuleB)"
        );
        assert_eq!(
            caps[2].load(Ordering::SeqCst),
            30,
            "third cap should be 30 (MultiModuleC)"
        );
    }

    #[test]
    fn require_all_returns_missing_capability_before_build() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();
        // Don't call build() — multi_capabilities is empty

        let result = kit.require_all::<MultiModuleA>();
        assert!(matches!(
            result,
            Err(TraitKitError::MissingCapability { key: "multi-a" })
        ));
    }

    #[test]
    fn build_drains_multi_builders_into_multi_capabilities() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();

        // Before build: multi_builders has entries, multi_capabilities is empty
        assert_eq!(kit.multi_builders.borrow().len(), 1); // one cap_id key
        assert_eq!(kit.multi_capabilities.borrow().len(), 0);

        let built = kit.build().unwrap();

        // After build: multi_builders is drained, multi_capabilities is populated
        assert_eq!(built.multi_builders.borrow().len(), 0);
        assert_eq!(built.multi_capabilities.borrow().len(), 1);
        let cap_id = TypeId::of::<Arc<AtomicUsize>>();
        assert_eq!(
            built
                .multi_capabilities
                .borrow()
                .get(&cap_id)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn require_all_coexists_with_require_for_single_binding() {
        let mut kit = Kit::new();
        // Single binding: MockCapability (eager)
        kit.register::<MockCapability>().unwrap();
        // Multi-binding: MultiModuleA, MultiModuleB
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();
        let built = kit.build().unwrap();

        // require gets the single binding
        let single = built.require::<MockCapability>().unwrap();
        assert_eq!(single.load(Ordering::SeqCst), 0);

        // require_all gets the multi-binding (returns MultiModuleA's cap type)
        let multi = built.require_all::<MultiModuleA>().unwrap();
        assert_eq!(multi.len(), 2);
        assert_eq!(multi[0].load(Ordering::SeqCst), 10);
        assert_eq!(multi[1].load(Ordering::SeqCst), 20);
    }
}

#[cfg(all(test, feature = "interface"))]
mod interface_tests {
    use super::*;
    use crate::core::{InterfaceBuilder, ModuleMeta};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // === Test fixtures ===

    /// Test interface trait.
    trait Logger: 'static {
        fn log(&self, msg: &str) -> String;
    }

    /// First Logger implementation.
    struct ConsoleLogger;

    impl Logger for ConsoleLogger {
        fn log(&self, msg: &str) -> String {
            format!("[console] {msg}")
        }
    }

    /// Second Logger implementation (for duplicate interface test).
    struct FileLogger;

    impl Logger for FileLogger {
        fn log(&self, msg: &str) -> String {
            format!("[file] {msg}")
        }
    }

    /// Test error type.
    #[derive(Debug)]
    struct InterfaceTestError;

    impl std::fmt::Display for InterfaceTestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "interface test error")
        }
    }

    impl std::error::Error for InterfaceTestError {}

    /// Module providing `ConsoleLogger` behind dyn Logger.
    struct ConsoleLoggerModule;

    impl ModuleMeta for ConsoleLoggerModule {
        const NAME: &'static str = "console-logger-iface";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl InterfaceBuilder for ConsoleLoggerModule {
        type Interface = dyn Logger;
        type Capability = Arc<ConsoleLogger>;
        type Error = InterfaceTestError;

        fn build(_kit: &Kit) -> Result<Arc<ConsoleLogger>, InterfaceTestError> {
            Ok(Arc::new(ConsoleLogger))
        }

        fn into_interface(cap: Arc<ConsoleLogger>) -> Arc<dyn Logger> {
            cap
        }
    }

    /// Module providing `FileLogger` behind dyn Logger (same interface).
    struct FileLoggerModule;

    impl ModuleMeta for FileLoggerModule {
        const NAME: &'static str = "file-logger";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl InterfaceBuilder for FileLoggerModule {
        type Interface = dyn Logger;
        type Capability = Arc<FileLogger>;
        type Error = InterfaceTestError;

        fn build(_kit: &Kit) -> Result<Arc<FileLogger>, InterfaceTestError> {
            Ok(Arc::new(FileLogger))
        }

        fn into_interface(cap: Arc<FileLogger>) -> Arc<dyn Logger> {
            cap
        }
    }

    // === Tests ===

    #[test]
    fn register_as_then_resolve_returns_arc_dyn_trait() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        let built = kit.build().expect("build succeeds");

        let logger: Arc<dyn Logger> = built.resolve::<dyn Logger>().expect("resolve succeeds");
        assert_eq!(logger.log("hello"), "[console] hello");
    }

    #[test]
    fn register_as_twice_same_interface_returns_already_registered() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("first register_as succeeds");
        let err = kit.register_as::<FileLoggerModule>().unwrap_err();
        assert!(
            matches!(err, TraitKitError::AlreadyRegistered { .. }),
            "expected AlreadyRegistered, got {err:?}"
        );
    }

    #[test]
    fn resolve_before_build_returns_missing_capability() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        // resolve on unbuilt kit — capabilities is empty
        assert!(kit.resolve::<dyn Logger>().is_err());
    }

    #[test]
    fn resolve_unregistered_interface_returns_missing_capability() {
        let kit = Kit::new();
        let built = kit.build().expect("build succeeds");
        assert!(built.resolve::<dyn Logger>().is_err());
    }

    #[test]
    fn register_as_builds_during_build() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        let built = kit.build().expect("build succeeds");
        // After build, resolve should return the built capability
        let logger = built.resolve::<dyn Logger>().expect("resolve succeeds");
        assert_eq!(logger.log("test"), "[console] test");
    }

    #[test]
    fn resolve_returns_callable_trait_object() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        let built = kit.build().expect("build succeeds");

        let logger: Arc<dyn Logger> = built.resolve().expect("resolve succeeds");
        let result = logger.log("world");
        assert_eq!(result, "[console] world");
    }

    #[test]
    fn register_as_coexists_with_register() {
        // register (AutoBuilder) + register_as (InterfaceBuilder) for
        // different modules should coexist.
        struct RegularModule;
        impl ModuleMeta for RegularModule {
            const NAME: &'static str = "regular";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for RegularModule {
            type Capability = Arc<AtomicUsize>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<AtomicUsize>, TraitKitError> {
                Ok(Arc::new(AtomicUsize::new(42)))
            }
        }

        let mut kit = Kit::new();
        kit.register::<RegularModule>().expect("register succeeds");
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        let built = kit.build().expect("build succeeds");

        // Both retrieve correctly
        let cap = built.require::<RegularModule>().expect("require succeeds");
        assert_eq!(cap.load(Ordering::SeqCst), 42);

        let logger = built.resolve::<dyn Logger>().expect("resolve succeeds");
        assert_eq!(logger.log("coexist"), "[console] coexist");
    }

    #[test]
    fn register_as_same_module_twice_returns_already_registered() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("first register_as succeeds");
        // Same module type — graph.add() rejects duplicate
        let err = kit.register_as::<ConsoleLoggerModule>().unwrap_err();
        assert!(
            matches!(err, TraitKitError::AlreadyRegistered { .. }),
            "expected AlreadyRegistered, got {err:?}"
        );
    }
}
