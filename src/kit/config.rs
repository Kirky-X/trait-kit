// Copyright © 2026 Kirky.X

//! Configuration loader trait for the "loader pattern" integration with confers.
//!
//! trait-kit defines the `Configurable` trait as a backend-agnostic interface;
//! users bridge to `confers::Config` derive macro's `load_sync()` (or any other
//! source) by implementing this trait. The Kit then loads and stores the value
//! through its `TypeMap` backend, keeping `set_config`/`config` synchronous and
//! type-safe.

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
