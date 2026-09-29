// Copyright (c) 2026 Kirky.X🌠
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

#[cfg(feature = "async")]
use std::sync::Mutex;

use serde::Serialize;

/// Feature-gated fields aggregated on `Kit` (same pattern as `ObserverFields`).
#[derive(Default)]
pub(crate) struct ReportFields {
    /// Per-module build records in build-completion order.
    modules: RefCell<Vec<ModuleReportEntry>>,
    /// Override records captured at `override_module(_strict)` call time.
    overrides: RefCell<Vec<OverrideRecord>>,
    /// Config override records captured at `merge_config` call time.
    config_overrides: RefCell<Vec<ConfigOverrideRecord>>,
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

    /// Record a config-level override (`merge_config`): `applied == false`
    /// surfaces an override that was dropped because the target config did
    /// not exist, instead of silently swallowing it.
    ///
    /// The only caller is the `confers`-gated `Kit::merge_config`, so the
    /// method follows the same gate and stays dead-code-free under
    /// `report` alone (the snapshot field remains, always empty then).
    #[cfg(feature = "confers")]
    pub(crate) fn push_config_override(&self, record: ConfigOverrideRecord) {
        self.config_overrides.borrow_mut().push(record);
    }

    /// Drain the recorded config-override history (record order), leaving
    /// the accumulator empty. Rotates the history so high-frequency
    /// `merge_config` callers can keep it bounded by design.
    pub(crate) fn take_config_overrides(&self) -> Vec<ConfigOverrideRecord> {
        std::mem::take(&mut *self.config_overrides.borrow_mut())
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
            config_overrides: self.config_overrides.borrow().clone(),
            total_elapsed_us: *self.total_elapsed_us.borrow(),
            ..BuildReport::default()
        }
    }
}

/// Feature-gated fields aggregated on `AsyncKit` — the `Mutex` counterpart
/// of [`ReportFields`]. The async kit is a documented `Send + Sync` type, so
/// the `RefCell` original cannot be reused; the `modules`/`topo_order`/
/// `total_elapsed_us`/`contract` locks are confined to the build path (one
/// short critical section per module), while `config_overrides` is touched
/// on every `merge_config` call — a user API runnable at any time on any
/// thread, outside the build path. `AsyncKit<Ready>` reads the snapshot once
/// via `build_report()`.
#[cfg(feature = "async")]
#[derive(Default)]
pub(crate) struct AsyncReportFields {
    /// Per-module build records in build-completion order.
    modules: Mutex<Vec<ModuleReportEntry>>,
    /// Config override records captured at `merge_config` call time.
    config_overrides: Mutex<Vec<ConfigOverrideRecord>>,
    /// Topological order (module names) captured after `graph.validate()`.
    topo_order: Mutex<Vec<&'static str>>,
    /// Total wall time of `build()` (set when build finishes).
    total_elapsed_us: Mutex<Option<u64>>,
    /// Contract entries captured at registration time.
    contract: Mutex<Vec<ContractEntry>>,
}

#[cfg(feature = "async")]
impl AsyncReportFields {
    /// Record a module built from its `build_fn` (with construction time).
    pub(crate) fn push_built(&self, name: &'static str, elapsed_us: u64, deps: Vec<&'static str>) {
        self.modules
            .lock()
            .expect("AsyncKit report modules lock poisoned")
            .push(ModuleReportEntry {
                name,
                state: ModuleBuildState::Built,
                deps,
                elapsed_us: Some(elapsed_us),
            });
    }

    /// Record a config-level override (`merge_config`): `applied == false`
    /// surfaces an override that was dropped because the target config did
    /// not exist, instead of silently swallowing it.
    ///
    /// The only caller is the `confers`-gated `AsyncKit::merge_config`, so
    /// the method follows the same gate and stays dead-code-free under
    /// `async,report` alone (the snapshot field remains, always empty then).
    #[cfg(feature = "confers")]
    pub(crate) fn push_config_override(&self, record: ConfigOverrideRecord) {
        self.config_overrides
            .lock()
            .expect("AsyncKit report config_overrides lock poisoned")
            .push(record);
    }

    /// Drain the recorded config-override history (record order), leaving
    /// the accumulator empty. Rotates the history so high-frequency
    /// `merge_config` callers can keep it bounded by design; the swap runs
    /// inside the mutex, so concurrent `merge_config`/`take` calls never
    /// lose a record.
    pub(crate) fn take_config_overrides(&self) -> Vec<ConfigOverrideRecord> {
        std::mem::take(
            &mut *self
                .config_overrides
                .lock()
                .expect("AsyncKit report config_overrides lock poisoned"),
        )
    }

    /// Record the validated topological order (module names).
    pub(crate) fn set_topo_order(&self, names: Vec<&'static str>) {
        *self
            .topo_order
            .lock()
            .expect("AsyncKit report topo_order lock poisoned") = names;
    }

    /// Record the total `build()` wall time in microseconds.
    pub(crate) fn set_total_elapsed_us(&self, us: u64) {
        *self
            .total_elapsed_us
            .lock()
            .expect("AsyncKit report total_elapsed_us lock poisoned") = Some(us);
    }

    /// Record a module contract entry (name, version, capability, deps).
    pub(crate) fn push_contract(&self, entry: ContractEntry) {
        self.contract
            .lock()
            .expect("AsyncKit report contract lock poisoned")
            .push(entry);
    }

    /// Snapshot the registered module contracts.
    pub(crate) fn contract_snapshot(&self) -> ContractManifest {
        ContractManifest {
            schema_version: CONTRACT_SCHEMA_VERSION,
            modules: self
                .contract
                .lock()
                .expect("AsyncKit report contract lock poisoned")
                .clone(),
        }
    }

    /// Snapshot the accumulated build facts into a [`BuildReport`].
    ///
    /// The module-level `overrides` is always empty: the `override_module`
    /// family is a sync-only surface. Config-level overrides recorded via
    /// `merge_config` (including dropped ones, `applied == false`) do show
    /// up under `config_overrides`.
    pub(crate) fn snapshot(&self) -> BuildReport {
        BuildReport {
            topo_order: self
                .topo_order
                .lock()
                .expect("AsyncKit report topo_order lock poisoned")
                .clone(),
            modules: self
                .modules
                .lock()
                .expect("AsyncKit report modules lock poisoned")
                .clone(),
            config_overrides: self
                .config_overrides
                .lock()
                .expect("AsyncKit report config_overrides lock poisoned")
                .clone(),
            total_elapsed_us: *self
                .total_elapsed_us
                .lock()
                .expect("AsyncKit report total_elapsed_us lock poisoned"),
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

/// One config-level override captured in a [`BuildReport`], in record order.
///
/// Record order matches call order only for sequential (single-threaded)
/// call patterns: the async kit is a shared `Send + Sync` type, so
/// concurrent `merge_config` calls interleave their "apply" and "record"
/// steps across two independent locks, and the record sequence reflects
/// lock arrival rather than the order overrides actually landed.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigOverrideRecord {
    /// Overridden config type name (`std::any::type_name`). The exact
    /// format is not promised to be stable across compilers or versions —
    /// diagnostic use only; match by suffix, never by full equality.
    pub config: &'static str,
    /// Which API applied the override (e.g. `"merge_config"`).
    pub source: &'static str,
    /// Whether this call went through the apply step, i.e. the target
    /// config existed. `false` means the override was dropped — surfaced
    /// here instead of being silently swallowed. `true` does not promise
    /// the final config still contains the override: a concurrent
    /// `set_config`/`merge_config` may overwrite it afterwards
    /// (last-writer-wins).
    pub applied: bool,
}

impl ConfigOverrideRecord {
    /// Build the record for one `merge_config` call; keeps the `source`
    /// tag single-sourced for the sync and async call sites. Same feature
    /// gate as its only callers (`Kit::merge_config`/`AsyncKit::merge_config`).
    #[cfg(feature = "confers")]
    pub(crate) fn from_merge_config<C: ?Sized>(applied: bool) -> Self {
        Self {
            config: std::any::type_name::<C>(),
            source: "merge_config",
            applied,
        }
    }
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
    /// Override registrations observed at registration time (module-level
    /// `override_module` family; config-level overrides are recorded
    /// separately in [`BuildReport::config_overrides`]).
    pub overrides: Vec<OverrideRecord>,
    /// Config-level override records in record order (`merge_config`
    /// family; `applied == false` marks an override dropped on a missing
    /// config). Under concurrent async calls, record order is lock arrival
    /// order and may differ from the order overrides actually landed.
    ///
    /// # Usage warning: unbounded growth
    ///
    /// One entry per `merge_config` call, no cap or truncation — roughly
    /// 40 bytes each, on both `Kit` and the shared `Send + Sync` `AsyncKit`
    /// where `merge_config` is a runtime API callable at any time from any
    /// thread. A kit merging on every reload callback accumulates
    /// megabytes over a long process lifetime, and every `build_report()`
    /// snapshot clones the whole history. Drain periodically with
    /// `take_config_overrides()` (kit-level, `report` feature) at a
    /// natural rotation point instead of letting the history grow.
    pub config_overrides: Vec<ConfigOverrideRecord>,
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
            config_overrides: Vec::new(),
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
    #[cfg(feature = "confers")]
    fn config_override_records_reach_snapshot() {
        let fields = ReportFields::default();
        fields.push_config_override(ConfigOverrideRecord {
            config: "demo::HostConfig",
            source: "merge_config",
            applied: true,
        });
        fields.push_config_override(ConfigOverrideRecord {
            config: "demo::MissingConfig",
            source: "merge_config",
            applied: false,
        });

        let report = fields.snapshot();
        assert_eq!(report.config_overrides.len(), 2);
        assert_eq!(report.config_overrides[0].config, "demo::HostConfig");
        assert_eq!(report.config_overrides[0].source, "merge_config");
        assert!(report.config_overrides[0].applied);
        assert!(!report.config_overrides[1].applied);
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

    /// `merge_config` 的覆盖事实进报告：应用与被丢弃（目标配置不存在）都要
    /// 显性呈现，不允许静默吞并。
    #[cfg(all(test, feature = "confers"))]
    mod config_override_tests {
        use super::*;

        #[derive(Clone, Debug)]
        struct HostConfig {
            host: String,
            port: u16,
        }

        #[derive(Clone, Default)]
        struct HostConfigOverride {
            host: Option<String>,
            port: Option<u16>,
        }

        impl crate::kit::ConfigInherit for HostConfig {
            type Override = HostConfigOverride;
            fn apply_override(&mut self, ovr: &Self::Override) {
                if let Some(ref h) = ovr.host {
                    self.host.clone_from(h);
                }
                if let Some(ref p) = ovr.port {
                    self.port = *p;
                }
            }
        }

        #[test]
        fn merge_config_records_applied_override_in_build_report() {
            let mut kit = Kit::new();
            kit.register::<RptLeaf>().expect("register leaf");
            kit.set_config(HostConfig {
                host: "localhost".into(),
                port: 5432,
            });
            kit.merge_config::<HostConfig>(HostConfigOverride {
                host: Some("override.example.com".into()),
                port: None,
            });
            let ready = kit.build().expect("build ok");

            let report = ready.build_report();
            assert_eq!(report.config_overrides.len(), 1);
            let record = &report.config_overrides[0];
            assert_eq!(record.source, "merge_config");
            assert!(
                record.config.ends_with("HostConfig"),
                "config type name: {}",
                record.config
            );
            assert!(record.applied, "existing target config → applied");

            // 合并语义本身不受影响：Some 字段胜出，None 字段保留原值。
            let cfg = ready.config::<HostConfig>().expect("config present");
            assert_eq!(cfg.host, "override.example.com");
            assert_eq!(cfg.port, 5432);
        }

        #[test]
        fn merge_config_records_dropped_override_when_target_missing() {
            let mut kit = Kit::new();
            kit.register::<RptLeaf>().expect("register leaf");
            kit.merge_config::<HostConfig>(HostConfigOverride::default());
            let ready = kit.build().expect("build ok");

            let report = ready.build_report();
            assert_eq!(report.config_overrides.len(), 1);
            assert!(
                !report.config_overrides[0].applied,
                "dropped override must be surfaced in the report"
            );
            assert!(ready.config::<HostConfig>().is_err(), "config still absent");
        }

        #[test]
        fn config_override_records_preserve_call_order() {
            let mut kit = Kit::new();
            kit.register::<RptLeaf>().expect("register leaf");
            kit.set_config(HostConfig {
                host: "base".into(),
                port: 1,
            });
            kit.merge_config::<HostConfig>(HostConfigOverride {
                host: Some("first".into()),
                port: None,
            });
            kit.merge_config::<HostConfig>(HostConfigOverride {
                host: Some("second".into()),
                port: None,
            });
            let ready = kit.build().expect("build ok");

            let report = ready.build_report();
            assert_eq!(report.config_overrides.len(), 2);
            assert!(report.config_overrides.iter().all(|r| r.applied));
            // Sequential (single-threaded) calls: record order mirrors call
            // order, so the later merge wins — under concurrent async calls
            // the record sequence is lock arrival order only.
            let cfg = ready.config::<HostConfig>().expect("config present");
            assert_eq!(cfg.host, "second");
        }

        /// 排空语义：take 返回全部记录并清空历史，drain 后 snapshot 保持
        /// 为空——高频 `merge_config` 的长生命周期 kit 以此防无界累积。
        #[test]
        fn take_config_overrides_drains_report_history() {
            let mut kit = Kit::new();
            kit.register::<RptLeaf>().expect("register leaf");
            kit.set_config(HostConfig {
                host: "base".into(),
                port: 1,
            });
            kit.merge_config::<HostConfig>(HostConfigOverride {
                host: Some("first".into()),
                port: None,
            });
            kit.merge_config::<HostConfig>(HostConfigOverride {
                host: Some("second".into()),
                port: None,
            });

            let drained = kit.take_config_overrides();
            assert_eq!(drained.len(), 2);
            assert!(drained.iter().all(|r| r.applied));
            assert!(
                kit.take_config_overrides().is_empty(),
                "second take returns empty"
            );

            let ready = kit.build().expect("build ok");
            assert!(
                ready.build_report().config_overrides.is_empty(),
                "drained history stays drained across build"
            );
        }

        #[test]
        fn config_override_records_json_round_trip() {
            let mut kit = Kit::new();
            kit.register::<RptLeaf>().expect("register leaf");
            kit.set_config(HostConfig {
                host: "localhost".into(),
                port: 5432,
            });
            kit.merge_config::<HostConfig>(HostConfigOverride {
                host: Some("json.example.com".into()),
                port: None,
            });
            let ready = kit.build().expect("build ok");

            let json = ready.build_report().to_json().expect("serialize report");
            let value = BuildReport::from_json_str(&json).expect("valid JSON");
            assert_eq!(value["config_overrides"][0]["source"], "merge_config");
            assert_eq!(value["config_overrides"][0]["applied"], true);
        }
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

#[cfg(all(test, feature = "async"))]
mod async_report_fields_tests {
    use super::*;

    #[test]
    fn async_report_fields_accumulate_and_snapshot() {
        let fields = AsyncReportFields::default();
        fields.push_built("a", 12, vec![]);
        fields.push_built("b", 34, vec!["a"]);
        fields.set_topo_order(vec!["a", "b"]);
        fields.set_total_elapsed_us(100);
        fields.push_contract(ContractEntry {
            module: "a",
            version: "1.0.0",
            capability: "Cap",
            deps: vec![],
        });

        let report = fields.snapshot();
        assert_eq!(report.schema_version, SCHEMA_VERSION);
        assert_eq!(report.modules.len(), 2);
        assert_eq!(report.modules[0].name, "a");
        assert_eq!(report.modules[0].state, ModuleBuildState::Built);
        assert_eq!(report.modules[0].elapsed_us, Some(12));
        assert_eq!(report.modules[1].deps, vec!["a"]);
        assert_eq!(report.topo_order, vec!["a", "b"]);
        assert_eq!(report.total_elapsed_us, Some(100));
        assert!(report.overrides.is_empty(), "async has no override surface");

        let manifest = fields.contract_snapshot();
        assert_eq!(manifest.schema_version, CONTRACT_SCHEMA_VERSION);
        assert_eq!(manifest.modules.len(), 1);
        assert_eq!(manifest.modules[0].module, "a");
    }

    #[test]
    #[cfg(feature = "confers")]
    fn async_config_override_records_reach_snapshot() {
        let fields = AsyncReportFields::default();
        fields.push_config_override(ConfigOverrideRecord {
            config: "demo::HostConfig",
            source: "merge_config",
            applied: false,
        });

        let report = fields.snapshot();
        assert_eq!(report.config_overrides.len(), 1);
        assert_eq!(report.config_overrides[0].source, "merge_config");
        assert!(!report.config_overrides[0].applied);
    }

    #[test]
    fn async_report_fields_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AsyncReportFields>();
    }
}
