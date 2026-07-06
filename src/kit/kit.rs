// Copyright © 2026 Kirky.X

//! Kit — the capability and configuration management center.
//!
//! Uses typestate pattern: `Kit` (unbuilt) → `Kit<Ready>` (after `build()`).

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(feature = "confers-hot-reload")]
use std::rc::Rc;

use crate::core::error::KitError;
use crate::core::meta::{AutoBuilder, BuildFn};

#[cfg(feature = "confers-encryption")]
use super::config::EncryptedBlob;
use super::graph::{DependencyGraph, GraphError, ModuleEntry};
use super::typemap::TypeMap;

/// Marker type for the unbuilt state.
pub struct Unbuilt;

/// Marker type for the ready (built) state.
pub struct Ready;

/// Type alias for hot-reload subscriber callbacks (single-threaded, `!Sync`).
#[cfg(feature = "confers-hot-reload")]
type SubscriberMap = RefCell<HashMap<TypeId, Vec<Rc<dyn Fn()>>>>;

/// Type alias for the encrypted config store (single-threaded, `!Sync`).
#[cfg(feature = "confers-encryption")]
type EncryptedConfigMap = RefCell<HashMap<TypeId, EncryptedBlob>>;

/// The capability and configuration management center.
pub struct Kit<S = Unbuilt> {
    builders: RefCell<HashMap<TypeId, BuildFn>>,
    graph: DependencyGraph,
    configs: TypeMap,
    capabilities: TypeMap,
    #[cfg(feature = "confers-hot-reload")]
    subscribers: SubscriberMap,
    #[cfg(feature = "confers-encryption")]
    encrypted_configs: EncryptedConfigMap,
    _state: std::marker::PhantomData<S>,
}

impl Kit {
    /// Create a new empty Kit.
    #[must_use]
    pub fn new() -> Self {
        Kit {
            builders: RefCell::new(HashMap::new()),
            graph: DependencyGraph::new(),
            configs: TypeMap::new(),
            capabilities: TypeMap::new(),
            #[cfg(feature = "confers-hot-reload")]
            subscribers: RefCell::new(HashMap::new()),
            #[cfg(feature = "confers-encryption")]
            encrypted_configs: RefCell::new(HashMap::new()),
            _state: std::marker::PhantomData,
        }
    }

    /// Register a module for construction.
    ///
    /// # Errors
    ///
    /// Returns `KitError::AlreadyRegistered` if a module with the same `TypeId` was already registered.
    pub fn register<M: AutoBuilder>(&mut self) -> Result<(), KitError> {
        let entry = ModuleEntry {
            type_id: TypeId::of::<M>(),
            name: M::NAME,
            dependencies: M::dependencies().iter().map(|(n, id)| (*n, *id)).collect(),
        };

        self.graph
            .add(entry)
            .map_err(|name| KitError::AlreadyRegistered { module: name })?;

        let build_fn: BuildFn = Box::new(|kit| {
            let capability =
                M::build(kit).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
            Ok(Box::new(capability) as Box<dyn Any>)
        });

        self.builders
            .borrow_mut()
            .insert(TypeId::of::<M>(), build_fn);
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
    /// Returns `KitError::BuildFailed` if `Configurable::load` fails.
    #[cfg(feature = "confers")]
    pub fn load_config<C: super::config::Configurable>(&self) -> Result<(), KitError> {
        let config = C::load().map_err(|e| KitError::BuildFailed {
            module: "load_config",
            source: e,
        })?;
        self.set_config(config);
        Ok(())
    }

    /// Retrieve a capability. During build phase, returns already-built capabilities.
    ///
    /// # Errors
    ///
    /// Returns `KitError::MissingCapability` if the module has not been built yet.
    pub fn require<M: AutoBuilder>(&self) -> Result<M::Capability, KitError> {
        self.require_capability::<M>()
    }

    /// Get a configuration value.
    ///
    /// # Errors
    ///
    /// Returns `KitError::MissingConfig` if no value of type `C` was set.
    pub fn config<C: Clone + 'static>(&self) -> Result<C, KitError> {
        self.get_config::<C>()
    }

    /// Validate the dependency graph and build all modules in topological order.
    ///
    /// After this call, all capabilities are available via `require()`.
    ///
    /// # Errors
    ///
    /// Returns `KitError::DependencyMissing` if a registered module depends on an unregistered module.
    /// Returns `KitError::CycleDetected` if a dependency cycle is found.
    /// Returns `KitError::MissingCapability` if a build function is missing for a sorted module.
    /// Returns `KitError::BuildFailed` if a module's `build` callback returns an error.
    pub fn build(self) -> Result<Kit<Ready>, KitError> {
        // Validate the graph
        let sorted = match self.graph.validate() {
            Ok(sorted) => sorted,
            Err(GraphError::DependencyMissing { module, missing }) => {
                return Err(KitError::DependencyMissing { module, missing });
            }
            Err(GraphError::CycleDetected { cycle }) => {
                return Err(KitError::CycleDetected { cycle });
            }
        };

        // Build all modules in topological order.
        // We borrow self immutably to pass &Kit to build functions,
        // and use RefCell to mutably access builders.
        {
            let kit_ref: &Self = &self;

            for type_id in &sorted {
                let build_fn = kit_ref.builders.borrow_mut().remove(type_id).ok_or(
                    KitError::MissingCapability {
                        key: kit_ref.module_name(*type_id),
                    },
                )?;

                let module_name = kit_ref.module_name(*type_id);

                let result = (build_fn)(kit_ref);
                match result {
                    Ok(boxed) => {
                        kit_ref.capabilities.insert_boxed(*type_id, boxed);
                    }
                    Err(e) => {
                        return Err(KitError::BuildFailed {
                            module: module_name,
                            source: e,
                        });
                    }
                }
            }
        }
        // All borrows dropped here. Now we can move fields out of self.

        Ok(Kit {
            builders: self.builders,
            graph: self.graph,
            configs: self.configs,
            capabilities: self.capabilities,
            #[cfg(feature = "confers-hot-reload")]
            subscribers: self.subscribers,
            #[cfg(feature = "confers-encryption")]
            encrypted_configs: self.encrypted_configs,
            _state: std::marker::PhantomData,
        })
    }

    fn module_name(&self, type_id: TypeId) -> &'static str {
        self.graph
            .entries()
            .iter()
            .find(|e| e.type_id == type_id)
            .map_or("<unknown>", |e| e.name)
    }
}

impl<S> Kit<S> {
    /// Retrieve a capability by its module type.
    fn require_capability<M: AutoBuilder>(&self) -> Result<M::Capability, KitError> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
            .ok_or(KitError::MissingCapability { key: M::NAME })
    }

    /// Get a configuration value.
    fn get_config<C: Clone + 'static>(&self) -> Result<C, KitError> {
        self.configs
            .get_cloned::<C>()
            .ok_or(KitError::MissingConfig {
                key: std::any::type_name::<C>(),
            })
    }

    /// Subscribe a callback to be invoked when config of type `C` is reloaded.
    ///
    /// Requires the `confers-hot-reload` feature. The callback receives no
    /// arguments; use `Kit::config::<C>()` inside it to read the new value.
    /// Callbacks are stored in a `RefCell` (single-threaded, `!Sync`).
    ///
    /// Layer 2 of the inheritance system: cargo feature chain
    /// `confers-hot-reload` → `confers-macros` → `confers`.
    #[cfg(feature = "confers-hot-reload")]
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
    /// Requires the `confers-hot-reload` feature. Calls `C::load()`, stores
    /// the result via `set_config`, then invokes every `subscribe::<C>`
    /// callback. Errors from `load()` are mapped to `KitError::BuildFailed`.
    ///
    /// # Errors
    ///
    /// Returns `KitError::BuildFailed` if `Configurable::load` fails.
    #[cfg(feature = "confers-hot-reload")]
    pub fn reload_config<C: super::config::Configurable>(&self) -> Result<(), KitError> {
        let config = C::load().map_err(|e| KitError::BuildFailed {
            module: "reload_config",
            source: e,
        })?;
        self.configs.insert(config);
        // Notify subscribers: clone the Rc list out to avoid holding the
        // RefCell borrow across user callbacks (which may re-enter subscribe).
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

    // === Level 4: encrypted config storage ===

    /// Encrypt and store a configuration value.
    ///
    /// Requires the `confers-encryption` feature. Serializes `value` to JSON,
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
    /// Returns `KitError::BuildFailed` if serialization, key derivation, or
    /// encryption fails.
    #[cfg(feature = "confers-encryption")]
    pub fn set_encrypted<C>(&self, value: &C, master_key: &[u8]) -> Result<(), KitError>
    where
        C: super::config::ModuleConfig + serde::Serialize,
    {
        use super::config::{derive_field_key, XChaCha20Crypto};

        let plaintext = serde_json::to_vec(value).map_err(|e| KitError::BuildFailed {
            module: "set_encrypted",
            source: Box::new(e),
        })?;

        let field_key =
            derive_field_key(master_key, C::PATH, "v1").map_err(|e| KitError::BuildFailed {
                module: "set_encrypted",
                source: Box::new(e),
            })?;

        let (nonce, ciphertext) = XChaCha20Crypto::new()
            .encrypt(&plaintext, &field_key)
            .map_err(|e| KitError::BuildFailed {
                module: "set_encrypted",
                source: Box::new(e),
            })?;

        self.encrypted_configs
            .borrow_mut()
            .insert(TypeId::of::<C>(), EncryptedBlob { nonce, ciphertext });
        Ok(())
    }

    /// Retrieve and decrypt a configuration value.
    ///
    /// Requires the `confers-encryption` feature. Looks up the encrypted
    /// blob for type `C`, derives the per-field key from `master_key` and
    /// `C::PATH`, decrypts with XChaCha20-Poly1305, then deserializes from
    /// JSON. The `master_key` must match the one passed to `set_encrypted`.
    ///
    /// # Errors
    ///
    /// Returns `KitError::MissingConfig` if no encrypted blob for `C` exists.
    /// Returns `KitError::BuildFailed` if key derivation, decryption, or
    /// deserialization fails (e.g. wrong master key, tampered ciphertext).
    #[cfg(feature = "confers-encryption")]
    pub fn get_encrypted<C>(&self, master_key: &[u8]) -> Result<C, KitError>
    where
        C: super::config::ModuleConfig + serde::de::DeserializeOwned,
    {
        use super::config::{derive_field_key, XChaCha20Crypto};

        let blob = self
            .encrypted_configs
            .borrow()
            .get(&TypeId::of::<C>())
            .cloned()
            .ok_or(KitError::MissingConfig {
                key: std::any::type_name::<C>(),
            })?;

        let field_key =
            derive_field_key(master_key, C::PATH, "v1").map_err(|e| KitError::BuildFailed {
                module: "get_encrypted",
                source: Box::new(e),
            })?;

        let plaintext = XChaCha20Crypto::new()
            .decrypt(&blob.nonce, &blob.ciphertext, &field_key)
            .map_err(|e| KitError::BuildFailed {
                module: "get_encrypted",
                source: Box::new(e),
            })?;

        serde_json::from_slice(&plaintext).map_err(|e| KitError::BuildFailed {
            module: "get_encrypted",
            source: Box::new(e),
        })
    }

    /// Check if an encrypted config of type `C` is registered.
    #[cfg(feature = "confers-encryption")]
    pub fn contains_encrypted<C: super::config::ModuleConfig>(&self) -> bool {
        self.encrypted_configs
            .borrow()
            .contains_key(&TypeId::of::<C>())
    }
}

impl Kit<Ready> {
    /// Retrieve a capability by its module type.
    ///
    /// # Errors
    ///
    /// Returns `KitError::MissingCapability` if the module was not registered or not built.
    pub fn require<M: AutoBuilder>(&self) -> Result<M::Capability, KitError> {
        self.require_capability::<M>()
    }

    /// Retrieve an optional capability. Returns `None` if not built.
    pub fn optional<M: AutoBuilder>(&self) -> Option<M::Capability> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
    }

    /// Get a configuration value.
    ///
    /// # Errors
    ///
    /// Returns `KitError::MissingConfig` if no value of type `C` was set.
    pub fn config<C: Clone + 'static>(&self) -> Result<C, KitError> {
        self.get_config::<C>()
    }

    /// Check if a capability has been built.
    pub fn contains<M: AutoBuilder>(&self) -> bool {
        self.capabilities.contains_by_type_id(TypeId::of::<M>())
    }

    /// Check if a config is registered.
    pub fn contains_config<C: Clone + 'static>(&self) -> bool {
        self.configs.get_cloned::<C>().is_some()
    }
}

impl Default for Kit {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Kit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kit").finish()
    }
}

impl std::fmt::Debug for Kit<Ready> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kit<Ready>").finish()
    }
}
