// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Generic type-keyed map backed by `RefCell<HashMap<TypeId, Box<dyn Any>>>`.
//!
//! Single-threaded by design (Kit is `!Sync`).

use std::any::{Any, TypeId};
use std::cell::{Ref, RefCell};
use std::collections::HashMap;

/// A type-keyed map for storing capabilities and configs.
///
/// Single-threaded by design. Uses `RefCell` for interior mutability
/// (no locking overhead, no poisoning). Kit is `!Sync` by design.
pub struct TypeMap {
    inner: RefCell<HashMap<TypeId, Box<dyn Any>>>,
}

impl TypeMap {
    /// Create an empty map.
    pub fn new() -> Self {
        TypeMap {
            inner: RefCell::new(HashMap::new()),
        }
    }

    /// Remove all entries from the map, dropping all stored values.
    pub fn clear(&self) {
        self.inner.borrow_mut().clear();
    }

    /// Insert a value for the given type key. Overwrites any existing entry.
    pub fn insert<T: 'static>(&self, value: T) {
        let key = TypeId::of::<T>();
        self.inner.borrow_mut().insert(key, Box::new(value));
    }

    /// Insert a boxed value by raw `TypeId`.
    pub fn insert_boxed(&self, type_id: TypeId, value: Box<dyn Any>) {
        self.inner.borrow_mut().insert(type_id, value);
    }

    /// Returns `true` if the map contains a value for the given `TypeId`.
    pub fn contains_by_type_id(&self, type_id: TypeId) -> bool {
        self.inner.borrow().contains_key(&type_id)
    }

    /// Returns `true` if the map contains a value of type `T`.
    pub fn contains<T: 'static>(&self) -> bool {
        self.inner.borrow().contains_key(&TypeId::of::<T>())
    }

    /// Returns the number of entries currently stored.
    pub(crate) fn len(&self) -> usize {
        self.inner.borrow().len()
    }

    /// Downcast the stored value to `T` and clone it.
    /// Returns `None` if the key doesn't exist or the type doesn't match.
    pub fn get_cloned<T: Clone + 'static>(&self) -> Option<T> {
        let key = TypeId::of::<T>();
        self.inner
            .borrow()
            .get(&key)
            .and_then(|boxed| boxed.downcast_ref::<T>())
            .cloned()
    }

    /// Downcast by raw `TypeId` to the given type and clone.
    pub fn get_cloned_by_type_id<T: Clone + 'static>(&self, type_id: TypeId) -> Option<T> {
        self.inner
            .borrow()
            .get(&type_id)
            .and_then(|boxed| boxed.downcast_ref::<T>())
            .cloned()
    }

    /// Borrow the inner `HashMap` by reference.
    ///
    /// Returns a `Ref` that holds a read lock on the interior `RefCell`.
    /// While the `Ref` is alive, calling any mutating method (insert, etc.)
    /// will panic due to `borrow_mut` conflict. Keep the `Ref` lifetime short.
    pub fn inner_ref(&self) -> Ref<'_, HashMap<TypeId, Box<dyn Any>>> {
        self.inner.borrow()
    }

    /// Get a reference to a stored value by raw `TypeId`.
    ///
    /// Returns a `Ref` guard that keeps the `RefCell` read-borrowed.
    /// The downcast reference is valid as long as the guard is alive.
    ///
    /// Note: this method performs a second `HashMap` lookup internally to
    /// satisfy the borrow checker (the `Ref` guard must outlive the
    /// downcast reference). For the clone-based variant see
    /// [`get_cloned_by_type_id`](Self::get_cloned_by_type_id).
    #[allow(
        clippy::type_complexity,
        reason = "return type bundles the RefCell guard with the downcast reference"
    )]
    pub fn get_ref_by_type_id<T: 'static>(
        &self,
        type_id: TypeId,
    ) -> Option<(Ref<'_, HashMap<TypeId, Box<dyn Any>>>, &T)> {
        let guard = self.inner.borrow();
        if guard.get(&type_id)?.downcast_ref::<T>().is_some() {
            // SAFETY: we just verified the downcast succeeds; re-access
            // through the same guard is sound.
            #[allow(unsafe_code, clippy::transmute_ptr_to_ptr)]
            Some(unsafe {
                let ptr: *const T = guard
                    .get(&type_id)
                    .unwrap()
                    .downcast_ref::<T>()
                    .unwrap();
                (guard, &*ptr)
            })
        } else {
            None
        }
    }
}

impl Default for TypeMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn insert_then_get_cloned_returns_value() {
        let map = TypeMap::new();
        map.insert(42i32);
        assert_eq!(map.get_cloned::<i32>(), Some(42));
    }

    #[test]
    fn no_send_sync_bound_required() {
        // Rc<i32> is !Send + !Sync; if TypeMap required Send + Sync bounds,
        // this would not compile.
        let map = TypeMap::new();
        let rc = Rc::new(42);
        map.insert(rc.clone());
        assert_eq!(map.get_cloned::<Rc<i32>>(), Some(rc));
    }

    #[test]
    fn insert_boxed_and_get_by_type_id() {
        let map = TypeMap::new();
        let type_id = std::any::TypeId::of::<i32>();
        map.insert_boxed(type_id, Box::new(42i32));
        assert!(map.contains_by_type_id(type_id));
        assert_eq!(map.get_cloned_by_type_id::<i32>(type_id), Some(42));
    }

    #[test]
    fn overwrite_existing_entry() {
        let map = TypeMap::new();
        map.insert(1i32);
        map.insert(2i32);
        assert_eq!(map.get_cloned::<i32>(), Some(2));
    }

    #[test]
    fn get_cloned_returns_none_for_missing_key() {
        let map = TypeMap::new();
        assert_eq!(map.get_cloned::<i32>(), None);
    }

    #[test]
    fn contains_returns_correct_bool() {
        let map = TypeMap::new();
        assert!(!map.contains::<i32>());
        map.insert(42i32);
        assert!(map.contains::<i32>());
        assert!(!map.contains::<u64>());
    }

    #[test]
    fn len_returns_entry_count() {
        let map = TypeMap::new();
        assert_eq!(map.len(), 0);
        map.insert(1i32);
        assert_eq!(map.len(), 1);
        map.insert("a".to_string());
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn default_creates_empty_map() {
        let map = TypeMap::default();
        assert_eq!(map.len(), 0);
        assert!(map.get_cloned::<i32>().is_none());
    }

    #[test]
    fn contains_by_type_id_returns_false_for_missing() {
        let map = TypeMap::new();
        let tid = std::any::TypeId::of::<i32>();
        assert!(!map.contains_by_type_id(tid));
    }

    #[test]
    fn get_cloned_by_type_id_returns_none_for_wrong_type() {
        let map = TypeMap::new();
        let i32_id = std::any::TypeId::of::<i32>();
        map.insert_boxed(i32_id, Box::new(42i32));
        // Request wrong type via a different TypeId
        let u64_id = std::any::TypeId::of::<u64>();
        assert_eq!(map.get_cloned_by_type_id::<u64>(u64_id), None);
        // Even right TypeId but wrong downcast type returns None
        assert_eq!(map.get_cloned_by_type_id::<u64>(i32_id), None);
    }

    #[test]
    fn inner_ref_returns_reference_to_inserted_value() {
        let map = TypeMap::new();
        map.insert(42i32);
        let r: Ref<'_, HashMap<TypeId, Box<dyn Any>>> = map.inner_ref();
        let i32_id = TypeId::of::<i32>();
        let value = r.get(&i32_id).and_then(|b| b.downcast_ref::<i32>());
        assert_eq!(value, Some(&42));
    }

    #[test]
    fn inner_ref_is_empty_for_new_map() {
        let map = TypeMap::new();
        let r = map.inner_ref();
        assert!(r.is_empty());
    }

    #[test]
    #[should_panic(expected = "already mutably borrowed")]
    fn inner_ref_panics_if_mutably_borrowed() {
        let map = TypeMap::new();
        let _guard = map.inner.borrow_mut();
        // This should panic because inner is already mutably borrowed
        let _ = map.inner_ref();
    }

    #[test]
    fn get_ref_by_type_id_returns_some_for_existing() {
        let map = TypeMap::new();
        map.insert(42i32);
        let i32_id = std::any::TypeId::of::<i32>();
        let result = map.get_ref_by_type_id::<i32>(i32_id);
        assert!(result.is_some());
        let (_guard, val) = result.unwrap();
        assert_eq!(*val, 42);
    }

    #[test]
    fn get_ref_by_type_id_returns_none_for_missing() {
        let map = TypeMap::new();
        let i32_id = std::any::TypeId::of::<i32>();
        let result = map.get_ref_by_type_id::<i32>(i32_id);
        assert!(result.is_none());
    }

    #[test]
    fn get_ref_by_type_id_returns_none_for_wrong_type() {
        let map = TypeMap::new();
        map.insert(42i32);
        let i32_id = std::any::TypeId::of::<i32>();
        // Request as u64 — downcast should fail
        let result = map.get_ref_by_type_id::<u64>(i32_id);
        assert!(result.is_none());
    }

    #[test]
    fn clear_removes_all_entries() {
        let map = TypeMap::new();
        map.insert(42i32);
        map.insert("hello".to_string());
        assert_eq!(map.len(), 2);
        map.clear();
        assert_eq!(map.len(), 0);
        assert!(map.get_cloned::<i32>().is_none());
    }
}
