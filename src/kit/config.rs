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
