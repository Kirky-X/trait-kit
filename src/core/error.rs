// Copyright © 2026 Kirky.X

//! Error types for Kit operations.

use thiserror::Error;

/// The error type for Kit operations.
#[derive(Debug, Error)]
pub enum KitError {
    /// A dependency cycle was detected during build validation.
    #[error("dependency cycle detected: {}", cycle.join(" → "))]
    CycleDetected {
        /// The cycle path, e.g. `["A", "B", "C", "A"]`.
        cycle: Vec<&'static str>,
    },

    /// A module depends on another module that was not registered.
    #[error("module `{module}` depends on `{missing}` which is not registered")]
    DependencyMissing {
        /// The module that has the missing dependency.
        module: &'static str,
        /// The missing dependency.
        missing: &'static str,
    },

    /// `build()` was not called before `require()`.
    #[error("kit is not ready; call build() first")]
    NotReady,

    /// A module with the same name was already registered.
    #[error("module `{module}` is already registered")]
    AlreadyRegistered {
        /// The duplicate module name.
        module: &'static str,
    },

    /// Module build failed.
    #[error("failed to build module `{module}`: {source}")]
    BuildFailed {
        /// The module name.
        module: &'static str,
        /// The original build error.
        #[source]
        source: Box<dyn std::error::Error>,
    },

    /// Required capability not found after build.
    #[error("missing capability `{key}`")]
    MissingCapability {
        /// The capability key name.
        key: &'static str,
    },

    /// Required configuration not found.
    #[error("missing config `{key}`")]
    MissingConfig {
        /// The config key name.
        key: &'static str,
    },
}
