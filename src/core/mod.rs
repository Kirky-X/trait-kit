// Copyright © 2026 Kirky.X. All rights reserved.

//! Core traits and types for module definition.

pub mod builder;
pub mod capability;
pub mod config;
pub mod error;
pub mod marker;
pub mod module;

pub use builder::{ModuleBuilder, WithConfig, WithRequirements};
pub use capability::CapabilityKey;
pub use config::{ConfigHandle, ConfigKey};
pub use error::BuildError;
pub use marker::{NoConfig, NoRequirements};
pub use module::Module;
