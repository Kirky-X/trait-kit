// Copyright © 2026 Kirky.X. All rights reserved.

//! Config store — internal storage for configuration handles.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::RwLock;

use crate::core::config::{ConfigHandle, ConfigKey};
use crate::kit::error::KitError;

/// Internal storage for configuration handles.
///
/// Stores `ConfigHandle<T>` values, not raw `Arc<ArcSwap<T>>`.
/// All handles returned share the same underlying swap cell.
pub struct ConfigStore {
    inner: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl ConfigStore {
    /// Create a new empty config store.
    pub fn new() -> Self {
        ConfigStore {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Set a configuration value.
    ///
    /// Creates a new `ConfigHandle<T>` and stores it.
    /// If key already exists, updates the existing handle's value.
    pub fn set_config<K>(&self, config: K::Config)
    where
        K: ConfigKey,
    {
        let mut store = self.inner.write().expect("ConfigStore write lock poisoned");
        let key = TypeId::of::<K>();

        // Check if key exists; if so, update the existing handle instead of replacing it
        if let Some(existing) = store.get(&key) {
            let handle = existing
                .downcast_ref::<ConfigHandle<K::Config>>()
                .expect("type mismatch in config store");
            handle.set(config);
            return;
        }

        // Create new handle
        let handle = ConfigHandle::new(config);
        store.insert(key, Box::new(handle));
    }

    /// Get a configuration handle.
    ///
    /// Returns `Err(KitError::MissingConfig)` if key not found.
    /// The returned handle shares the underlying swap cell with the stored one.
    pub fn config<K>(&self) -> Result<ConfigHandle<K::Config>, KitError>
    where
        K: ConfigKey,
    {
        let store = self.inner.read().expect("ConfigStore read lock poisoned");
        let key = TypeId::of::<K>();
        let boxed = store
            .get(&key)
            .ok_or(KitError::MissingConfig { key: K::NAME })?;
        let handle = boxed
            .downcast_ref::<ConfigHandle<K::Config>>()
            .expect("type mismatch in config store");
        Ok(handle.clone())
    }

    /// Check if a configuration exists.
    pub fn contains_config<K>(&self) -> bool
    where
        K: ConfigKey,
    {
        let store = self.inner.read().expect("ConfigStore read lock poisoned");
        store.contains_key(&TypeId::of::<K>())
    }
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::ConfigKey;

    struct TestConfig;
    impl ConfigKey for TestConfig {
        type Config = i32;
        const NAME: &'static str = "test_config";
    }

    #[test]
    fn default_creates_empty_store() {
        let store = ConfigStore::default();
        let inner = store.inner.read().expect("ConfigStore read lock poisoned");
        assert!(inner.is_empty(), "default ConfigStore should be empty");
    }

    #[test]
    fn set_config_stores_handle() {
        let store = ConfigStore::default();
        assert!(!store.contains_config::<TestConfig>());
        store.set_config::<TestConfig>(42);
        assert!(store.contains_config::<TestConfig>());
    }

    #[test]
    fn set_config_replaces_existing() {
        let store = ConfigStore::default();
        store.set_config::<TestConfig>(1);
        store.set_config::<TestConfig>(2);
        let handle = store.config::<TestConfig>().unwrap();
        assert_eq!(*handle.load(), 2);
    }

    #[test]
    fn config_returns_stored_handle() {
        let store = ConfigStore::default();
        store.set_config::<TestConfig>(100);
        let handle = store.config::<TestConfig>().unwrap();
        assert_eq!(*handle.load(), 100);
    }

    #[test]
    fn config_missing_returns_error() {
        let store = ConfigStore::default();
        let result = store.config::<TestConfig>();
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(KitError::MissingConfig { key: "test_config" })
        ));
    }
}
