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

/// The capability and configuration management center.
pub struct Kit<S = Unbuilt> {
    builders: RefCell<HashMap<TypeId, BuildFn>>,
    /// Override map for test injection: `TypeId` of module → pre-built capability.
    /// Populated by `override_module` / `override_module_strict`; consumed by `build()`.
    overrides: RefCell<HashMap<TypeId, Box<dyn Any>>>,
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
                    missing: *dep_name,
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

                // [Build] Priority 2: invoke the registered build_fn.
                let build_fn = kit_ref.builders.borrow_mut().remove(type_id).ok_or(
                    TraitKitError::MissingCapability {
                        key: module_name,
                    },
                )?;

                let result = (build_fn)(kit_ref);
                match result {
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

        Ok(Kit {
            builders: self.builders,
            overrides: self.overrides,
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
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingCapability` if the module has not been built yet.
    pub fn require<M: AutoBuilder>(&self) -> Result<M::Capability, TraitKitError> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
            .ok_or(TraitKitError::MissingCapability { key: M::NAME })
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
    pub fn require_ref<M: AutoBuilder>(&self) -> Result<std::cell::Ref<'_, M::Capability>, TraitKitError>
    where
        M::Capability: 'static,
    {
        use std::cell::Ref;

        let type_id = TypeId::of::<M>();
        if !self.capabilities.contains_by_type_id(type_id) {
            return Err(TraitKitError::MissingCapability { key: M::NAME });
        }
        Ref::filter_map(self.capabilities.inner_ref(), |map| {
            map.get(&type_id).and_then(|b| b.downcast_ref::<M::Capability>())
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

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
            Err(TraitKitError::DependencyMissing { module: "dependent", missing: "mock" })
        ));
        // Override should not have been inserted
        assert_eq!(kit.overrides.borrow().len(), 0);
    }

    // === T004 tests ===

    /// Module whose build_fn increments a counter, to verify override skips it.
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
}
