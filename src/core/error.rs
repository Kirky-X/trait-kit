// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Error types for Kit operations.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum KitError {
    #[error("dependency cycle detected: {}", cycle.join(" → "))]
    CycleDetected { cycle: Vec<&'static str> },

    #[error("module `{module}` depends on `{missing}` which is not registered")]
    DependencyMissing {
        module: &'static str,
        missing: &'static str,
    },

    #[deprecated(note = "typestate pattern makes this unreachable; will be removed in 0.2.0")]
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
