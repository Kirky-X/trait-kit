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

// === Level 2: module-level config inheritance ===

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
#[cfg(feature = "confers-macros")]
pub trait ModuleConfig: Clone + 'static {
    /// Configuration file path relative to the application root.
    const PATH: &'static str;

    /// Return the default configuration value (fallback when loading fails
    /// or no source is configured).
    fn default_value() -> Self;
}
