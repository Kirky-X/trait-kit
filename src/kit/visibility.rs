// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Capability visibility — restrict which caller modules may `require` a
//! capability (feature `visibility`).
//!
//! Visibility is a soft in-process boundary: it prevents misuse and
//! unauthorized retrieval by plugins or tenant modules that hold a
//! `Kit<Ready>` reference. It is not a defense against adversarial memory
//! access — that is the job of OS/hardware boundaries.

/// Declares which callers may require a module's capability.
///
/// Patterns are matched against `ModuleMeta::NAME` values: a pattern
/// ending in `*` matches by prefix; otherwise the match is exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityVisibility {
    /// Any holder of `Kit<Ready>` may require the capability (default).
    Public,
    /// Only callers whose `ModuleMeta::NAME` matches one of the patterns
    /// may require the capability via `require_for` / `require_as`.
    Restricted(&'static [&'static str]),
}

impl CapabilityVisibility {
    /// Returns `true` when `caller` may retrieve a capability governed by
    /// this visibility.
    ///
    /// `Public` allows any caller. `Restricted` allows the caller when its
    /// name matches at least one pattern: a pattern ending in `*` matches
    /// by prefix, any other pattern must equal the caller name exactly.
    #[must_use]
    pub fn allows(&self, caller: &str) -> bool {
        match self {
            Self::Public => true,
            Self::Restricted(patterns) => patterns.iter().any(|pattern| {
                if let Some(prefix) = pattern.strip_suffix('*') {
                    caller.starts_with(prefix)
                } else {
                    caller == *pattern
                }
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_allows_any_caller() {
        let vis = CapabilityVisibility::Public;
        assert!(vis.allows("audit::AuditModule"));
        assert!(vis.allows(""));
        assert!(vis.allows("anything::Else"));
    }

    #[test]
    fn restricted_exact_pattern_hits_and_misses() {
        let vis = CapabilityVisibility::Restricted(&["audit::AuditModule"]);
        assert!(vis.allows("audit::AuditModule"));
        assert!(!vis.allows("audit::OtherModule"));
        assert!(!vis.allows("data::DbPool"));
    }

    #[test]
    fn restricted_prefix_pattern_hits_prefix_only() {
        let vis = CapabilityVisibility::Restricted(&["data::*"]);
        assert!(vis.allows("data::DbPool"));
        // Shared prefix beyond the `::` separator must not match.
        assert!(!vis.allows("daemons::X"));
        assert!(!vis.allows("data"));
    }

    #[test]
    fn restricted_empty_patterns_deny_everything() {
        let vis = CapabilityVisibility::Restricted(&[]);
        assert!(!vis.allows("audit::AuditModule"));
    }
}
