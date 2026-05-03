// Copyright © 2026 Kirky.X. All rights reserved.

//! Module trait — the standard interface for all modules.

use std::error::Error;

use super::builder::ModuleBuilder;

/// The standard interface that all modules must implement.
///
/// A module declares:
/// - `NAME`: A diagnostic name for the module.
/// - `Config`: The configuration type required for initialization.
/// - `Requirements`: The dependencies required for initialization.
/// - `Capability`: The capability facade exposed to consumers.
/// - `Error`: The initialization error type.
/// - `Builder`: The standard builder type that implements `ModuleBuilder<Self>`.
///
/// # Associated Types
///
/// - `Config`: Use `NoConfig` if the module requires no configuration.
/// - `Requirements`: Use `NoRequirements` if the module has no dependencies.
/// - `Capability`: Typically `Arc<dyn SomeTrait + Send + Sync>`.
/// - `Error`: Must satisfy `std::error::Error + Send + Sync + 'static`.
/// - `Builder`: Must implement `ModuleBuilder<Self>`.
///
/// # Example
///
/// ```
/// use trait_kit::prelude::*;
/// use std::sync::Arc;
///
/// struct MyModule;
/// impl Module for MyModule {
///     const NAME: &'static str = "my_module";
///     type Config = NoConfig;
///     type Requirements = NoRequirements;
///     type Capability = Arc<i32>;
///     type Error = std::convert::Infallible;
///     type Builder = MyBuilder;
/// }
///
/// struct MyBuilder;
/// impl ModuleBuilder<MyModule> for MyBuilder {
///     fn build(self) -> Result<Arc<i32>, std::convert::Infallible> {
///         Ok(Arc::new(42))
///     }
/// }
/// ```
pub trait Module: Sized {
    /// The diagnostic name of this module.
    const NAME: &'static str;

    /// The configuration type required for initialization.
    /// Use `NoConfig` if no configuration is needed.
    type Config;

    /// The dependencies required for initialization.
    /// Use `NoRequirements` if no dependencies are needed.
    type Requirements;

    /// The capability facade exposed to consumers.
    /// Typically `Arc<dyn SomeTrait + Send + Sync>`.
    type Capability;

    /// The initialization error type.
    /// Must satisfy `std::error::Error + Send + Sync + 'static`.
    type Error: Error + Send + Sync + 'static;

    /// The standard builder for this module.
    /// Must implement `ModuleBuilder<Self>` (enforced at usage points).
    type Builder: ModuleBuilder<Self>;
}
