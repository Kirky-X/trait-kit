// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Send + Sync type-keyed map backed by `Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>`.
//!
//! Multi-threaded counterpart to `TypeMap`. Safe to share
//! across threads (`Send + Sync`); uses `RwLock` for interior mutability.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, RwLock, RwLockReadGuard};

/// A type-keyed map for storing async capabilities and configs.
///
/// Multi-threaded by design. Uses `Arc<RwLock<...>>` for interior mutability
/// (safe to share across threads, poisoning-aware). The sync `TypeMap`
/// stays `!Sync` for single-threaded performance.
pub struct AsyncTypeMap {
    inner: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

/// A read guard bundling [`AsyncTypeMap`]'s read lock with the downcast
/// value reference.
///
/// The lock guard and the `&T` are packaged together so they cannot be
/// separated: the value is only reachable through [`Deref`] while the guard
/// is alive, and the guard cannot be dropped while a reference derived from
/// it is still borrowed. This makes it impossible to release the read lock
/// and then trigger a release of the referenced value (e.g. via `insert`)
/// while still holding `&T`.
pub struct AsyncTypeMapReadGuard<'a, T> {
    /// Holds the read lock; never read directly — kept alive so its `Drop`
    /// releases the lock only after the borrowed `value` is no longer used.
    #[expect(dead_code, reason = "RAII 锁卫字段：仅凭 Drop 释放读锁，从不读取")]
    guard: RwLockReadGuard<'a, HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    value: &'a T,
}

impl<T> Deref for AsyncTypeMapReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
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

    /// Acquire a read guard and return it bundled with a reference to the
    /// stored value downcast to `T`.
    ///
    /// The returned [`AsyncTypeMapReadGuard`] keeps the read lock held; the
    /// value is reachable only through the guard (`Deref`), so guard and
    /// reference share one lifetime and cannot be separated. Dropping the
    /// guard releases the lock.
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned.
    #[must_use]
    pub fn read_by_type_id<'a, T: 'static>(
        &'a self,
        type_id: TypeId,
    ) -> Option<AsyncTypeMapReadGuard<'a, T>> {
        let guard = self
            .inner
            .read()
            .expect("AsyncTypeMap poisoned: another thread panicked while holding the lock");
        #[allow(unsafe_code)]
        // SAFETY: `value` points into the value stored inside the
        // lock-protected map. The returned wrapper holds the read guard alive
        // until `'a`, so the map cannot be mutated while `value` is reachable
        // and the reference stays valid for as long as the wrapper is alive.
        unsafe {
            let ptr: *const T = std::ptr::from_ref(guard.get(&type_id)?.downcast_ref::<T>()?);
            let value: &'a T = &*ptr;
            Some(AsyncTypeMapReadGuard { guard, value })
        }
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

    #[test]
    fn read_by_type_id_returns_some_for_existing() {
        let map = AsyncTypeMap::new();
        map.insert(42i32);
        let i32_id = std::any::TypeId::of::<i32>();
        let result = map.read_by_type_id::<i32>(i32_id);
        assert!(result.is_some());
        let guard = result.unwrap();
        assert_eq!(*guard, 42);
    }

    #[test]
    fn read_by_type_id_returns_none_for_missing() {
        let map = AsyncTypeMap::new();
        let i32_id = std::any::TypeId::of::<i32>();
        let result = map.read_by_type_id::<i32>(i32_id);
        assert!(result.is_none());
    }

    #[test]
    fn read_by_type_id_returns_none_for_wrong_type() {
        let map = AsyncTypeMap::new();
        map.insert(42i32);
        let i32_id = std::any::TypeId::of::<i32>();
        // Request as u64 — downcast should fail
        let result = map.read_by_type_id::<u64>(i32_id);
        assert!(result.is_none());
    }

    #[test]
    fn read_guard_and_reference_are_inseparable() {
        // 回归钉子：read_by_type_id 现返回守卫包装类型，&T 与读锁 guard 绑定
        // 在同一结构上 —— 引用只能经 Deref 获取，且 guard 在引用存活期间无法
        // 被 drop。旧 API 返回 (guard, &T) 二元组，允许先 drop guard 释放读锁、
        // 再经 &self 的内部锁 insert 触发旧值释放，导致 &T 悬垂（可读出垃圾值）。
        let map = AsyncTypeMap::new();
        map.insert(42i32);
        let tid = std::any::TypeId::of::<i32>();

        let guard = map.read_by_type_id::<i32>(tid).unwrap();
        assert_eq!(*guard, 42);

        // 引用派生自 guard：引用存续期间 guard 必然存活（编译期保证）。
        let val_ref: &i32 = &guard;
        assert_eq!(*val_ref, 42);

        // 只有放弃引用之后才能 drop guard，读锁随之释放。
        drop(guard);
        map.insert(7i32); // 锁可重新获取；此时已无引用存留
        assert_eq!(map.get_cloned::<i32>(), Some(7));
    }

    #[test]
    fn debug_format_contains_len() {
        let map = AsyncTypeMap::new();
        map.insert(42i32);
        let debug = format!("{map:?}");
        assert!(debug.contains("AsyncTypeMap"));
        assert!(debug.contains("len"));
    }
}
