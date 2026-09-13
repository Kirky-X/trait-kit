// Copyright (c) 2026 Kirky.X
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

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

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

// ─── MessageCatalog（轻量级 FTL 消息翻译） ──────────────────────────────────

/// 轻量级 FTL 消息目录。
///
/// 解析 Fluent FTL 格式的 `key = value` 消息，支持 `{ $var }` 变量替换。
/// 无需 `fluent-bundle` 运行时，避免自引用结构问题。
///
/// # 宽松解析行为与已知限制
///
/// [`parse`](Self::parse) 逐行扫描，**静默丢弃**无法解析的内容，不做任何
/// 报错或日志。已知限制：
///
/// - 不支持多行消息（Fluent 的缩进续行）；
/// - 不支持嵌套 placeholder 与 selector（`.match` 等 Fluent 结构）；
/// - 不含 `=` 的行被直接忽略；
/// - 含 `=` 的续行会被误当作独立消息，解析出一个假 key。
#[derive(Debug)]
pub(crate) struct MessageCatalog {
    messages: HashMap<String, String>,
}

impl MessageCatalog {
    /// 从 FTL 格式字符串解析消息目录。
    ///
    /// 支持的格式（宽松逐行解析，见类型文档的限制说明）：
    /// - `# 注释行`（忽略）
    /// - 空行（忽略）
    /// - `message-id = 消息文本`（解析为 key-value）
    /// - 不含 `=` 的行（静默丢弃）
    pub(crate) fn parse(ftl: &str) -> Self {
        let mut messages = HashMap::new();
        for line in ftl.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                messages.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
        Self { messages }
    }

    /// 翻译消息 key，带参数替换。
    ///
    /// `{ $var }` 占位符被替换为 `args` 中对应的值，花括号内空白数量不限
    /// （`{$var}` / `{ $var }` / `{  $var  }` 等价）。
    ///
    /// 替换为单遍扫描：命中占位符后插入值并跳过整段，插入的参数值
    /// **不会被再次扫描**，因此参数值中形如 `{ $other }` 的文本按字面
    /// 保留（不存在按顺序 `replace` 时的注入污染问题）。未知 key、非
    /// `$key` 形式或无配对 `}` 的花括号均按字面保留。
    ///
    /// 如果消息 key 不存在，返回 key 本身作为 fallback。
    pub(crate) fn translate(&self, message_id: &str, args: &[(&str, &str)]) -> String {
        let Some(template) = self.messages.get(message_id) else {
            return message_id.to_string();
        };
        let mut result = String::with_capacity(template.len());
        let mut rest = template.as_str();
        while let Some(open) = rest.find('{') {
            result.push_str(&rest[..open]);
            // 找与该 `{` 配对的 `}`；找不到则按字面保留 `{` 并继续扫描。
            let Some(close) = rest[open + 1..].find('}') else {
                result.push('{');
                rest = &rest[open + 1..];
                continue;
            };
            let inner = rest[open + 1..open + 1 + close].trim();
            let replaced = inner.strip_prefix('$').and_then(|key| {
                args.iter()
                    .find(|&&(arg_key, _)| arg_key == key)
                    .map(|&(_, value)| value)
            });
            if let Some(value) = replaced {
                result.push_str(value);
                rest = &rest[open + 1 + close + 1..];
            } else {
                // 非占位符或未知 key：按字面保留 `{`，从其后继续扫描。
                result.push('{');
                rest = &rest[open + 1..];
            }
        }
        result.push_str(rest);
        result
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

    /// 内部构造：根据 locale 选择 FTL 内容并解析。
    ///
    /// `locale_tag` 存储小写化后的标签，保证与 FTL 目录选择
    /// （同样基于小写判断）使用同一形态。
    fn build(locale: &str) -> Self {
        let normalized = locale.to_lowercase();
        let ftl_content = if normalized.starts_with("zh") {
            messages::ZH_FTL
        } else {
            messages::EN_FTL
        };
        Self {
            catalog: MessageCatalog::parse(ftl_content),
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

    // ─── MessageCatalog 测试 ────────────────────────────────────────────────

    #[test]
    fn catalog_parse_simple_ftl() {
        let catalog = MessageCatalog::parse("hello = Hello, world!\nbye = Goodbye!");
        assert_eq!(catalog.translate("hello", &[]), "Hello, world!");
        assert_eq!(catalog.translate("bye", &[]), "Goodbye!");
    }

    #[test]
    fn catalog_parse_skips_comments_and_blanks() {
        let ftl = "# comment\n\nkey = value\n# another comment\n";
        let catalog = MessageCatalog::parse(ftl);
        assert_eq!(catalog.translate("key", &[]), "value");
    }

    #[test]
    fn catalog_translate_with_variables() {
        let catalog = MessageCatalog::parse("greet = Hello, { $name }!");
        let result = catalog.translate("greet", &[("name", "World")]);
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn catalog_translate_unknown_key_returns_key() {
        let catalog = MessageCatalog::parse("key = value");
        assert_eq!(catalog.translate("unknown", &[]), "unknown");
    }

    // ─── translate() 单遍扫描替换行为 ───────────────────────────────────────

    #[test]
    fn catalog_translate_arg_value_is_not_rescanned() {
        // 注入防护：前一 arg 的值含 `{ $source }` 字样时，必须原样保留，
        // 不得被后续 args 替换污染（旧实现按顺序 String::replace 会把
        // `{ $source }` 替换为 BOOM）。
        let catalog = MessageCatalog::parse("build = { $context }: { $source }");
        let result = catalog.translate("build", &[("context", "{ $source }"), ("source", "BOOM")]);
        assert_eq!(result, "{ $source }: BOOM");
    }

    #[test]
    fn catalog_translate_placeholder_spacing_variants() {
        // `{$key}` / `{  $key  }` 等空格变体均应替换成功。
        let catalog = MessageCatalog::parse("msg = [{$x}] [{ $y }] [{  $z  }]");
        let result = catalog.translate("msg", &[("x", "1"), ("y", "2"), ("z", "3")]);
        assert_eq!(result, "[1] [2] [3]");
    }

    #[test]
    fn catalog_translate_unknown_or_non_placeholder_braces_kept_literal() {
        let catalog = MessageCatalog::parse("msg = hi { $known } { $unknown } {literal} {}");
        let result = catalog.translate("msg", &[("known", "K")]);
        assert_eq!(result, "hi K { $unknown } {literal} {}");
    }

    #[test]
    fn catalog_translate_unclosed_brace_kept_literal() {
        let catalog = MessageCatalog::parse("msg = o{ops and trailing {");
        let result = catalog.translate("msg", &[]);
        assert_eq!(result, "o{ops and trailing {");
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
