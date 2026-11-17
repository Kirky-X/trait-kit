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

/// Runtime policy hook on the require path (feature `guard`).
///
/// Implemented by ecosystem crates — e.g. limiteron (tenant not
/// rate-limited) or garrison (session authenticated) — this crate only
/// defines the trait and never depends on them. Unlike
/// [`CapabilityVisibility`] (a static boundary declared at registration),
/// a guard consults runtime context on every guarded retrieval.
pub trait CapabilityGuard: Send + Sync {
    /// Decide whether `caller` may retrieve the capability of the module
    /// named `target` (`ModuleMeta::NAME`).
    fn allow(&self, caller: &'static str, target: &'static str) -> bool;
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

/// `CapabilityGuard` 参数透传与判定（feature `guard`，R-guard-001）。
#[cfg(all(test, feature = "guard"))]
mod guard_tests {
    use super::*;

    /// 记录调用参数的 mock guard。
    struct RecordingGuard {
        allowed_caller: &'static str,
        seen: std::sync::Mutex<Vec<(&'static str, &'static str)>>,
    }

    impl CapabilityGuard for RecordingGuard {
        fn allow(&self, caller: &'static str, target: &'static str) -> bool {
            self.seen.lock().expect("lock").push((caller, target));
            caller == self.allowed_caller
        }
    }

    #[test]
    fn guard_receives_caller_and_target_verbatim() {
        let guard = RecordingGuard {
            allowed_caller: "audit::AuditModule",
            seen: std::sync::Mutex::new(Vec::new()),
        };
        assert!(guard.allow("audit::AuditModule", "data::Database"));
        assert!(!guard.allow("plugin::PluginModule", "data::Database"));
        let seen = guard.seen.lock().expect("lock");
        assert_eq!(
            *seen,
            vec![
                ("audit::AuditModule", "data::Database"),
                ("plugin::PluginModule", "data::Database"),
            ],
            "caller/target must be forwarded verbatim"
        );
    }
}
