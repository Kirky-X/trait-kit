// Copyright © 2026 Kirky.X. All rights reserved.

//! Capability store — internal storage for capabilities.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::core::capability::CapabilityKey;
use crate::kit::error::KitError;

/// Internal storage for capabilities.
///
/// Uses `HashMap<TypeId, Box<dyn Any + Send + Sync>>` for type-erased storage.
/// All methods use `RwLock` for thread-safe access.
pub struct CapabilityStore {
    inner: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl CapabilityStore {
    /// Create a new empty capability store.
    pub fn new() -> Self {
        CapabilityStore {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Register a capability.
    ///
    /// Returns `Err(KitError::DuplicateCapability)` if key already exists.
    pub fn provide<K>(&self, value: Arc<K::Capability>) -> Result<(), KitError>
    where
        K: CapabilityKey,
    {
        let mut store = self
            .inner
            .write()
            .expect("CapabilityStore write lock poisoned");
        let key = TypeId::of::<K>();
        if store.contains_key(&key) {
            return Err(KitError::DuplicateCapability { key: K::NAME });
        }
        store.insert(key, Box::new(value));
        Ok(())
    }

    /// Register or replace a capability.
    ///
    /// If key exists, replaces the value. If not, inserts.
    pub fn replace<K>(&self, value: Arc<K::Capability>)
    where
        K: CapabilityKey,
    {
        let mut store = self
            .inner
            .write()
            .expect("CapabilityStore write lock poisoned");
        store.insert(TypeId::of::<K>(), Box::new(value));
    }

    /// Retrieve a capability.
    ///
    /// Returns `Err(KitError::MissingCapability)` if key not found.
    pub fn require<K>(&self) -> Result<Arc<K::Capability>, KitError>
    where
        K: CapabilityKey,
    {
        let store = self
            .inner
            .read()
            .expect("CapabilityStore read lock poisoned");
        let key = TypeId::of::<K>();
        let boxed = store
            .get(&key)
            .ok_or(KitError::MissingCapability { key: K::NAME })?;
        let downcasted = boxed
            .downcast_ref::<Arc<K::Capability>>()
            .expect("type mismatch in capability store");
        Ok(Arc::clone(downcasted))
    }

    /// Check if a capability exists.
    pub fn contains<K>(&self) -> bool
    where
        K: CapabilityKey,
    {
        let store = self
            .inner
            .read()
            .expect("CapabilityStore read lock poisoned");
        store.contains_key(&TypeId::of::<K>())
    }
}

impl Default for CapabilityStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestKey;
    impl CapabilityKey for TestKey {
        type Capability = dyn Send + Sync;
        const NAME: &'static str = "test_key";
    }

    #[test]
    fn default_creates_empty_store() {
        let store = CapabilityStore::default();
        let inner = store
            .inner
            .read()
            .expect("CapabilityStore read lock poisoned");
        assert!(inner.is_empty(), "default CapabilityStore should be empty");
    }

    #[test]
    fn provide_inserts_capability() {
        let store = CapabilityStore::default();
        assert!(!store.contains::<TestKey>());
        store
            .provide::<TestKey>(Arc::new(42i32) as Arc<dyn Send + Sync>)
            .unwrap();
        assert!(store.contains::<TestKey>());
    }

    #[test]
    fn provide_duplicate_returns_error() {
        let store = CapabilityStore::default();
        store
            .provide::<TestKey>(Arc::new(1i32) as Arc<dyn Send + Sync>)
            .unwrap();
        let err = store
            .provide::<TestKey>(Arc::new(2i32) as Arc<dyn Send + Sync>)
            .unwrap_err();
        assert!(matches!(
            err,
            KitError::DuplicateCapability { key: "test_key" }
        ));
    }

    #[test]
    fn replace_overwrites_existing() {
        let store = CapabilityStore::default();
        assert!(!store.contains::<TestKey>());
        store.replace::<TestKey>(Arc::new(1i32) as Arc<dyn Send + Sync>);
        assert!(store.contains::<TestKey>());
    }

    #[test]
    fn require_returns_stored_capability() {
        let store = CapabilityStore::default();
        store
            .provide::<TestKey>(Arc::new(42i32) as Arc<dyn Send + Sync>)
            .unwrap();
        let result = store.require::<TestKey>();
        assert!(result.is_ok());
    }

    #[test]
    fn require_missing_returns_error() {
        let store = CapabilityStore::default();
        let result = store.require::<TestKey>();
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(KitError::MissingCapability { key: "test_key" })
        ));
    }

    #[test]
    fn contains_returns_false_for_missing() {
        let store = CapabilityStore::default();
        assert!(!store.contains::<TestKey>());
    }
}
