// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Core traits and types for module declaration.

pub mod macros;
pub mod meta;

#[cfg(feature = "lifecycle")]
pub mod lifecycle;
#[cfg(feature = "health")]
pub mod health;
#[cfg(feature = "observability")]
pub mod observer;

#[cfg(feature = "async")]
pub use meta::AsyncAutoBuilder;
pub(crate) use meta::BuildFn;
pub use meta::{AutoBuilder, ModuleMeta};
#[cfg(feature = "interface")]
pub use meta::{Interface, InterfaceBuilder};

#[cfg(feature = "lifecycle")]
pub use lifecycle::Lifecycle;
#[cfg(all(feature = "lifecycle", feature = "async"))]
pub use lifecycle::AsyncLifecycle;

#[cfg(feature = "health")]
pub use health::{HealthCheck, HealthStatus};
#[cfg(all(feature = "health", feature = "async"))]
pub use health::AsyncHealthCheck;

#[cfg(feature = "observability")]
pub use observer::BuildObserver;
