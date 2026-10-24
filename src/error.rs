// Copyright (c) 2026 Kirky.X🌠
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

    /// 装饰器目标模块未注册。
    DecoratorTargetMissing {
        /// 被装饰的模块名。
        module: &'static str,
    },

    /// 依赖能力版本不满足要求。
    VersionIncompatible {
        /// 发起依赖的模块。
        module: &'static str,
        /// 提供能力的依赖模块。
        dependency: &'static str,
        /// 要求的最低版本。
        required: &'static str,
        /// 实际提供的版本。
        provided: &'static str,
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

    /// 能力存在但类型不匹配。
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
    #[expect(
        clippy::too_many_lines,
        reason = "全变体手写 Display（i18n 消息模板），拆分子函数只会割裂模板与变体的对应关系"
    )]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CycleDetected { cycle } => {
                let cycle = if cycle.is_empty() {
                    tr("trait-kit-diag-unknown-cycle", &[])
                } else {
                    cycle.join(" → ")
                };
                write!(
                    f,
                    "{}",
                    tr("trait-kit-error-cycle-detected", &[("cycle", &cycle)])
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
            Self::DecoratorTargetMissing { module } => write!(
                f,
                "{}",
                tr(
                    "trait-kit-error-decorator-target-missing",
                    &[("module", *module)],
                ),
            ),
            Self::VersionIncompatible {
                module,
                dependency,
                required,
                provided,
            } => write!(
                f,
                "{}",
                tr(
                    "trait-kit-error-version-incompatible",
                    &[
                        ("module", *module),
                        ("dependency", *dependency),
                        ("required", *required),
                        ("provided", *provided),
                    ],
                ),
            ),
            Self::BuildFailed { context, source } => {
                let source_str = sanitize_source_text(&source.to_string());
                write!(
                    f,
                    "{}",
                    tr(
                        "trait-kit-error-build-failed",
                        &[
                            ("context", public_context(context)),
                            ("source", &source_str),
                        ],
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
                let source_str = sanitize_source_text(&source.to_string());
                write!(
                    f,
                    "{}",
                    tr(
                        "trait-kit-error-lifecycle-failed",
                        &[
                            ("context", public_context(context)),
                            ("source", &source_str),
                        ],
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

/// 调试输出里的底层错误占位：只显示脱敏后的文本。
struct RedactedSource(String);

impl fmt::Debug for RedactedSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// 错误类型的调试输出。
///
/// 形状与派生实现一致（变体名 + 字段），但底层错误原文与内部调用点
/// 标签都经过脱敏：下游用 `{:?}` 或 tracing 的 `?err` 记录时不会带出
/// 服务器绝对路径、口令与内部函数名。原始上下文仍可通过变体的公开
/// 字段读取。
impl fmt::Debug for TraitKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CycleDetected { cycle } => f
                .debug_struct("CycleDetected")
                .field("cycle", cycle)
                .finish(),
            Self::DependencyMissing { module, missing } => f
                .debug_struct("DependencyMissing")
                .field("module", module)
                .field("missing", missing)
                .finish(),
            Self::AlreadyRegistered { module } => f
                .debug_struct("AlreadyRegistered")
                .field("module", module)
                .finish(),
            Self::DecoratorTargetMissing { module } => f
                .debug_struct("DecoratorTargetMissing")
                .field("module", module)
                .finish(),
            Self::VersionIncompatible {
                module,
                dependency,
                required,
                provided,
            } => f
                .debug_struct("VersionIncompatible")
                .field("module", module)
                .field("dependency", dependency)
                .field("required", required)
                .field("provided", provided)
                .finish(),
            Self::BuildFailed { context, source } => f
                .debug_struct("BuildFailed")
                .field("context", &public_context(context))
                .field(
                    "source",
                    &RedactedSource(sanitize_source_text(&source.to_string())),
                )
                .finish(),
            Self::MissingCapability { key } => f
                .debug_struct("MissingCapability")
                .field("key", key)
                .finish(),
            Self::CapabilityTypeMismatch { key } => f
                .debug_struct("CapabilityTypeMismatch")
                .field("key", key)
                .finish(),
            Self::MissingConfig { key } => {
                f.debug_struct("MissingConfig").field("key", key).finish()
            }
            #[cfg(feature = "lifecycle")]
            Self::LifecycleFailed { context, source } => f
                .debug_struct("LifecycleFailed")
                .field("context", &public_context(context))
                .field(
                    "source",
                    &RedactedSource(sanitize_source_text(&source.to_string())),
                )
                .finish(),
            #[cfg(feature = "shutdown")]
            Self::ShutdownTimedOut { phases } => f
                .debug_struct("ShutdownTimedOut")
                .field("phases", phases)
                .finish(),
        }
    }
}

/// 底层错误原文的最大字符数：公开 `Display` 里的原文不无限长。
const MAX_SOURCE_TEXT_LEN: usize = 512;

/// 脱敏底层错误原文后再进入公开 `Display`。
///
/// 公开错误文本可能回显给客户端，服务器绝对路径与凭据片段不应随之
/// 泄漏；未脱敏的原文仍可通过 [`std::error::Error::source`] 取回。
fn sanitize_source_text(text: &str) -> String {
    crate::redact::truncate(&crate::redact::redact_text(text, true), MAX_SOURCE_TEXT_LEN)
}

/// 内部调用点标签登记：变体 → 写入公开 `context` 字段的原始标签。
///
/// 变体集合、原始标签与反查表由同一次展开生成，新增标签不会与
/// 登记表脱节；对外文案映射见 [`ContextLabel::public`]（穷尽匹配，
/// 漏登记即编译失败）。
macro_rules! context_labels {
    ($($variant:ident => $raw:literal),* $(,)?) => {
        /// 内部调用点标签：与模块名等用户上下文区分的封闭集合。
        ///
        /// 内部调用点的标签只能取自本枚举：写进公开 `context` 字段的
        /// 是原始标签，进入对外错误文本的是 [`ContextLabel::public`]
        /// 给出的类别文案。模块名（`ModuleMeta::NAME`）与本地化诊断
        /// 短语等非标签形态的上下文不在本枚举内，原样保留。
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub(crate) enum ContextLabel {
            $($variant,)*
        }

        impl ContextLabel {
            /// 写入公开 `context` 字段的原始标签。
            const fn raw(self) -> &'static str {
                match self {
                    $(Self::$variant => $raw,)*
                }
            }

            /// 按原始标签反查登记项。
            fn from_raw(raw: &str) -> Option<Self> {
                match raw {
                    $($raw => Some(Self::$variant),)*
                    _ => None,
                }
            }

            /// 全部登记项：测试据此核对登记表与折叠结果一致。
            #[cfg(test)]
            const ALL: &'static [Self] = &[$(Self::$variant,)*];
        }
    };
}

context_labels! {
    LoadConfig => "load_config",
    LoadConfigWith => "load_config_with",
    LoadConfigWithSerialize => "load_config_with (serialize)",
    LoadConfigWithDeserialize => "load_config_with (deserialize)",
    LoadAndValidate => "load_and_validate",
    ReloadConfig => "reload_config",
    SetEncrypted => "set_encrypted",
    GetEncrypted => "get_encrypted",
    GetEncryptedWithVersion => "get_encrypted_with_version",
    RotateMasterKey => "rotate_master_key",
    RotateMasterKeyOldKey => "rotate_master_key (old key)",
    RotateMasterKeyNewKey => "rotate_master_key (new key)",
    OnReady => "on_ready",
    Shutdown => "shutdown",
    ShutdownBridge => "register_shutdown_into",
}

impl ContextLabel {
    /// 进入对外错误文本的类别文案。
    ///
    /// 穷尽匹配：新增标签变体而未给出对外文案时编译失败，内部调用点
    /// 不会以原始标签形态漏进错误文本。本地化诊断短语里的操作名也
    /// 取自这里（短语整体原样保留，只有操作名换成类别文案）。
    pub(crate) const fn public(self) -> &'static str {
        match self {
            Self::LoadConfig | Self::LoadConfigWith => "configuration load",
            Self::LoadConfigWithSerialize => "configuration load (serialize)",
            Self::LoadConfigWithDeserialize => "configuration load (deserialize)",
            Self::LoadAndValidate => "configuration load (validated)",
            Self::ReloadConfig => "configuration reload",
            Self::SetEncrypted => "encrypted configuration write",
            Self::GetEncrypted | Self::GetEncryptedWithVersion => "encrypted configuration read",
            Self::RotateMasterKey => "master key rotation",
            Self::RotateMasterKeyOldKey => "master key rotation (old key)",
            Self::RotateMasterKeyNewKey => "master key rotation (new key)",
            Self::OnReady => "readiness hook",
            Self::Shutdown => "shutdown",
            Self::ShutdownBridge => "hook registration",
        }
    }
}

impl From<ContextLabel> for String {
    fn from(label: ContextLabel) -> Self {
        label.raw().to_string()
    }
}

/// 主密钥供给方上下文的族前缀：provider 类型名随运行时变化。
///
/// 类型名留在 `context` 字段里供程序化定位，进入对外文本前按族前缀
/// 折叠，类型名不外泄。
pub(crate) const KEY_PROVIDER_CONTEXT_PREFIX: &str = "key provider (";

/// 主密钥供给方的上下文标签：类型名进字段、不进对外文本。
#[cfg(feature = "encryption")]
pub(crate) fn key_provider_context(provider_type: &str) -> String {
    format!("{KEY_PROVIDER_CONTEXT_PREFIX}{provider_type})")
}

/// 对外文案里的上下文标签：内部调用点名不进入错误文本。
///
/// `BuildFailed` / `LifecycleFailed` 的 `context` 记录的是内部调用点
/// （`load_config`、`rotate_master_key (old key)` 之类），这些名字随
/// 错误文本流向调用方会暴露内部实现结构。内部调用点标签一律登记在
/// [`ContextLabel`]，此处折叠成稳定的类别标签；模块名
/// （`ModuleMeta::NAME`）与本地化诊断短语等非标签形态的上下文原样
/// 保留。原始标签仍可通过变体的公开字段读取（程序化匹配不受影响）。
fn public_context(context: &str) -> &str {
    if let Some(label) = ContextLabel::from_raw(context) {
        return label.public();
    }
    // 主密钥供给方：provider 类型名随运行时变化，按族前缀折叠。
    if context.starts_with(KEY_PROVIDER_CONTEXT_PREFIX) {
        return "master key provider";
    }
    context
}

/// Coarse failure classification for precise downstream matching.
///
/// `TraitKitError::kind` maps every variant onto one of these kinds so
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
    /// Classify this error.
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
            | Self::AlreadyRegistered { .. }
            | Self::DecoratorTargetMissing { .. }
            | Self::VersionIncompatible { .. } => ErrorKind::Other,
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
    fn error_kind_classifies_every_variant() {
        fn boxed(
            e: impl std::error::Error + Send + 'static,
        ) -> Box<dyn std::error::Error + Send + 'static> {
            Box::new(e)
        }
        assert!(matches!(
            TraitKitError::MissingCapability { key: "k".into() }.kind(),
            ErrorKind::Missing
        ));
        assert!(matches!(
            TraitKitError::MissingConfig { key: "k".into() }.kind(),
            ErrorKind::Missing
        ));
        assert!(matches!(
            TraitKitError::CapabilityTypeMismatch { key: "k".into() }.kind(),
            ErrorKind::TypeMismatch
        ));
        assert!(matches!(
            TraitKitError::BuildFailed {
                context: "ctx".into(),
                source: boxed(std::io::Error::other("boom")),
            }
            .kind(),
            ErrorKind::InitFailed
        ));
        #[cfg(feature = "lifecycle")]
        assert!(matches!(
            TraitKitError::LifecycleFailed {
                context: "ctx".into(),
                source: boxed(std::io::Error::other("boom")),
            }
            .kind(),
            ErrorKind::InitFailed
        ));
        assert!(matches!(
            TraitKitError::CycleDetected {
                cycle: vec!["a", "b"],
            }
            .kind(),
            ErrorKind::Other
        ));
        assert!(matches!(
            TraitKitError::DependencyMissing {
                module: "m",
                missing: "d"
            }
            .kind(),
            ErrorKind::Other
        ));
        assert!(matches!(
            TraitKitError::AlreadyRegistered { module: "m" }.kind(),
            ErrorKind::Other
        ));
        assert!(matches!(
            TraitKitError::DecoratorTargetMissing { module: "m" }.kind(),
            ErrorKind::Other
        ));
        assert!(matches!(
            TraitKitError::VersionIncompatible {
                module: "m",
                dependency: "d",
                required: "1.0.0",
                provided: "0.9.0",
            }
            .kind(),
            ErrorKind::Other
        ));
        #[cfg(feature = "shutdown")]
        assert!(matches!(
            TraitKitError::ShutdownTimedOut { phases: Vec::new() }.kind(),
            ErrorKind::Other
        ));
    }

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

    /// 公开错误文本不得带出服务器绝对路径与凭据：原文仍可经
    /// `Error::source()` 取回。
    #[test]
    fn build_failed_display_redacts_paths_and_credentials() {
        let err = TraitKitError::BuildFailed {
            context: "load_config".into(),
            source: Box::new(std::io::Error::other(
                "Configuration file not found: /etc/app/config/prod.toml (password=hunter2)",
            )),
        };
        let msg = format!("{err}");
        assert!(
            !msg.contains("/etc/app/config"),
            "absolute path must not leak: {msg}"
        );
        assert!(!msg.contains("hunter2"), "credentials must not leak: {msg}");
        assert!(
            msg.contains("prod.toml"),
            "file name stays available for diagnosis: {msg}"
        );
    }

    #[test]
    fn build_failed_display_redacts_windows_paths() {
        let err = TraitKitError::BuildFailed {
            context: "load_config".into(),
            source: Box::new(std::io::Error::other(
                r"cannot open C:\app\config\prod.toml",
            )),
        };
        let msg = format!("{err}");
        assert!(!msg.contains(r"app\config"), "path must not leak: {msg}");
        assert!(msg.contains("prod.toml"), "file name stays: {msg}");
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
    fn lifecycle_failed_display_contains_public_context_and_source() {
        let err = TraitKitError::LifecycleFailed {
            context: "on_ready".into(),
            source: Box::new(std::io::Error::other("fail")),
        };
        let msg = format!("{err}");
        // 内部调用点名不进入错误文本，只有对外类别标签。
        assert!(
            !msg.contains("on_ready"),
            "internal call-site label must not leak: {msg}"
        );
        assert!(
            msg.contains("readiness hook"),
            "should contain public context: got '{msg}'"
        );
        assert!(msg.contains("fail"), "should contain source: got '{msg}'");
    }

    /// 内部调用点标签不进入对外错误文本：模块名等非标签形态原样保留。
    #[test]
    fn build_failed_display_hides_internal_call_site_labels() {
        let err = TraitKitError::BuildFailed {
            context: "load_config_with".into(),
            source: Box::new(std::io::Error::other("boom")),
        };
        let msg = format!("{err}");
        assert!(
            !msg.contains("load_config_with"),
            "internal call-site label must not leak: {msg}"
        );
        assert!(
            msg.contains("configuration"),
            "should contain public context: got '{msg}'"
        );

        let module_err = TraitKitError::BuildFailed {
            context: "counter-module".into(),
            source: Box::new(std::io::Error::other("boom")),
        };
        let module_msg = format!("{module_err}");
        assert!(
            module_msg.contains("counter-module"),
            "module names stay visible: {module_msg}"
        );
    }

    /// 模块名不按字符串形态被当成内部调用点折叠：折叠只针对登记在册
    /// 的内部调用点标签。
    #[test]
    fn module_names_survive_the_context_folding() {
        let err = TraitKitError::BuildFailed {
            context: "my_module".into(),
            source: Box::new(std::io::Error::other("boom")),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("my_module"),
            "a module name must stay visible: {msg}"
        );
        assert!(
            !msg.contains("operation"),
            "a module name must not be folded into a generic category: {msg}"
        );
    }

    /// 主密钥供给方的 provider 类型名不进入对外错误文本。
    #[test]
    fn key_provider_type_name_stays_out_of_public_text() {
        let err = TraitKitError::BuildFailed {
            context: "key provider (env)".into(),
            source: Box::new(std::io::Error::other("boom")),
        };
        let msg = format!("{err}");
        assert!(
            !msg.contains("env"),
            "the provider type name must not leak: {msg}"
        );
        assert!(
            msg.contains("master key provider"),
            "the public category stays available: {msg}"
        );
    }

    /// 关闭桥接的本地化上下文不携带内部函数名：桥接阶段仍留在文案里。
    #[test]
    fn shutdown_bridge_context_hides_internal_operation_name() {
        let context = tr(
            "trait-kit-error-shutdown-bridge-stranded",
            &[
                ("operation", ContextLabel::ShutdownBridge.public()),
                ("phase", "drain_queue"),
                ("stranded", "2"),
            ],
        );
        let err = TraitKitError::BuildFailed {
            context,
            source: Box::new(std::io::Error::other("boom")),
        };
        let msg = format!("{err}");
        assert!(
            !msg.contains("register_shutdown_into"),
            "the internal operation name must not leak: {msg}"
        );
        assert!(
            msg.contains("hook registration"),
            "the public operation wording stays available: {msg}"
        );
        assert!(
            msg.contains("drain_queue"),
            "the bridge phase stays available for diagnosis: {msg}"
        );
    }

    /// 登记在册的内部调用点标签一律折叠为对外类别文案：原始标签
    /// 不残留在对外文本里。
    #[test]
    fn every_registered_label_folds_to_its_public_category() {
        for label in ContextLabel::ALL {
            let public = public_context(label.raw());
            assert_eq!(
                public,
                label.public(),
                "label {label:?} must fold to its registered public wording"
            );
            if public != label.raw() {
                assert!(
                    !public.contains(label.raw()),
                    "the raw label must not survive in the public text: {public}"
                );
            }
        }
    }

    /// `{:?}` 不得带出底层错误原文：下游日志与 tracing 的 `?err` 会
    /// 原样记录调试输出。
    #[test]
    fn debug_output_redacts_source_text() {
        let err = TraitKitError::BuildFailed {
            context: "load_config".into(),
            source: Box::new(std::io::Error::other(
                "Configuration file not found: /etc/app/config/prod.toml (password=hunter2)",
            )),
        };
        let debug = format!("{err:?}");
        assert!(debug.contains("BuildFailed"), "variant name stays: {debug}");
        assert!(
            !debug.contains("/etc/app/config"),
            "absolute path must not leak: {debug}"
        );
        assert!(
            !debug.contains("hunter2"),
            "credentials must not leak: {debug}"
        );
        assert!(
            !debug.contains("load_config"),
            "internal call-site label must not leak: {debug}"
        );
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

// Display branches for rarely-hit variants

#[cfg(test)]
mod display_branch_tests {
    use super::TraitKitError;

    #[test]
    fn decorator_target_missing_display_renders_module() {
        let err = TraitKitError::DecoratorTargetMissing { module: "m" };
        let msg = format!("{err}");
        assert_eq!(
            msg,
            "decorator target module `m` is not registered (checked at registration time)"
        );
    }

    #[test]
    fn capability_type_mismatch_display_renders_key() {
        let err = TraitKitError::CapabilityTypeMismatch { key: "k".into() };
        let msg = format!("{err}");
        assert_eq!(msg, "capability type mismatch for `k`");
    }
}
