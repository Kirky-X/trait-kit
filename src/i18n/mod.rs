// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 国际化（i18n）支持 — Fluent FTL 消息翻译 + ICU4X 本地化格式化。
//!
//! 提供两大能力：
//!
//! 1. **消息翻译**（[`I18nManager`] + [`tr`]）：基于 Fluent FTL 消息文件，
//!    支持中英文切换，系统语言环境自动检测。
//! 2. **本地化格式化**（[`I18nFormatter`]）：ICU4X 驱动的数字/日期/复数/排序格式化。
//!
//! # 启动初始化
//!
//! 在应用启动时调用 [`I18nManager::init`] 自动检测系统语言环境：
//!
//! ```rust
//! use trait_kit::i18n::I18nManager;
//!
//! let mgr = I18nManager::init();
//! ```
//!
//! 或使用指定 locale：
//!
//! ```rust
//! use trait_kit::i18n::I18nManager;
//!
//! let mgr = I18nManager::init_with_locale("zh-CN");
//! ```

#[cfg(feature = "i18n")]
mod i18n_impl;
mod messages;

use std::fmt;
use std::sync::OnceLock;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

#[cfg(feature = "i18n")]
use icu::collator::CollatorBorrowed;
#[cfg(feature = "i18n")]
use icu::datetime::DateTimeFormatter;
#[cfg(feature = "i18n")]
use icu::datetime::fieldsets::YMD;
#[cfg(feature = "i18n")]
use icu::decimal::DecimalFormatter;
#[cfg(feature = "i18n")]
use icu::locale::Locale;
#[cfg(feature = "i18n")]
use icu::plurals::PluralRules;

// ─── I18nError ──────────────────────────────────────────────────────────────

/// 国际化操作返回的错误类型。
#[derive(Debug, Clone)]
pub enum I18nError {
    /// BCP-47 locale 字符串解析失败。
    InvalidLocale {
        /// 原始输入。
        input: String,
        /// 失败原因。
        reason: String,
    },
    /// 数值无法格式化（如 NaN、Infinity 或解析失败）。
    InvalidNumber {
        /// 原始输入。
        input: String,
        /// 失败原因。
        reason: String,
    },
    /// 日期分量越界或无效。
    DateError(String),
    /// ICU4X 数据或格式化失败。
    FormatError(String),
}

impl fmt::Display for I18nError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // tr() 通过 OnceLock 惰性初始化，build() 不调用 Display，
        // 因此不存在递归风险。
        match self {
            Self::InvalidLocale { input, reason } => {
                write!(
                    f,
                    "{}",
                    tr(
                        "i18n-error-invalid-locale",
                        &[("input", input), ("reason", reason)]
                    ),
                )
            }
            Self::InvalidNumber { input, reason } => {
                write!(
                    f,
                    "{}",
                    tr(
                        "i18n-error-invalid-number",
                        &[("input", input), ("reason", reason)]
                    ),
                )
            }
            Self::DateError(detail) => {
                write!(f, "{}", tr("i18n-error-date", &[("detail", detail)]))
            }
            Self::FormatError(detail) => {
                write!(f, "{}", tr("i18n-error-format", &[("detail", detail)]))
            }
        }
    }
}

impl std::error::Error for I18nError {}

// ─── I18nFormatter（ICU4X 格式化） ──────────────────────────────────────────

/// 基于 ICU4X 编译数据的 locale 感知格式化器。
///
/// 通过 BCP-47 locale 标签（如 `"en-US"`、`"zh-CN"`）构造。
/// 所有格式化器在构造时 eagerly 创建，后续格式化调用低分配。
///
/// # Thread safety
///
/// 本类型**不是 `Send` 也不是 `Sync`**：ICU4X 的 `DecimalFormatter`、
/// `PluralRules`、`DateTimeFormatter` 内部持有 `Yoke`/`Rc`（自引用结构）。
/// 每线程独立构造一个实例；不要放入 `static`、`Arc` 或跨线程/async 任务传递。
#[cfg(feature = "i18n")]
#[derive(Debug)]
pub struct I18nFormatter {
    /// 已解析的 locale。
    /// 测试用于断言解析结果；生产路径经各格式化器隐式持有。
    #[allow(dead_code, reason = "introspection + test assertions")]
    pub(crate) locale: Locale,
    /// 小数（数字）格式化器。
    pub(crate) decimal_formatter: DecimalFormatter,
    /// 该 locale 的复数规则。
    pub(crate) plural_rules: PluralRules,
    /// 字符串排序比较器。
    pub(crate) collator: CollatorBorrowed<'static>,
    /// 日期（YMD·medium）格式化器。
    pub(crate) date_formatter: DateTimeFormatter<YMD>,
}

// ─── Fluent 消息目录（fluent-bundle concurrent 双束） ────────────────────────

/// 全局英文（回退）束：进程级共享，首次访问时从内嵌 `EN_FTL` 构建。
static EN_BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();

/// 全局中文束：进程级共享，首次访问时从内嵌 `ZH_FTL` 构建。
static ZH_BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();

/// 从 FTL 源构建并发（`Send + Sync`）Fluent 束。
///
/// 解析失败不 panic：`try_new` 返回的部分资源照常入束（损坏消息由
/// Fluent 在查询时按语义降级），与统一 i18n 参考基线一致。
fn build_bundle(lang: &str, ftl: &str) -> FluentBundle<FluentResource> {
    let resource = FluentResource::try_new(ftl.to_string()).unwrap_or_else(|e| e.0);
    let langid: LanguageIdentifier = lang
        .parse()
        .unwrap_or_else(|_| "en".parse().expect("'en' is a valid language identifier"));
    let mut bundle = FluentBundle::new_concurrent(vec![langid]);
    // 关闭 Unicode 隔离符，避免插值文本两侧被 \u{2068}/\u{2069} 包裹。
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource)
        .expect("single FTL resource should add without conflict");
    bundle
}

/// 从任意（非全局 static）束中格式化一条消息；key 缺失返回 `None`。
fn format_from_custom_bundle(
    bundle: &FluentBundle<FluentResource>,
    message_id: &str,
    args: &[(&str, &str)],
) -> Option<String> {
    let msg = bundle.get_message(message_id)?;
    let pattern = msg.value()?;
    let mut fluent_args = FluentArgs::new();
    for (name, value) in args {
        fluent_args.set(*name, FluentValue::from(*value));
    }
    let mut errors = vec![];
    Some(
        bundle
            .format_pattern(pattern, Some(&fluent_args), &mut errors)
            .to_string(),
    )
}

/// 从指定语言的全局束中格式化一条消息；key 缺失返回 `None`。
///
/// 未知语言（非 `zh`）一律归 en 束——回退终结于 en。
fn format_from_bundle(lang: &str, message_id: &str, args: &[(&str, &str)]) -> Option<String> {
    let bundle = match lang {
        "zh" => ZH_BUNDLE.get_or_init(|| build_bundle("zh", messages::ZH_FTL)),
        _ => EN_BUNDLE.get_or_init(|| build_bundle("en", messages::EN_FTL)),
    };
    format_from_custom_bundle(bundle, message_id, args)
}

/// Fluent 消息目录（完整 Fluent 语法，由 `fluent-bundle` 支撑）。
///
/// 两种形态：
///
/// - [`MessageCatalog::global`]：进程级 EN/ZH 双束（[`OnceLock`] 缓存），
///   查询链为当前语言束 → en 束 → key 本身；
/// - [`MessageCatalog::parse`]：Kit 模块 overlay 目录（运行时 FTL 片段），
///   自包含，缺 key 返回 key 本身（由 `Kit::module_tr` 再回退全局目录）。
pub(crate) enum MessageCatalog {
    /// 全局双束目录（`I18nManager` 持有）。
    Global {
        /// 语言束选择：`"zh"` 或 `"en"`（一切未知语言归 en）。
        lang: &'static str,
    },
    /// Kit 模块 overlay 目录（持有独立构建的束；仅 `i18n` feature 的
    /// `Kit::module_tr` 使用）。
    #[cfg(feature = "i18n")]
    Overlay {
        /// 由模块 FTL 片段合并构建的束。
        bundle: FluentBundle<FluentResource>,
    },
}

impl MessageCatalog {
    /// 全局目录：小写化 locale 标签以 `zh` 开头选 zh 束，其余（含未知
    /// 语言）一律归 en 束。
    pub(crate) fn global(locale_tag: &str) -> Self {
        let lang = if locale_tag.starts_with("zh") {
            "zh"
        } else {
            "en"
        };
        Self::Global { lang }
    }

    /// 从 FTL 片段解析 Kit overlay 目录（完整 Fluent 语法）。
    ///
    /// 每个片段独立入束且后入者覆盖先入者（`add_resource_overriding`，
    /// 与旧实现跨片段"最后写入获胜"语义一致）。解析失败的行按 Fluent
    /// 语义静默降级，不 panic。
    #[cfg(feature = "i18n")]
    pub(crate) fn parse(lang: &'static str, ftl_fragments: &[&str]) -> Self {
        let langid: LanguageIdentifier = lang
            .parse()
            .unwrap_or_else(|_| "en".parse().expect("'en' is a valid language identifier"));
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        bundle.set_use_isolating(false);
        for fragment in ftl_fragments {
            let resource = FluentResource::try_new((*fragment).to_string()).unwrap_or_else(|e| e.0);
            bundle.add_resource_overriding(resource);
        }
        Self::Overlay { bundle }
    }

    /// 翻译消息 key，带参数替换。
    ///
    /// `{ $var }` 占位符由 Fluent 引擎解析。全局目录查询链为当前语言 →
    /// en → key 本身；overlay 目录缺 key 返回 key 本身。任何路径不 panic。
    pub(crate) fn translate(&self, message_id: &str, args: &[(&str, &str)]) -> String {
        match self {
            Self::Global { lang } => format_from_bundle(lang, message_id, args)
                .or_else(|| {
                    if *lang == "en" {
                        None
                    } else {
                        format_from_bundle("en", message_id, args)
                    }
                })
                .unwrap_or_else(|| message_id.to_string()),
            #[cfg(feature = "i18n")]
            Self::Overlay { bundle } => {
                format_from_custom_bundle(bundle, message_id, args)
                    .unwrap_or_else(|| message_id.to_string())
            }
        }
    }
}

impl fmt::Debug for MessageCatalog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // FluentBundle 未实现 Debug：只输出形态与语言摘要。
        match self {
            Self::Global { lang } => f
                .debug_struct("MessageCatalog")
                .field("kind", &"global")
                .field("lang", lang)
                .finish(),
            #[cfg(feature = "i18n")]
            Self::Overlay { .. } => f
                .debug_struct("MessageCatalog")
                .field("kind", &"overlay")
                .finish(),
        }
    }
}

// ─── I18nManager（全局状态 + 消息翻译） ─────────────────────────────────────

/// 全局 [`I18nManager`] 实例。
static GLOBAL_I18N: OnceLock<I18nManager> = OnceLock::new();

/// Fluent 消息翻译管理器。
///
/// 持有 FTL 消息目录和当前 locale 信息。
/// 通过 [`I18nManager::init`] 自动检测系统语言环境并初始化。
#[derive(Debug)]
pub struct I18nManager {
    catalog: MessageCatalog,
    locale_tag: String,
}

impl I18nManager {
    /// 检测系统语言环境并初始化全局管理器。
    ///
    /// 首次调用时根据系统 locale 加载对应的 FTL 消息文件。
    /// 后续调用直接返回已初始化的实例。
    pub fn init() -> &'static Self {
        GLOBAL_I18N.get_or_init(|| {
            #[cfg(feature = "i18n")]
            let locale_str = detect_system_locale();
            #[cfg(not(feature = "i18n"))]
            let locale_str = String::from("en-US");
            Self::build(&locale_str)
        })
    }

    /// 使用指定 BCP-47 locale 标签初始化全局管理器。
    ///
    /// 如果全局管理器已初始化（无论此前初始化还是并发竞争中其他线程抢先），
    /// 直接返回现有实例（不做任何重建）。无效 locale 由内部 `build` 的
    /// 回退逻辑处理（英文目录兜底），不视为错误。
    ///
    /// # Panics
    ///
    /// 不会 panic。`OnceLock::set` 失败后通过 `unwrap` 获取的是已设置的值，保证安全。
    pub fn init_with_locale(locale: &str) -> &'static Self {
        // 快路径：已初始化时不做 FTL 解析等无用构建。
        if let Some(existing) = GLOBAL_I18N.get() {
            return existing;
        }
        let manager = Self::build(locale);
        // 并发竞争中败者直接复用胜者的实例（与上方快路径语义一致）。
        if GLOBAL_I18N.set(manager).is_err() {
            return GLOBAL_I18N
                .get()
                .expect("winner just initialized the manager");
        }
        GLOBAL_I18N.get().unwrap()
    }

    /// 获取全局 [`I18nManager`] 实例。
    ///
    /// 如果 [`init`](Self::init) 或 [`init_with_locale`](Self::init_with_locale)
    /// 尚未调用，返回 `None`。
    #[must_use]
    pub fn global() -> Option<&'static I18nManager> {
        GLOBAL_I18N.get()
    }

    /// 翻译消息 key，带参数替换。
    ///
    /// 如果消息 key 不存在，返回 key 本身作为 fallback。
    #[must_use]
    pub fn translate(&self, message_id: &str, args: &[(&str, &str)]) -> String {
        self.catalog.translate(message_id, args)
    }

    /// 当前 locale 的 BCP-47 标签。
    #[must_use]
    pub fn locale_tag(&self) -> &str {
        &self.locale_tag
    }

    /// 内部构造：根据 locale 选择全局语言束。
    ///
    /// `locale_tag` 存储小写化后的标签，保证与语言束选择
    /// （同样基于小写判断）使用同一形态。
    fn build(locale: &str) -> Self {
        let normalized = locale.to_lowercase();
        Self {
            catalog: MessageCatalog::global(&normalized),
            locale_tag: normalized,
        }
    }
}

/// 便捷函数：翻译消息 key。
///
/// 如果全局 [`I18nManager`] 未初始化，自动调用 [`I18nManager::init`]。
/// 如果消息 key 不存在，返回 key 本身作为 fallback。
///
/// # 示例
///
/// ```rust
/// use trait_kit::i18n::tr;
///
/// let msg = tr("trait-kit-error-already-registered", &[("module", "my-module")]);
/// ```
#[must_use]
pub fn tr(message_id: &str, args: &[(&str, &str)]) -> String {
    let mgr = I18nManager::init();
    mgr.translate(message_id, args)
}

/// 检测系统语言环境，返回 BCP-47 标签。
///
/// 如果检测失败或返回空值，回退到 `"en-US"`。
#[cfg(feature = "i18n")]
fn detect_system_locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string())
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "i18n")]
    use std::cmp::Ordering;

    #[cfg(feature = "i18n")]
    use icu::plurals::PluralCategory;

    use serial_test::serial;

    // ─── Fluent 目录守卫测试 ────────────────────────────────────────────────
    //
    // 断言经 format_from_bundle / 局部 MessageCatalog 直接指定语言，
    // 不触碰全局 GLOBAL_I18N 单例，并行执行安全。

    /// FTL 键集合提取：逐行取 `key = ` 前缀（守卫用宽松匹配，键字符集
    /// 限定 `[a-z0-9-]`，跳过续行/文本行）。
    fn ftl_keys(ftl: &'static str) -> std::collections::BTreeSet<&'static str> {
        ftl.lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, _)| key.trim())
            .filter(|key| {
                !key.is_empty()
                    && key.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
                    })
            })
            .collect()
    }

    /// 守卫：EN/ZH FTL 键集合必须一致（键齐性）。
    #[test]
    fn ftl_key_parity_en_zh() {
        let en = ftl_keys(messages::EN_FTL);
        let zh = ftl_keys(messages::ZH_FTL);
        assert!(!en.is_empty(), "EN FTL must yield keys");
        let missing_in_zh: Vec<_> = en.difference(&zh).collect();
        let missing_in_en: Vec<_> = zh.difference(&en).collect();
        assert!(
            missing_in_zh.is_empty() && missing_in_en.is_empty(),
            "EN/ZH key sets diverge: missing_in_zh={missing_in_zh:?} \
             missing_in_en={missing_in_en:?}"
        );
    }

    #[test]
    fn bundle_en_and_zh_lookup_with_args() {
        assert_eq!(
            format_from_bundle(
                "en",
                "trait-kit-error-already-registered",
                &[("module", "m")],
            )
            .as_deref(),
            Some("module `m` is already registered"),
        );
        assert_eq!(
            format_from_bundle(
                "zh",
                "trait-kit-error-missing-capability",
                &[("key", "cap")],
            )
            .as_deref(),
            Some("缺少能力 `cap`"),
        );
    }

    /// 守卫：未知语言回退 en 束，不 panic。
    #[test]
    fn bundle_unknown_lang_falls_back_to_en_bundle() {
        assert_eq!(
            format_from_bundle(
                "ar",
                "trait-kit-error-missing-capability",
                &[("key", "cap")],
            )
            .as_deref(),
            Some("missing capability `cap`"),
        );
    }

    /// 守卫：缺 key 不 panic——束查询返 `None`，目录翻译返 key 本身。
    #[test]
    fn bundle_missing_key_returns_none_and_translate_returns_key() {
        assert_eq!(format_from_bundle("en", "nonexistent-key", &[]), None);
        assert_eq!(format_from_bundle("zh", "nonexistent-key", &[]), None);
        let catalog = MessageCatalog::global("zh");
        assert_eq!(catalog.translate("nonexistent-key", &[]), "nonexistent-key");
    }

    #[test]
    fn bundle_arg_value_is_not_reparsed_as_pattern() {
        // Fluent 单遍解析：参数值中的 "{ $context }" 按字面插入，不会被
        // 再次解析（与旧实现的单遍扫描注入防护语义一致）。
        let out = format_from_bundle(
            "en",
            "trait-kit-error-build-failed",
            &[("context", "ctx"), ("source", "{ $context } injected")],
        );
        assert_eq!(
            out.as_deref(),
            Some("failed to build `ctx`: { $context } injected"),
        );
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn overlay_catalog_resolves_overrides_and_falls_back_to_key() {
        let catalog = MessageCatalog::parse(
            "en",
            &["ovk-a = Alpha { $n }\novk-b = Beta", "ovk-a = Alpha2 { $n }"],
        );
        assert_eq!(
            catalog.translate("ovk-a", &[("n", "1")]),
            "Alpha2 1",
            "跨片段同名消息应后入者覆盖先入者"
        );
        assert_eq!(catalog.translate("ovk-b", &[]), "Beta");
        // overlay 缺 key 返 key 本身（由 Kit::module_tr 再回退全局目录）。
        assert_eq!(catalog.translate("ovk-missing", &[]), "ovk-missing");
    }

    // ─── I18nManager 测试 ───────────────────────────────────────────────────
    //
    // 下列测试共享进程级 GLOBAL_I18N 单例（init()/init_with_locale()/tr()
    // 都读写同一个 OnceLock），故统一以 #[serial(i18n_global)] 串行执行。

    #[test]
    #[serial(i18n_global)]
    fn manager_init_returns_valid_instance() {
        let mgr = I18nManager::init();
        assert!(
            !mgr.locale_tag().is_empty(),
            "locale tag should be non-empty"
        );
    }

    #[test]
    #[serial(i18n_global)]
    fn manager_translate_message() {
        let mgr = I18nManager::init();
        let msg = mgr.translate(
            "trait-kit-error-already-registered",
            &[("module", "test-mod")],
        );
        assert!(
            msg.contains("test-mod"),
            "translated message should contain module name: got '{msg}'"
        );
    }

    #[test]
    #[serial(i18n_global)]
    fn manager_translate_unknown_key_returns_key() {
        let mgr = I18nManager::init();
        let msg = mgr.translate("nonexistent-key", &[]);
        assert_eq!(msg, "nonexistent-key");
    }

    #[test]
    #[serial(i18n_global)]
    fn tr_convenience_function_works() {
        let msg = tr("trait-kit-error-missing-capability", &[("key", "my-cap")]);
        assert!(
            msg.contains("my-cap"),
            "tr() output should contain key: got '{msg}'"
        );
    }

    // ─── I18nFormatter 测试 ─────────────────────────────────────────────────

    #[cfg(feature = "i18n")]
    #[test]
    fn test_locale_parsing_en() {
        let fmt = I18nFormatter::new("en-US");
        assert!(fmt.is_ok(), "en-US should parse successfully");
        let fmt = fmt.unwrap();
        assert_eq!(fmt.locale.to_string(), "en-US");
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_locale_parsing_zh() {
        let fmt = I18nFormatter::new("zh-CN");
        assert!(fmt.is_ok(), "zh-CN should parse successfully");
        let fmt = fmt.unwrap();
        assert_eq!(fmt.locale.to_string(), "zh-CN");
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_invalid_locale() {
        let result = I18nFormatter::new("not-a-valid-locale!!!");
        assert!(result.is_err(), "invalid locale should return error");
        match result.err().unwrap() {
            I18nError::InvalidLocale { input, .. } => assert_eq!(input, "not-a-valid-locale!!!"),
            other => panic!("expected InvalidLocale, got {other:?}"),
        }
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_format_number_en() {
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_number(1_234_567.89_f64).expect("format number");
        assert!(
            result.contains(','),
            "en-US number should contain thousands separator: got '{result}'"
        );
        assert!(
            result.contains('.'),
            "en-US number should contain decimal point: got '{result}'"
        );
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_format_number_zh() {
        let fmt = I18nFormatter::new("zh-CN").expect("zh-CN locale");
        let result = fmt.format_number(1_234_567.89_f64).expect("format number");
        assert!(
            !result.is_empty(),
            "zh-CN number should be non-empty: got '{result}'"
        );
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_format_number_not_finite() {
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        assert!(fmt.format_number(f64::NAN).is_err());
        assert!(fmt.format_number(f64::INFINITY).is_err());
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_plural_rules_en() {
        let fmt = I18nFormatter::new("en").expect("en locale");
        assert_eq!(
            fmt.plural_category(1).expect("plural 1"),
            PluralCategory::One,
            "en: count=1 should be One"
        );
        assert_eq!(
            fmt.plural_category(2).expect("plural 2"),
            PluralCategory::Other,
            "en: count=2 should be Other"
        );
        assert_eq!(
            fmt.plural_category(0).expect("plural 0"),
            PluralCategory::Other,
            "en: count=0 should be Other"
        );
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_collator_basic() {
        let fmt = I18nFormatter::new("en").expect("en locale");
        assert_eq!(
            fmt.compare("apple", "banana").expect("compare"),
            Ordering::Less,
            "apple < banana"
        );
        assert_eq!(
            fmt.compare("banana", "apple").expect("compare"),
            Ordering::Greater,
            "banana > apple"
        );
        assert_eq!(
            fmt.compare("apple", "apple").expect("compare"),
            Ordering::Equal,
            "apple == apple"
        );
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_format_date_en() {
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_date(2026, 7, 11).expect("format date");
        assert!(
            result.contains("2026"),
            "date should contain year: got '{result}'"
        );
        assert!(
            !result.is_empty(),
            "date should be non-empty: got '{result}'"
        );
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_format_date_invalid_month() {
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_date(2026, 13, 1);
        assert!(result.is_err(), "month 13 should be invalid");
        assert!(matches!(result.unwrap_err(), I18nError::DateError(_)));
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_format_date_invalid_day() {
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_date(2026, 2, 30);
        assert!(result.is_err(), "Feb 30 should be invalid");
        assert!(matches!(result.unwrap_err(), I18nError::DateError(_)));
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_format_number_integer() {
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_number(42.0).expect("format integer-like float");
        assert!(
            result.contains('4'),
            "should contain digit 4: got '{result}'"
        );
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_plural_category_zero() {
        let fmt = I18nFormatter::new("zh-CN").expect("zh-CN locale");
        let cat = fmt.plural_category(0).expect("plural 0");
        assert_eq!(
            cat,
            PluralCategory::Other,
            "Chinese uses Other for all counts"
        );
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn test_compare_equal_strings() {
        let fmt = I18nFormatter::new("de-DE").expect("de-DE locale");
        let result = fmt.compare("abc", "abc").expect("compare");
        assert_eq!(result, Ordering::Equal);
    }

    // ─── I18nError Display 测试 ─────────────────────────────────────────────

    #[test]
    fn error_display_invalid_locale() {
        let err = I18nError::InvalidLocale {
            input: "bad".into(),
            reason: "parse failed".into(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("bad"),
            "error display should contain input: got '{msg}'"
        );
    }

    #[test]
    fn error_display_date_error() {
        let err = I18nError::DateError("month out of range".into());
        let msg = err.to_string();
        assert!(
            msg.contains("month out of range"),
            "error display should contain detail: got '{msg}'"
        );
    }

    #[test]
    fn error_display_invalid_number() {
        let err = I18nError::InvalidNumber {
            input: "NaN".into(),
            reason: "not finite".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("NaN"), "should contain input: got '{msg}'");
    }

    #[test]
    fn error_display_format_error() {
        let err = I18nError::FormatError("formatting failed".into());
        let msg = err.to_string();
        assert!(msg.contains("formatting failed"), "got '{msg}'");
    }

    #[test]
    #[serial(i18n_global)]
    fn i18n_manager_init_with_locale() {
        // init_with_locale 与 init()/tr() 使用同一个 GLOBAL_I18N OnceLock；
        // 语义幂等：已初始化时（无论先前用的哪个 locale）直接返回现有实例。
        let mgr = I18nManager::init_with_locale("en-US");
        assert!(
            std::ptr::eq(mgr, I18nManager::global().expect("init just succeeded")),
            "init_with_locale must return the global instance"
        );
    }

    #[test]
    #[serial(i18n_global)]
    fn i18n_manager_global_returns_some_after_init() {
        I18nManager::init_with_locale("en-US");
        assert!(I18nManager::global().is_some());
    }

    #[test]
    fn i18n_manager_translate_and_locale_tag() {
        let manager = I18nManager::build("en-US");
        // build() 存储小写化后的 locale 标签（与 FTL 选择逻辑一致）。
        let tag = manager.locale_tag();
        assert_eq!(tag, "en-us");
        let msg = manager.translate("nonexistent-key", &[]);
        assert_eq!(msg, "nonexistent-key");
    }

    #[test]
    fn i18n_manager_build_zh_cn() {
        let manager = I18nManager::build("zh-CN");
        // build() 存储小写化后的 locale 标签（与 FTL 选择逻辑一致）。
        assert_eq!(manager.locale_tag(), "zh-cn");
        // 大写输入 "ZH" 同样命中 zh 目录。
        let upper = I18nManager::build("ZH");
        assert_eq!(upper.locale_tag(), "zh");
        assert!(
            upper
                .translate("trait-kit-error-already-registered", &[])
                .contains("已注册"),
            "'ZH' 输入应选择 zh 目录：got '{}'",
            upper.translate("trait-kit-error-already-registered", &[])
        );
    }
}
