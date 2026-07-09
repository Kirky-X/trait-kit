// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Kit — the capability and configuration management center.
//!
//! Uses typestate pattern: `Kit` (unbuilt) → `Kit<Ready>` (after `build()`).

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(feature = "hot-reload")]
use std::rc::Rc;

use crate::core::error::KitError;
use crate::core::meta::{AutoBuilder, BuildFn};

#[cfg(feature = "encryption")]
use super::config::EncryptedBlob;
use super::graph::{DependencyGraph, GraphError, ModuleEntry};
use super::typemap::TypeMap;

/// HKDF key-derivation version label bound into every per-field key.
/// Bumping this rotates all encrypted configs without changing master keys.
#[cfg(feature = "encryption")]
const KEY_DERIVATION_VERSION: &str = "v1";

/// Derive a per-field encryption key, mapping HKDF failures to `KitError`.
#[cfg(feature = "encryption")]
fn derive_kit_field_key(
    master_key: &[u8],
    path: &'static str,
    context: &'static str,
) -> Result<[u8; 32], KitError> {
    super::config::derive_field_key(master_key, path, KEY_DERIVATION_VERSION).map_err(|e| {
        KitError::BuildFailed {
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
            let capability = M::build(kit)
                .map_err(|e| -> Box<dyn std::error::Error + Send + 'static> { Box::new(e) })?;
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
    /// Returns `KitError::DependencyMissing` if a registered module depends on an unregistered module.
    /// Returns `KitError::CycleDetected` if a dependency cycle is found.
    /// Returns `KitError::MissingCapability` if a build function is missing for a sorted module.
    /// Returns `KitError::BuildFailed` if a module's `build` callback returns an error.
    pub fn build(self) -> Result<Kit<Ready>, KitError> {
        let sorted = match self.graph.validate() {
            Ok(sorted) => sorted,
            Err(GraphError::DependencyMissing { module, missing }) => {
                return Err(KitError::DependencyMissing { module, missing });
            }
            Err(GraphError::CycleDetected { cycle }) => {
                return Err(KitError::CycleDetected { cycle });
            }
        };

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
                            context: module_name,
                            source: e,
                        });
                    }
                }
            }
        }

        Ok(Kit {
            builders: self.builders,
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
    /// Returns `KitError::MissingCapability` if the module has not been built yet.
    pub fn require<M: AutoBuilder>(&self) -> Result<M::Capability, KitError> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
            .ok_or(KitError::MissingCapability { key: M::NAME })
    }

    /// Get a configuration value.
    ///
    /// # Errors
    ///
    /// Returns `KitError::MissingConfig` if no value of type `C` was set.
    pub fn config<C: Clone + 'static>(&self) -> Result<C, KitError> {
        self.configs
            .get_cloned::<C>()
            .ok_or(KitError::MissingConfig {
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
    /// callback. Errors from `load()` are mapped to `KitError::BuildFailed`.
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
    /// Returns `KitError::BuildFailed` if `Configurable::load` fails.
    #[cfg(feature = "hot-reload")]
    pub fn reload_config<C: super::config::Configurable>(&self) -> Result<(), KitError> {
        let config = C::load().map_err(|e| KitError::BuildFailed {
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
    /// Returns `KitError::BuildFailed` if serialization, key derivation, or
    /// encryption fails.
    #[cfg(feature = "encryption")]
    pub fn set_encrypted<C>(&self, value: &C, master_key: &[u8]) -> Result<(), KitError>
    where
        C: super::config::ModuleConfig + serde::Serialize,
    {
        use super::config::XChaCha20Crypto;

        let plaintext = serde_json::to_vec(value).map_err(|e| KitError::BuildFailed {
            context: "set_encrypted",
            source: Box::new(e),
        })?;

        let field_key = derive_kit_field_key(master_key, C::PATH, "set_encrypted")?;

        let (nonce, ciphertext) = XChaCha20Crypto::new()
            .encrypt(&plaintext, &field_key)
            .map_err(|e| KitError::BuildFailed {
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
    pub fn contains_encrypted<C: super::config::ModuleConfig>(&self) -> bool {
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
    pub fn load_config_or_default<C>(&self) -> Result<(), KitError>
    where
        C: super::config::Configurable + super::config::ModuleConfig,
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
    /// Returns `KitError::MissingConfig` if no encrypted blob for `C` exists.
    /// Returns `KitError::BuildFailed` if key derivation, decryption, or
    /// deserialization fails (e.g. wrong master key, tampered ciphertext).
    #[cfg(feature = "encryption")]
    pub fn get_encrypted<C>(&self, master_key: &[u8]) -> Result<C, KitError>
    where
        C: super::config::ModuleConfig + serde::de::DeserializeOwned,
    {
        use super::config::XChaCha20Crypto;

        let blob = self
            .encrypted_configs
            .borrow()
            .get(&TypeId::of::<C>())
            .cloned()
            .ok_or(KitError::MissingConfig {
                key: std::any::type_name::<C>(),
            })?;

        let field_key = derive_kit_field_key(master_key, C::PATH, "get_encrypted")?;

        let plaintext = XChaCha20Crypto::new()
            .decrypt(&blob.nonce, &blob.ciphertext, &field_key)
            .map_err(|e| KitError::BuildFailed {
                context: "get_encrypted",
                source: Box::new(e),
            })?;

        serde_json::from_slice(&plaintext).map_err(|e| KitError::BuildFailed {
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
