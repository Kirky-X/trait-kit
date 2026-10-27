// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! i18n — Fluent 消息翻译 + ICU4X 本地化格式化。
//!
//! 演示：
//! - `I18nManager::init` — 默认英文（严格，不随宿主系统语言漂移）
//! - `I18nManager::init_with_system_locale` — 按系统语言协商（opt-in）
//! - `I18nManager::init_with_locale` — 使用指定 locale 初始化
//! - `tr` — 便捷翻译函数
//! - `I18nFormatter::new` — ICU4X locale 感知格式化
//! - `TraitKitError` 的 `Display` 输出自动本地化
//!
//! Run: `cargo run -p trait-kit-examples --example i18n`

use trait_kit::TraitKitError;
use trait_kit::i18n::{I18nFormatter, I18nManager, tr};

fn main() {
    // 全局 I18nManager 初始化
    // 按系统语言环境协商（如 zh-CN、en-US）；默认 init 为严格英文。
    let mgr = I18nManager::init_with_system_locale();
    println!(
        "{}",
        tr(
            "trait-kit-example-i18n-system-locale",
            &[("locale", mgr.locale_tag())]
        )
    );

    // 消息翻译（tr 便捷函数）
    let msg = tr(
        "trait-kit-error-already-registered",
        &[("module", "my-module")],
    );
    println!(
        "{}",
        tr(
            "trait-kit-example-i18n-translated-message",
            &[("message", &msg)]
        )
    );

    // 指定 locale 翻译
    // 注意：全局实例已初始化，init_with_locale 会直接返回现有实例；
    // 这里演示通过 I18nManager 实例直接翻译
    let en_msg = mgr.translate(
        "trait-kit-error-missing-capability",
        &[("key", "database-pool")],
    );
    println!(
        "{}",
        tr(
            "trait-kit-example-i18n-en-translation",
            &[("message", &en_msg)]
        )
    );

    // TraitKitError Display 自动本地化
    let err = TraitKitError::AlreadyRegistered {
        module: "auth-service",
    };
    println!(
        "{}",
        tr(
            "trait-kit-example-i18n-error-message",
            &[("error", &err.to_string())]
        )
    );

    let err2 = TraitKitError::MissingCapability {
        key: "cache-backend".into(),
    };
    println!(
        "{}",
        tr(
            "trait-kit-example-i18n-error-message",
            &[("error", &err2.to_string())]
        )
    );

    // ICU4X 本地化格式化
    // 英文 (US)
    let fmt_en = I18nFormatter::new("en-US").expect("en-US locale");
    let number = fmt_en.format_number(1_234_567.89).expect("format number");
    println!(
        "{}",
        tr(
            "trait-kit-example-i18n-en-us-number",
            &[("number", &number)]
        )
    );

    let date = fmt_en.format_date(2026, 7, 11).expect("format date");
    println!(
        "{}",
        tr("trait-kit-example-i18n-en-us-date", &[("date", &date)])
    );

    // 中文 (简体)
    let fmt_zh = I18nFormatter::new("zh-CN").expect("zh-CN locale");
    let number_zh = fmt_zh.format_number(42_000.5).expect("format number zh");
    println!(
        "{}",
        tr(
            "trait-kit-example-i18n-zh-cn-number",
            &[("number", &number_zh)]
        )
    );

    // 复数规则
    let fmt_plural = I18nFormatter::new("en").expect("en locale");
    let one = fmt_plural.plural_category(1).expect("plural(1)");
    let other = fmt_plural.plural_category(5).expect("plural(5)");
    let one_label = format!("{one:?}");
    let other_label = format!("{other:?}");
    println!(
        "{}",
        tr(
            "trait-kit-example-i18n-en-plural",
            &[("one", &one_label), ("other", &other_label)]
        )
    );

    // 字符串排序
    let fmt_collator = I18nFormatter::new("en").expect("en locale");
    let cmp = fmt_collator.compare("apple", "banana").expect("compare");
    let cmp_label = format!("{cmp:?}");
    println!(
        "{}",
        tr(
            "trait-kit-example-i18n-en-collation",
            &[("cmp", &cmp_label)]
        )
    );
    assert!(cmp.is_lt(), "apple should sort before banana");

    // 错误处理
    let bad = I18nFormatter::new("not-a-locale!!!");
    assert!(bad.is_err(), "invalid locale should return error");

    let nan_result = fmt_en.format_number(f64::NAN);
    assert!(nan_result.is_err(), "NaN should return error");

    println!("\ni18n: OK");
}
