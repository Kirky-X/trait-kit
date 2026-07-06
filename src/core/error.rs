// Copyright © 2026 Kirky.X

//! Error types for Kit operations.

use std::fmt;

/// The error type for Kit operations.
#[derive(Debug)]
pub enum KitError {
    /// A dependency cycle was detected during build validation.
    CycleDetected {
        /// The cycle path, e.g. ["A", "B", "C", "A"].
        cycle: Vec<&'static str>,
    },

    /// A module depends on another module that was not registered.
    DependencyMissing {
        /// The module that has the missing dependency.
        module: &'static str,
        /// The missing dependency.
        missing: &'static str,
    },

    /// `build()` was not called before `require()`.
    NotReady,

    /// A module with the same name was already registered.
    AlreadyRegistered {
        /// The duplicate module name.
        module: &'static str,
    },

    /// Module build failed.
    BuildFailed {
        /// The module name.
        module: &'static str,
        /// The original build error.
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Required capability not found after build.
    MissingCapability {
        /// The capability key name.
        key: &'static str,
    },

    /// Required configuration not found.
    MissingConfig {
        /// The config key name.
        key: &'static str,
    },
}

impl fmt::Display for KitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KitError::CycleDetected { cycle } => {
                write!(f, "dependency cycle detected: {}", cycle.join(" → "))
            }
            KitError::DependencyMissing { module, missing } => {
                write!(
                    f,
                    "module `{module}` depends on `{missing}` which is not registered"
                )
            }
            KitError::NotReady => write!(f, "kit is not ready; call build() first"),
            KitError::AlreadyRegistered { module } => {
                write!(f, "module `{module}` is already registered")
            }
            KitError::BuildFailed { module, source } => {
                write!(f, "failed to build module `{module}`: {source}")
            }
            KitError::MissingCapability { key } => {
                write!(f, "missing capability `{key}`")
            }
            KitError::MissingConfig { key } => {
                write!(f, "missing config `{key}`")
            }
        }
    }
}

impl std::error::Error for KitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            KitError::BuildFailed { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}
