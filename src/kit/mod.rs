// Copyright © 2026 Kirky.X

//! Kit — the capability and configuration management center.

pub mod graph;
#[allow(clippy::module_inception)]
pub mod kit;
pub(crate) mod typemap;

pub use kit::{Kit, Ready, Unbuilt};
