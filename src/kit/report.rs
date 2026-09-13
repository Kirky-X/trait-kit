// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Structured build report export.
//!
//! Collects per-module build facts while `Kit::build` executes and exposes
//! them as a machine-readable [`BuildReport`] (JSON via `to_json`), alongside
//! the human-oriented `graph_dot()` / `graph_mermaid()` exports.
//!
//! Requires the `report` feature (pulls `serde` + `serde_json`). Without the
//! feature no timing/stat collection code exists in the build path — the
//! default build is zero-cost.

use std::cell::RefCell;

use serde::Serialize;

/// Feature-gated fields aggregated on `Kit` (same pattern as `ObserverFields`).
#[derive(Default)]
pub(crate) struct ReportFields {
    /// Per-module build records in build-completion order.
    modules: RefCell<Vec<ModuleReportEntry>>,
    /// Override records captured at `override_module(_strict)` call time.
    overrides: RefCell<Vec<OverrideRecord>>,
    /// Topological order (module names) captured after `graph.validate()`.
    topo_order: RefCell<Vec<&'static str>>,
    /// Total wall time of `build()` (set when build finishes).
    total_elapsed_us: RefCell<Option<u64>>,
    /// Contract entries captured at registration time.
    contract: RefCell<Vec<ContractEntry>>,
}

impl ReportFields {
    /// Record a module built from its `build_fn` (with construction time).
    pub(crate) fn push_built(&self, name: &'static str, elapsed_us: u64, deps: Vec<&'static str>) {
        self.modules.borrow_mut().push(ModuleReportEntry {
            name,
            state: ModuleBuildState::Built,
            deps,
            elapsed_us: Some(elapsed_us),
        });
    }

    /// Record a module that was satisfied by a pre-built override.
    pub(crate) fn push_overridden(&self, name: &'static str, deps: Vec<&'static str>) {
        self.modules.borrow_mut().push(ModuleReportEntry {
            name,
            state: ModuleBuildState::Overridden,
            deps,
            elapsed_us: None,
        });
    }

    /// Record a lazy module (construction deferred to first `require()`).
    pub(crate) fn push_lazy(&self, name: &'static str, deps: Vec<&'static str>) {
        self.modules.borrow_mut().push(ModuleReportEntry {
            name,
            state: ModuleBuildState::Lazy,
            deps,
            elapsed_us: None,
        });
    }

    /// Record an override registration (source: `override_module` or
    /// `override_module_strict`).
    pub(crate) fn push_override_record(&self, record: OverrideRecord) {
        self.overrides.borrow_mut().push(record);
    }

    /// Record the validated topological order (module names).
    pub(crate) fn set_topo_order(&self, names: Vec<&'static str>) {
        *self.topo_order.borrow_mut() = names;
    }

    /// Record the total `build()` wall time in microseconds.
    pub(crate) fn set_total_elapsed_us(&self, us: u64) {
        *self.total_elapsed_us.borrow_mut() = Some(us);
    }

    /// Record a module contract entry (name, version, capability, deps).
    pub(crate) fn push_contract(&self, entry: ContractEntry) {
        self.contract.borrow_mut().push(entry);
    }

    /// Snapshot the registered module contracts.
    pub(crate) fn contract_snapshot(&self) -> ContractManifest {
        ContractManifest {
            schema_version: CONTRACT_SCHEMA_VERSION,
            modules: self.contract.borrow().clone(),
        }
    }

    /// Snapshot the accumulated build facts into a [`BuildReport`].
    pub(crate) fn snapshot(&self) -> BuildReport {
        BuildReport {
            topo_order: self.topo_order.borrow().clone(),
            modules: self.modules.borrow().clone(),
            overrides: self.overrides.borrow().clone(),
            total_elapsed_us: *self.total_elapsed_us.borrow(),
            ..BuildReport::default()
        }
    }
}

/// One module's contract: name, declared version, capability type, deps.
#[derive(Debug, Clone, Serialize)]
pub struct ContractEntry {
    /// Module name (`ModuleMeta::NAME`).
    pub module: &'static str,
    /// Declared capability version (`ModuleMeta::VERSION`).
    pub version: &'static str,
    /// Concrete capability type name (`std::any::type_name`).
    pub capability: &'static str,
    /// Dependency module names.
    pub deps: Vec<&'static str>,
}

/// Machine-readable contract manifest of all registered modules.
#[derive(Debug, Clone, Serialize)]
pub struct ContractManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// One entry per registered module, registration order.
    pub modules: Vec<ContractEntry>,
}

/// Current [`ContractManifest`] schema version.
pub const CONTRACT_SCHEMA_VERSION: u32 = 1;

impl ContractManifest {
    /// Serialize to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_json` error instead of embedding it in
    /// an otherwise-valid-looking JSON body.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

/// Build state of a module as observed by the build pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleBuildState {
    /// Built by invoking its `build_fn` during `Kit::build`.
    Built,
    /// Deferred: `build_fn` runs on first `require()`.
    Lazy,
    /// Satisfied by a pre-built override (`override_module` family).
    Overridden,
}

/// Per-module entry of a [`BuildReport`].
#[derive(Debug, Clone, Serialize)]
pub struct ModuleReportEntry {
    /// Module name (`ModuleMeta::NAME`).
    pub name: &'static str,
    /// How the module's capability was produced.
    pub state: ModuleBuildState,
    /// Names of the declared dependencies.
    pub deps: Vec<&'static str>,
    /// Construction time in microseconds (`None` unless `state == Built`).
    pub elapsed_us: Option<u64>,
}

/// One override registration captured in a [`BuildReport`].
#[derive(Debug, Clone, Serialize)]
pub struct OverrideRecord {
    /// Overridden module name (`ModuleMeta::NAME`); `"(unregistered)"` when
    /// the override targeted a type that was never registered in the graph.
    pub module: &'static str,
    /// Which API injected the override.
    pub source: &'static str,
}

/// Structured, machine-readable report of a completed `Kit::build`.
///
/// Obtained from [`Kit<Ready>::build_report`](crate::kit::Kit::build_report);
/// serialize with [`BuildReport::to_json`] (or any `serde_json` encoder).
#[derive(Debug, Clone, Serialize)]
pub struct BuildReport {
    /// Report schema version (bump on breaking shape changes).
    pub schema_version: u32,
    /// Module names in validated topological order.
    pub topo_order: Vec<&'static str>,
    /// Per-module build records in build-completion order.
    pub modules: Vec<ModuleReportEntry>,
    /// Override registrations observed at registration time.
    pub overrides: Vec<OverrideRecord>,
    /// Total `build()` wall time in microseconds.
    pub total_elapsed_us: Option<u64>,
}

impl Default for BuildReport {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            topo_order: Vec::new(),
            modules: Vec::new(),
            overrides: Vec::new(),
            total_elapsed_us: None,
        }
    }
}

/// Current [`BuildReport`] schema version.
pub const SCHEMA_VERSION: u32 = 1;

impl BuildReport {
    /// Serialize the report to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_json` error instead of embedding it in
    /// an otherwise-valid-looking JSON body.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Parse a JSON string back into a generic JSON value (test helper for
    /// downstream round-trip assertions).
    ///
    /// # Errors
    ///
    /// Returns the `serde_json` error if the string is not valid JSON.
    pub fn from_json_str(s: &str) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AutoBuilder, ModuleMeta};
    use crate::kit::Kit;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct Cap(u32);

    #[derive(Debug)]
    struct TestError;

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "test error")
        }
    }
    impl std::error::Error for TestError {}

    struct RptLeaf;
    impl ModuleMeta for RptLeaf {
        const NAME: &'static str = "rpt-leaf";
    }
    impl AutoBuilder for RptLeaf {
        type Capability = Arc<Cap>;
        type Error = TestError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(Cap(1)))
        }
    }

    struct RptTop;
    impl ModuleMeta for RptTop {
        const NAME: &'static str = "rpt-top";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            static DEPS: &[(&str, std::any::TypeId)] = &[(
                <RptLeaf as ModuleMeta>::NAME,
                std::any::TypeId::of::<RptLeaf>(),
            )];
            DEPS
        }
    }
    impl AutoBuilder for RptTop {
        type Capability = Arc<Cap>;
        type Error = TestError;
        fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
            let leaf = kit.require::<RptLeaf>().map_err(|_| TestError)?;
            Ok(Arc::new(Cap(leaf.0 + 1)))
        }
    }

    struct RptLazy;
    impl ModuleMeta for RptLazy {
        const NAME: &'static str = "rpt-lazy";
    }
    impl AutoBuilder for RptLazy {
        type Capability = Arc<Cap>;
        type Error = TestError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(Cap(42)))
        }
    }

    #[test]
    fn build_report_lists_modules_in_completion_order_with_states() {
        let mut kit = Kit::new();
        kit.register::<RptTop>().expect("register top");
        kit.register::<RptLeaf>().expect("register leaf");
        kit.register_lazy::<RptLazy>().expect("register lazy");
        kit.override_module::<RptLeaf>(Arc::new(Cap(99)));
        let ready = kit.build().expect("build ok");

        let report = ready.build_report();
        // Topological order: leaf precedes top; the dependency-free lazy
        // module may appear anywhere between them (Kahn queue order).
        assert_eq!(report.topo_order.len(), 3);
        let idx = |n: &str| {
            report
                .topo_order
                .iter()
                .position(|m| *m == n)
                .expect("in topo")
        };
        assert!(idx("rpt-leaf") < idx("rpt-top"), "leaf before top");

        let leaf = report
            .modules
            .iter()
            .find(|m| m.name == "rpt-leaf")
            .expect("leaf entry");
        assert_eq!(leaf.state, ModuleBuildState::Overridden);
        assert_eq!(leaf.elapsed_us, None);

        let top = report
            .modules
            .iter()
            .find(|m| m.name == "rpt-top")
            .expect("top entry");
        assert_eq!(top.state, ModuleBuildState::Built);
        assert!(top.elapsed_us.is_some(), "built module carries elapsed");
        assert_eq!(top.deps, vec!["rpt-leaf"]);

        let lazy = report
            .modules
            .iter()
            .find(|m| m.name == "rpt-lazy")
            .expect("lazy entry");
        assert_eq!(lazy.state, ModuleBuildState::Lazy);
    }

    #[test]
    fn build_report_records_override_source() {
        let mut kit = Kit::new();
        kit.register::<RptLeaf>().expect("register");
        kit.override_module_strict::<RptLeaf>(Arc::new(Cap(7)))
            .expect("strict override");
        let ready = kit.build().expect("build ok");
        let report = ready.build_report();
        assert_eq!(report.overrides.len(), 1);
        assert_eq!(report.overrides[0].module, "rpt-leaf");
        assert_eq!(report.overrides[0].source, "override_module_strict");
    }

    #[test]
    fn build_report_total_elapsed_recorded() {
        let mut kit = Kit::new();
        kit.register::<RptLeaf>().expect("register");
        let ready = kit.build().expect("build ok");
        let report = ready.build_report();
        assert!(report.total_elapsed_us.is_some());
    }

    #[test]
    fn build_report_json_round_trips() {
        let mut kit = Kit::new();
        kit.register::<RptTop>().expect("register top");
        kit.register::<RptLeaf>().expect("register leaf");
        let ready = kit.build().expect("build ok");

        let json = ready.build_report().to_json().expect("serialize report");
        let value = BuildReport::from_json_str(&json).expect("valid JSON");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["topo_order"][0], "rpt-leaf");
        assert_eq!(value["modules"].as_array().map(Vec::len), Some(2));
        let top = value["modules"]
            .as_array()
            .expect("array")
            .iter()
            .find(|m| m["name"] == "rpt-top")
            .cloned()
            .expect("top entry");
        assert_eq!(top["state"], "built");
        assert!(top["elapsed_us"].as_u64().is_some());
    }

    #[test]
    fn report_fields_accumulate_across_lifecycles() {
        let fields = ReportFields::default();
        fields.push_built("a", 12, vec![]);
        fields.push_lazy("b", vec!["a"]);
        fields.push_overridden("c", vec![]);
        fields.push_override_record(OverrideRecord {
            module: "c",
            source: "override_module",
        });
        fields.set_topo_order(vec!["a", "b", "c"]);
        fields.set_total_elapsed_us(100);

        let report = BuildReport {
            topo_order: fields.topo_order.borrow().clone(),
            modules: fields.modules.borrow().clone(),
            overrides: fields.overrides.borrow().clone(),
            total_elapsed_us: *fields.total_elapsed_us.borrow(),
            ..BuildReport::default()
        };
        assert_eq!(report.modules.len(), 3);
        assert_eq!(report.overrides.len(), 1);
        assert_eq!(report.total_elapsed_us, Some(100));
    }
}

#[cfg(test)]
mod contract_manifest_tests {
    use crate::core::{AutoBuilder, ModuleMeta};
    use crate::kit::Kit;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct ManifestCap;

    #[derive(Debug)]
    struct ManifestError;
    impl std::fmt::Display for ManifestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "manifest error")
        }
    }
    impl std::error::Error for ManifestError {}

    struct ManifestLeaf;
    impl ModuleMeta for ManifestLeaf {
        const NAME: &'static str = "manifest-leaf";
        const VERSION: &'static str = "2.1.0";
    }
    impl AutoBuilder for ManifestLeaf {
        type Capability = Arc<ManifestCap>;
        type Error = ManifestError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(ManifestCap))
        }
    }

    struct ManifestTop;
    impl ModuleMeta for ManifestTop {
        const NAME: &'static str = "manifest-top";
        const VERSION: &'static str = "0.3.0";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            static DEPS: &[(&str, std::any::TypeId)] = &[(
                <ManifestLeaf as ModuleMeta>::NAME,
                std::any::TypeId::of::<ManifestLeaf>(),
            )];
            DEPS
        }
    }
    impl AutoBuilder for ManifestTop {
        type Capability = Arc<ManifestCap>;
        type Error = ManifestError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(ManifestCap))
        }
    }

    #[test]
    fn contract_manifest_lists_modules_with_versions_and_capabilities() {
        let mut kit = Kit::new();
        kit.register::<ManifestTop>().expect("top");
        kit.register::<ManifestLeaf>().expect("leaf");
        let ready = kit.build().expect("build ok");

        let manifest = ready.contract_manifest();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.modules.len(), 2);

        let leaf = manifest
            .modules
            .iter()
            .find(|m| m.module == "manifest-leaf")
            .expect("leaf");
        assert_eq!(leaf.version, "2.1.0");
        assert!(leaf.deps.is_empty());
        assert!(
            leaf.capability.contains("ManifestCap"),
            "capability type name: {}",
            leaf.capability
        );

        let top = manifest
            .modules
            .iter()
            .find(|m| m.module == "manifest-top")
            .expect("top");
        assert_eq!(top.version, "0.3.0");
        assert_eq!(top.deps, vec!["manifest-leaf"]);
    }

    #[test]
    fn contract_manifest_json_round_trips() {
        let mut kit = Kit::new();
        kit.register::<ManifestLeaf>().expect("leaf");
        let ready = kit.build().expect("build ok");

        let json = ready
            .contract_manifest()
            .to_json()
            .expect("serialize manifest");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["modules"][0]["module"], "manifest-leaf");
        assert_eq!(value["modules"][0]["version"], "2.1.0");
    }
}
