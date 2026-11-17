// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Integration tests for the `describe()` capability-graph snapshot
//! (feature `report`).

#![cfg(feature = "report")]

use std::any::TypeId;
use std::sync::Arc;

use trait_kit::core::{AutoBuilder, ModuleMeta};
use trait_kit::kit::Kit;

struct Base;
impl ModuleMeta for Base {
    const NAME: &'static str = "snap-base";
}
impl AutoBuilder for Base {
    type Capability = Arc<u32>;
    type Error = std::io::Error;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(1))
    }
}

/// Mid depends on base; both cache and lazy-gateway depend on mid.
struct Mid;
impl ModuleMeta for Mid {
    const NAME: &'static str = "snap-mid";
    fn dependencies() -> &'static [(&'static str, TypeId)] {
        static DEPS: &[(&str, TypeId)] = &[("snap-base", TypeId::of::<Base>())];
        DEPS
    }
}
impl AutoBuilder for Mid {
    type Capability = Arc<u32>;
    type Error = std::io::Error;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(2))
    }
}

struct Cache;
impl ModuleMeta for Cache {
    const NAME: &'static str = "snap-cache";
    fn dependencies() -> &'static [(&'static str, TypeId)] {
        static DEPS: &[(&str, TypeId)] = &[("snap-mid", TypeId::of::<Mid>())];
        DEPS
    }
}
impl AutoBuilder for Cache {
    type Capability = Arc<u32>;
    type Error = std::io::Error;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(3))
    }
}

/// Lazy module: state must surface as `lazy` in the snapshot.
struct LazyGateway;
impl ModuleMeta for LazyGateway {
    const NAME: &'static str = "snap-gateway";
    fn dependencies() -> &'static [(&'static str, TypeId)] {
        static DEPS: &[(&str, TypeId)] = &[("snap-mid", TypeId::of::<Mid>())];
        DEPS
    }
}
impl AutoBuilder for LazyGateway {
    type Capability = Arc<u32>;
    type Error = std::io::Error;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(4))
    }
}

/// 四模块图（base ← mid ← {cache, lazy-gateway}）：行数、deps/dependents
/// 互为镜像、lazy 状态、JSON round-trip 后 topo_order 不变（R-des-002）。
#[test]
fn describe_snapshot_mirrors_edges_and_round_trips() {
    let mut kit = Kit::new();
    kit.register::<Base>().expect("register base");
    kit.register::<Mid>().expect("register mid");
    kit.register::<Cache>().expect("register cache");
    kit.register_lazy::<LazyGateway>()
        .expect("register lazy gateway");
    let ready = kit.build().expect("build");
    let description = ready.describe();

    assert_eq!(description.modules.len(), 4, "one row per module");
    let by_name = |name: &str| {
        description
            .modules
            .iter()
            .find(|m| m.name == name)
            .unwrap_or_else(|| panic!("missing module row: {name}"))
    };

    // Forward/reverse mirror around mid.
    let mid = by_name("snap-mid");
    assert_eq!(mid.deps, vec!["snap-base"]);
    let mut dependents = mid.dependents.clone();
    dependents.sort_unstable();
    assert_eq!(dependents, vec!["snap-cache", "snap-gateway"]);
    assert_eq!(by_name("snap-base").dependents, vec!["snap-mid"]);
    assert!(by_name("snap-cache").dependents.is_empty());

    // Lazy state surfaces.
    assert_eq!(
        by_name("snap-gateway").state,
        trait_kit::kit::report::ModuleBuildState::Lazy
    );
    assert_eq!(
        by_name("snap-base").state,
        trait_kit::kit::report::ModuleBuildState::Built
    );

    // JSON round-trip preserves topo_order.
    let json = description.to_json().expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
    let topo: Vec<String> = value["topo_order"]
        .as_array()
        .expect("topo_order array")
        .iter()
        .map(|v| v.as_str().expect("string entry").to_string())
        .collect();
    let expected: Vec<String> = description
        .topo_order
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(topo, expected);
}
