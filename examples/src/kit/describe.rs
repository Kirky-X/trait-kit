// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Capability graph snapshot + security posture audit.
//!
//! Demonstrates:
//! - `Kit<Ready>::describe()` — read-only capability-graph snapshot (JSON)
//! - `Kit<Ready>::security_posture()` — verifiable architecture-safety report
//! - governance wiring that flips checks to `Pass`: forbid policy +
//!   Restricted visibility
//!
//! Run: `cargo run -p trait-kit-examples --example describe --features audit,policy,visibility`

use std::any::TypeId;

use trait_kit::kit::policy::{DependencyPolicy, ForbiddenEdge, ModulePattern};
use trait_kit::kit::visibility::CapabilityVisibility;
use trait_kit::prelude::*;

#[derive(Clone, Debug)]
struct Cap;
#[derive(Debug)]
struct BuildErr;
impl std::fmt::Display for BuildErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "describe example error")
    }
}
impl std::error::Error for BuildErr {}

struct CoreSvc;
impl ModuleMeta for CoreSvc {
    const NAME: &'static str = "app::core";
}
impl AutoBuilder for CoreSvc {
    type Capability = Cap;
    type Error = BuildErr;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap)
    }
}

/// Facade depends on core and is the only allowed `require_for` caller.
struct Facade;
impl ModuleMeta for Facade {
    const NAME: &'static str = "app::facade";
    fn dependencies() -> &'static [(&'static str, TypeId)] {
        static DEPS: &[(&str, TypeId)] = &[("app::core", TypeId::of::<CoreSvc>())];
        DEPS
    }
}
impl AutoBuilder for Facade {
    type Capability = Cap;
    type Error = BuildErr;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap)
    }
}

/// Gateway also depends on core (reverse edge visible in `dependents`).
struct Gateway;
impl ModuleMeta for Gateway {
    const NAME: &'static str = "app::gateway";
    fn dependencies() -> &'static [(&'static str, TypeId)] {
        static DEPS: &[(&str, TypeId)] = &[("app::core", TypeId::of::<CoreSvc>())];
        DEPS
    }
}
impl AutoBuilder for Gateway {
    type Capability = Cap;
    type Error = BuildErr;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap)
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register::<CoreSvc>().expect("register core");
    kit.register::<Facade>().expect("register facade");
    kit.register::<Gateway>().expect("register gateway");

    // Architecture rule: the gateway must not reach into the facade layer
    // (dependency-policy check flips to Pass once rules are configured;
    // the example graph satisfies it, so the build succeeds).
    kit.dependency_policy(DependencyPolicy {
        forbid: vec![ForbiddenEdge {
            dependent: ModulePattern::Exact("app::gateway"),
            dependency: ModulePattern::Exact("app::facade"),
        }],
        required: Vec::new(),
    });

    // Only `app::facade` may retrieve the core capability
    // (capability-visibility check flips to Pass once a Restricted
    // visibility is declared).
    kit.set_visibility::<CoreSvc>(CapabilityVisibility::Restricted(&["app::facade"]));

    let ready = kit.build().expect("build");

    println!("== describe() ==");
    let description = ready.describe();
    println!("{}", description.to_json().expect("serialize description"));

    println!("\n== security_posture() ==");
    let posture = ready.security_posture();
    for check in &posture.checks {
        println!(
            "[{status:>4}] {id:<22} {detail}",
            status = format!("{:?}", check.status).to_uppercase(),
            id = check.id,
            detail = check.detail,
        );
    }
    println!("is_ok: {}", posture.is_ok());
}
