// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 国际化（i18n）支持 — Fluent FTL 消息翻译 + ICU4X 本地化格式化。
//!
//! 提供两大能力：
//!
//! 1. **消息翻译**（[`I18nManager`] + [`tr`]）：基于 Fluent FTL 语言包
//!    （`locales/{en,zh}/`，按域拆分），默认英文，显式协商可切中文。
//! 2. **本地化格式化**（[`I18nFormatter`]）：ICU4X 驱动的数字/日期/复数/排序格式化。
//!
//! # 启动初始化
//!
//! [`I18nManager::init`] 的默认显示语言严格为英文，不随宿主系统语言漂移
//! （库输出因此保持确定性）：
//!
//! ```rust
//! use trait_kit::i18n::I18nManager;
//!
//! let mgr = I18nManager::init();
//! ```
//!
//! 语言协商保留：显式指定 locale 即可切换目录（如中文）：
//!
//! ```rust
//! use trait_kit::i18n::I18nManager;
//!
//! let mgr = I18nManager::init_with_locale("zh-CN");
//! ```
//!
//! 需要跟随宿主系统语言时，调用 [`I18nManager::init_with_system_locale`]
//! （仅 `i18n` feature 下可用），它检测到 `zh*` 才启用中文目录。
//!
//! # 宿主注册覆盖
//!
//! 宿主框架可经 [`I18nManager::register_translation`] 注册进程级翻译
//! 覆盖（注册优先于内置语言包，占位符 `{ $name }` 替换语义与内置路径
//! 一致），供 sdforge 等宿主把自己的翻译存储桥接到本管理器；测试隔离
//! 用 [`I18nManager::clear_registered_translations`]。

#[cfg(feature = "i18n")]
mod i18n_impl;
mod messages;

use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, OnceLock, RwLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

#[cfg(feature = "i18n")]
use icu_collator::CollatorBorrowed;
#[cfg(feature = "i18n")]
use icu_datetime::DateTimeFormatter;
#[cfg(feature = "i18n")]
use icu_datetime::fieldsets::YMD;
#[cfg(feature = "i18n")]
use icu_decimal::DecimalFormatter;
#[cfg(feature = "i18n")]
use icu_plurals::PluralRules;

// I18nError

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
        // tr 通过 OnceLock 惰性初始化，build 不调用 Display，
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

// I18nFormatter（ICU4X 格式化）

/// 基于 ICU4X 编译数据的 locale 感知格式化器。
///
/// ICU4X 复数类别类型重导出：下游仓（含受依赖环约束、无法直依赖 `icu_plurals` 的仓）
/// 经 trait-kit 使用 ICU 格式化时，无需自持 ICU 依赖面，即可命名本类型。
#[cfg(feature = "i18n")]
pub use icu_plurals::PluralCategory;

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
    /// 小数（数字）格式化器。
    pub(crate) decimal_formatter: DecimalFormatter,
    /// 该 locale 的复数规则。
    pub(crate) plural_rules: PluralRules,
    /// 字符串排序比较器。
    pub(crate) collator: CollatorBorrowed<'static>,
    /// 日期（YMD·medium）格式化器。
    pub(crate) date_formatter: DateTimeFormatter<YMD>,
}

// Fluent 消息目录（fluent-bundle concurrent 双束）

/// 全局英文（回退）束：进程级共享，首次访问时从内嵌 `locales/en/*.ftl` 构建。
static EN_BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();

/// 全局中文束：进程级共享，首次访问时从内嵌 `locales/zh/*.ftl` 构建。
static ZH_BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();

/// 从 FTL 片段构建并发（`Send + Sync`）Fluent 束。
///
/// 每个域文件是一个片段，按数组顺序依次入束。解析失败不 panic：
/// `try_new` 返回的部分资源照常入束（损坏消息由 Fluent 在查询时按语义
/// 降级），与统一 i18n 参考基线一致。
fn build_bundle(lang: &str, ftls: &[&str]) -> FluentBundle<FluentResource> {
    let langid: LanguageIdentifier = lang
        .parse()
        .unwrap_or_else(|_| "en".parse().expect("'en' is a valid language identifier"));
    let mut bundle = FluentBundle::new_concurrent(vec![langid]);
    // 关闭 Unicode 隔离符，避免插值文本两侧被 \u{2068}/\u{2069} 包裹。
    bundle.set_use_isolating(false);
    for ftl in ftls {
        let resource = FluentResource::try_new((*ftl).to_string()).unwrap_or_else(|e| e.0);
        bundle
            .add_resource(resource)
            .expect("domain FTL fragments must not declare conflicting message ids");
    }
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
        "zh" => ZH_BUNDLE.get_or_init(|| build_bundle("zh", messages::ZH_FTLS)),
        _ => EN_BUNDLE.get_or_init(|| build_bundle("en", messages::EN_FTLS)),
    };
    format_from_custom_bundle(bundle, message_id, args)
}

// 宿主注册覆盖层（进程级）
//
// 注册面：I18nManager::register_translation / clear_registered_translations。
// 查询链最高优先级：注册[当前语言] → 内置[当前语言] → 注册[en] →
// 内置[en] → key 本身（与 sdforge 宿主注册面 translate_for 同一链路，
// 供其桥接存储层后语义对齐）。

/// 宿主注册翻译表：归一化语言（`"zh"` / `"en"`）→ 该语言的
/// `key → 模板` 表。
///
/// 注册阶段写（写锁），翻译热路径读（读锁）：读路径为一次惰性初始化
/// 原子读加无竞争读锁，相对 Fluent 格式化的分配开销可忽略；表为空时
/// 语言桶查找即未命中，查询行为与无任何注册时完全一致（内置束路径
/// 本身无锁）。
static REGISTERED_TRANSLATIONS: LazyLock<RwLock<HashMap<&'static str, HashMap<String, String>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 归一化 locale 到注册桶语言（`"zh"` / `"en"`）。
///
/// 与 [`MessageCatalog::global`] 的语言束选择同一规则：小写化后以
/// `zh` 开头选 zh，其余（含未知语言）一律归 en。
fn normalize_registered_lang(locale: &str) -> &'static str {
    if locale.to_lowercase().starts_with("zh") {
        "zh"
    } else {
        "en"
    }
}

/// 查询注册覆盖层；未注册返回 `None`。占位符按
/// [`format_registered_template`] 语义替换。任何路径不 panic。
fn registered_lookup(lang: &str, message_id: &str, args: &[(&str, &str)]) -> Option<String> {
    let registered = REGISTERED_TRANSLATIONS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    registered
        .get(lang)?
        .get(message_id)
        .map(|template| format_registered_template(template, args))
}

/// 宿主注册模板的 `{ $name }` 占位符替换（单遍扫描）。
///
/// 宿主字符串是任意文本（键可含 `.`、值可含任意花括号），不走 Fluent
/// 解析——解析失败会静默丢失注册项。占位符形态与内置 Fluent 路径一致：
/// `{ $name }` 与 `{$name}` 均按去空白后 `$` 前缀识别；参数值按字面
/// 插入、不会被再次解析（单遍，与内置路径的注入防护语义一致）；未知
/// 占位符与非占位符花括号原样保留。
fn format_registered_template(template: &str, args: &[(&str, &str)]) -> String {
    if args.is_empty() || !template.contains('{') {
        return template.to_string();
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(close_rel) = rest[open..].find('}') else {
            break;
        };
        let inner = rest[open + 1..open + close_rel].trim();
        match inner.strip_prefix('$').map(str::trim) {
            Some(name) if !name.is_empty() => {
                out.push_str(&rest[..open]);
                match args.iter().find(|(arg_name, _)| *arg_name == name) {
                    Some((_, value)) => out.push_str(value),
                    // 未知占位符：保留原 `{ $name }` 文本。
                    None => out.push_str(&rest[open..=open + close_rel]),
                }
                rest = &rest[open + close_rel + 1..];
            }
            _ => {
                // 非占位符花括号：原样输出开括号，从其后继续扫描。
                out.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Fluent 消息目录（完整 Fluent 语法，由 `fluent-bundle` 支撑）。
///
/// 两种形态：
///
/// - [`MessageCatalog::global`]：进程级 EN/ZH 双束（[`OnceLock`] 缓存），
///   查询链为注册覆盖[当前语言] → 当前语言束 → 注册覆盖[en] → en 束 →
///   key 本身；
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
    /// `{ $var }` 占位符由 Fluent 引擎解析。全局目录查询链为
    /// 注册覆盖[当前语言] → 当前语言束 → 注册覆盖[en] → en 束 →
    /// key 本身；overlay 目录缺 key 返回 key 本身。任何路径不 panic。
    pub(crate) fn translate(&self, message_id: &str, args: &[(&str, &str)]) -> String {
        match self {
            Self::Global { lang } => registered_lookup(lang, message_id, args)
                .or_else(|| format_from_bundle(lang, message_id, args))
                .or_else(|| {
                    if *lang == "en" {
                        None
                    } else {
                        registered_lookup("en", message_id, args)
                            .or_else(|| format_from_bundle("en", message_id, args))
                    }
                })
                .unwrap_or_else(|| message_id.to_string()),
            #[cfg(feature = "i18n")]
            Self::Overlay { bundle } => format_from_custom_bundle(bundle, message_id, args)
                .unwrap_or_else(|| message_id.to_string()),
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

// I18nManager（全局状态 + 消息翻译）

/// 全局 [`I18nManager`] 实例。
static GLOBAL_I18N: OnceLock<I18nManager> = OnceLock::new();

/// Fluent 消息翻译管理器。
///
/// 持有 FTL 消息目录和当前 locale 信息。
/// 通过 [`I18nManager::init`] 初始化为英文；语言协商走
/// [`I18nManager::init_with_locale`] / [`I18nManager::init_with_system_locale`]。
#[derive(Debug)]
pub struct I18nManager {
    catalog: MessageCatalog,
    locale_tag: String,
}

impl I18nManager {
    /// 初始化全局管理器，默认显示语言严格为英文。
    ///
    /// 不检测系统语言环境（宿主为中文系统时也输出英文），保证库的输出
    /// 确定性；需要系统语言协商时用
    /// [`init_with_system_locale`](Self::init_with_system_locale)，需要指定
    /// 语言时用 [`init_with_locale`](Self::init_with_locale)。
    ///
    /// 首次调用时加载语言包。后续调用直接返回已初始化的实例。
    pub fn init() -> &'static Self {
        GLOBAL_I18N.get_or_init(|| Self::build("en"))
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

    /// 按宿主系统语言环境初始化全局管理器（语言协商 opt-in 入口）。
    ///
    /// 检测到 `zh*` 启用中文目录，其余（含检测失败）一律回退英文。与
    /// [`init_with_locale`](Self::init_with_locale) 一样幂等：全局实例已存在
    /// 时直接返回现有实例。
    #[cfg(feature = "i18n")]
    #[must_use]
    pub fn init_with_system_locale() -> &'static Self {
        Self::init_with_locale(&detect_system_locale())
    }

    /// 获取全局 [`I18nManager`] 实例。
    ///
    /// 如果 [`init`](Self::init)、[`init_with_locale`](Self::init_with_locale)
    /// 或 `init_with_system_locale` 尚未调用，返回 `None`。
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

    /// 注册一条宿主翻译覆盖（进程级，全局生效）。
    ///
    /// 语义（与 sdforge 宿主注册面对齐，供其桥接翻译存储层）：
    ///
    /// - `locale` 小写化后以 `zh` 开头归入 zh 注册桶，其余（含未知
    ///   语言）归入 en 注册桶——与语言束选择同一规则；
    /// - 同一 `(语言桶, key)` 重复注册时后写覆盖先写；
    /// - 注册覆盖位于查询链最高优先级：注册(当前语言) → 内置(当前
    ///   语言) → 注册(en) → 内置(en) → key 本身；
    /// - `value` 为任意文本模板，`{ $name }` 占位符由 [`translate`](Self::translate)
    ///   的 `args` 单遍替换，与内置路径的占位符形态及注入防护语义一致。
    ///
    /// 注册阶段写、翻译热路径读；未注册任何条目时查询行为与无本 API
    /// 时完全一致。测试隔离用
    /// [`clear_registered_translations`](Self::clear_registered_translations)。
    ///
    /// # Examples
    ///
    /// ```rust
    /// use trait_kit::i18n::{tr, I18nManager};
    ///
    /// I18nManager::register_translation("en", "demo-greeting", "hello { $who }");
    /// assert_eq!(tr("demo-greeting", &[("who", "kit")]), "hello kit");
    /// ```
    pub fn register_translation(locale: &str, key: &str, value: &str) {
        let lang = normalize_registered_lang(locale);
        let mut registered = REGISTERED_TRANSLATIONS
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registered
            .entry(lang)
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    /// 清空全部宿主注册翻译（测试隔离用）。
    ///
    /// 只清注册覆盖层：内置 en/zh 语言包（惰性重建、不可移除）与全局
    /// locale 不受影响，清空后内置消息照常翻译。
    pub fn clear_registered_translations() {
        REGISTERED_TRANSLATIONS
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
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

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "i18n")]
    use std::cmp::Ordering;

    #[cfg(feature = "i18n")]
    use icu_plurals::PluralCategory;

    use serial_test::serial;

    // Fluent 目录守卫测试
    //
    // 断言经 format_from_bundle / 局部 MessageCatalog 直接指定语言，
    // 不触碰全局 GLOBAL_I18N 单例，并行执行安全。

    /// FTL 键集合提取：逐行取 `key = ` 前缀（守卫用宽松匹配，键字符集
    /// 限定 `[a-z0-9-]`，跳过续行/文本行）。
    fn ftl_keys(ftls: &[&'static str]) -> std::collections::BTreeSet<&'static str> {
        ftls.iter()
            .flat_map(|ftl| ftl.lines())
            .filter_map(|line| line.split_once('='))
            .map(|(key, _)| key.trim())
            .filter(|key| {
                !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            })
            .collect()
    }

    /// 守卫：EN/ZH 语言包（含全部域文件）键集合必须一致（键齐性）。
    #[test]
    fn ftl_key_parity_en_zh() {
        let en = ftl_keys(messages::EN_FTLS);
        let zh = ftl_keys(messages::ZH_FTLS);
        assert!(!en.is_empty(), "EN FTL must yield keys");
        let missing_in_zh: Vec<_> = en.difference(&zh).collect();
        let missing_in_en: Vec<_> = zh.difference(&en).collect();
        assert!(
            missing_in_zh.is_empty() && missing_in_en.is_empty(),
            "EN/ZH key sets diverge: missing_in_zh={missing_in_zh:?} \
             missing_in_en={missing_in_en:?}"
        );
    }

    /// 守卫：`locales/` 磁盘目录与内嵌清单一致（新增域文件漏挂即失败）。
    #[test]
    fn locales_dir_matches_embedded_list() {
        let locales_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let mut disk_files: Vec<String> = Vec::new();
        for lang in ["en", "zh"] {
            let entries =
                std::fs::read_dir(locales_dir.join(lang)).expect("locales/{lang} must exist");
            for entry in entries.filter_map(Result::ok) {
                if entry.path().extension().and_then(|e| e.to_str()) == Some("ftl") {
                    disk_files.push(format!(
                        "locales/{lang}/{}",
                        entry.file_name().to_string_lossy()
                    ));
                }
            }
        }
        disk_files.sort();

        let embedded: std::collections::BTreeSet<&str> = messages::EMBEDDED_LOCALES
            .iter()
            .map(|(path, _)| *path)
            .collect();
        let disk: std::collections::BTreeSet<&str> =
            disk_files.iter().map(String::as_str).collect();
        assert_eq!(
            embedded, disk,
            "EMBEDDED_LOCALES and the locales/ directory must list exactly the same files"
        );

        for (path, content) in messages::EMBEDDED_LOCALES {
            let rel = path
                .strip_prefix("locales/")
                .expect("paths are locales/-relative");
            let on_disk = std::fs::read_to_string(locales_dir.join(rel))
                .unwrap_or_else(|e| panic!("read {path}: {e}"));
            assert_eq!(
                content, &on_disk,
                "embedded {path} has drifted from the file on disk"
            );
        }
    }

    /// 守卫：自硬编码接入的键在 EN/ZH 束中按预期渲染（FTL 行语法错误会
    /// 被 Fluent 静默降级，此处逐字断言以防回退到 key 本身）。
    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "键表随接入批次增长，守卫需在同一处逐键断言 EN/ZH 渲染结果"
    )]
    fn bundle_hardcoded_i18n_keys_render_in_en_and_zh() {
        for (key, args, en, zh) in [
            (
                "trait-kit-error-shutdown-hooks-panicked",
                &[("failures", "3")][..],
                "3 shutdown hook(s) panicked and were isolated",
                "3 个关闭钩子已 panic 并被隔离",
            ),
            (
                "trait-kit-log-shutdown-bridge-stranded",
                &[("error", "boom"), ("stranded", "2")][..],
                "register_shutdown_into: coordinator rejected a hook (boom); \
                 2 remaining lifecycle hook(s) are stranded and will never run",
                "register_shutdown_into：协调器拒绝了一个钩子（boom）；\
                 剩余 2 个生命周期钩子未转移，将永远不会执行",
            ),
            (
                "trait-kit-soft-build-degraded",
                &[("module", "m"), ("error", "boom")][..],
                "module `m` build failed: boom; degrading to fallback",
                "模块 `m` 构建失败: boom；降级到回退实现",
            ),
            (
                "trait-kit-probe-timeout-measured",
                &[("measured_ms", "42")][..],
                "probe timed out after 42ms",
                "探针在 42 毫秒后超时",
            ),
            (
                "trait-kit-probe-registry-stopped",
                &[][..],
                "probe registry stopped by shutdown",
                "探针注册表已因关闭而停止",
            ),
            (
                "trait-kit-probe-timeout-no-verdict",
                &[][..],
                "probe timed out before returning a verdict",
                "探针在给出结论前超时",
            ),
            (
                "trait-kit-error-panic-payload-non-string",
                &[][..],
                "non-string panic payload",
                "非字符串 panic 载荷",
            ),
            (
                "trait-kit-error-graph-dependency-missing",
                &[("module", "a"), ("missing", "b")][..],
                "module `a` depends on unregistered module `b`",
                "模块 `a` 依赖的模块 `b` 未注册",
            ),
            (
                "trait-kit-error-graph-cycle-detected",
                &[("cycle", "a -> b")][..],
                "dependency cycle detected: a -> b",
                "检测到依赖循环: a -> b",
            ),
            (
                "trait-kit-preset-provider-not-injected",
                &[][..],
                "ConfersConfigModule built without a provider; call \
                 register_confers_config(kit, provider) first",
                "ConfersConfigModule 构建时未注入 provider；\
                 请先调用 register_confers_config(kit, provider)",
            ),
            (
                "trait-kit-example-shutdown-title",
                &[][..],
                "=== ShutdownCoordinator demo ===",
                "=== ShutdownCoordinator 示例 ===",
            ),
            (
                "trait-kit-example-shutdown-results-header",
                &[][..],
                "--- shutdown results ---",
                "--- 关闭结果 ---",
            ),
            (
                "trait-kit-example-shutdown-result-line",
                &[
                    ("phase", "stop_requests"),
                    ("phase_debug", "StopRequests"),
                    ("status", "done"),
                    ("elapsed", "1.2ms"),
                ][..],
                "stop_requests (StopRequests): done (elapsed 1.2ms)",
                "stop_requests (StopRequests): done (耗时 1.2ms)",
            ),
            (
                "trait-kit-example-shutdown-status-done",
                &[][..],
                "✓ done",
                "✓ 完成",
            ),
            (
                "trait-kit-example-shutdown-ok",
                &[("total_hooks", "5")][..],
                "shutdown: OK (5 hooks executed)",
                "shutdown: OK (共执行 5 个钩子)",
            ),
            (
                "trait-kit-example-i18n-system-locale",
                &[("locale", "zh-cn")][..],
                "system locale: zh-cn",
                "系统 locale: zh-cn",
            ),
            (
                "trait-kit-example-i18n-en-us-number",
                &[("number", "1,234,567.89")][..],
                "en-US number: 1,234,567.89",
                "en-US 数字: 1,234,567.89",
            ),
            (
                "trait-kit-example-i18n-en-plural",
                &[("one", "One"), ("other", "Other")][..],
                "en plural: 1 -> One, 5 -> Other",
                "en 复数: 1 -> One, 5 -> Other",
            ),
            (
                "trait-kit-example-i18n-en-collation",
                &[("cmp", "Less")][..],
                "en collation: 'apple' vs 'banana' -> Less",
                "en 排序: 'apple' vs 'banana' -> Less",
            ),
        ] {
            assert_eq!(
                format_from_bundle("en", key, args).as_deref(),
                Some(en),
                "{key}: en bundle degraded to the raw key"
            );
            assert_eq!(
                format_from_bundle("zh", key, args).as_deref(),
                Some(zh),
                "{key}: zh bundle degraded to the raw key"
            );
        }
    }

    /// 守卫：对外类别标签在 EN/ZH 束中按预期渲染（无占位符）。
    ///
    /// 与 `bundle_hardcoded_i18n_keys_render_in_en_and_zh` 同口径：FTL 行
    /// 语法错误会被 Fluent 静默降级，此处逐字断言以防回退到 key 本身。
    #[test]
    fn bundle_public_context_labels_render_in_en_and_zh() {
        for (key, en, zh) in [
            (
                "trait-kit-context-configuration-load",
                "configuration load",
                "配置加载",
            ),
            (
                "trait-kit-context-configuration-load-serialize",
                "configuration load (serialize)",
                "配置加载（序列化）",
            ),
            (
                "trait-kit-context-configuration-load-deserialize",
                "configuration load (deserialize)",
                "配置加载（反序列化）",
            ),
            (
                "trait-kit-context-configuration-load-validated",
                "configuration load (validated)",
                "配置加载（已校验）",
            ),
            (
                "trait-kit-context-configuration-reload",
                "configuration reload",
                "配置重载",
            ),
            (
                "trait-kit-context-encrypted-config-write",
                "encrypted configuration write",
                "加密配置写入",
            ),
            (
                "trait-kit-context-encrypted-config-read",
                "encrypted configuration read",
                "加密配置读取",
            ),
            (
                "trait-kit-context-master-key-rotation",
                "master key rotation",
                "主密钥轮换",
            ),
            (
                "trait-kit-context-master-key-rotation-old-key",
                "master key rotation (old key)",
                "主密钥轮换（旧密钥）",
            ),
            (
                "trait-kit-context-master-key-rotation-new-key",
                "master key rotation (new key)",
                "主密钥轮换（新密钥）",
            ),
            (
                "trait-kit-context-readiness-hook",
                "readiness hook",
                "就绪钩子",
            ),
            ("trait-kit-context-shutdown", "shutdown", "关闭"),
            (
                "trait-kit-context-hook-registration",
                "hook registration",
                "钩子注册",
            ),
            (
                "trait-kit-context-master-key-provider",
                "master key provider",
                "主密钥供给方",
            ),
        ] {
            assert_eq!(
                format_from_bundle("en", key, &[]).as_deref(),
                Some(en),
                "{key}: en bundle degraded to the raw key"
            );
            assert_eq!(
                format_from_bundle("zh", key, &[]).as_deref(),
                Some(zh),
                "{key}: zh bundle degraded to the raw key"
            );
        }
    }

    /// 守卫：配置/密钥类错误文案在 EN/ZH 束中按预期渲染。
    #[test]
    fn bundle_config_and_key_error_keys_render_in_en_and_zh() {
        for (key, args, en, zh) in [
            (
                "trait-kit-error-master-key-too-short",
                &[
                    ("key_label", "master_key"),
                    ("min_len", "32"),
                    ("actual", "5"),
                ][..],
                "master_key must be at least 32 bytes, got 5",
                "`master_key` 至少需要 32 字节，实际 5 字节",
            ),
            (
                "trait-kit-error-master-key-weak-material",
                &[("key_label", "master_key")][..],
                "master_key must not be constant-byte key material",
                "`master_key` 不能是常量字节密钥材料",
            ),
            (
                "trait-kit-error-config-nesting-depth-exceeded",
                &[("max_depth", "32")][..],
                "config nesting depth exceeds 32 levels",
                "配置嵌套深度超过 32 层",
            ),
            (
                "trait-kit-error-key-version-mismatch",
                &[
                    ("key", "DbConfig"),
                    ("stored", "5"),
                    ("expected_version", "7"),
                ][..],
                "key-version mismatch for `DbConfig`: stored v5, expected v7",
                "`DbConfig` 的密钥版本不匹配: 已存 v5，期望 v7",
            ),
            (
                "trait-kit-error-snapshot-downcast-failed",
                &[("key", "DbConfig")][..],
                "DbConfig (snapshot downcast)",
                "DbConfig（快照类型转换失败）",
            ),
            (
                "i18n-error-number-not-finite",
                &[][..],
                "value is not finite (NaN or Infinity)",
                "数值不是有限数（NaN 或 Infinity）",
            ),
            (
                "trait-kit-health-capability-not-found",
                &[][..],
                "capability not found",
                "未找到能力",
            ),
        ] {
            assert_eq!(
                format_from_bundle("en", key, args).as_deref(),
                Some(en),
                "{key}: en bundle degraded to the raw key"
            );
            assert_eq!(
                format_from_bundle("zh", key, args).as_deref(),
                Some(zh),
                "{key}: zh bundle degraded to the raw key"
            );
        }
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
            &[
                "ovk-a = Alpha { $n }\novk-b = Beta",
                "ovk-a = Alpha2 { $n }",
            ],
        );
        assert_eq!(
            catalog.translate("ovk-a", &[("n", "1")]),
            "Alpha2 1",
            "later fragments with the same message id must override earlier ones"
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

    /// 守卫：默认初始化严格输出英文（不随宿主系统语言漂移）。
    ///
    /// 冻结 r2 契约：`init()` 不检测系统 locale；语言协商必须显式调用
    /// `init_with_locale` / `init_with_system_locale`。
    #[test]
    #[serial(i18n_global)]
    fn manager_init_defaults_to_english() {
        let mgr = I18nManager::init();
        assert_eq!(mgr.locale_tag(), "en");
        assert_eq!(
            mgr.translate("trait-kit-error-already-registered", &[("module", "m")]),
            "module `m` is already registered"
        );
    }

    /// 守卫：系统语言协商入口复用既有全局实例（幂等、不重建）。
    ///
    /// 先显式锚定 en，断言与宿主 locale 无关：无论检测结果为何，
    /// `init_with_system_locale()` 都必须返回同一个全局实例。
    #[cfg(feature = "i18n")]
    #[test]
    #[serial(i18n_global)]
    fn manager_init_with_system_locale_reuses_global_instance() {
        let pinned = I18nManager::init_with_locale("en");
        let negotiated = I18nManager::init_with_system_locale();
        assert!(
            std::ptr::eq(pinned, negotiated),
            "system-locale entry must reuse the existing global instance"
        );
        assert_eq!(negotiated.locale_tag(), "en");
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

    // ─── 宿主注册翻译测试 ─────────────────────────────────────────────────
    //
    // 注册表是进程级可变状态（与 GLOBAL_I18N 同族），统一
    // #[serial(i18n_global)] 串行；每个用例自清自理（首尾各 clear 一次），
    // 不依赖串行组内执行顺序。

    #[test]
    #[serial(i18n_global)]
    fn register_translation_overrides_builtin() {
        I18nManager::clear_registered_translations();
        let mgr = I18nManager::build("en");
        let builtin = "module `m` is already registered";
        assert_eq!(
            mgr.translate("trait-kit-error-already-registered", &[("module", "m")]),
            builtin
        );
        I18nManager::register_translation(
            "en",
            "trait-kit-error-already-registered",
            "host override: { $module }",
        );
        assert_eq!(
            mgr.translate("trait-kit-error-already-registered", &[("module", "m")]),
            "host override: m"
        );
        I18nManager::clear_registered_translations();
    }

    #[test]
    #[serial(i18n_global)]
    fn register_translation_normalizes_locale_to_buckets() {
        I18nManager::clear_registered_translations();
        // zh-CN（含大写 ZH 前缀）归 zh 桶；未知语言归 en 桶。
        I18nManager::register_translation("zh-CN", "reg-loc-a", "中文值");
        I18nManager::register_translation("fr-FR", "reg-loc-b", "french text lands in en");
        assert_eq!(
            I18nManager::build("zh").translate("reg-loc-a", &[]),
            "中文值"
        );
        assert_eq!(
            I18nManager::build("en").translate("reg-loc-b", &[]),
            "french text lands in en"
        );
        // en 目录下查 zh 注册键：链路后段（注册[en]/内置[en]）均未命中
        // → 返 key 本身，注册不跨桶串味。
        assert_eq!(
            I18nManager::build("en").translate("reg-loc-a", &[]),
            "reg-loc-a"
        );
        I18nManager::clear_registered_translations();
    }

    #[test]
    #[serial(i18n_global)]
    fn register_translation_last_write_wins() {
        I18nManager::clear_registered_translations();
        I18nManager::register_translation("en", "reg-lww", "first");
        I18nManager::register_translation("en", "reg-lww", "second");
        assert_eq!(I18nManager::build("en").translate("reg-lww", &[]), "second");
        I18nManager::clear_registered_translations();
    }

    #[test]
    #[serial(i18n_global)]
    fn register_translation_fallback_chain_prefers_builtin_over_registered_en() {
        I18nManager::clear_registered_translations();
        // 仅注册 en 覆盖内置已有键：zh 目录查询仍取内置 zh（内置[zh]
        // 先于注册[en]），en 目录查询取注册覆盖。
        I18nManager::register_translation(
            "en",
            "trait-kit-error-already-registered",
            "registered-en override",
        );
        let zh = I18nManager::build("zh");
        assert!(
            zh.translate("trait-kit-error-already-registered", &[("module", "m")])
                .contains("已注册"),
            "builtin zh must win over registered en for the zh catalog"
        );
        assert_eq!(
            I18nManager::build("en").translate("trait-kit-error-already-registered", &[]),
            "registered-en override"
        );
        I18nManager::clear_registered_translations();
        // 仅注册 en 的新键（无内置条目）：zh 目录查询经注册[en] 兜底命中。
        I18nManager::register_translation("en", "reg-chain-new", "en-only value");
        assert_eq!(
            I18nManager::build("zh").translate("reg-chain-new", &[]),
            "en-only value"
        );
        I18nManager::clear_registered_translations();
    }

    #[test]
    #[serial(i18n_global)]
    fn clear_registered_translations_restores_builtin_and_key_fallback() {
        I18nManager::clear_registered_translations();
        I18nManager::register_translation("en", "trait-kit-error-already-registered", "gone");
        I18nManager::clear_registered_translations();
        assert_eq!(
            I18nManager::build("en")
                .translate("trait-kit-error-already-registered", &[("module", "m")]),
            "module `m` is already registered",
            "builtin message must translate again after clear"
        );
        assert_eq!(
            I18nManager::build("en").translate("reg-cleared-missing", &[]),
            "reg-cleared-missing",
            "unknown key must fall back to the key itself after clear"
        );
    }

    #[test]
    #[serial(i18n_global)]
    fn register_translation_placeholder_parity_with_builtin() {
        I18nManager::clear_registered_translations();
        // 同一占位符形态与参数：注册路径与内置 Fluent 路径同样完成替换、
        // 不留花括号（间距形态 `{ $x }` / `{$x}` 均识别）。
        I18nManager::register_translation(
            "en",
            "reg-parity-spaced",
            "module `{ $module }` is taken",
        );
        I18nManager::register_translation(
            "en",
            "reg-parity-compact",
            "module `{$module}` is taken",
        );
        let builtin = I18nManager::build("en")
            .translate("trait-kit-error-already-registered", &[("module", "m")]);
        let spaced = I18nManager::build("en").translate("reg-parity-spaced", &[("module", "m")]);
        let compact = I18nManager::build("en").translate("reg-parity-compact", &[("module", "m")]);
        assert_eq!(builtin, "module `m` is already registered");
        assert_eq!(spaced, "module `m` is taken");
        assert_eq!(compact, "module `m` is taken");
        // 单遍替换：参数值中的 `{ $module }` 按字面插入，不再解析
        // （与内置路径 bundle_arg_value_is_not_reparsed_as_pattern 同语义）。
        let injected = I18nManager::build("en")
            .translate("reg-parity-spaced", &[("module", "{ $module } injected")]);
        assert_eq!(injected, "module `{ $module } injected` is taken");
        // 未知占位符保留原文；非占位符花括号原样输出。
        I18nManager::register_translation(
            "en",
            "reg-parity-unknown",
            "keep { $nope } and {literal}",
        );
        assert_eq!(
            I18nManager::build("en").translate("reg-parity-unknown", &[("x", "1")]),
            "keep { $nope } and {literal}"
        );
        I18nManager::clear_registered_translations();
    }

    #[test]
    #[serial(i18n_global)]
    fn register_translation_resolves_for_both_language_buckets() {
        I18nManager::clear_registered_translations();
        // 同一键在两个语言桶各注册一条：en/zh 目录都能解析（键集一致，
        // 不依赖内置键，注册面自身即双语闭环）。
        I18nManager::register_translation("en", "reg-both", "english value");
        I18nManager::register_translation("zh", "reg-both", "中文值");
        assert_eq!(
            I18nManager::build("en").translate("reg-both", &[]),
            "english value"
        );
        assert_eq!(
            I18nManager::build("zh").translate("reg-both", &[]),
            "中文值"
        );
        I18nManager::clear_registered_translations();
    }

    // ─── I18nFormatter 测试 ─────────────────────────────────────────────────

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
            "'ZH' input must select the zh catalog: got '{}'",
            upper.translate("trait-kit-error-already-registered", &[])
        );
    }
}

#[cfg(test)]
mod catalog_debug_tests {
    use super::I18nManager;
    #[cfg(feature = "i18n")]
    use super::MessageCatalog;

    /// `MessageCatalog` 的 Debug 走 Global 分支（FluentBundle 未实现
    /// Debug，只输出形态与语言摘要）。
    #[test]
    fn manager_debug_renders_catalog_summary() {
        let mgr = I18nManager::init();
        let debug = format!("{mgr:?}");
        assert!(
            debug.contains("MessageCatalog"),
            "catalog in debug: {debug}"
        );
        assert!(debug.contains("lang"), "lang summary in debug: {debug}");
    }

    /// Overlay 目录（parse 构造）走同一 Debug 的另一分支。
    #[cfg(feature = "i18n")]
    #[test]
    fn overlay_catalog_debug_reports_kind_without_lang() {
        let overlay = MessageCatalog::parse("zh", &["greet = 你好"]);
        let debug = format!("{overlay:?}");
        assert!(debug.contains("overlay"), "overlay kind in debug: {debug}");
        assert!(!debug.contains("lang"), "overlay exposes no lang field");
        assert_eq!(overlay.translate("greet", &[]), "你好");
    }
}
