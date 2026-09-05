// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Configuration loader trait for the "loader pattern" integration with confers.
//!
//! trait-kit defines the `Configurable` trait as a backend-agnostic interface;
//! users bridge to `confers::Config` derive macro's `load_sync()` (or any other
//! source) by implementing this trait. The Kit then loads and stores the value
//! through its `TypeMap` backend, keeping `set_config`/`config` synchronous and
//! type-safe.
//!
//! Level 2 (`confers` feature) adds the `ModuleConfig` trait for
//! module-level config metadata (path + default) and re-exports the
//! `confers::Config` derive macro so users can `use trait_kit::kit::Config;`.
//!
//! # Three-tier inheritance system (三层继承)
//!
//! The confers integration is built on a three-tier inheritance model:
//!
//! 1. **Module capability inheritance (模块能力继承)** — `#[derive(Config)]`
//!    auto-implements serialization, deserialization, reload subscription,
//!    encryption markers, and validation rules. `ModuleConfig` binds each
//!    config type to its module's configuration path (`PATH`).
//!
//! 2. **Cargo feature inheritance (cargo feature 继承)** — feature flags form
//!    a dependency chain: `encryption` → `reload` →
//!    `confers`. Enabling a higher level automatically
//!    enables all lower levels.
//!
//! 3. **Config value inheritance (配置值继承)** — the encryption key is
//!    derived from `ModuleConfig::PATH` via HKDF, so the same master key
//!    produces different field keys for different modules.

/// Trait for types that can load themselves from a configuration source.
///
/// Implementors typically delegate to `confers::Config`'s derived `load_sync()`
/// method, but any loader (file parse, env scan, network fetch) is allowed.
///
/// # Errors
///
/// Implementations should return an error when loading fails (missing file,
/// invalid format, type mismatch, etc.).
#[cfg(feature = "confers")]
pub trait Configurable: Clone + 'static {
    /// Load the configuration value from its source.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration could not be loaded.
    fn load() -> Result<Self, Box<dyn std::error::Error + Send + 'static>>;
}

/// Re-export of the `confers::Config` derive macro.
///
/// Allows `use trait_kit::kit::Config;` to derive the configuration loader
/// implementation backed by confers' `load_sync()` / `load_file()` codegen.
#[cfg(feature = "confers")]
pub use confers::Config;

/// Trait for module-level configuration metadata.
///
/// Layer 1 of the three-tier inheritance system: each module declares its
/// configuration path and a default value. Combined with `#[derive(Config)]`
/// (re-exported as [`Config`]), modules gain both loading and fallback
/// capabilities. `ModuleConfig` does not require `Configurable` — a module
/// may provide a default without a loader, or vice versa.
///
/// `default_value()` is not invoked automatically by `Kit` internally;
/// callers must opt-in via [`Kit::load_config_or_default`](super::kit::Kit::load_config_or_default)
/// when they want load-with-fallback semantics.
#[cfg(feature = "confers")]
pub trait ModuleConfig: Clone + 'static {
    /// Configuration file path relative to the application root.
    const PATH: &'static str;

    /// Return the default configuration value (fallback when loading fails
    /// or no source is configured).
    fn default_value() -> Self;
}

/// Trait for configuration types that support validation after loading.
///
/// Implementors define validation rules that are checked by
/// `Kit::load_and_validate` after the configuration is loaded. If validation
/// fails, the configuration is not stored in the Kit.
///
/// This trait is backend-agnostic — users may implement validation by hand,
/// via `garde`, or any other mechanism.
#[cfg(feature = "confers")]
pub trait Validatable: Clone + 'static {
    /// Validate the configuration value.
    ///
    /// Returns `Ok(())` if valid, or `Err` with all failure reasons.
    ///
    /// # Errors
    ///
    /// Returns `Err(Vec<String>)` containing every validation failure
    /// when the configuration is invalid.
    fn validate(&self) -> Result<(), Vec<String>>;
}

/// Error type for configuration validation failures.
///
/// Wraps a list of validation error messages into a single `Error + Send`
/// suitable for `TraitKitError::BuildFailed::source`.
#[cfg(feature = "confers")]
#[derive(Debug)]
pub struct ValidationError {
    /// Individual validation failure messages.
    pub errors: Vec<String>,
}

#[cfg(feature = "confers")]
impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "validation failed: {}", self.errors.join("; "))
    }
}

#[cfg(feature = "confers")]
impl std::error::Error for ValidationError {}

/// Trait for compile-time safe field-level configuration override.
///
/// Types implementing `ConfigInherit` declare an associated `Override` type
/// where each field is wrapped in `Option<T>`. Only fields set to `Some`
/// in the override are applied; `None` fields leave the original value intact.
///
/// Designed for use with `#[derive(ConfigInherit)]` from `trait-kit-derive`,
/// which auto-generates the `Override` type and `apply_override` implementation.
///
/// # Example (manual implementation)
///
/// ```ignore
/// struct DbConfig { host: String, port: u16 }
///
/// #[derive(Clone, Default)]
/// struct DbConfigOverride {
///     host: Option<String>,
///     port: Option<u16>,
/// }
///
/// impl ConfigInherit for DbConfig {
///     type Override = DbConfigOverride;
///     fn apply_override(&mut self, ovr: &Self::Override) {
///         if let Some(ref h) = ovr.host { self.host = h.clone(); }
///         if let Some(ref p) = ovr.port { self.port = *p; }
///     }
/// }
/// ```
#[cfg(feature = "confers")]
pub trait ConfigInherit: Clone + 'static {
    /// Override type with each field wrapped in `Option<T>`.
    type Override: Clone + Default + 'static;

    /// Apply non-`None` fields from the override to `self`.
    fn apply_override(&mut self, ovr: &Self::Override);
}

/// Trait for declaring which config fields participate in the shared namespace.
///
/// The shared namespace is a `serde_json::Map<String, Value>` overlay inside
/// the Kit that bridges values across different config types. When project A
/// and project B both declare `host` as a shared field, A's `host` value
/// flows to B automatically via `extract_shared` → `inject_shared`.
///
/// Uses `serde_json::Value` (not `String`) to preserve type information and
/// avoid parse failures.
///
/// Designed for use with `#[derive(SharedConfig)]` from `trait-kit-derive`,
/// which parses `#[shared(field1, field2)]` attributes to auto-generate
/// both methods.
#[cfg(feature = "confers")]
pub trait SharedConfig: Clone + 'static {
    /// Extract shared fields as a JSON map.
    fn extract_shared(&self) -> serde_json::Map<String, serde_json::Value>;

    /// Inject shared fields from a JSON map into `self`.
    ///
    /// Fields with type-mismatched values are silently skipped (no panic).
    fn inject_shared(&mut self, shared: &serde_json::Map<String, serde_json::Value>);
}

use std::collections::HashMap;
use std::hash::BuildHasher;

/// Interpolate `${VAR}` and `${VAR:-default}` patterns in a JSON value.
///
/// Recursively walks the JSON structure, replacing patterns in String values
/// only. Object keys and non-String variants are left unchanged. Unknown
/// variables without a default are preserved as-is.
#[cfg(feature = "confers")]
pub fn interpolate_json_value<S: BuildHasher>(
    value: &mut serde_json::Value,
    vars: &HashMap<String, String, S>,
) {
    match value {
        serde_json::Value::String(s) => {
            *s = interpolate_string(s, vars);
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                interpolate_json_value(item, vars);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map {
                interpolate_json_value(v, vars);
            }
        }
        _ => {}
    }
}

/// Recursively deep-merge `overlay` into `base`.
///
/// - When both `base` and `overlay` are JSON Objects, merge key-by-key:
///   - Keys only in `overlay` are inserted into `base`.
///   - Keys in both where both values are Objects are merged recursively.
///   - Keys in both where values are not both Objects: `overlay` wins (replaces).
/// - Arrays are treated as atomic values (replaced, not element-wise merged).
/// - Scalars are replaced.
#[cfg(feature = "confers")]
pub(crate) fn merge_json_deep(
    base: &mut serde_json::Value,
    overlay: &serde_json::Value,
) {
    match (base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            for (key, overlay_val) in overlay_map {
                if let Some(base_val) = base_map.get_mut(key) {
                    merge_json_deep(base_val, overlay_val);
                } else {
                    base_map.insert(key.clone(), overlay_val.clone());
                }
            }
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
    }
}

/// Replace `${VAR}` and `${VAR:-default}` patterns in a single string.
#[cfg(feature = "confers")]
fn interpolate_string<S: BuildHasher>(s: &str, vars: &HashMap<String, String, S>) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_name = String::new();
            let mut found_close = false;
            let mut has_default = false;
            let mut default_value = String::new();
            while let Some(c) = chars.next() {
                if c == '}' {
                    found_close = true;
                    break;
                }
                if c == ':' && !has_default {
                    // Check for `:-` default syntax
                    if chars.peek() == Some(&'-') {
                        chars.next(); // consume '-'
                        has_default = true;
                        continue;
                    }
                }
                if has_default {
                    default_value.push(c);
                } else {
                    var_name.push(c);
                }
            }
            if found_close {
                if let Some(val) = vars.get(&var_name) {
                    result.push_str(val);
                } else if has_default {
                    result.push_str(&default_value);
                } else {
                    // Preserve original pattern
                    result.push_str("${");
                    result.push_str(&var_name);
                    result.push('}');
                }
            } else {
                // Unclosed `${`, preserve as-is
                result.push_str("${");
                result.push_str(&var_name);
                if has_default {
                    result.push_str(":-");
                    result.push_str(&default_value);
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Re-export of confers' XChaCha20-Poly1305 cipher (synchronous API).
#[cfg(feature = "encryption")]
pub use confers::XChaCha20Crypto;

/// Re-export of confers' HKDF-based per-field key derivation.
#[cfg(feature = "encryption")]
pub use confers::derive_field_key;

/// Encrypted configuration blob: nonce + ciphertext.
///
/// Stored in `Kit`'s `encrypted_configs` map keyed by `TypeId`. Use
/// [`Kit::set_encrypted`](super::kit::Kit::set_encrypted) /
/// [`Kit::get_encrypted`](super::kit::Kit::get_encrypted) to populate
/// and read values.
///
/// Layer 3 of the inheritance system: the encryption key is derived from
/// `ModuleConfig::PATH`, so the encrypted blob is bound to the module's
/// declared configuration path.
#[cfg(feature = "encryption")]
#[derive(Clone)]
pub struct EncryptedBlob {
    /// XChaCha20-Poly1305 nonce (24 bytes).
    nonce: Vec<u8>,
    /// Ciphertext + Poly1305 authentication tag.
    ciphertext: Vec<u8>,
}

#[cfg(feature = "encryption")]
impl std::fmt::Debug for EncryptedBlob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedBlob")
            .field("nonce", &"[REDACTED]")
            .field("ciphertext", &"[REDACTED]")
            .finish()
    }
}

#[cfg(feature = "encryption")]
impl EncryptedBlob {
    /// Create a new encrypted blob from raw nonce and ciphertext.
    #[must_use]
    pub(crate) fn new(nonce: Vec<u8>, ciphertext: Vec<u8>) -> Self {
        Self { nonce, ciphertext }
    }

    /// Returns the XChaCha20-Poly1305 nonce (24 bytes).
    #[must_use]
    pub fn nonce(&self) -> &[u8] {
        &self.nonce
    }

    /// Returns the ciphertext + Poly1305 authentication tag.
    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

#[cfg(all(test, feature = "encryption"))]
mod encrypted_blob_tests {
    use super::EncryptedBlob;

    #[test]
    fn getters_return_raw_slices() {
        let blob = EncryptedBlob::new(vec![1, 2, 3], vec![4, 5, 6]);
        assert_eq!(blob.nonce(), &[1, 2, 3]);
        assert_eq!(blob.ciphertext(), &[4, 5, 6]);
    }

    #[test]
    fn getters_return_empty_for_empty_blob() {
        let blob = EncryptedBlob::new(Vec::new(), Vec::new());
        assert!(blob.nonce().is_empty());
        assert!(blob.ciphertext().is_empty());
    }

    #[test]
    fn clone_produces_equal_blob() {
        let blob = EncryptedBlob::new(vec![1, 2, 3], vec![4, 5, 6]);
        let cloned = blob.clone();
        assert_eq!(blob.nonce(), cloned.nonce());
        assert_eq!(blob.ciphertext(), cloned.ciphertext());
    }

    #[test]
    fn debug_format_redacts_sensitive_data() {
        let blob = EncryptedBlob::new(vec![1, 2, 3], vec![4, 5, 6]);
        let s = format!("{blob:?}");
        assert!(s.contains("EncryptedBlob"));
        assert!(s.contains("[REDACTED]"));
        // Ensure raw byte values are NOT leaked
        assert!(!s.contains("[1, 2, 3]"));
        assert!(!s.contains("[4, 5, 6]"));
    }
}

#[cfg(all(test, feature = "confers"))]
mod merge_json_tests {
    use super::merge_json_deep;
    use serde_json::json;

    #[test]
    fn empty_objects_merge_to_empty() {
        let mut base = json!({});
        let overlay = json!({});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({}));
    }

    #[test]
    fn overlay_inserts_new_keys() {
        let mut base = json!({"a": 1});
        let overlay = json!({"b": 2});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn overlay_replaces_scalars() {
        let mut base = json!({"a": 1, "b": "old"});
        let overlay = json!({"a": 99, "b": "new"});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"a": 99, "b": "new"}));
    }

    #[test]
    fn nested_objects_merge_recursively() {
        let mut base = json!({"a": {"b": 1, "c": 2}});
        let overlay = json!({"a": {"c": 3, "d": 4}});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"a": {"b": 1, "c": 3, "d": 4}}));
    }

    #[test]
    fn deeply_nested_merge() {
        let mut base = json!({"l1": {"l2": {"l3": {"keep": true, "override": "old"}}}});
        let overlay = json!({"l1": {"l2": {"l3": {"override": "new"}}}});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(
            base,
            json!({"l1": {"l2": {"l3": {"keep": true, "override": "new"}}}})
        );
    }

    #[test]
    fn arrays_are_replaced_not_merged() {
        let mut base = json!({"arr": [1, 2, 3]});
        let overlay = json!({"arr": [4]});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"arr": [4]}));
    }

    #[test]
    fn object_replaces_non_object() {
        let mut base = json!({"x": 42});
        let overlay = json!({"x": {"nested": true}});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"x": {"nested": true}}));
    }

    #[test]
    fn non_object_replaces_object() {
        let mut base = json!({"x": {"nested": true}});
        let overlay = json!({"x": 42});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"x": 42}));
    }

    #[test]
    fn null_overlay_replaces_value() {
        let mut base = json!({"a": 1});
        let overlay = json!({"a": null});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"a": null}));
    }
}
