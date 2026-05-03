// Copyright © 2026 Kirky.X. All rights reserved.

//! Config key trait and ConfigHandle for configuration management.

use std::sync::Arc;

use arc_swap::ArcSwap;

/// The key trait for identifying configuration in Kit.
///
/// Each configuration is identified by a unique key type that implements this trait.
/// The `Config` associated type specifies the configuration value type.
/// The `NAME` constant provides a diagnostic name for error messages.
///
/// # Type Constraints
///
/// The `Config` type must satisfy `Send + Sync + 'static` for thread-safe storage.
/// Note: `Config` must be `Sized` (no `?Sized` bound) because `ArcSwap<T>` requires `T: Sized`.
///
/// The key type itself must satisfy `'static` for TypeId stability.
pub trait ConfigKey: 'static {
    /// The configuration value type.
    /// Must satisfy `Send + Sync + 'static` and `Sized`.
    type Config: Send + Sync + 'static;

    /// The diagnostic name for this config key.
    /// Used in error messages like `MissingConfig { key: "logger_config" }`.
    const NAME: &'static str;
}

/// A shared handle to a configuration value.
///
/// `ConfigHandle<T>` wraps an `Arc<ArcSwap<T>>` to provide:
/// - `load()`: Read the current configuration snapshot (returns `Arc<T>`).
/// - `set()`: Update the configuration value.
/// - `clone`: Share the same underlying swap cell across multiple handles.
///
/// Multiple `ConfigHandle<T>` clones point to the same underlying swap cell.
/// Updates via one handle are immediately visible to all other handles.
///
/// Old snapshots (returned by `load()`) remain valid after updates.
///
/// # Type Constraints
///
/// `T` must satisfy `Send + Sync + 'static` and `Sized`.
#[derive(Debug)]
pub struct ConfigHandle<T: Send + Sync + 'static> {
    inner: Arc<ArcSwap<T>>,
}

impl<T: Send + Sync + 'static> ConfigHandle<T> {
    /// Create a new ConfigHandle with an initial value.
    pub fn new(value: T) -> Self {
        ConfigHandle {
            inner: Arc::new(ArcSwap::new(Arc::new(value))),
        }
    }

    /// Load the current configuration snapshot.
    ///
    /// Returns an `Arc<T>` that remains valid even after subsequent `set()` calls.
    pub fn load(&self) -> Arc<T> {
        self.inner.load_full()
    }

    /// Update the configuration value.
    ///
    /// All clones of this handle will see the new value on their next `load()` call.
    pub fn set(&self, value: T) {
        self.inner.store(Arc::new(value));
    }
}

impl<T: Send + Sync + 'static> Clone for ConfigHandle<T> {
    fn clone(&self) -> Self {
        ConfigHandle {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    struct TestConfig;
    impl ConfigKey for TestConfig {
        type Config = i32;
        const NAME: &'static str = "test_config";
    }

    #[test]
    fn new_creates_handle_with_initial_value() {
        let handle = ConfigHandle::new(42);
        assert_eq!(*handle.load(), 42);
    }

    #[test]
    fn load_returns_current_value() {
        let handle = ConfigHandle::new(10);
        assert_eq!(*handle.load(), 10);
    }

    #[test]
    fn set_updates_value() {
        let handle = ConfigHandle::new(1);
        handle.set(2);
        assert_eq!(*handle.load(), 2);
    }

    #[test]
    fn clone_shares_same_inner_cell() {
        let handle = ConfigHandle::new(100);
        let cloned = handle.clone();
        handle.set(200);
        assert_eq!(*cloned.load(), 200);
    }
}
