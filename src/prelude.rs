// Copyright © 2026 Kirky.X. All rights reserved.

//! Re-exports of the most commonly used types and traits.

pub use crate::core::builder::{ModuleBuilder, WithConfig, WithRequirements};
pub use crate::core::capability::CapabilityKey;
pub use crate::core::config::{ConfigHandle, ConfigKey};
pub use crate::core::error::BuildError;
pub use crate::core::marker::{NoConfig, NoRequirements};
pub use crate::core::module::Module;
pub use crate::kit::builder::IntoKitModuleBuilder;
pub use crate::kit::error::KitError;
pub use crate::kit::Kit;
