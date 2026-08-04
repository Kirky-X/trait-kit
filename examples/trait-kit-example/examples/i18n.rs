// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! i18n feature — ICU4X-backed locale-aware formatting.
//!
//! Demonstrates:
//! - `I18nFormatter::new(locale)` — create a locale-aware formatter
//! - `format_number()` — locale-sensitive number formatting
//! - `format_date()` — locale-sensitive date formatting
//! - `plural_category()` — plural rules for a locale
//! - `compare()` — locale-sensitive string collation
//!
//! Run: `cargo run -p trait-kit-example --example i18n --features i18n`

use trait_kit::i18n::I18nFormatter;

fn main() {
    // ─── English (US) ───────────────────────────────────────────────────
    let fmt_en = I18nFormatter::new("en-US").expect("en-US locale");

    let number = fmt_en.format_number(1_234_567.89).expect("format number");
    println!("en-US number: {number}");
    assert!(number.contains(','), "en-US should use comma thousands separator");

    let date = fmt_en.format_date(2026, 7, 11).expect("format date");
    println!("en-US date: {date}");
    assert!(date.contains("2026"), "date should contain year");

    // ─── Chinese (Simplified) ───────────────────────────────────────────
    let fmt_zh = I18nFormatter::new("zh-CN").expect("zh-CN locale");

    let number_zh = fmt_zh.format_number(42_000.5).expect("format number zh");
    println!("zh-CN number: {number_zh}");

    // ─── Plural rules ───────────────────────────────────────────────────
    let fmt_plural = I18nFormatter::new("en").expect("en locale");
    let one = fmt_plural.plural_category(1).expect("plural(1)");
    let other = fmt_plural.plural_category(5).expect("plural(5)");
    println!("en plural: 1 -> {one:?}, 5 -> {other:?}");

    // ─── String collation ───────────────────────────────────────────────
    let fmt_collator = I18nFormatter::new("en").expect("en locale");
    let cmp = fmt_collator.compare("apple", "banana").expect("compare");
    println!("en collation: 'apple' vs 'banana' -> {cmp:?}");
    assert!(cmp.is_lt(), "apple should sort before banana");

    // ─── Error handling ─────────────────────────────────────────────────
    let bad = I18nFormatter::new("not-a-locale!!!");
    assert!(bad.is_err(), "invalid locale should return error");

    let nan_result = fmt_en.format_number(f64::NAN);
    assert!(nan_result.is_err(), "NaN should return error");

    println!("i18n: OK");
}
