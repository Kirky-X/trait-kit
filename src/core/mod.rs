// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Core traits and types for module declaration.
//!
//! This module defines the foundational traits that all trait-kit modules
//! must implement:
//!
//! - [`ModuleMeta`] — module identity and dependency declaration.
//! - [`AutoBuilder`] — synchronous capability construction.
//! - [`AsyncAutoBuilder`] — async counterpart (requires `async` feature).
//!
//! Optional sub-modules provide lifecycle hooks, health checks,
//! observability callbacks, and interface-builder support, each gated
//! behind its respective feature flag.

pub mod macros;
pub mod meta;

#[cfg(feature = "health")]
pub mod health;
#[cfg(feature = "lifecycle")]
pub mod lifecycle;
#[cfg(feature = "observer")]
pub mod observer;

#[cfg(feature = "async")]
pub use meta::AsyncAutoBuilder;
pub(crate) use meta::BuildFn;
pub use meta::{AutoBuilder, ModuleMeta};
#[cfg(feature = "interface")]
pub use meta::{Interface, InterfaceBuilder};

#[cfg(all(feature = "lifecycle", feature = "async"))]
pub use lifecycle::AsyncLifecycle;
#[cfg(feature = "lifecycle")]
pub use lifecycle::Lifecycle;

#[cfg(all(feature = "health", feature = "async"))]
pub use health::AsyncHealthCheck;
#[cfg(feature = "health")]
pub use health::{HealthCheck, HealthStatus};

#[cfg(feature = "observer")]
pub use observer::BuildObserver;
