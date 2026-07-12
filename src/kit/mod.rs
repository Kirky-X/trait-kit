// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Kit — the capability and configuration management center.

pub mod graph;
#[allow(clippy::module_inception)]
pub mod kit;
pub(crate) mod typemap;

#[cfg(feature = "async")]
pub mod async_kit;
#[cfg(feature = "async")]
pub mod async_typemap;

#[cfg(feature = "confers")]
pub mod config;

pub use graph::{DependencyGraph, GraphError, ModuleEntry};
pub use kit::{Kit, Ready, Unbuilt};
pub(crate) use typemap::TypeMap;

#[cfg(feature = "async")]
pub use async_kit::{AsyncKit, Ready as AsyncReady, Unbuilt as AsyncUnbuilt};
#[cfg(feature = "async")]
pub use async_typemap::AsyncTypeMap;

#[cfg(feature = "confers-macros")]
pub use config::Config;
#[cfg(feature = "confers")]
pub use config::Configurable;
#[cfg(feature = "confers-macros")]
pub use config::ModuleConfig;

#[cfg(feature = "encryption")]
pub use config::EncryptedBlob;
#[cfg(feature = "encryption")]
pub(crate) use config::XChaCha20Crypto;
