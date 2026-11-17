// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Read-only capability-graph snapshot (`Kit<Ready>::describe`).
//!
//! Aggregates the `report` feature's build data (`topo_order`, per-module
//! `deps`/`state`) with reverse edges (`dependents`) into a single
//! queryable/serializable view. `BuildReport` stays untouched (it records
//! the build *process*; this is a runtime *snapshot*).

use crate::kit::Ready;
use crate::kit::kit::Kit;
use crate::kit::report::ModuleBuildState;

/// One module's row in the read-only capability graph snapshot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleDescription {
    /// Module name (`ModuleMeta::NAME`).
    pub name: &'static str,
    /// How the module's capability was produced (built/lazy/overridden).
    pub state: ModuleBuildState,
    /// Names of the declared dependencies (forward edges).
    pub deps: Vec<&'static str>,
    /// Names of the modules that depend on this one (reverse edges).
    pub dependents: Vec<&'static str>,
}

/// Read-only snapshot of the built Kit's capability graph.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KitDescription {
    /// One row per registered module, in topological order.
    pub modules: Vec<ModuleDescription>,
    /// Validated topological order (module names).
    pub topo_order: Vec<&'static str>,
}

impl KitDescription {
    /// Serialize to JSON (`serde_json` ships with the `report` feature).
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` when serialization fails (non-finite
    /// floats or depth limits — neither occurs with this schema).
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

impl Kit<Ready> {
    /// Aggregate read-only view of the built kit: who depends on whom, in
    /// which build state. See [`KitDescription`].
    ///
    /// Requires the `report` feature.
    #[cfg(feature = "report")]
    #[must_use]
    pub fn describe(&self) -> KitDescription {
        let report = self.build_report();
        let dependents_of = |name: &str| -> Vec<&'static str> {
            report
                .modules
                .iter()
                .filter(|entry| entry.deps.contains(&name))
                .map(|entry| entry.name)
                .collect()
        };
        KitDescription {
            modules: report
                .modules
                .iter()
                .map(|entry| ModuleDescription {
                    name: entry.name,
                    state: entry.state,
                    deps: entry.deps.clone(),
                    dependents: dependents_of(entry.name),
                })
                .collect(),
            topo_order: report.topo_order,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空 Kit 的 `describe()` 返回空 modules 与空 topo_order（R-des-002）。
    #[test]
    fn empty_kit_describes_empty_snapshot() {
        let kit = Kit::new();
        let ready = kit.build().expect("build empty kit");
        let description = ready.describe();
        assert!(description.modules.is_empty());
        assert!(description.topo_order.is_empty());
    }

    /// 三模块链 A ← B ← C：B 行 `deps == ["A"]`、`dependents == ["C"]`，
    /// modules 顺序与 `topo_order` 一致（R-des-002）。
    #[test]
    fn chain_describe_mirrors_forward_and_reverse_edges() {
        use crate::core::{AutoBuilder, ModuleMeta};
        use std::any::TypeId;
        use std::sync::Arc;

        struct ChainA;
        impl ModuleMeta for ChainA {
            const NAME: &'static str = "desc-a";
        }
        impl AutoBuilder for ChainA {
            type Capability = Arc<u32>;
            type Error = std::io::Error;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                Ok(Arc::new(1))
            }
        }

        struct ChainB;
        impl ModuleMeta for ChainB {
            const NAME: &'static str = "desc-b";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] = &[("desc-a", TypeId::of::<ChainA>())];
                DEPS
            }
        }
        impl AutoBuilder for ChainB {
            type Capability = Arc<u32>;
            type Error = std::io::Error;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                Ok(Arc::new(2))
            }
        }

        struct ChainC;
        impl ModuleMeta for ChainC {
            const NAME: &'static str = "desc-c";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] = &[("desc-b", TypeId::of::<ChainB>())];
                DEPS
            }
        }
        impl AutoBuilder for ChainC {
            type Capability = Arc<u32>;
            type Error = std::io::Error;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                Ok(Arc::new(3))
            }
        }

        let mut kit = Kit::new();
        kit.register::<ChainA>().expect("register a");
        kit.register::<ChainB>().expect("register b");
        kit.register::<ChainC>().expect("register c");
        let ready = kit.build().expect("build");
        let description = ready.describe();

        assert_eq!(description.topo_order, vec!["desc-a", "desc-b", "desc-c"]);
        let names: Vec<&str> = description.modules.iter().map(|m| m.name).collect();
        assert_eq!(names, description.topo_order, "modules follow topo order");

        let b = &description.modules[1];
        assert_eq!(b.deps, vec!["desc-a"]);
        assert_eq!(b.dependents, vec!["desc-c"]);
    }

    /// `to_json()` 输出可被 `serde_json::from_str` 还原（schema 有效）。
    #[test]
    fn kit_description_to_json_round_trips() {
        let description = KitDescription {
            modules: vec![ModuleDescription {
                name: "m",
                state: ModuleBuildState::Built,
                deps: vec![],
                dependents: vec![],
            }],
            topo_order: vec!["m"],
        };
        let json = description.to_json().expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(value["topo_order"][0], "m");
        assert_eq!(value["modules"][0]["name"], "m");
    }
}
