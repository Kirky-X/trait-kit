// Copyright © 2026 Kirky.X. All rights reserved.

//! Kit — the capability and configuration management center.

use std::sync::Arc;

use crate::core::capability::CapabilityKey;
use crate::core::config::{ConfigHandle, ConfigKey};

use crate::kit::capability_store::CapabilityStore;
use crate::kit::config_store::ConfigStore;
use crate::kit::error::KitError;

/// The inner storage for Kit.
///
/// Held behind `Arc<KitInner>` to enable shared ownership.
struct KitInner {
    capabilities: CapabilityStore,
    configs: ConfigStore,
}

/// The capability and configuration management center.
///
/// Kit is a shared facade that holds capabilities and configurations.
/// It uses interior mutability (`&self` methods for write operations).
///
/// # Cloning Semantics
///
/// `Kit::clone()` shares the same `KitInner`. Changes made on one clone
/// are immediately visible on all other clones.
///
/// # Thread Safety
///
/// Kit satisfies `Send + Sync`. Internal storage uses `RwLock`.
#[derive(Clone)]
pub struct Kit {
    inner: Arc<KitInner>,
}

impl Kit {
    /// Create a new empty Kit.
    pub fn new() -> Self {
        Kit {
            inner: Arc::new(KitInner {
                capabilities: CapabilityStore::new(),
                configs: ConfigStore::new(),
            }),
        }
    }

    // === Capability API ===

    /// Register a capability.
    ///
    /// Returns `Err(KitError::DuplicateCapability)` if key already exists.
    pub fn provide<K>(&self, value: Arc<K::Capability>) -> Result<(), KitError>
    where
        K: CapabilityKey,
    {
        self.inner.capabilities.provide::<K>(value)
    }

    /// Register or replace a capability.
    ///
    /// If key exists, replaces the value. If not, inserts.
    pub fn replace<K>(&self, value: Arc<K::Capability>)
    where
        K: CapabilityKey,
    {
        self.inner.capabilities.replace::<K>(value)
    }

    /// Retrieve a capability.
    ///
    /// Returns `Err(KitError::MissingCapability)` if key not found.
    pub fn require<K>(&self) -> Result<Arc<K::Capability>, KitError>
    where
        K: CapabilityKey,
    {
        self.inner.capabilities.require::<K>()
    }

    /// Check if a capability exists.
    pub fn contains<K>(&self) -> bool
    where
        K: CapabilityKey,
    {
        self.inner.capabilities.contains::<K>()
    }

    // === Config API ===

    /// Set a configuration value.
    pub fn set_config<K>(&self, config: K::Config)
    where
        K: ConfigKey,
    {
        self.inner.configs.set_config::<K>(config)
    }

    /// Get a configuration handle.
    ///
    /// Returns `Err(KitError::MissingConfig)` if key not found.
    pub fn config<K>(&self) -> Result<ConfigHandle<K::Config>, KitError>
    where
        K: ConfigKey,
    {
        self.inner.configs.config::<K>()
    }

    /// Check if a configuration exists.
    pub fn contains_config<K>(&self) -> bool
    where
        K: ConfigKey,
    {
        self.inner.configs.contains_config::<K>()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::capability::CapabilityKey;
    use crate::core::config::ConfigKey;

    struct TestKey;
    impl CapabilityKey for TestKey {
        type Capability = dyn Send + Sync;
        const NAME: &'static str = "test_key";
    }

    struct TestCfg;
    impl ConfigKey for TestCfg {
        type Config = i32;
        const NAME: &'static str = "test_cfg";
    }

    #[test]
    fn default_creates_empty_kit() {
        let kit = Kit::default();
        assert!(!kit.contains::<TestKey>());
        assert!(kit.require::<TestKey>().is_err());
        assert!(!kit.contains_config::<TestCfg>());
        assert!(kit.config::<TestCfg>().is_err());
    }

    #[test]
    fn provide_and_require_capability() {
        let kit = Kit::new();
        kit.provide::<TestKey>(Arc::new(1i32) as Arc<dyn Send + Sync>)
            .unwrap();
        assert!(kit.contains::<TestKey>());
        assert!(kit.require::<TestKey>().is_ok());
    }

    #[test]
    fn replace_capability() {
        let kit = Kit::new();
        assert!(!kit.contains::<TestKey>());
        kit.replace::<TestKey>(Arc::new(1i32) as Arc<dyn Send + Sync>);
        assert!(kit.contains::<TestKey>());
    }

    #[test]
    fn provide_duplicate_returns_error() {
        let kit = Kit::new();
        kit.provide::<TestKey>(Arc::new(1i32) as Arc<dyn Send + Sync>)
            .unwrap();
        let err = kit
            .provide::<TestKey>(Arc::new(2i32) as Arc<dyn Send + Sync>)
            .unwrap_err();
        assert_eq!(err.to_string(), "capability `test_key` already exists");
    }

    #[test]
    fn config_missing_returns_error() {
        let kit = Kit::new();
        let result = kit.config::<TestCfg>();
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(KitError::MissingConfig { key: "test_cfg" })
        ));
    }

    #[test]
    fn set_config_and_retrieve() {
        let kit = Kit::new();
        assert!(!kit.contains_config::<TestCfg>());
        kit.set_config::<TestCfg>(42);
        assert!(kit.contains_config::<TestCfg>());
        let handle = kit.config::<TestCfg>().unwrap();
        assert_eq!(*handle.load(), 42);
    }

    #[test]
    fn require_missing_returns_error() {
        let kit = Kit::new();
        let result = kit.require::<TestKey>();
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(KitError::MissingCapability { key: "test_key" })
        ));
    }

    #[test]
    fn debug_format_outputs_kit() {
        let kit = Kit::new();
        assert_eq!(format!("{:?}", kit), "Kit");
    }

    #[test]
    fn clone_shares_inner_state() {
        let kit = Kit::new();
        kit.set_config::<TestCfg>(10);
        let cloned = kit.clone();
        kit.set_config::<TestCfg>(20);
        let handle = cloned.config::<TestCfg>().unwrap();
        assert_eq!(*handle.load(), 20);
    }
}
