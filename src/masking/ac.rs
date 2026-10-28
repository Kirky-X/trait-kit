// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Aho-Corasick accelerated masking path.
//!
//! When the `fast-masking` feature is enabled, [`AcMasker`] provides single-pass
//! multi-pattern replacement for fixed-string masking rules, outperforming
//! per-rule regex replacement when many literal patterns are present.

use aho_corasick::{AhoCorasick, MatchKind};

/// Aho-Corasick based fast masker for fixed-string patterns.
///
/// Performs a single scan of the input, replacing all matching patterns
/// simultaneously. This is O(n + m) where n = input length and m = total
/// match length, compared to O(n * k) for k sequential regex passes.
#[derive(Debug, Clone)]
pub struct AcMasker {
    automaton: AhoCorasick,
    replacements: Vec<String>,
}

impl AcMasker {
    /// Build a new `AcMasker` from `(pattern, replacement)` pairs.
    ///
    /// Patterns are matched as literal strings (no regex).
    /// Longer patterns are matched first to avoid partial replacement conflicts.
    ///
    /// # Errors
    ///
    /// Returns `None` if `patterns` is empty or the automaton fails to build.
    #[must_use]
    pub fn new(patterns: Vec<String>, replacements: Vec<String>) -> Option<Self> {
        if patterns.is_empty() || patterns.len() != replacements.len() {
            return None;
        }

        // Sort by length descending so longer patterns take priority in overlap.
        let mut paired: Vec<(String, String)> = patterns.into_iter().zip(replacements).collect();
        paired.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));

        let sorted_replacements: Vec<String> = paired.iter().map(|(_, r)| r.clone()).collect();
        let sorted_pattern_strs: Vec<&str> = paired.iter().map(|(p, _)| p.as_str()).collect();

        let automaton = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&sorted_pattern_strs)
            .ok()?;

        Some(Self {
            automaton,
            replacements: sorted_replacements,
        })
    }

    /// Perform single-pass masking on `input`.
    ///
    /// All known patterns are replaced in one scan.
    #[must_use]
    pub fn mask_fast(&self, input: &str) -> String {
        self.automaton.replace_all(input, &self.replacements)
    }

    /// Returns the number of patterns in the automaton.
    #[must_use]
    pub fn pattern_count(&self) -> usize {
        self.replacements.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac_masker_basic() {
        let masker = AcMasker::new(
            vec!["secret".into(), "password".into()],
            vec!["***".into(), "###".into()],
        )
        .unwrap();

        assert_eq!(masker.mask_fast("my secret value"), "my *** value");
        assert_eq!(masker.mask_fast("password=abc"), "###=abc");
    }

    #[test]
    fn test_ac_masker_multiple_matches() {
        let masker = AcMasker::new(
            vec!["foo".into(), "bar".into()],
            vec!["***".into(), "###".into()],
        )
        .unwrap();

        assert_eq!(
            masker.mask_fast("foo and bar and foo again"),
            "*** and ### and *** again"
        );
    }

    #[test]
    fn test_ac_masker_no_match() {
        let masker = AcMasker::new(vec!["xyz".into()], vec!["***".into()]).unwrap();
        assert_eq!(masker.mask_fast("hello world"), "hello world");
    }

    #[test]
    fn test_ac_masker_empty_input() {
        let masker = AcMasker::new(vec!["a".into()], vec!["b".into()]).unwrap();
        assert_eq!(masker.mask_fast(""), "");
    }

    #[test]
    fn test_ac_masker_empty_patterns_returns_none() {
        assert!(AcMasker::new(vec![], vec![]).is_none());
    }

    #[test]
    fn test_ac_masker_mismatched_lengths_returns_none() {
        assert!(AcMasker::new(vec!["a".into()], vec!["b".into(), "c".into()]).is_none());
    }

    #[test]
    fn test_ac_masker_longer_pattern_priority() {
        // "abcdef" contains "abc" — longer pattern should win
        let masker = AcMasker::new(
            vec!["abc".into(), "abcdef".into()],
            vec!["SHORT".into(), "LONG".into()],
        )
        .unwrap();

        let result = masker.mask_fast("abcdef");
        // AcMasker sorts by length desc, so "abcdef" is tried first
        assert_eq!(result, "LONG");
    }

    #[test]
    fn test_ac_masker_pattern_count() {
        let masker = AcMasker::new(
            vec!["a".into(), "b".into(), "c".into()],
            vec!["1".into(), "2".into(), "3".into()],
        )
        .unwrap();
        assert_eq!(masker.pattern_count(), 3);
    }

    #[test]
    fn test_ac_masker_special_chars_as_literal() {
        // AC treats patterns as literal — regex metacharacters are not special
        let masker = AcMasker::new(vec!["user@example.com".into()], vec!["***".into()]).unwrap();
        assert_eq!(masker.mask_fast("email: user@example.com"), "email: ***");
    }
}
