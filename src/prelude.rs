// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Re-exports of the most commonly used types and traits.

pub use crate::core::error::KitError;
pub use crate::core::meta::{AutoBuilder, ModuleMeta};
pub use crate::kit::{Kit, Ready, Unbuilt};

#[cfg(feature = "confers")]
pub use crate::kit::config::Configurable;

#[cfg(feature = "confers-macros")]
pub use crate::kit::config::ModuleConfig;
