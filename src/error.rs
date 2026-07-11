// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Error types for Kit operations.

use thiserror::Error;

/// Unified trait-kit error type.
///
/// Follows the `ProjectNameError` naming convention used across the base workspace.
#[derive(Debug, Error)]
pub enum TraitKitError {
    #[error("dependency cycle detected: {}", cycle.join(" → "))]
    CycleDetected { cycle: Vec<&'static str> },

    #[error("module `{module}` depends on `{missing}` which is not registered")]
    DependencyMissing {
        module: &'static str,
        missing: &'static str,
    },

    #[deprecated(note = "typestate pattern makes this unreachable; will be removed in 0.3.0")]
    #[error("kit is not ready; call build() first")]
    NotReady,

    #[error("module `{module}` is already registered")]
    AlreadyRegistered { module: &'static str },

    #[error("failed to build `{context}`: {source}")]
    BuildFailed {
        context: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + 'static>,
    },

    #[error("missing capability `{key}`")]
    MissingCapability { key: &'static str },

    #[error("missing config `{key}`")]
    MissingConfig { key: &'static str },
}

/// Convenience `Result` alias for trait-kit operations.
pub type TraitKitResult<T> = std::result::Result<T, TraitKitError>;
