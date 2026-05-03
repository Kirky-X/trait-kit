// Copyright © 2026 Kirky.X. All rights reserved.

//! Capability key trait for identifying and typing capabilities.

/// The key trait for identifying capabilities in Kit.
///
/// Each capability is identified by a unique key type that implements this trait.
/// The `Capability` associated type specifies the trait object type of the capability.
/// The `NAME` constant provides a diagnostic name for error messages.
///
/// # Type Constraints
///
/// The `Capability` type must satisfy:
/// - `?Sized` — allows trait objects like `dyn SomeTrait`
/// - `Send + Sync + 'static` — required for thread-safe storage in Kit
///
/// The key type itself must satisfy `'static` for TypeId stability.
pub trait CapabilityKey: 'static {
    /// The capability trait object type.
    /// Must satisfy `?Sized + Send + Sync + 'static`.
    type Capability: ?Sized + Send + Sync + 'static;

    /// The diagnostic name for this capability key.
    /// Used in error messages like `MissingCapability { key: "main_logger" }`.
    const NAME: &'static str;
}
