// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Feature toggle system for runtime module enable/disable.
//!
//! The toggle system extends the `conditional` feature's compile-time
//! predicate-based registration (`register_if`) to runtime: modules can be
//! conditionally registered based on string-keyed feature flags that can be
//! toggled on/off at any point during the application lifecycle.
//!
//! # Backend selection
//!
//! - **`confers` feature enabled**: [`ConfersToggle`] delegates boolean toggles
//!   to `confers::FeatureToggleRegistry` (thread-safe, config-loadable).
//!   Non-boolean typed values fall through to an internal memory side-map.
//! - **Otherwise**: [`MemoryToggle`] provides a pure in-memory `HashMap` backend.
//!
//! Requires the `toggle` feature (which implies `conditional`).

use std::collections::HashMap;

// ─── ToggleValue ────────────────────────────────────────────────────────

/// Typed toggle value for non-boolean runtime configuration.
#[derive(Debug, Clone, PartialEq)]
pub enum ToggleValue {
    /// Boolean flag.
    Bool(bool),
    /// Integer value.
    Int(i64),
    /// Floating-point value.
    Float(f64),
    /// String value.
    Str(String),
}

impl std::fmt::Display for ToggleValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(v) => write!(f, "{v}"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Str(v) => write!(f, "{v}"),
        }
    }
}

// ─── ToggleBackend trait ────────────────────────────────────────────────

/// Abstraction over toggle storage backends.
///
/// Implemented by [`MemoryToggle`] (default) and [`ConfersToggle`]
/// (when `confers` feature is enabled).
pub trait ToggleBackend {
    /// Get a toggle value. Returns `None` if the key does not exist.
    fn get(&self, key: &str) -> Option<ToggleValue>;
    /// Set a toggle value. Overwrites any existing value.
    fn set(&mut self, key: String, value: ToggleValue);
    /// Remove a toggle. Returns the previous value if it existed.
    fn remove(&mut self, key: &str) -> Option<ToggleValue>;
    /// List all toggles as `(key, value)` pairs.
    fn list(&self) -> Vec<(String, ToggleValue)>;
    /// Check if a toggle key exists.
    fn contains(&self, key: &str) -> bool;
}

// ─── MemoryToggle ───────────────────────────────────────────────────────

/// Pure in-memory toggle backend using `HashMap`.
///
/// Always available regardless of feature flags. Suitable for single-threaded
/// `Kit` (which uses `RefCell` for interior mutability).
#[derive(Debug, Default, Clone)]
pub struct MemoryToggle {
    map: HashMap<String, ToggleValue>,
}

impl MemoryToggle {
    /// Create an empty memory toggle backend.
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

impl ToggleBackend for MemoryToggle {
    fn get(&self, key: &str) -> Option<ToggleValue> {
        self.map.get(key).cloned()
    }

    fn set(&mut self, key: String, value: ToggleValue) {
        self.map.insert(key, value);
    }

    fn remove(&mut self, key: &str) -> Option<ToggleValue> {
        self.map.remove(key)
    }

    fn list(&self) -> Vec<(String, ToggleValue)> {
        self.map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
}

// ─── ConfersToggle ──────────────────────────────────────────────────────

/// Confers-backed toggle backend.
///
/// Boolean toggles are stored in `confers::FeatureToggleRegistry` (which
/// supports config loading, concurrent access, and descriptions). Non-boolean
/// typed values are kept in a side `HashMap` since confers' registry is
/// boolean-only.
#[cfg(feature = "confers")]
pub struct ConfersToggle {
    registry: confers::FeatureToggleRegistry,
    typed_side: HashMap<String, ToggleValue>,
}

#[cfg(feature = "confers")]
impl ConfersToggle {
    /// Create a new confers-backed toggle with an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: confers::FeatureToggleRegistry::new(),
            typed_side: HashMap::new(),
        }
    }

    /// Create from an existing `FeatureToggleRegistry`.
    #[must_use]
    pub fn with_registry(registry: confers::FeatureToggleRegistry) -> Self {
        Self {
            registry,
            typed_side: HashMap::new(),
        }
    }

    /// Access the underlying confers registry (e.g. for `load_from_config`).
    #[must_use]
    pub fn registry(&self) -> &confers::FeatureToggleRegistry {
        &self.registry
    }
}

#[cfg(feature = "confers")]
impl Default for ConfersToggle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "confers")]
impl ToggleBackend for ConfersToggle {
    fn get(&self, key: &str) -> Option<ToggleValue> {
        // Check typed side first
        if let Some(v) = self.typed_side.get(key) {
            return Some(v.clone());
        }
        // Fall back to confers registry (boolean only)
        if self.registry.len() > 0 {
            let infos = self.registry.list();
            for info in &infos {
                if info.name == key {
                    return Some(ToggleValue::Bool(info.enabled));
                }
            }
        }
        None
    }

    fn set(&mut self, key: String, value: ToggleValue) {
        match &value {
            ToggleValue::Bool(enabled) => {
                // Register in confers if not present, then set state
                if !self.registry.is_enabled(&key) && !self.registry.list().iter().any(|i| i.name == key) {
                    self.registry.register(key.clone(), "", *enabled);
                }
                if *enabled {
                    self.registry.enable(&key);
                } else {
                    self.registry.disable(&key);
                }
                // Remove from typed side if it was there
                self.typed_side.remove(&key);
            }
            _ => {
                // Non-boolean: store in side map
                self.typed_side.insert(key, value);
            }
        }
    }

    fn remove(&mut self, key: &str) -> Option<ToggleValue> {
        // Try typed side first
        if let Some(v) = self.typed_side.remove(key) {
            return Some(v);
        }
        // Check confers registry
        let infos = self.registry.list();
        for info in &infos {
            if info.name == key {
                // Can't truly remove from confers registry, but disable it
                self.registry.disable(key);
                return Some(ToggleValue::Bool(info.enabled));
            }
        }
        None
    }

    fn list(&self) -> Vec<(String, ToggleValue)> {
        let mut result: Vec<(String, ToggleValue)> = self
            .typed_side
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // Add boolean toggles from confers that aren't in the typed side
        for info in self.registry.list() {
            if !self.typed_side.contains_key(&info.name) {
                result.push((info.name, ToggleValue::Bool(info.enabled)));
            }
        }
        result
    }

    fn contains(&self, key: &str) -> bool {
        if self.typed_side.contains_key(key) {
            return true;
        }
        self.registry.list().iter().any(|i| i.name == key)
    }
}

// ─── Backend type alias ─────────────────────────────────────────────────

/// The concrete toggle backend type used by `Kit` and `AsyncKit`.
///
/// - With `confers` feature: [`ConfersToggle`] (confers registry + typed side-map).
/// - Without `confers`: [`MemoryToggle`] (pure HashMap).
#[cfg(feature = "confers")]
pub type ToggleBackendType = ConfersToggle;

/// The concrete toggle backend type used by `Kit` and `AsyncKit`.
#[cfg(not(feature = "confers"))]
pub type ToggleBackendType = MemoryToggle;

// ─── Typed toggle keys (T209) ───────────────────────────────────────────

/// Compile-time toggle key binding (T209).
///
/// Implement this trait (usually via the [`define_toggle_key!`] macro) to get
/// a typed handle [`ToggleHandle`] whose `get`/`set` cannot suffer from
/// misspelled string keys: the key is fixed once at the type level.
pub trait ToggleKey {
    /// The string key stored in the toggle backend.
    const KEY: &'static str;
}

/// Declare a named [`ToggleKey`] type in one line.
///
/// ```ignore
/// trait_kit::kit::toggle::define_toggle_key!(PrdMode = "prd-mode");
/// // `PrdMode::KEY == "prd-mode"`
/// ```
#[macro_export]
macro_rules! define_toggle_key {
    ($name:ident = $key:literal) => {
        #[derive(Debug, Clone, Copy, Default)]
        struct $name;

        impl $crate::kit::toggle::ToggleKey for $name {
            const KEY: &'static str = $key;
        }
    };
    (pub $name:ident = $key:literal) => {
        #[derive(Debug, Clone, Copy, Default)]
        pub struct $name;

        impl $crate::kit::toggle::ToggleKey for $name {
            const KEY: &'static str = $key;
        }
    };
}

/// Strongly typed toggle handle bound to a [`ToggleKey`] (T209).
///
/// Obtained from `Kit<Ready>::toggle_handle::<K>()`. All operations go through
/// `K::KEY`, so a typo is a compile error (unknown type) rather than a silent
/// runtime miss.
pub struct ToggleHandle<'a, K: ToggleKey> {
    kit: &'a crate::kit::Kit<crate::kit::Ready>,
    _marker: std::marker::PhantomData<K>,
}

impl<K: ToggleKey> std::fmt::Debug for ToggleHandle<'_, K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ToggleHandle<{}>({:?})", std::any::type_name::<K>(), K::KEY)
    }
}

impl<'a, K: ToggleKey> ToggleHandle<'a, K> {
    /// Current boolean state of the toggle (false when unset or non-bool).
    #[must_use]
    pub fn get(&self) -> bool {
        self.kit.is_toggle_enabled(K::KEY)
    }

    /// Set the toggle's boolean state.
    pub fn set(&self, enabled: bool) {
        self.kit.enable_toggle(K::KEY, enabled);
    }

    /// The underlying string key (rarely needed; prefer typed usage).
    #[must_use]
    pub const fn key(&self) -> &'static str {
        K::KEY
    }
}

#[cfg(feature = "toggle")]
impl crate::kit::Kit<crate::kit::Ready> {
    /// Create a typed toggle handle for key `K` (T209).
    ///
    /// Requires the `toggle` feature.
    #[must_use]
    pub fn toggle_handle<K: ToggleKey>(&self) -> ToggleHandle<'_, K> {
        ToggleHandle {
            kit: self,
            _marker: std::marker::PhantomData,
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // -- MemoryToggle tests --

    #[test]
    fn memory_toggle_set_get() {
        let mut t = MemoryToggle::new();
        t.set("feature_a".into(), ToggleValue::Bool(true));
        assert_eq!(t.get("feature_a"), Some(ToggleValue::Bool(true)));
    }

    #[test]
    fn memory_toggle_overwrite() {
        let mut t = MemoryToggle::new();
        t.set("key".into(), ToggleValue::Int(1));
        t.set("key".into(), ToggleValue::Int(2));
        assert_eq!(t.get("key"), Some(ToggleValue::Int(2)));
    }

    #[test]
    fn memory_toggle_remove() {
        let mut t = MemoryToggle::new();
        t.set("x".into(), ToggleValue::Str("hello".into()));
        let prev = t.remove("x");
        assert_eq!(prev, Some(ToggleValue::Str("hello".into())));
        assert_eq!(t.get("x"), None);
    }

    #[test]
    fn memory_toggle_list() {
        let mut t = MemoryToggle::new();
        t.set("a".into(), ToggleValue::Bool(true));
        t.set("b".into(), ToggleValue::Float(3.14));
        let list = t.list();
        assert_eq!(list.len(), 2);
        let keys: Vec<&str> = list.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"b"));
    }

    #[test]
    fn memory_toggle_contains() {
        let mut t = MemoryToggle::new();
        assert!(!t.contains("missing"));
        t.set("present".into(), ToggleValue::Bool(false));
        assert!(t.contains("present"));
    }

    #[test]
    fn memory_toggle_missing_key_returns_none() {
        let t = MemoryToggle::new();
        assert_eq!(t.get("nonexistent"), None);
    }

    #[test]
    fn toggle_value_display() {
        assert_eq!(ToggleValue::Bool(true).to_string(), "true");
        assert_eq!(ToggleValue::Int(42).to_string(), "42");
        assert_eq!(ToggleValue::Float(1.5).to_string(), "1.5");
        assert_eq!(ToggleValue::Str("hi".into()).to_string(), "hi");
    }

    // -- ConfersToggle tests --

    #[cfg(feature = "confers")]
    mod confers_tests {
        use super::*;

        #[test]
        fn confers_toggle_bool_roundtrip() {
            let mut t = ConfersToggle::new();
            t.set("feature".into(), ToggleValue::Bool(true));
            assert_eq!(t.get("feature"), Some(ToggleValue::Bool(true)));
            t.set("feature".into(), ToggleValue::Bool(false));
            assert_eq!(t.get("feature"), Some(ToggleValue::Bool(false)));
        }

        #[test]
        fn confers_toggle_typed_side() {
            let mut t = ConfersToggle::new();
            t.set("limit".into(), ToggleValue::Int(100));
            assert_eq!(t.get("limit"), Some(ToggleValue::Int(100)));
        }

        #[test]
        fn confers_toggle_list_merges() {
            let mut t = ConfersToggle::new();
            t.set("bool_flag".into(), ToggleValue::Bool(true));
            t.set("typed_val".into(), ToggleValue::Str("hello".into()));
            let list = t.list();
            assert_eq!(list.len(), 2);
        }

        #[test]
        fn confers_toggle_contains() {
            let mut t = ConfersToggle::new();
            assert!(!t.contains("x"));
            t.set("x".into(), ToggleValue::Bool(false));
            assert!(t.contains("x"));
        }

        #[test]
        fn confers_toggle_remove_typed() {
            let mut t = ConfersToggle::new();
            t.set("val".into(), ToggleValue::Float(1.0));
            let prev = t.remove("val");
            assert_eq!(prev, Some(ToggleValue::Float(1.0)));
            assert!(!t.contains("val"));
        }
    }
}

#[cfg(all(test, feature = "toggle", feature = "confers"))]
mod typed_handle_tests {
    use super::*;
    use crate::kit::Kit;

    struct PrdModeKey;
    impl ToggleKey for PrdModeKey {
        const KEY: &'static str = "prd-mode";
    }

    struct DebugModeKey;
    impl ToggleKey for DebugModeKey {
        const KEY: &'static str = "debug-mode";
    }

    #[test]
    fn typed_handle_set_get_round_trip() {
        let kit = Kit::new();
        kit.enable_toggle("prd-mode", false);
        let ready = kit.build().expect("build ok");

        let handle = ready.toggle_handle::<PrdModeKey>();
        assert!(!handle.get());
        handle.set(true);
        assert!(handle.get());

        // Distinct key type → independent handle over its own key.
        let other = ready.toggle_handle::<DebugModeKey>();
        assert!(!other.get(), "different key untouched by PrdModeKey set");
    }

    #[test]
    fn typed_handle_shares_backend_with_string_api() {
        let mut kit = Kit::new();
        kit.enable_toggle("prd-mode", true);
        let ready = kit.build().expect("build ok");

        let handle = ready.toggle_handle::<PrdModeKey>();
        assert!(handle.get());
        assert!(ready.is_toggle_enabled("prd-mode"), "typed and string APIs share the backend");
        handle.set(false);
        assert!(!ready.is_toggle_enabled("prd-mode"));
        assert_eq!(handle.key(), "prd-mode");
    }

    #[test]
    fn define_toggle_key_macro_generates_usable_key() {
        crate::define_toggle_key!(TestMacroKey = "macro-key");
        assert_eq!(<TestMacroKey as ToggleKey>::KEY, "macro-key");
    }
}
