// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! ICU4X-backed internationalization formatting.
//!
//! Provides locale-aware number formatting, date formatting, plural rules,
//! and string collation via the `icu` crate (ICU4X 2.x).
//!
//! Enable with the `i18n` cargo feature:
//! ```toml
//! [dependencies]
//! trait-kit = { version = "...", features = ["i18n"] }
//! ```

mod i18n_impl;
mod i18n_types;

pub use i18n_types::{I18nError, I18nFormatter};

#[cfg(test)]
mod tests {
    use super::*;

    use std::cmp::Ordering;

    use icu::plurals::PluralCategory;

    #[test]
    fn test_locale_parsing_en() {
        let fmt = I18nFormatter::new("en-US");
        assert!(fmt.is_ok(), "en-US should parse successfully");
    }

    #[test]
    fn test_locale_parsing_zh() {
        let fmt = I18nFormatter::new("zh-CN");
        assert!(fmt.is_ok(), "zh-CN should parse successfully");
    }

    #[test]
    fn test_invalid_locale() {
        let result = I18nFormatter::new("not-a-valid-locale!!!");
        assert!(result.is_err(), "invalid locale should return error");
        match result.err().unwrap() {
            I18nError::InvalidLocale { input, .. } => assert_eq!(input, "not-a-valid-locale!!!"),
            other => panic!("expected InvalidLocale, got {other:?}"),
        }
    }

    #[test]
    fn test_format_number_en() {
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_number(1_234_567.89_f64).expect("format number");
        // en-US: thousands separator is comma, decimal separator is period
        // Expected: "1,234,567.89"
        assert!(
            result.contains(','),
            "en-US number should contain thousands separator: got '{result}'"
        );
        assert!(
            result.contains('.'),
            "en-US number should contain decimal point: got '{result}'"
        );
    }

    #[test]
    fn test_format_number_zh() {
        let fmt = I18nFormatter::new("zh-CN").expect("zh-CN locale");
        let result = fmt.format_number(1_234_567.89_f64).expect("format number");
        // zh-CN: thousands separator is comma, decimal separator is period
        // (Chinese uses Western number formatting in modern CLDR)
        assert!(
            !result.is_empty(),
            "zh-CN number should be non-empty: got '{result}'"
        );
    }

    #[test]
    fn test_format_number_not_finite() {
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        assert!(fmt.format_number(f64::NAN).is_err());
        assert!(fmt.format_number(f64::INFINITY).is_err());
    }

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
}
