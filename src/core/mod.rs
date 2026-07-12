// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Core traits and types for module declaration.

pub mod meta;

#[cfg(feature = "async")]
pub use meta::AsyncAutoBuilder;
pub(crate) use meta::BuildFn;
pub use meta::{AutoBuilder, ModuleMeta};
