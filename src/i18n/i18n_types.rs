// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Type definitions for ICU4X-backed internationalization formatting.

use icu::collator::CollatorBorrowed;
use icu::decimal::DecimalFormatter;
use icu::locale::Locale;
use icu::plurals::PluralRules;
use thiserror::Error;

/// Errors returned by [`I18nFormatter`] operations.
#[derive(Debug, Error)]
pub enum I18nError {
    /// BCP-47 locale string could not be parsed.
    #[error("invalid locale '{input}': {reason}")]
    InvalidLocale { input: String, reason: String },
    /// Number value could not be formatted (e.g. NaN, Infinity, or parse failure).
    #[error("invalid number '{input}': {reason}")]
    InvalidNumber { input: String, reason: String },
    /// Date component out of range or otherwise invalid.
    #[error("date error: {0}")]
    DateError(String),
    /// Underlying ICU4X data or formatting failure.
    #[error("formatting error: {0}")]
    FormatError(String),
}

/// Locale-aware formatter backed by ICU4X compiled data.
///
/// Construct with [`I18nFormatter::new`] using a BCP-47 locale tag
/// (e.g. `"en-US"`, `"zh-CN"`). All formatters are created eagerly so
/// that repeated formatting calls are allocation-light.
pub struct I18nFormatter {
    /// The parsed locale used by this formatter.
    pub(crate) locale: Locale,
    /// Decimal (number) formatter.
    pub(crate) decimal_formatter: DecimalFormatter,
    /// Plural rules for the locale.
    pub(crate) plural_rules: PluralRules,
    /// Collator for string comparison.
    pub(crate) collator: CollatorBorrowed<'static>,
}
