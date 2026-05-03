// Copyright © 2026 Kirky.X. All rights reserved.

//! Kit — the capability and configuration management center.

pub mod builder;
pub mod capability_store;
pub mod config_store;
pub mod error;
#[allow(clippy::module_inception)]
pub mod kit;

pub use error::KitError;
pub use kit::Kit;
