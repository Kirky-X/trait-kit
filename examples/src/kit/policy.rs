// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Dependency Policy — 构建期架构约束。
//!
//! Demonstrates:
//! - `Kit::dependency_policy(DependencyPolicy)` — 禁止/必选依赖边
//! - `TraitKitError::PolicyViolation` — 违规模块图 build 失败
//! - 违规分支打印错误后用合规图重建成功
//!
//! Run: `cargo run -p trait-kit-examples --example policy --features policy`

use std::any::TypeId;

use trait_kit::kit::{DependencyPolicy, ForbiddenEdge, ModulePattern, RequiredEdge};
use trait_kit::prelude::*;

#[derive(Clone, Debug)]
struct Cap;
#[derive(Debug)]
struct BuildErr;
impl std::fmt::Display for BuildErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "policy example error")
    }
}
impl std::error::Error for BuildErr {}

struct Auth;
impl ModuleMeta for Auth {
    const NAME: &'static str = "ex::auth";
}
impl AutoBuilder for Auth {
    type Capability = Cap;
    type Error = BuildErr;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap)
    }
}

/// business 依赖 auth（正向拓扑合法，但可被策略禁止）。
struct Business;
impl ModuleMeta for Business {
    const NAME: &'static str = "ex::business";
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

fn main() {
    // ── Scenario 1: forbid 违规——“auth 不得被任何模块依赖” ────────────
    let mut kit = Kit::new();
    kit.register::<Auth>().expect("register auth");
    kit.register::<Business>().expect("register business");
    kit.dependency_policy(DependencyPolicy {
        // 入边禁令：dependent=Any + dependency=Exact(auth) ——
        // “审计/基础模块不被业务模块反向依赖”类约束的通用形态。
        forbid: vec![ForbiddenEdge {
            dependent: ModulePattern::Any,
            dependency: ModulePattern::Exact("ex::auth"),
        }],
        required: Vec::new(),
    });
    match kit.build() {
        Err(TraitKitError::PolicyViolation {
            dependent,
            dependency,
            ..
        }) => {
            println!("[forbid] violation: {dependent} must not depend on {dependency}");
        }
        other => panic!("expected PolicyViolation, got {other:?}"),
    }

    // ── Scenario 2: 合规图 + required——business 必须依赖 auth ──────────
    let mut good = Kit::new();
    good.register::<Auth>().expect("register auth");
    good.register::<Business>().expect("register business");
    good.dependency_policy(DependencyPolicy {
        forbid: Vec::new(),
        required: vec![RequiredEdge {
            dependent: ModulePattern::Prefix("ex::business"),
            dependency: ModulePattern::Exact("ex::auth"),
        }],
    });
    let ready = good.build().expect("compliant graph builds");
    println!(
        "[required] compliant build ok: summary = {:?}",
        ready.dependency_policy_summary()
    );

    println!("policy example finished");
}
