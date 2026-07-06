// Copyright © 2026 Kirky.X

//! Generic type-keyed map backed by `RwLock<HashMap<TypeId, Box<dyn Any>>>`.

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::RwLock;

/// A thread-safe, type-keyed map for storing capabilities and configs.
pub struct TypeMap {
    inner: RwLock<HashMap<TypeId, Box<dyn std::any::Any + Send + Sync>>>,
}

impl TypeMap {
    /// Create an empty map.
    pub fn new() -> Self {
        TypeMap {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Insert a value for the given type key. Overwrites any existing entry.
    pub fn insert<T: Send + Sync + 'static>(&self, value: T) {
        let key = TypeId::of::<T>();
        self.inner
            .write()
            .expect("TypeMap write lock poisoned")
            .insert(key, Box::new(value));
    }

    /// Insert a boxed value by raw TypeId.
    pub fn insert_boxed(&self, type_id: TypeId, value: Box<dyn std::any::Any + Send + Sync>) {
        self.inner
            .write()
            .expect("TypeMap write lock poisoned")
            .insert(type_id, value);
    }

    /// Returns `true` if the map contains a value for the given TypeId.
    pub fn contains_by_type_id(&self, type_id: TypeId) -> bool {
        self.inner
            .read()
            .expect("TypeMap read lock poisoned")
            .contains_key(&type_id)
    }

    /// Downcast the stored value to `T` and clone it.
    /// Returns `None` if the key doesn't exist or the type doesn't match.
    pub fn get_cloned<T: Clone + 'static>(&self) -> Option<T> {
        let key = TypeId::of::<T>();
        self.inner
            .read()
            .expect("TypeMap read lock poisoned")
            .get(&key)
            .and_then(|boxed| boxed.downcast_ref::<T>())
            .cloned()
    }

    /// Downcast by raw TypeId to the given type and clone.
    pub fn get_cloned_by_type_id<T: Clone + 'static>(&self, type_id: TypeId) -> Option<T> {
        self.inner
            .read()
            .expect("TypeMap read lock poisoned")
            .get(&type_id)
            .and_then(|boxed| boxed.downcast_ref::<T>())
            .cloned()
    }
}

impl Default for TypeMap {
    fn default() -> Self {
        Self::new()
    }
}
