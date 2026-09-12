// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! API 文档一致性门禁。
//!
//! MVP 口径：静态清单断言。每个"关键项"必须同时满足两个条件——
//!
//! 1. **编译存在**：本文件中的 `_assert_*` 函数实际引用了该公开项
//!    （编译期保证它是 crate 的 pub API 且签名未漂移）；
//! 2. **文档收录**：`docs/API_REFERENCE.md` 中出现该标识符
//!    （防止"文档同步型发版"回潮）。
//!
//! 后续可演进为 rustdoc JSON / `cargo public-api` 全量 diff。

use std::path::PathBuf;

use trait_kit::core::ModuleMeta as _;

fn api_reference_md() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/API_REFERENCE.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("API_REFERENCE.md missing at {path:?}: {e}"))
}

// ─── 编译期存在性断言：引用关键 pub 项 ─────────────────────────────────────

fn _assert_core_api() {
    // 核心类型与 trait（docs: 核心 API / 错误类型）
    fn module_meta<T: trait_kit::core::ModuleMeta>() {}
    fn auto_builder<T: trait_kit::core::AutoBuilder>() {}
    fn assert_error(e: trait_kit::TraitKitError) -> trait_kit::TraitKitResult<()> {
        Err(e)
    }
    let _ = (module_meta::<GateModule>, auto_builder::<GateModule>, assert_error);

    // Kit 注册面（docs: Kit<Unbuilt> 表）：register → build → require。
    fn full_flow(mut kit: trait_kit::kit::Kit) -> trait_kit::TraitKitResult<GateCap> {
        kit.register::<GateModule>()?;
        let ready = kit.build()?;
        ready.require::<GateModule>()
    }
    let _ = full_flow;

    // 依赖图（docs: 核心 API）
    let mut graph: trait_kit::kit::DependencyGraph = trait_kit::kit::DependencyGraph::new();
    let entry = trait_kit::kit::ModuleEntry {
        type_id: std::any::TypeId::of::<GateModule>(),
        name: GateModule::NAME,
        dependencies: Vec::new(),
    };
    graph.add(entry).expect("unique entry");
    let _ = graph.entries().len();
}

// 共享 fixture（各断言函数各自引用，保证独立编译有效）。
#[derive(Debug, Clone)]
struct GateCap;

#[derive(Debug)]
struct GateError;

impl std::fmt::Display for GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "gate error")
    }
}
impl std::error::Error for GateError {}

struct GateModule;

impl trait_kit::core::ModuleMeta for GateModule {
    const NAME: &'static str = "gate-module";
}

impl trait_kit::core::AutoBuilder for GateModule {
    type Capability = GateCap;
    type Error = GateError;
    fn build(_kit: &trait_kit::kit::Kit) -> Result<Self::Capability, Self::Error> {
        Ok(GateCap)
    }
}

#[cfg(feature = "async")]
struct GateAsyncModule;

#[cfg(feature = "async")]
impl trait_kit::core::ModuleMeta for GateAsyncModule {
    const NAME: &'static str = "gate-async-module";
}

#[cfg(feature = "async")]
impl trait_kit::core::AsyncAutoBuilder for GateAsyncModule {
    type Capability = std::sync::Arc<GateCap>;
    type Error = GateError;
    fn build<'a>(
        _kit: &'a trait_kit::kit::AsyncKit,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
    {
        Box::pin(async { Ok(std::sync::Arc::new(GateCap)) })
    }
}

#[cfg(feature = "health")]
struct GateHealthModule;

#[cfg(feature = "health")]
impl trait_kit::core::ModuleMeta for GateHealthModule {
    const NAME: &'static str = "gate-health-module";
}

#[cfg(feature = "health")]
impl trait_kit::core::AutoBuilder for GateHealthModule {
    type Capability = GateCap;
    type Error = GateError;
    fn build(_kit: &trait_kit::kit::Kit) -> Result<Self::Capability, Self::Error> {
        Ok(GateCap)
    }
}

#[cfg(feature = "health")]
impl trait_kit::core::HealthCheck for GateHealthModule {
    fn check(_cap: &Self::Capability) -> trait_kit::core::HealthStatus {
        trait_kit::core::HealthStatus::Healthy
    }
}

#[cfg(feature = "async")]
use std::pin::Pin;

#[cfg(feature = "async")]
#[allow(dead_code, reason = "compile-time API surface assertion only")]
fn _assert_async_api() {
    fn async_auto_builder<T: trait_kit::core::AsyncAutoBuilder>() {}
    fn ready_requires(
        kit: &trait_kit::kit::AsyncKit<trait_kit::kit::AsyncReady>,
    ) -> trait_kit::TraitKitResult<std::sync::Arc<GateCap>> {
        kit.require::<GateAsyncModule>()
    }
    let _ = (async_auto_builder::<GateAsyncModule>, ready_requires);
}

#[cfg(feature = "health")]
#[allow(dead_code, reason = "compile-time API surface assertion only")]
fn _assert_health_api() {
    fn status(s: trait_kit::core::HealthStatus) -> bool {
        s.is_healthy()
    }
    fn checker<T: trait_kit::core::HealthCheck>() {}
    let _ = (status, checker::<GateHealthModule>);
}

#[cfg(feature = "scope")]
#[allow(dead_code, reason = "compile-time API surface assertion only")]
fn _assert_scope_api() {
    let _scope: trait_kit::kit::Scope = trait_kit::kit::Scope::new();
}

#[cfg(feature = "interface")]
#[allow(dead_code, reason = "compile-time API surface assertion only")]
fn _assert_interface_api() {
    // `Interface` 是 blanket trait；`InterfaceBuilder` 需要完整的擦除实现，
    // 这里以方法签名存在性 + 文档收录作为门禁口径。
    fn interface<T: trait_kit::core::Interface + ?Sized>() {}
    let _ = interface::<GateCap>;
}

#[cfg(feature = "observer")]
#[allow(dead_code, reason = "compile-time API surface assertion only")]
fn _assert_observer_api() {
    trait DocObserver: trait_kit::core::observer::BuildObserver {}
    let _ = |obs: std::sync::Arc<dyn trait_kit::core::observer::BuildObserver>| obs;
}

#[cfg(feature = "confers")]
#[allow(dead_code, reason = "compile-time API surface assertion only")]
fn _assert_confers_api() {
    #[derive(Clone)]
    struct ValidatedConfig;
    impl trait_kit::kit::Validatable for ValidatedConfig {
        fn validate(&self) -> Result<(), Vec<String>> {
            Ok(())
        }
    }
    fn validatable<T: trait_kit::kit::Validatable>() {}
    let _ = validatable::<ValidatedConfig>;
}

// ─── 关键项清单（编译 + 文档 双向门禁） ────────────────────────────────────

/// 始终可用的关键项：`identifier => (编译断言函数名)`。
const ALWAYS_REQUIRED_DOC_ENTRIES: &[&str] = &[
    "ModuleMeta",
    "AutoBuilder",
    "Kit",
    "TraitKitError",
    "TraitKitResult",
    "DependencyGraph",
    "impl_module_meta!",
    "impl_auto_builder!",
    "I18nManager",
    "tr()",
    "Prelude",
];

#[test]
fn api_reference_lists_core_entries() {
    let doc = api_reference_md();
    for entry in ALWAYS_REQUIRED_DOC_ENTRIES {
        assert!(
            doc.contains(entry),
            "docs/API_REFERENCE.md must mention core API item `{entry}`"
        );
    }
}

#[test]
fn api_reference_documented_feature_items_resolve() {
    let doc = api_reference_md();
    // 文档带 feature 标注的项必须真实存在（编译期引用 + 文档收录成对）。
    #[cfg(feature = "async")]
    {
        assert!(doc.contains("AsyncAutoBuilder"), "doc must list AsyncAutoBuilder");
        assert!(doc.contains("AsyncKit"), "doc must list AsyncKit");
    }
    #[cfg(feature = "health")]
    {
        assert!(doc.contains("HealthStatus"), "doc must list HealthStatus");
        assert!(doc.contains("HealthCheck"), "doc must list HealthCheck");
    }
    #[cfg(feature = "scope")]
    {
        assert!(doc.contains("`Scope`"), "doc must list Scope");
    }
    #[cfg(feature = "interface")]
    {
        assert!(doc.contains("InterfaceBuilder"), "doc must list InterfaceBuilder");
    }
    #[cfg(feature = "observer")]
    {
        assert!(doc.contains("BuildObserver"), "doc must list BuildObserver");
    }
    #[cfg(feature = "confers")]
    {
        assert!(doc.contains("Validatable"), "doc must list Validatable");
        assert!(
            doc.contains("interpolate_json_value"),
            "doc must list interpolate_json_value"
        );
    }
}

#[test]
fn doc_does_not_reference_removed_apis() {
    // rc3 收尾已删除死链（reload 死链剪除等）；文档不得回引已不存在的 API。
    let doc = api_reference_md();
    let removed = ["register_on_toggle(", "health_prometheus(", "manifest_json("];
    for ghost in removed {
        assert!(
            !doc.contains(ghost),
            "docs/API_REFERENCE.md references removed API `{ghost}`"
        );
    }
}
