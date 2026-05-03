// Copyright © 2026 Kirky.X. All rights reserved.

//! Build error types for module initialization.

use std::error::Error;
use std::fmt;

/// Default build error type for modules.
///
/// Modules can use this enum or define their own error type that satisfies
/// `std::error::Error + Send + Sync + 'static`.
#[derive(Debug)]
pub enum BuildError {
    /// Configuration was not provided to the builder.
    MissingConfig { module: &'static str },

    /// Requirements (dependencies) were not provided to the builder.
    MissingRequirements { module: &'static str },

    /// Configuration content is invalid.
    InvalidConfig {
        module: &'static str,
        reason: &'static str,
    },

    /// Module-specific failure with source error.
    ModuleFailed {
        module: &'static str,
        source: Box<dyn Error + Send + Sync>,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::MissingConfig { module } => {
                write!(f, "missing config for module `{module}`")
            }
            BuildError::MissingRequirements { module } => {
                write!(f, "missing requirements for module `{module}`")
            }
            BuildError::InvalidConfig { module, reason } => {
                write!(f, "invalid config for module `{module}`: {reason}")
            }
            BuildError::ModuleFailed { module, source } => {
                write!(f, "module `{module}` failed: {source}")
            }
        }
    }
}

impl Error for BuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            BuildError::ModuleFailed { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_display() {
        let error = BuildError::MissingConfig {
            module: "my_module",
        };
        let msg = error.to_string();
        assert!(msg.contains("my_module"));
        assert!(msg.contains("missing config"));
        assert!(error.source().is_none());
    }

    #[test]
    fn missing_requirements_display() {
        let error = BuildError::MissingRequirements {
            module: "test_module",
        };
        let msg = error.to_string();
        assert!(msg.contains("test_module"));
        assert!(msg.contains("missing requirements"));
        assert!(error.source().is_none());
    }

    #[test]
    fn invalid_config_display() {
        let error = BuildError::InvalidConfig {
            module: "cfg_module",
            reason: "bad value",
        };
        let msg = error.to_string();
        assert!(msg.contains("cfg_module"));
        assert!(msg.contains("invalid config"));
        assert!(msg.contains("bad value"));
        assert!(error.source().is_none());
    }

    #[test]
    fn module_failed_display_and_source() {
        let inner = std::io::Error::new(std::io::ErrorKind::Other, "inner error");
        let error = BuildError::ModuleFailed {
            module: "fail_mod",
            source: Box::new(inner),
        };
        let msg = error.to_string();
        assert!(msg.contains("fail_mod"));
        assert!(msg.contains("failed"));
        assert!(msg.contains("inner error"));
        assert!(error.source().is_some());
    }
}
