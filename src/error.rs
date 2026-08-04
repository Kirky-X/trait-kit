// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Error types for Kit operations.

use std::fmt;

use crate::i18n::tr;

/// Unified trait-kit error type.
///
/// Follows the `ProjectNameError` naming convention used across the base workspace.
///
/// `Display` 实现通过 [`tr`] 查询 Fluent 消息目录，自动根据当前 locale 输出对应语言文本。
#[derive(Debug)]
pub enum TraitKitError {
    /// 依赖图中检测到环。
    CycleDetected {
        /// 环路上的模块名称序列。
        cycle: Vec<&'static str>,
    },

    /// 依赖的模块未注册。
    DependencyMissing {
        /// 发起依赖的模块。
        module: &'static str,
        /// 缺失的依赖模块。
        missing: &'static str,
    },

    /// 模块重复注册。
    AlreadyRegistered {
        /// 重复注册的模块名。
        module: &'static str,
    },

    /// 模块构建失败。
    BuildFailed {
        /// 构建失败的上下文描述。
        context: &'static str,
        /// 底层错误源。
        source: Box<dyn std::error::Error + Send + 'static>,
    },

    /// 请求的能力不存在。
    MissingCapability {
        /// 缺失的能力标识。
        key: &'static str,
    },

    /// 请求的配置不存在。
    MissingConfig {
        /// 缺失的配置键。
        key: &'static str,
    },

    /// 生命周期钩子执行失败。
    #[cfg(feature = "lifecycle")]
    LifecycleFailed {
        /// 钩子所属模块的上下文描述。
        context: &'static str,
        /// 底层错误源。
        source: Box<dyn std::error::Error + Send + 'static>,
    },
}

impl fmt::Display for TraitKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CycleDetected { cycle } => {
                write!(
                    f,
                    "{}",
                    tr(
                        "trait-kit-error-cycle-detected",
                        &[("cycle", &cycle.join(" → "))],
                    )
                )
            }
            Self::DependencyMissing { module, missing } => {
                write!(
                    f,
                    "{}",
                    tr(
                        "trait-kit-error-dependency-missing",
                        &[("module", *module), ("missing", *missing)],
                    )
                )
            }
            Self::AlreadyRegistered { module } => {
                write!(
                    f,
                    "{}",
                    tr("trait-kit-error-already-registered", &[("module", *module)]),
                )
            }
            Self::BuildFailed { context, source } => {
                let source_str = source.to_string();
                write!(
                    f,
                    "{}",
                    tr(
                        "trait-kit-error-build-failed",
                        &[("context", *context), ("source", &source_str)],
                    )
                )
            }
            Self::MissingCapability { key } => {
                write!(
                    f,
                    "{}",
                    tr("trait-kit-error-missing-capability", &[("key", *key)]),
                )
            }
            Self::MissingConfig { key } => {
                write!(
                    f,
                    "{}",
                    tr("trait-kit-error-missing-config", &[("key", *key)]),
                )
            }
            #[cfg(feature = "lifecycle")]
            Self::LifecycleFailed { context, source } => {
                let source_str = source.to_string();
                write!(
                    f,
                    "{}",
                    tr(
                        "trait-kit-error-lifecycle-failed",
                        &[("context", *context), ("source", &source_str)],
                    )
                )
            }
        }
    }
}

impl std::error::Error for TraitKitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BuildFailed { source, .. } => Some(source.as_ref()),
            #[cfg(feature = "lifecycle")]
            Self::LifecycleFailed { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// Convenience `Result` alias for trait-kit operations.
///
/// Provided for ergonomic use in downstream crates.
pub type TraitKitResult<T> = std::result::Result<T, TraitKitError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_detected_display_contains_modules() {
        let err = TraitKitError::CycleDetected {
            cycle: vec!["A", "B", "C"],
        };
        let msg = format!("{err}");
        assert!(msg.contains('A'), "should contain module A: got '{msg}'");
        assert!(msg.contains('B'), "should contain module B: got '{msg}'");
        assert!(msg.contains('C'), "should contain module C: got '{msg}'");
    }

    #[test]
    fn dependency_missing_display_contains_both_modules() {
        let err = TraitKitError::DependencyMissing {
            module: "mod-a",
            missing: "mod-b",
        };
        let msg = format!("{err}");
        assert!(msg.contains("mod-a"), "should contain module: got '{msg}'");
        assert!(
            msg.contains("mod-b"),
            "should contain missing dep: got '{msg}'"
        );
    }

    #[test]
    fn already_registered_display_contains_module() {
        let err = TraitKitError::AlreadyRegistered {
            module: "my-module",
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("my-module"),
            "should contain module name: got '{msg}'"
        );
    }

    #[test]
    fn build_failed_display_contains_context_and_source() {
        let err = TraitKitError::BuildFailed {
            context: "build",
            source: Box::new(std::io::Error::other("oops")),
        };
        let msg = format!("{err}");
        assert!(msg.contains("build"), "should contain context: got '{msg}'");
        assert!(
            msg.contains("oops"),
            "should contain source error: got '{msg}'"
        );
    }

    #[test]
    fn missing_capability_display_contains_key() {
        let err = TraitKitError::MissingCapability { key: "cap" };
        let msg = format!("{err}");
        assert!(msg.contains("cap"), "should contain key: got '{msg}'");
    }

    #[test]
    fn missing_config_display_contains_key() {
        let err = TraitKitError::MissingConfig { key: "db.url" };
        let msg = format!("{err}");
        assert!(
            msg.contains("db.url"),
            "should contain config key: got '{msg}'"
        );
    }

    #[cfg(feature = "lifecycle")]
    #[test]
    fn lifecycle_failed_display_contains_context_and_source() {
        let err = TraitKitError::LifecycleFailed {
            context: "on_ready",
            source: Box::new(std::io::Error::other("fail")),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("on_ready"),
            "should contain context: got '{msg}'"
        );
        assert!(msg.contains("fail"), "should contain source: got '{msg}'");
    }

    #[test]
    fn error_source_returns_inner_for_build_failed() {
        let err = TraitKitError::BuildFailed {
            context: "build",
            source: Box::new(std::io::Error::other("oops")),
        };
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn error_source_returns_none_for_simple_variants() {
        let err = TraitKitError::MissingConfig { key: "x" };
        assert!(std::error::Error::source(&err).is_none());
    }

    #[cfg(feature = "lifecycle")]
    #[test]
    fn error_source_returns_inner_for_lifecycle_failed() {
        let err = TraitKitError::LifecycleFailed {
            context: "on_ready",
            source: Box::new(std::io::Error::other("fail")),
        };
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn error_debug_format() {
        let err = TraitKitError::MissingConfig { key: "x" };
        let debug = format!("{err:?}");
        assert!(debug.contains("MissingConfig"));
    }
}
