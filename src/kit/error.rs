// Copyright © 2026 Kirky.X

//! Kit error types.

use std::error::Error;

/// The error type for Kit operations.
///
/// All Kit operations return `Result<T, KitError>`.
#[derive(Debug, thiserror::Error)]
pub enum KitError {
    /// Build failed for a module.
    #[error("failed to build module `{module}`")]
    BuildFailed {
        /// The module name (`Module::NAME`).
        module: &'static str,
        /// The original build error.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    /// Required capability is missing from Kit.
    #[error("missing capability `{key}`")]
    MissingCapability {
        /// The capability key name (`CapabilityKey::NAME`).
        key: &'static str,
    },

    /// Capability already exists in Kit (duplicate registration).
    #[error("capability `{key}` already exists")]
    DuplicateCapability {
        /// The capability key name (`CapabilityKey::NAME`).
        key: &'static str,
    },

    /// Required configuration is missing from Kit.
    #[error("missing config `{key}`")]
    MissingConfig {
        /// The config key name (`ConfigKey::NAME`).
        key: &'static str,
    },
}
