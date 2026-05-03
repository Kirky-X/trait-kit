// Copyright © 2026 Kirky.X. All rights reserved.

//! Builder traits for module initialization.

use super::module::Module;

/// The standard builder trait for module initialization.
///
/// All modules must have a builder that implements this trait.
/// The `build` method is the only required method and returns
/// the module's capability or an error.
pub trait ModuleBuilder<M: Module> {
    /// Build the module's capability.
    ///
    /// Returns `Ok(M::Capability)` on success, or `Err(M::Error)` on failure.
    fn build(self) -> Result<M::Capability, M::Error>;
}

/// Trait for builders that accept configuration.
///
/// Modules with `Config != NoConfig` should have builders that implement this trait.
pub trait WithConfig<M: Module> {
    /// Inject configuration into the builder.
    ///
    /// Returns `Self` for method chaining.
    fn config(self, config: M::Config) -> Self;
}

/// Trait for builders that accept dependencies.
///
/// Modules with `Requirements != NoRequirements` should have builders that implement this trait.
pub trait WithRequirements<M: Module> {
    /// Inject requirements (dependencies) into the builder.
    ///
    /// Returns `Self` for method chaining.
    fn requirements(self, requirements: M::Requirements) -> Self;
}
