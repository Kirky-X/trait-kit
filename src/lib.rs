// Copyright © 2026 Kirky.X. All rights reserved.

//! trait-kit — 模块标准接口与能力管理中心
//!
//! 提供模块定义标准接口和 Kit 能力管理中心的轻量实现。

pub mod core;
pub mod kit;

pub mod prelude {
    pub use crate::core::builder::{ModuleBuilder, WithConfig, WithRequirements};
    pub use crate::core::capability::CapabilityKey;
    pub use crate::core::config::{ConfigHandle, ConfigKey};
    pub use crate::core::error::BuildError;
    pub use crate::core::marker::{NoConfig, NoRequirements};
    pub use crate::core::module::Module;
    pub use crate::kit::builder::IntoKitModuleBuilder;
    pub use crate::kit::error::KitError;
    pub use crate::kit::Kit;
}
