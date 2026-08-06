// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! i18n — Fluent 消息翻译 + ICU4X 本地化格式化。
//!
//! 演示：
//! - `I18nManager::init()` — 自动检测系统语言环境并初始化
//! - `I18nManager::init_with_locale()` — 使用指定 locale 初始化
//! - `tr()` — 便捷翻译函数
//! - `I18nFormatter::new()` — ICU4X locale 感知格式化
//! - `TraitKitError` 的 `Display` 输出自动本地化
//!
//! Run: `cargo run -p trait-kit-example --example i18n`

use trait_kit::i18n::{tr, I18nFormatter, I18nManager};
use trait_kit::TraitKitError;

fn main() {
    // ─── 全局 I18nManager 初始化 ─────────────────────────────────────────
    // 自动检测系统语言环境（如 zh-CN、en-US）
    let mgr = I18nManager::init();
    println!("系统 locale: {}", mgr.locale_tag());

    // ─── 消息翻译（tr 便捷函数） ─────────────────────────────────────────
    let msg = tr(
        "trait-kit-error-already-registered",
        &[("module", "my-module")],
    );
    println!("翻译结果: {msg}");

    // ─── 指定 locale 翻译 ────────────────────────────────────────────────
    // 注意：全局实例已初始化，init_with_locale 会返回错误（已初始化）
    // 这里演示通过 I18nManager 实例直接翻译
    let en_msg = mgr.translate(
        "trait-kit-error-missing-capability",
        &[("key", "database-pool")],
    );
    println!("英文翻译: {en_msg}");

    // ─── TraitKitError Display 自动本地化 ────────────────────────────────
    let err = TraitKitError::AlreadyRegistered {
        module: "auth-service",
    };
    println!("错误消息: {err}");

    let err2 = TraitKitError::MissingCapability {
        key: "cache-backend".into(),
    };
    println!("错误消息: {err2}");

    // ─── ICU4X 本地化格式化 ──────────────────────────────────────────────
    // 英文 (US)
    let fmt_en = I18nFormatter::new("en-US").expect("en-US locale");
    let number = fmt_en.format_number(1_234_567.89).expect("format number");
    println!("en-US 数字: {number}");

    let date = fmt_en.format_date(2026, 7, 11).expect("format date");
    println!("en-US 日期: {date}");

    // 中文 (简体)
    let fmt_zh = I18nFormatter::new("zh-CN").expect("zh-CN locale");
    let number_zh = fmt_zh.format_number(42_000.5).expect("format number zh");
    println!("zh-CN 数字: {number_zh}");

    // ─── 复数规则 ────────────────────────────────────────────────────────
    let fmt_plural = I18nFormatter::new("en").expect("en locale");
    let one = fmt_plural.plural_category(1).expect("plural(1)");
    let other = fmt_plural.plural_category(5).expect("plural(5)");
    println!("en 复数: 1 -> {one:?}, 5 -> {other:?}");

    // ─── 字符串排序 ──────────────────────────────────────────────────────
    let fmt_collator = I18nFormatter::new("en").expect("en locale");
    let cmp = fmt_collator.compare("apple", "banana").expect("compare");
    println!("en 排序: 'apple' vs 'banana' -> {cmp:?}");
    assert!(cmp.is_lt(), "apple should sort before banana");

    // ─── 错误处理 ────────────────────────────────────────────────────────
    let bad = I18nFormatter::new("not-a-locale!!!");
    assert!(bad.is_err(), "invalid locale should return error");

    let nan_result = fmt_en.format_number(f64::NAN);
    assert!(nan_result.is_err(), "NaN should return error");

    println!("\ni18n: OK");
}
