// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Send + Sync type-keyed map backed by `Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>`.
//!
//! Multi-threaded counterpart to [`super::typemap::TypeMap`]. Safe to share
//! across threads (`Send + Sync`); uses `RwLock` for interior mutability.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A type-keyed map for storing async capabilities and configs.
///
/// Multi-threaded by design. Uses `Arc<RwLock<...>>` for interior mutability
/// (safe to share across threads, poisoning-aware). The sync [`super::typemap::TypeMap`]
/// stays `!Sync` for single-threaded performance.
pub struct AsyncTypeMap {
    inner: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

impl AsyncTypeMap {
    /// Create an empty map.
    #[must_use]
    pub fn new() -> Self {
        AsyncTypeMap {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a value for the given type key. Overwrites any existing entry.
    ///
    /// Requires `T: Send + Sync + 'static` so the stored value can cross
    /// thread boundaries (async build runs on a runtime).
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned (a worker thread panicked
    /// while holding the write lock). Lock poisoning indicates a logic bug
    /// in the async build pipeline and should fail loudly.
    pub fn insert<T: Send + Sync + 'static>(&self, value: T) {
        let key = TypeId::of::<T>();
        let mut guard = self
            .inner
            .write()
            .expect("AsyncTypeMap poisoned: another thread panicked while holding the lock");
        guard.insert(key, Box::new(value));
    }

    /// Insert a boxed value by raw `TypeId`. Type-erased counterpart of [`Self::insert`].
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned. See [`Self::insert`] for context.
    pub fn insert_boxed(&self, type_id: TypeId, value: Box<dyn Any + Send + Sync>) {
        let mut guard = self
            .inner
            .write()
            .expect("AsyncTypeMap poisoned: another thread panicked while holding the lock");
        guard.insert(type_id, value);
    }

    /// Returns `true` if the map contains a value of type `T`.
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned. See [`Self::insert`] for context.
    #[must_use]
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.contains_by_type_id(TypeId::of::<T>())
    }

    /// Returns `true` if the map contains a value for the given `TypeId`.
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned. See [`Self::insert`] for context.
    #[must_use]
    pub fn contains_by_type_id(&self, type_id: TypeId) -> bool {
        let guard = self
            .inner
            .read()
            .expect("AsyncTypeMap poisoned: another thread panicked while holding the lock");
        guard.contains_key(&type_id)
    }

    /// Downcast the stored value to `T` and clone it.
    ///
    /// Returns `None` if the key doesn't exist or the type doesn't match.
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned. See [`Self::insert`] for context.
    #[must_use]
    pub fn get_cloned<T: Clone + Send + Sync + 'static>(&self) -> Option<T> {
        self.get_cloned_by_type_id::<T>(TypeId::of::<T>())
    }

    /// Downcast by raw `TypeId` to the given type and clone.
    ///
    /// Returns `None` if the `TypeId` is absent or the downcast target type
    /// doesn't match the originally inserted type.
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned. See [`Self::insert`] for context.
    #[must_use]
    pub fn get_cloned_by_type_id<T: Clone + Send + Sync + 'static>(
        &self,
        type_id: TypeId,
    ) -> Option<T> {
        let guard = self
            .inner
            .read()
            .expect("AsyncTypeMap poisoned: another thread panicked while holding the lock");
        guard
            .get(&type_id)
            .and_then(|boxed| boxed.downcast_ref::<T>())
            .cloned()
    }

    /// Returns the number of entries currently stored.
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned. See [`Self::insert`] for context.
    pub(crate) fn len(&self) -> usize {
        let guard = self
            .inner
            .read()
            .expect("AsyncTypeMap poisoned: another thread panicked while holding the lock");
        guard.len()
    }
}

impl Default for AsyncTypeMap {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AsyncTypeMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let guard = self
            .inner
            .read()
            .expect("AsyncTypeMap poisoned: another thread panicked while holding the lock");
        f.debug_struct("AsyncTypeMap")
            .field("len", &guard.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn insert_then_get_cloned_returns_value() {
        let map = AsyncTypeMap::new();
        map.insert(42i32);
        assert_eq!(map.get_cloned::<i32>(), Some(42));
    }

    #[test]
    fn insert_boxed_and_get_by_type_id() {
        let map = AsyncTypeMap::new();
        let type_id = std::any::TypeId::of::<i32>();
        map.insert_boxed(type_id, Box::new(42i32));
        assert!(map.contains_by_type_id(type_id));
        assert_eq!(map.get_cloned_by_type_id::<i32>(type_id), Some(42));
    }

    #[test]
    fn overwrite_existing_entry() {
        let map = AsyncTypeMap::new();
        map.insert(1i32);
        map.insert(2i32);
        assert_eq!(map.get_cloned::<i32>(), Some(2));
    }

    #[test]
    fn get_cloned_returns_none_for_missing_key() {
        let map = AsyncTypeMap::new();
        assert_eq!(map.get_cloned::<i32>(), None);
    }

    #[test]
    fn contains_returns_correct_bool() {
        let map = AsyncTypeMap::new();
        assert!(!map.contains::<i32>());
        map.insert(42i32);
        assert!(map.contains::<i32>());
        assert!(!map.contains::<u64>());
    }

    #[test]
    fn contains_by_type_id_returns_false_for_missing() {
        let map = AsyncTypeMap::new();
        let tid = std::any::TypeId::of::<i32>();
        assert!(!map.contains_by_type_id(tid));
    }

    #[test]
    fn get_cloned_by_type_id_returns_none_for_wrong_type() {
        let map = AsyncTypeMap::new();
        let i32_id = std::any::TypeId::of::<i32>();
        map.insert_boxed(i32_id, Box::new(42i32));
        // Right TypeId, wrong downcast target.
        let u64_id = std::any::TypeId::of::<u64>();
        assert_eq!(map.get_cloned_by_type_id::<u64>(u64_id), None);
        assert_eq!(map.get_cloned_by_type_id::<u64>(i32_id), None);
    }

    #[test]
    fn len_returns_entry_count() {
        let map = AsyncTypeMap::new();
        assert_eq!(map.len(), 0);
        map.insert(1i32);
        assert_eq!(map.len(), 1);
        map.insert("a".to_string());
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn default_creates_empty_map() {
        let map = AsyncTypeMap::default();
        assert_eq!(map.len(), 0);
        assert!(map.get_cloned::<i32>().is_none());
    }

    #[test]
    fn cross_thread_access_does_not_panic() {
        // Spawn N threads each doing insert + get_cloned on shared Arc<AsyncTypeMap>.
        // Verifies Send + Sync contract: no UB, no panic under contention.
        let map = Arc::new(AsyncTypeMap::new());
        map.insert(0i32);

        let mut handles = Vec::new();
        for i in 1..=8 {
            let m = Arc::clone(&map);
            handles.push(thread::spawn(move || {
                m.insert(i);
                let _ = m.get_cloned::<i32>();
                assert!(m.contains::<i32>());
            }));
        }
        for h in handles {
            h.join().expect("worker thread panicked");
        }

        // After 8 threads inserted distinct i32 values, the last writer wins.
        assert!(map.contains::<i32>());
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn arc_clone_shares_state() {
        // Arc<AsyncTypeMap> clone shares the underlying RwLock — writes via one
        // handle are observable through the other.
        let map = Arc::new(AsyncTypeMap::new());
        let map2 = Arc::clone(&map);
        map2.insert(7i32);
        assert_eq!(map.get_cloned::<i32>(), Some(7));
    }
}
