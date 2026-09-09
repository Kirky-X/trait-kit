// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Implementation of [`I18nFormatter`] methods.

use std::cmp::Ordering;
use std::str::FromStr;

use icu::collator::Collator;
use icu::collator::options::CollatorOptions;
use icu::datetime::DateTimeFormatter;
use icu::datetime::fieldsets::YMD;
use icu::datetime::input::Date;
use icu::decimal::DecimalFormatter;
use icu::decimal::input::Decimal;
use icu::decimal::options::DecimalFormatterOptions;
use icu::locale::Locale;
use icu::plurals::{PluralCategory, PluralRules, PluralRulesOptions};
use writeable::Writeable;

use super::{I18nError, I18nFormatter};

impl I18nFormatter {
    /// Create a new formatter for the given BCP-47 locale tag.
    ///
    /// # Errors
    /// Returns [`I18nError::InvalidLocale`] if the tag cannot be parsed,
    /// or [`I18nError::FormatError`] if ICU4X lacks compiled data for it.
    pub fn new(locale: &str) -> Result<Self, I18nError> {
        let parsed = Locale::from_str(locale).map_err(|e| I18nError::InvalidLocale {
            input: locale.to_string(),
            reason: e.to_string(),
        })?;

        let decimal_formatter =
            DecimalFormatter::try_new((&parsed).into(), DecimalFormatterOptions::default())
                .map_err(|e| I18nError::FormatError(e.to_string()))?;

        let plural_rules = PluralRules::try_new((&parsed).into(), PluralRulesOptions::default())
            .map_err(|e| I18nError::FormatError(e.to_string()))?;

        let collator = Collator::try_new((&parsed).into(), CollatorOptions::default())
            .map_err(|e| I18nError::FormatError(e.to_string()))?;

        let date_formatter = DateTimeFormatter::try_new((&parsed).into(), YMD::medium())
            .map_err(|e| I18nError::FormatError(e.to_string()))?;

        Ok(Self {
            locale: parsed,
            decimal_formatter,
            plural_rules,
            collator,
            date_formatter,
        })
    }

    /// Format a floating-point number with locale-sensitive grouping
    /// and decimal separators.
    ///
    /// # Errors
    /// Returns [`I18nError::InvalidNumber`] for non-finite values or
    /// if the value cannot be parsed into a fixed decimal.
    pub fn format_number(&self, value: f64) -> Result<String, I18nError> {
        if !value.is_finite() {
            return Err(I18nError::InvalidNumber {
                input: value.to_string(),
                reason: "value is not finite (NaN or Infinity)".into(),
            });
        }
        // f64 的 `Display` 输出最短往返（shortest round-trip）的定点表示：
        // 0.3 → "0.3"（不泄漏二进制展开），且定点表示永不产生科学计数法
        // （`{:e}` 才会），因此 `Decimal::from_str` 可直接解析；最短表示
        // 本身无尾零，无需修剪。
        let repr = format!("{value}");
        let decimal = Decimal::from_str(&repr).map_err(|e| I18nError::InvalidNumber {
            input: repr.clone(),
            reason: e.to_string(),
        })?;
        let formatted = self.decimal_formatter.format(&decimal);
        Ok(formatted.write_to_string().into_owned())
    }

    /// Format an ISO calendar date (year / month / day) using a medium
    /// length locale-specific pattern.
    ///
    /// # Errors
    /// Returns [`I18nError::DateError`] if any component is out of range,
    /// or [`I18nError::FormatError`] if the formatter cannot be constructed.
    pub fn format_date(&self, year: i32, month: u8, day: u8) -> Result<String, I18nError> {
        let date =
            Date::try_new_iso(year, month, day).map_err(|e| I18nError::DateError(e.to_string()))?;

        // YMD 为 date-only fieldset，可直接格式化 Date，无需包装午夜 Time。
        // formatter 在 `new()` 中 eagerly 创建（与文档承诺一致）。
        let formatted = self.date_formatter.format(&date);
        Ok(formatted.write_to_string().into_owned())
    }

    /// Return the plural category for `count` in the formatter's locale.
    ///
    /// # Errors
    /// This method does not currently fail, but returns `Result` for API
    /// consistency with the other formatting methods.
    pub fn plural_category(&self, count: u64) -> Result<PluralCategory, I18nError> {
        Ok(self.plural_rules.category_for(count))
    }

    /// Compare two strings using locale-sensitive collation rules.
    ///
    /// # Errors
    /// This method does not currently fail, but returns `Result` for API
    /// consistency with the other formatting methods.
    pub fn compare(&self, a: &str, b: &str) -> Result<Ordering, I18nError> {
        Ok(self.collator.compare(a, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_number_shortest_roundtrip_no_binary_noise() {
        // 旧实现 `format!("{value:.20}")` 会泄漏 f64 二进制展开
        // （0.3 → "0.2999999999999999889..."）；Display 为最短往返表示。
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_number(0.3).expect("format 0.3");
        assert_eq!(result, "0.3", "0.3 must format as shortest round-trip repr");
        assert!(
            !result.contains("2999"),
            "binary expansion must not leak: got '{result}'"
        );
    }

    #[test]
    fn format_number_tiny_value_does_not_collapse_to_zero() {
        // 旧实现对 1e-300 级小值经 ".20" 截断 + 尾零修剪后输出 "0"；
        // Display 定点展开保留全部有效位（en-US 分组只作用于整数部分）。
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        let result = fmt.format_number(1e-30).expect("format 1e-30");
        assert_ne!(result, "0", "1e-30 must not collapse to 0: got '{result}'");
        assert!(
            result.starts_with("0."),
            "fixed-point output must not use scientific notation: got '{result}'"
        );
        assert!(
            result.ends_with('1'),
            "least significant digit must survive: got '{result}'"
        );
    }

    #[test]
    fn format_number_exact_grouping_and_decimals() {
        let fmt = I18nFormatter::new("en-US").expect("en-US locale");
        assert_eq!(
            fmt.format_number(1_234_567.89).expect("format 1234567.89"),
            "1,234,567.89"
        );
        assert_eq!(fmt.format_number(42.0).expect("format 42.0"), "42");
    }
}
