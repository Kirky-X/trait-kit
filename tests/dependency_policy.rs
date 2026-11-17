// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Integration tests for build-time dependency policy (feature `policy`）：
//! forbid 违规 / required 满足 / 未设策略行为不变 三场景。
#![cfg(feature = "policy")]

use std::any::TypeId;

use trait_kit::kit::{DependencyPolicy, ForbiddenEdge, ModulePattern, PolicyRule, RequiredEdge};
use trait_kit::prelude::*;

#[derive(Clone, Debug)]
struct Cap;
#[derive(Debug)]
struct BuildErr;
impl std::fmt::Display for BuildErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "policy e2e error")
    }
}
impl std::error::Error for BuildErr {}

struct Auth;
impl ModuleMeta for Auth {
    const NAME: &'static str = "e2e::auth";
}
impl AutoBuilder for Auth {
    type Capability = Cap;
    type Error = BuildErr;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap)
    }
}

struct Business;
impl ModuleMeta for Business {
    const NAME: &'static str = "e2e::business";
    fn dependencies() -> &'static [(&'static str, TypeId)] {
        static DEPS: &[(&str, TypeId)] = &[(<Auth as ModuleMeta>::NAME, TypeId::of::<Auth>())];
        DEPS
    }
}
impl AutoBuilder for Business {
    type Capability = Cap;
    type Error = BuildErr;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap)
    }
}

/// 场景 1：Policy 禁止 auth 依赖 business——违规图 `build()` 返回
/// `PolicyViolation` 且错误文本含双方模块名（R-pol-003）。
#[test]
fn forbidden_edge_fails_build_and_names_both_modules() {
    let mut kit = Kit::new();
    kit.register::<Auth>().expect("register auth");
    kit.register::<Business>().expect("register business");
    kit.dependency_policy(DependencyPolicy {
        // 方向语义：dependent 不得依赖 dependency。business 依赖 auth →
        // 用 Any 表达“auth 不被任何模块依赖”（入边禁令）。
        forbid: vec![ForbiddenEdge {
            dependent: ModulePattern::Any,
            dependency: ModulePattern::Exact("e2e::auth"),
        }],
        required: Vec::new(),
    });
    let err = kit.build().expect_err("business → auth must be forbidden");
    assert!(
        matches!(
            err,
            TraitKitError::PolicyViolation {
                rule: PolicyRule::ForbiddenDependency,
                ..
            }
        ),
        "wrong error: {err:?}"
    );
    let msg = format!("{err}");
    assert!(msg.contains("e2e::business"), "dependent named: '{msg}'");
    assert!(msg.contains("e2e::auth"), "dependency named: '{msg}'");
}

/// 场景 2：required 要求 business 依赖 auth，图满足 → build 成功。
#[test]
fn required_edge_satisfied_builds() {
    let mut kit = Kit::new();
    kit.register::<Auth>().expect("register auth");
    kit.register::<Business>().expect("register business");
    kit.dependency_policy(DependencyPolicy {
        forbid: Vec::new(),
        required: vec![RequiredEdge {
            dependent: ModulePattern::Prefix("e2e::"),
            dependency: ModulePattern::Exact("e2e::auth"),
        }],
    });
    let ready = kit.build().expect("graph satisfies the required edge");
    assert_eq!(ready.dependency_policy_summary(), Some((0, 1)));
}

/// 场景 3：未设置策略的相同图 build 成功——行为与现状一致。
#[test]
fn same_graph_without_policy_builds_unchanged() {
    let mut kit = Kit::new();
    kit.register::<Auth>().expect("register auth");
    kit.register::<Business>().expect("register business");
    let ready = kit.build().expect("no policy, current behavior");
    assert_eq!(ready.dependency_policy_summary(), None);
    let _cap = ready.require::<Business>().expect("business built");
}
