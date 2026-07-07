// Copyright © 2026 Kirky.X

//! Kit — the capability and configuration management center.

pub mod graph;
#[allow(clippy::module_inception)]
pub mod kit;
pub(crate) mod typemap;

#[cfg(feature = "confers")]
pub mod config;

pub use kit::{Kit, Ready, Unbuilt};

#[cfg(feature = "confers-macros")]
pub use config::Config;

#[cfg(feature = "encryption")]
pub use config::EncryptedBlob;
