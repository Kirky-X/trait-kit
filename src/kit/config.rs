// Copyright © 2026 Kirky.X

//! Configuration loader trait for the "loader pattern" integration with confers.
//!
//! trait-kit defines the `Configurable` trait as a backend-agnostic interface;
//! users bridge to `confers::Config` derive macro's `load_sync()` (or any other
//! source) by implementing this trait. The Kit then loads and stores the value
//! through its `TypeMap` backend, keeping `set_config`/`config` synchronous and
//! type-safe.
//!
//! Level 2 (`confers-macros` feature) adds the `ModuleConfig` trait for
//! module-level config metadata (path + default) and re-exports the
//! `confers::Config` derive macro so users can `use trait_kit::kit::Config;`.
//!
//! # Three-tier inheritance system (三层继承)
//!
//! The confers integration is built on a three-tier inheritance model:
//!
//! 1. **Module capability inheritance (模块能力继承)** — `#[derive(Config)]`
//!    auto-implements serialization, deserialization, hot-reload subscription,
//!    encryption markers, and validation rules. `ModuleConfig` binds each
//!    config type to its module's configuration path (`PATH`).
//!
//! 2. **Cargo feature inheritance (cargo feature 继承)** — feature flags form
//!    a dependency chain: `confers-encryption` → `confers-hot-reload` →
//!    `confers-macros` → `confers`. Enabling a higher level automatically
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
    fn load() -> Result<Self, Box<dyn std::error::Error>>;
}

/// Re-export of the `confers::Config` derive macro.
///
/// Allows `use trait_kit::kit::Config;` to derive the configuration loader
/// implementation backed by confers' `load_sync()` / `load_file()` codegen.
#[cfg(feature = "confers-macros")]
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
#[cfg(feature = "confers-macros")]
pub trait ModuleConfig: Clone + 'static {
    /// Configuration file path relative to the application root.
    const PATH: &'static str;

    /// Return the default configuration value (fallback when loading fails
    /// or no source is configured).
    fn default_value() -> Self;
}

/// Re-export of confers' XChaCha20-Poly1305 cipher (synchronous API).
#[cfg(feature = "confers-encryption")]
pub use confers::XChaCha20Crypto;

/// Re-export of confers' HKDF-based per-field key derivation.
#[cfg(feature = "confers-encryption")]
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
#[cfg(feature = "confers-encryption")]
#[derive(Debug, Clone)]
pub struct EncryptedBlob {
    /// XChaCha20-Poly1305 nonce (24 bytes).
    pub(crate) nonce: Vec<u8>,
    /// Ciphertext + Poly1305 authentication tag.
    pub(crate) ciphertext: Vec<u8>,
}

#[cfg(feature = "confers-encryption")]
impl EncryptedBlob {
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

#[cfg(all(test, feature = "confers-encryption"))]
mod encrypted_blob_tests {
    use super::EncryptedBlob;

    #[test]
    fn getters_return_raw_slices() {
        let blob = EncryptedBlob {
            nonce: vec![1, 2, 3],
            ciphertext: vec![4, 5, 6],
        };
        assert_eq!(blob.nonce(), &[1, 2, 3]);
        assert_eq!(blob.ciphertext(), &[4, 5, 6]);
    }

    #[test]
    fn getters_return_empty_for_empty_blob() {
        let blob = EncryptedBlob {
            nonce: Vec::new(),
            ciphertext: Vec::new(),
        };
        assert!(blob.nonce().is_empty());
        assert!(blob.ciphertext().is_empty());
    }

    #[test]
    fn clone_produces_equal_blob() {
        let blob = EncryptedBlob {
            nonce: vec![1, 2, 3],
            ciphertext: vec![4, 5, 6],
        };
        let cloned = blob.clone();
        assert_eq!(blob.nonce(), cloned.nonce());
        assert_eq!(blob.ciphertext(), cloned.ciphertext());
    }

    #[test]
    fn debug_format_contains_fields() {
        let blob = EncryptedBlob {
            nonce: vec![1, 2, 3],
            ciphertext: vec![4, 5, 6],
        };
        let s = format!("{:?}", blob);
        assert!(s.contains("EncryptedBlob"));
        assert!(s.contains("nonce"));
        assert!(s.contains("ciphertext"));
    }
}
