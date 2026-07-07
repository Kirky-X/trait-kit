// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Core traits for module declaration and dependency management.

/// Metadata trait for module registration.
pub trait ModuleMeta: 'static {
    /// The diagnostic name of this module.
    const NAME: &'static str;

    /// Returns (name, `TypeId`) pairs for modules this module depends on.
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)];
}

/// Builder trait for module construction.
///
/// Implemented by the user for each module.
pub trait AutoBuilder: ModuleMeta {
    /// The capability type this module provides. Must be Clone.
    type Capability: Clone + 'static;

    /// The error type returned on build failure.
    type Error: std::error::Error + 'static;

    /// Build the module's capability using the provided Kit.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if the module fails to build.
    fn build(kit: &crate::kit::Kit) -> Result<Self::Capability, Self::Error>;
}

/// Type-erased build function stored in the dependency graph.
///
/// Takes `&Kit<Unbuilt>` (same memory layout as `&Kit<Ready>`)
/// because during the build phase we only have the unbuilt Kit.
pub(crate) type BuildFn =
    Box<dyn FnOnce(&crate::kit::Kit) -> Result<Box<dyn std::any::Any>, Box<dyn std::error::Error>>>;
