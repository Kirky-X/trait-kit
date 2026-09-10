// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Error types for Kit operations.

use std::fmt;

use crate::i18n::tr;
#[cfg(feature = "shutdown")]
use crate::kit::shutdown::ShutdownPhase;

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
        /// 构建失败的上下文描述（支持 i18n 翻译后的文本）。
        context: String,
        /// 底层错误源。
        source: Box<dyn std::error::Error + Send + 'static>,
    },

    /// 请求的能力不存在。
    MissingCapability {
        /// 缺失的能力标识（支持 i18n 翻译后的文本）。
        key: String,
    },

    /// 能力存在但类型不匹配（T210）。
    ///
    /// 模块的 capability 已构建，但与请求的 `M::Capability` 类型不符
    /// （例如 override 注入了另一种能力类型）。
    CapabilityTypeMismatch {
        /// 能力所属模块的名称。
        key: String,
    },

    /// 请求的配置不存在。
    MissingConfig {
        /// 缺失的配置键（支持 i18n 翻译后的文本）。
        key: String,
    },

    /// 生命周期钩子执行失败。
    #[cfg(feature = "lifecycle")]
    LifecycleFailed {
        /// 钩子所属模块的上下文描述（支持 i18n 翻译后的文本）。
        context: String,
        /// 底层错误源。
        source: Box<dyn std::error::Error + Send + 'static>,
    },

    /// 优雅关闭超时。
    #[cfg(feature = "shutdown")]
    ShutdownTimedOut {
        /// 超时的关闭阶段列表。
        phases: Vec<crate::kit::shutdown::ShutdownPhase>,
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
                        &[("context", context.as_str()), ("source", &source_str)],
                    )
                )
            }
            Self::MissingCapability { key } => {
                write!(
                    f,
                    "{}",
                    tr(
                        "trait-kit-error-missing-capability",
                        &[("key", key.as_str())]
                    ),
                )
            }
            Self::CapabilityTypeMismatch { key } => {
                write!(
                    f,
                    "{}",
                    tr(
                        "trait-kit-error-capability-type-mismatch",
                        &[("key", key.as_str())],
                    ),
                )
            }
            Self::MissingConfig { key } => {
                write!(
                    f,
                    "{}",
                    tr("trait-kit-error-missing-config", &[("key", key.as_str())]),
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
                        &[("context", context.as_str()), ("source", &source_str)],
                    )
                )
            }
            #[cfg(feature = "shutdown")]
            Self::ShutdownTimedOut { phases } => {
                let phase_names: Vec<&str> = phases.iter().map(ShutdownPhase::as_str).collect();
                write!(
                    f,
                    "{}",
                    tr(
                        "trait-kit-error-shutdown-timed-out",
                        &[("phases", &phase_names.join(", "))],
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

/// Coarse failure classification for precise downstream matching (T210).
///
/// `TraitKitError::kind()` maps every variant onto one of these kinds so
/// callers can match on *why* an operation failed without tying themselves to
/// individual variants:
///
/// - [`ErrorKind::Missing`] — the requested capability/config was never registered.
/// - [`ErrorKind::InitFailed`] — the module exists but its construction failed
///   (retryable: lazy builders are restored after a failure).
/// - [`ErrorKind::TypeMismatch`] — a value exists but its type differs from
///   the requested one.
/// - [`ErrorKind::Other`] — everything else (graph/cycle/lifecycle/shutdown errors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The capability/config is absent entirely.
    Missing,
    /// Construction failed (see `BuildFailed` / `LifecycleFailed` sources).
    InitFailed,
    /// A stored value's type does not match the requested type.
    TypeMismatch,
    /// Any other failure (graph validation, lifecycle, shutdown, ...).
    Other,
}

impl TraitKitError {
    /// Classify this error (T210).
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::MissingCapability { .. } => ErrorKind::Missing,
            Self::MissingConfig { .. } => ErrorKind::Missing,
            Self::CapabilityTypeMismatch { .. } => ErrorKind::TypeMismatch,
            Self::BuildFailed { .. } => ErrorKind::InitFailed,
            #[cfg(feature = "lifecycle")]
            Self::LifecycleFailed { .. } => ErrorKind::InitFailed,
            Self::CycleDetected { .. }
            | Self::DependencyMissing { .. }
            | Self::AlreadyRegistered { .. } => ErrorKind::Other,
            #[cfg(feature = "shutdown")]
            Self::ShutdownTimedOut { .. } => ErrorKind::Other,
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
            cycle: vec!["alpha", "beta", "gamma"],
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("alpha"),
            "should contain module alpha: got '{msg}'"
        );
        assert!(
            msg.contains("beta"),
            "should contain module beta: got '{msg}'"
        );
        assert!(
            msg.contains("gamma"),
            "should contain module gamma: got '{msg}'"
        );
        // Modules appear in cycle order, joined by the arrow separator.
        assert!(
            msg.contains("alpha → beta → gamma"),
            "should contain the full cycle chain: got '{msg}'"
        );
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
            context: "build".into(),
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
        let err = TraitKitError::MissingCapability { key: "cap".into() };
        let msg = format!("{err}");
        assert!(msg.contains("cap"), "should contain key: got '{msg}'");
    }

    #[test]
    fn missing_config_display_contains_key() {
        let err = TraitKitError::MissingConfig {
            key: "db.url".into(),
        };
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
            context: "on_ready".into(),
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
            context: "build".into(),
            source: Box::new(std::io::Error::other("oops")),
        };
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn error_source_returns_none_for_simple_variants() {
        let err = TraitKitError::MissingConfig { key: "x".into() };
        assert!(std::error::Error::source(&err).is_none());
    }

    #[cfg(feature = "lifecycle")]
    #[test]
    fn error_source_returns_inner_for_lifecycle_failed() {
        let err = TraitKitError::LifecycleFailed {
            context: "on_ready".into(),
            source: Box::new(std::io::Error::other("fail")),
        };
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn error_source_returns_none_for_other_simple_variants() {
        let cycle = TraitKitError::CycleDetected {
            cycle: vec!["alpha", "beta"],
        };
        let missing = TraitKitError::DependencyMissing {
            module: "mod-a",
            missing: "mod-b",
        };
        let registered = TraitKitError::AlreadyRegistered {
            module: "my-module",
        };
        let cap = TraitKitError::MissingCapability { key: "cap".into() };
        assert!(std::error::Error::source(&cycle).is_none());
        assert!(std::error::Error::source(&missing).is_none());
        assert!(std::error::Error::source(&registered).is_none());
        assert!(std::error::Error::source(&cap).is_none());
    }

    #[cfg(feature = "shutdown")]
    #[test]
    fn error_source_returns_none_for_shutdown_timed_out() {
        let err = TraitKitError::ShutdownTimedOut {
            phases: vec![crate::kit::shutdown::ShutdownPhase::DrainQueue],
        };
        assert!(std::error::Error::source(&err).is_none());
    }

    #[cfg(feature = "shutdown")]
    #[test]
    fn shutdown_timed_out_display_contains_phases_and_timeout() {
        let err = TraitKitError::ShutdownTimedOut {
            phases: vec![
                crate::kit::shutdown::ShutdownPhase::DrainQueue,
                crate::kit::shutdown::ShutdownPhase::CloseConnections,
            ],
        };
        let msg = format!("{err}");
        // Phase names are embedded verbatim (locale-independent).
        assert!(
            msg.contains("drain_queue"),
            "should contain phase name drain_queue: got '{msg}'"
        );
        assert!(
            msg.contains("close_connections"),
            "should contain phase name close_connections: got '{msg}'"
        );
        // Phases appear as a comma-separated list in declaration order.
        assert!(
            msg.contains("drain_queue, close_connections"),
            "should contain the joined phase list: got '{msg}'"
        );
        // Timeout wording is locale-dependent (en / zh message catalogs).
        assert!(
            msg.contains("timed out") || msg.contains("超时"),
            "should contain timeout info: got '{msg}'"
        );
    }

    #[test]
    fn error_debug_format() {
        let err = TraitKitError::MissingConfig { key: "x".into() };
        let debug = format!("{err:?}");
        assert!(debug.contains("MissingConfig"));
    }
}
