// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Dependency policy — build-time architecture constraints on the module
//! dependency graph (feature `policy`).
//!
//! Positive validation (missing dependencies, cycles) lives in
//! [`crate::kit::graph::DependencyGraph::validate`]; this module adds the
//! negative direction: "module A must not depend on module B" and "every
//! module of kind K must depend on something of kind D" — enforced at
//! `Kit::build` before any module is constructed.

/// Module-name pattern used by policy edges.
///
/// Matched against `ModuleMeta::NAME` values: `Exact` requires equality,
/// `Prefix` matches by prefix, `Any` matches everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModulePattern {
    /// Matches the exact `ModuleMeta::NAME`.
    Exact(&'static str),
    /// Matches any name starting with the prefix (stored WITHOUT the
    /// trailing `*`).
    Prefix(&'static str),
    /// Matches any module.
    Any,
}

impl ModulePattern {
    /// Returns `true` when `name` matches this pattern.
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        match self {
            Self::Exact(want) => name == *want,
            Self::Prefix(prefix) => name.starts_with(prefix),
            Self::Any => true,
        }
    }
}

/// "No module matching `dependent` may depend on a module matching
/// `dependency`."
///
/// To forbid anyone depending on audit:
/// `ForbiddenEdge { dependent: Any, dependency: Exact("audit::AuditModule") }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForbiddenEdge {
    /// Pattern for the depending module (edge source).
    pub dependent: ModulePattern,
    /// Pattern for the depended-on module (edge target).
    pub dependency: ModulePattern,
}

/// "Every module matching `dependent` must depend on at least one module
/// matching `dependency`."
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequiredEdge {
    /// Pattern for the depending module (edge source).
    pub dependent: ModulePattern,
    /// Pattern for the depended-on module (edge target).
    pub dependency: ModulePattern,
}

/// Build-time dependency policy: forbidden edges (negative constraints)
/// and required edges (positive must-have constraints).
///
/// The empty policy (default) imposes no constraints.
#[derive(Debug, Clone, Default)]
pub struct DependencyPolicy {
    /// Edges no registered module graph may contain.
    pub forbid: Vec<ForbiddenEdge>,
    /// Edges every matching dependent module must satisfy.
    pub required: Vec<RequiredEdge>,
}

impl DependencyPolicy {
    /// Returns whether any edge matches the pair `(dependent, dependency)`
    /// by module name.
    #[must_use]
    pub fn forbids(&self, dependent: &str, dependency: &str) -> bool {
        self.forbid.iter().any(|edge| {
            edge.dependent.matches(dependent) && edge.dependency.matches(dependency)
        })
    }
}

/// Which rule kind produced a [`crate::error::TraitKitError::PolicyViolation`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyRule {
    /// A forbidden edge was matched.
    ForbiddenDependency,
    /// A required edge had no satisfying dependency.
    RequiredDependencyMissing,
}

#[cfg(test)]
mod tests {
    use super::*;

    // 最小三模块边表：auth → business，audit 无入边。
    const EDGES: &[(&str, &str)] = &[("pol::auth", "pol::business")];

    #[test]
    fn exact_pattern_hits_and_misses() {
        assert!(ModulePattern::Exact("pol::auth").matches("pol::auth"));
        assert!(!ModulePattern::Exact("pol::auth").matches("pol::audit"));
    }

    #[test]
    fn prefix_pattern_matches_prefix_only() {
        assert!(ModulePattern::Prefix("business").matches("business::Orders"));
        assert!(!ModulePattern::Prefix("business").matches("businessx::Y"));
        assert!(!ModulePattern::Prefix("business").matches("xbusiness"));
    }

    #[test]
    fn any_pattern_matches_everything() {
        assert!(ModulePattern::Any.matches("pol::auth"));
        assert!(ModulePattern::Any.matches(""));
        assert!(ModulePattern::Any.matches("anything::at::all"));
    }

    /// R-pol-001：三类边在最小三模块边表上的匹配表达式。
    #[test]
    fn forbidden_edge_matches_declared_graph_edge() {
        let policy = DependencyPolicy {
            forbid: vec![ForbiddenEdge {
                dependent: ModulePattern::Any,
                dependency: ModulePattern::Exact("pol::business"),
            }],
            required: Vec::new(),
        };
        // auth → business 命中（任何人不得依赖 business）。
        assert!(policy.forbids("pol::auth", "pol::business"));
        // audit 无入边：audit → business 同样命中禁令。
        assert!(policy.forbids("pol::audit", "pol::business"));
        // 方向相反不命中（business 依赖 auth 不在禁令内）。
        assert!(!policy.forbids("pol::business", "pol::auth"));
    }

    #[test]
    fn forbidden_edge_by_dependent_pattern() {
        let policy = DependencyPolicy {
            forbid: vec![ForbiddenEdge {
                dependent: ModulePattern::Exact("pol::auth"),
                dependency: ModulePattern::Any,
            }],
            required: Vec::new(),
        };
        assert!(policy.forbids("pol::auth", "pol::business"));
        assert!(!policy.forbids("pol::audit", "pol::business"));
    }

    #[test]
    fn required_edge_satisfied_by_declared_dependency() {
        let edge = RequiredEdge {
            dependent: ModulePattern::Prefix("pol::"),
            dependency: ModulePattern::Exact("pol::auth"),
        };
        // 边表里 auth 的依赖是 business，不是 auth → auth 不满足该 required。
        let auth_deps = EDGES
            .iter()
            .filter(|(d, _)| *d == "pol::auth")
            .map(|(_, dep)| *dep);
        assert!(!auth_deps.clone().any(|dep| edge.dependency.matches(dep)));
        // 反向：business 的依赖若含 auth 则满足（构造性验证匹配表达式）。
        assert!(["pol::auth"].iter().any(|n| edge.dependency.matches(n)));
    }

    #[test]
    fn default_policy_imposes_no_constraints() {
        let policy = DependencyPolicy::default();
        assert!(policy.forbid.is_empty());
        assert!(policy.required.is_empty());
        assert!(!policy.forbids("pol::auth", "pol::business"));
    }
}
