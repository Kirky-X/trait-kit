// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//
// E2E 缺口补盲（验收清单第 2 轮）：
// 覆盖既有测试资产未落到 e2e 目标的边界契约。每个模块按 feature 独立
// 门控，任意 feature 组合（含无 feature）可编译。

// 场景 249：DOT/Mermaid 的未注册依赖边差异与标签转义。
// `to_dot` 不校验依赖注册（照常输出边），`to_mermaid` 跳过未注册依赖
// （不产生误导自环）；反斜杠/双引号/换行按 DOT 字符串字面量转义。
mod gap249_graph_export_contrast {
    use std::any::TypeId;
    use trait_kit::kit::{DependencyGraph, ModuleEntry};

    #[test]
    fn unregistered_dependency_emits_dot_edge_but_mermaid_skips_it() {
        let mut graph = DependencyGraph::new();
        graph
            .add(ModuleEntry {
                type_id: TypeId::of::<u8>(),
                name: "dep-ok",
                dependencies: vec![],
            })
            .expect("add dep-ok");
        graph
            .add(ModuleEntry {
                type_id: TypeId::of::<u16>(),
                name: "consumer",
                dependencies: vec![
                    ("dep-ok", TypeId::of::<u8>()),
                    ("dep-missing", TypeId::of::<u32>()),
                ],
            })
            .expect("add consumer");

        let dot = graph.to_dot();
        assert!(
            dot.contains("\"dep-ok\" -> \"consumer\";"),
            "registered edge present in DOT: {dot}"
        );
        assert!(
            dot.contains("\"dep-missing\" -> \"consumer\";"),
            "to_dot emits edges without validating registration: {dot}"
        );

        let mermaid = graph.to_mermaid();
        assert!(
            mermaid.contains("n0[\"dep-ok\"] --> n1[\"consumer\"]"),
            "registered edge present in Mermaid: {mermaid}"
        );
        assert!(
            !mermaid.contains("dep-missing"),
            "to_mermaid skips unregistered dependencies (no misleading self-loop): {mermaid}"
        );
    }

    #[test]
    fn dot_and_mermaid_escape_backslash_quote_and_newline() {
        let mut graph = DependencyGraph::new();
        graph
            .add(ModuleEntry {
                type_id: TypeId::of::<u32>(),
                name: "we\"ird\\name\nx",
                dependencies: vec![],
            })
            .expect("add escaped node");

        let dot = graph.to_dot();
        assert!(
            dot.contains(r#""we\"ird\\name\nx";"#),
            "DOT label escapes backslash/quote/newline: {dot}"
        );

        let mermaid = graph.to_mermaid();
        assert!(
            mermaid.contains(r#"n0["we\"ird\\name\nx"]"#),
            "Mermaid label uses the same escapes: {mermaid}"
        );
    }
}

// 场景 251：set_config 与 set_config_arc 槽位隔离（交叉读取均
// MissingConfig），同槽二次写为 last-write-wins。
#[cfg(feature = "confers")]
mod gap251_config_arc_slot_isolation {
    use trait_kit::kit::Kit;
    use trait_kit::prelude::*;

    #[derive(Debug, Clone, PartialEq)]
    struct SlotCfg {
        v: u32,
    }

    #[test]
    fn plain_and_arc_slots_do_not_cross_read() {
        let kit = Kit::new();
        kit.set_config(SlotCfg { v: 1 });
        assert_eq!(kit.config::<SlotCfg>().expect("plain slot").v, 1);
        assert!(
            matches!(
                kit.config_arc::<SlotCfg>(),
                Err(TraitKitError::MissingConfig { .. })
            ),
            "config_arc must not see the plain slot"
        );

        let kit = Kit::new();
        kit.set_config_arc(SlotCfg { v: 2 });
        assert!(
            matches!(
                kit.config::<SlotCfg>(),
                Err(TraitKitError::MissingConfig { .. })
            ),
            "config must not see the arc slot"
        );
        assert_eq!(kit.config_arc::<SlotCfg>().expect("arc slot").v, 2);

        kit.set_config_arc(SlotCfg { v: 3 });
        assert_eq!(
            kit.config_arc::<SlotCfg>().expect("arc slot overwrite").v,
            3,
            "second write in the same slot is last-write-wins"
        );
    }
}

// 场景 252：健康历史默认容量 32、oldest-first 顺序、超容量逐出最旧样本，
// 以及缩容即时截断。
#[cfg(feature = "health")]
mod gap252_health_history_default_capacity {
    use std::sync::Arc;
    use trait_kit::core::{AutoBuilder, HealthCheck, HealthStatus};
    use trait_kit::kit::Kit;
    use trait_kit::{TraitKitError, impl_module_meta};

    struct HistMod;
    impl_module_meta!(HistMod, "gap252-hist");
    impl AutoBuilder for HistMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl HealthCheck for HistMod {
        fn check(_cap: &Self::Capability) -> HealthStatus {
            HealthStatus::degraded("sampled")
        }
    }

    #[test]
    fn default_capacity_is_32_and_ring_evicts_oldest_first() {
        let mut kit = Kit::new();
        kit.register::<HistMod>().expect("register");
        kit.register_health_check::<HistMod>();
        let ready = kit.build().expect("build");

        for _ in 0..40 {
            ready.record_health_history();
        }
        let history = ready.health_history();
        assert_eq!(history.len(), 32, "default ring capacity is 32");
        assert!(
            history
                .windows(2)
                .all(|w| w[0].sampled_at <= w[1].sampled_at),
            "history is oldest-first"
        );
        assert_eq!(history[0].module, "gap252-hist");

        ready.set_health_history_capacity(5);
        assert_eq!(
            ready.health_history().len(),
            5,
            "shrinking trims the oldest samples immediately"
        );
    }
}

// 场景 254：lazy 模块未首建即 shutdown——能力缺失的 on_shutdown 静默跳过，
// 不报错；eager 已构建模块的回调照常执行（对照）。
#[cfg(feature = "lifecycle")]
mod gap254_lazy_unbuilt_shutdown_skipped {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use trait_kit::core::{AutoBuilder, Lifecycle};
    use trait_kit::kit::Kit;
    use trait_kit::{TraitKitError, impl_module_meta};

    static EAGER_SHUT_CALLS: AtomicUsize = AtomicUsize::new(0);
    static LAZY_SHUT_CALLS: AtomicUsize = AtomicUsize::new(0);

    struct EagerMod;
    impl_module_meta!(EagerMod, "gap254-eager");
    impl AutoBuilder for EagerMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(3))
        }
    }
    impl Lifecycle for EagerMod {
        fn on_shutdown(_cap: &Arc<u32>) {
            EAGER_SHUT_CALLS.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct LazyMod;
    impl_module_meta!(LazyMod, "gap254-lazy");
    impl AutoBuilder for LazyMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(7))
        }
    }
    impl Lifecycle for LazyMod {
        fn on_shutdown(_cap: &Arc<u32>) {
            LAZY_SHUT_CALLS.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn unbuilt_lazy_module_on_shutdown_is_silently_skipped() {
        EAGER_SHUT_CALLS.store(0, Ordering::SeqCst);
        LAZY_SHUT_CALLS.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<EagerMod>().expect("register eager");
        kit.register_lazy::<LazyMod>().expect("register_lazy");
        kit.register_lifecycle::<EagerMod>();
        kit.register_lifecycle::<LazyMod>();
        let ready = kit.build().expect("build");
        assert!(!ready.contains::<LazyMod>(), "lazy module not yet built");
        ready.shutdown();
        assert_eq!(
            EAGER_SHUT_CALLS.load(Ordering::SeqCst),
            1,
            "an eager built module's on_shutdown runs"
        );
        assert_eq!(
            LAZY_SHUT_CALLS.load(Ordering::SeqCst),
            0,
            "no capability → no on_shutdown, and no error"
        );
    }

    /// 观测固化：lazy 槽位的能力存放在 `lazy_slots` 缓存而不是 `capabilities`
    /// TypeMap，shutdown 回调按 TypeMap 查能力，因此 lazy 模块即使已首建，
    /// `on_shutdown` 也静默跳过（既不报错也不执行）。这是当前实现的真实
    /// 口径，与「未首建跳过」一致。
    #[test]
    fn lazy_capability_lives_outside_typemap_so_shutdown_skips_it_either_way() {
        LAZY_SHUT_CALLS.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register_lazy::<LazyMod>().expect("register_lazy");
        kit.register_lifecycle::<LazyMod>();
        let ready = kit.build().expect("build");
        let _ = ready
            .require::<LazyMod>()
            .expect("first require builds lazily");
        assert!(ready.contains::<LazyMod>(), "lazy module is built now");
        ready.shutdown();
        assert_eq!(LAZY_SHUT_CALLS.load(Ordering::SeqCst), 0);
    }
}

// 场景 259：探针 panic 未被隔离——run_probes 直接展开 panic，而不是把
// panic 记成 unhealthy 并继续（与 shutdown hook / 事件订阅者的
// catch_unwind 隔离策略对照）。
#[cfg(all(feature = "async", feature = "probe"))]
mod gap259_probe_panic_propagates {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::{Context, Poll, Waker};
    use trait_kit::core::probe::{ProbeOutcome, ServiceProbe};
    use trait_kit::core::{AsyncAutoBuilder, HealthStatus};
    use trait_kit::kit::AsyncKit;
    use trait_kit::{TraitKitError, impl_module_meta};

    /// 最小单线程 Future 执行器（集成测试不可达 crate 内部的 block_on）。
    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct ProbeMod;
    impl_module_meta!(ProbeMod, "gap259-probe");
    impl AsyncAutoBuilder for ProbeMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            Box::pin(async { Ok(Arc::new(1)) })
        }
    }

    static HEALTHY_RAN: AtomicBool = AtomicBool::new(false);

    struct HealthyProbe;
    impl ServiceProbe for HealthyProbe {
        fn probe<'a>(&'a self) -> Pin<Box<dyn Future<Output = ProbeOutcome> + Send + 'a>> {
            Box::pin(async {
                HEALTHY_RAN.store(true, Ordering::SeqCst);
                ProbeOutcome {
                    status: HealthStatus::Healthy,
                    latency: std::time::Duration::ZERO,
                }
            })
        }
    }

    struct PanicProbe;
    impl ServiceProbe for PanicProbe {
        fn probe<'a>(&'a self) -> Pin<Box<dyn Future<Output = ProbeOutcome> + Send + 'a>> {
            Box::pin(async { panic!("probe exploded") })
        }
    }

    #[test]
    fn run_probes_lets_a_panicking_probe_unwind() {
        HEALTHY_RAN.store(false, Ordering::SeqCst);
        let mut kit = AsyncKit::new();
        kit.register::<ProbeMod>().expect("register");
        kit.register_probe("healthy-first", Arc::new(HealthyProbe));
        kit.register_probe("panics", Arc::new(PanicProbe));
        let ready = block_on(kit.build()).expect("build");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            block_on(ready.run_probes())
        }));
        assert!(
            result.is_err(),
            "a panicking probe must unwind the run_probes caller, not be \
             recorded as unhealthy"
        );
        assert!(
            HEALTHY_RAN.load(Ordering::SeqCst),
            "the preceding healthy probe already ran before the panic"
        );
    }
}

// 场景 264：健康事件的发布点口径——health_report / health_aggregate /
// health_json 每个模块各发布一条 HealthChanged；
// record_health_history 不发布事件。
#[cfg(all(feature = "health", feature = "report"))]
mod gap264_health_event_publish_ledger {
    use std::sync::{Arc, Mutex};
    use trait_kit::core::{AutoBuilder, HealthCheck, HealthStatus};
    use trait_kit::kit::Kit;
    use trait_kit::kit::events::{EventBus, KitEvent, MemoryEventBus};
    use trait_kit::{TraitKitError, impl_module_meta};

    struct SickMod;
    impl_module_meta!(SickMod, "gap264-sick");
    impl AutoBuilder for SickMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl HealthCheck for SickMod {
        fn check(_cap: &Self::Capability) -> HealthStatus {
            HealthStatus::unhealthy("simulated failure")
        }
    }

    #[test]
    fn report_aggregate_and_json_publish_once_per_module_record_does_not() {
        let bus = Arc::new(MemoryEventBus::new());
        let events: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        bus.subscribe(move |event| {
            if let KitEvent::HealthChanged { module, status, .. } = event {
                sink.lock()
                    .unwrap()
                    .push((module.to_string(), status.to_string()));
            }
        });

        let mut kit = Kit::new();
        kit.with_event_bus(Some(Arc::clone(&bus) as Arc<dyn EventBus>));
        kit.register::<SickMod>().expect("register");
        kit.register_health_check::<SickMod>();
        let ready = kit.build().expect("build");

        let report = ready.health_report();
        assert_eq!(report.len(), 1);
        assert_eq!(events.lock().unwrap().len(), 1, "health_report: one event");

        let aggregate = ready.health_aggregate();
        assert_eq!(aggregate.status, "unhealthy");
        assert_eq!(
            events.lock().unwrap().len(),
            2,
            "health_aggregate: one event"
        );

        let json = ready.health_json().expect("health json");
        assert!(json.contains("\"unhealthy\""));
        assert_eq!(events.lock().unwrap().len(), 3, "health_json: one event");

        ready.record_health_history();
        assert_eq!(
            events.lock().unwrap().len(),
            3,
            "record_health_history samples without publishing events"
        );

        let log = events.lock().unwrap();
        assert!(
            log.iter()
                .all(|(m, s)| m == "gap264-sick" && s == "unhealthy"),
            "every event names the module and the status: {log:?}"
        );
    }
}

// 场景 265：register_confers_config 二次调用返回 AlreadyRegistered，但失败
// 调用的副作用——provider 槽位已被第二次的 provider 覆盖——仍是可观察的。
#[cfg(feature = "presets")]
mod gap265_presets_duplicate_overwrites_provider_slot {
    use std::collections::HashMap;
    use std::sync::Arc;
    use trait_kit::kit::Kit;
    use trait_kit::kit::presets::{ConfersConfigModule, register_confers_config};
    use trait_kit::prelude::*;

    struct MapProvider {
        pairs: HashMap<String, confers::AnnotatedValue>,
    }

    impl MapProvider {
        fn boxed(value: &str) -> Arc<dyn confers::ConfigProvider> {
            let key = "k";
            let mut pairs = HashMap::new();
            pairs.insert(
                key.to_string(),
                confers::AnnotatedValue::new(
                    confers::ConfigValue::String(value.into()),
                    confers::SourceId::default(),
                    key,
                ),
            );
            Arc::new(Self { pairs })
        }
    }

    impl confers::ConfigProvider for MapProvider {
        fn get_raw(&self, key: &str) -> Option<&confers::AnnotatedValue> {
            self.pairs.get(key)
        }

        fn keys(&self) -> Vec<String> {
            self.pairs.keys().cloned().collect()
        }
    }

    #[test]
    fn duplicate_registration_errors_but_slot_holds_second_provider() {
        let mut kit = Kit::new();
        register_confers_config(&mut kit, MapProvider::boxed("A")).expect("first registration");

        let err = register_confers_config(&mut kit, MapProvider::boxed("B"))
            .expect_err("second registration must conflict");
        assert!(matches!(err, TraitKitError::AlreadyRegistered { .. }));

        let ready = kit.build().expect("build");
        let handle = ready
            .require::<ConfersConfigModule>()
            .expect("handle capability");
        assert_eq!(
            handle.get_string("k").as_deref(),
            Some("B"),
            "the failed call still overwrote the provider slot before register failed"
        );
    }
}

// 场景 268：两层嵌套子 Kit——能力只能逐层下钻，孙能力不穿透任何一层
// 命名空间。
#[cfg(feature = "compose")]
mod gap268_two_level_nested_sub_kit {
    use std::sync::Arc;
    use trait_kit::impl_module_meta;
    use trait_kit::kit::{SubKitModule, SubKitSpec};
    use trait_kit::prelude::*;

    struct Leaf;
    impl_module_meta!(Leaf, "gap268-leaf");
    impl AutoBuilder for Leaf {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(7))
        }
    }

    struct InnerSpec;
    impl SubKitSpec for InnerSpec {
        const NAME: &'static str = "gap268-inner";
        fn compose(kit: &mut Kit) {
            kit.register::<Leaf>().expect("register leaf");
        }
    }

    struct OuterSpec;
    impl SubKitSpec for OuterSpec {
        const NAME: &'static str = "gap268-outer";
        fn compose(kit: &mut Kit) {
            kit.register::<SubKitModule<InnerSpec>>()
                .expect("register inner sub-kit");
        }
    }

    #[test]
    fn grandchild_requires_two_level_drill_down() {
        let mut parent = Kit::new();
        parent
            .register::<SubKitModule<OuterSpec>>()
            .expect("register outer");
        let ready = parent.build().expect("parent build");

        assert!(
            !ready.contains::<Leaf>(),
            "grandchild must not leak into the parent namespace"
        );

        let outer = ready
            .require::<SubKitModule<OuterSpec>>()
            .expect("outer handle");
        assert!(
            !outer.contains::<Leaf>(),
            "the middle layer must not expose the grandchild directly"
        );
        assert_eq!(
            outer.module_count(),
            1,
            "outer child holds only the inner sub-kit"
        );

        let inner = outer
            .require::<SubKitModule<InnerSpec>>()
            .expect("drill down one level");
        assert!(inner.contains::<Leaf>());
        assert_eq!(*inner.require::<Leaf>().expect("grandchild capability"), 7);
    }
}

// 场景 1/52：空 Kit 构建与 `module_count` / `graph_dot` 口径。
mod gap001_empty_kit_and_module_count {
    use std::sync::Arc;
    use trait_kit::impl_module_meta;
    use trait_kit::kit::Kit;
    use trait_kit::prelude::*;

    struct M1;
    impl_module_meta!(M1, "gap001-m1");
    impl AutoBuilder for M1 {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }

    struct M2;
    impl_module_meta!(M2, "gap001-m2");
    impl AutoBuilder for M2 {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(2))
        }
    }

    #[test]
    fn empty_kit_builds_ready_with_zero_modules_and_valid_dot() {
        let ready = Kit::new().build().expect("empty Kit build succeeds");
        assert_eq!(ready.module_count(), 0, "no modules registered");
        let dot = ready.graph_dot();
        assert!(
            dot.starts_with("digraph {") && dot.trim_end().ends_with('}'),
            "valid DOT skeleton: {dot}"
        );
        assert!(!dot.contains("->"), "no edges on an empty graph: {dot}");
        assert_eq!(ready.graph_mermaid(), "graph TD");
    }

    #[test]
    fn module_count_reflects_registration_count() {
        let mut kit = Kit::new();
        kit.register::<M1>().expect("register m1");
        kit.register::<M2>().expect("register m2");
        let ready = kit.build().expect("build");
        assert_eq!(ready.module_count(), 2);
    }
}

// 场景 27：eager 模块 `require` 返回单例克隆——两次 require 同指针、
// builder 只执行一次。
mod gap027_require_returns_singleton_clone {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use trait_kit::impl_module_meta;
    use trait_kit::kit::Kit;
    use trait_kit::prelude::*;

    static BUILDS: AtomicUsize = AtomicUsize::new(0);

    struct Singleton;
    impl_module_meta!(Singleton, "gap027-singleton");
    impl AutoBuilder for Singleton {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(9))
        }
    }

    #[test]
    fn require_twice_yields_same_arc_without_rebuild() {
        BUILDS.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<Singleton>().expect("register");
        let ready = kit.build().expect("build");
        let a = ready.require::<Singleton>().expect("first require");
        let b = ready.require::<Singleton>().expect("second require");
        assert!(Arc::ptr_eq(&a, &b), "singleton clone aliases the same Arc");
        assert_eq!(*a, 9);
        assert_eq!(BUILDS.load(Ordering::SeqCst), 1, "build ran exactly once");
    }
}

// 场景 260/91/93：加密版本信封 API——显式版本写入与读回、版本不匹配
// fail-closed、错误旧钥轮换 fail-closed（密文保持原样）后正轮换成功。
#[cfg(feature = "encryption")]
mod gap260_encrypted_version_envelope {
    use trait_kit::TraitKitError;
    use trait_kit::kit::{Kit, ModuleConfig};

    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct VerCfg {
        token: String,
    }

    impl ModuleConfig for VerCfg {
        const PATH: &'static str = "gap260-ver";
        fn default_value() -> Self {
            Self {
                token: "default".into(),
            }
        }
    }

    // 32 字节测试主密钥（夹具，非真实凭据）。
    // pragma: allowlist secret
    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";
    // pragma: allowlist secret
    const KEY2: &[u8] = b"fedcba9876543210fedcba9876543210";

    #[test]
    fn explicit_version_write_read_back_and_mismatch() {
        let v5 = VerCfg { token: "v5".into() };
        let kit = Kit::new();
        kit.set_encrypted_with_version(&v5, KEY, 5)
            .expect("explicit-version write");
        let ready = kit.build().expect("build");

        assert_eq!(ready.encrypted_key_version::<VerCfg>().unwrap(), 5);
        assert_eq!(
            ready.get_encrypted_with_version::<VerCfg>(KEY, 5).unwrap(),
            v5
        );
        assert_eq!(ready.get_encrypted::<VerCfg>(KEY).unwrap(), v5);

        let err = ready
            .get_encrypted_with_version::<VerCfg>(KEY, 2)
            .expect_err("version mismatch must fail");
        match &err {
            TraitKitError::BuildFailed { context, .. } => {
                assert_eq!(context, "get_encrypted_with_version");
            }
            other => panic!("expected BuildFailed, got {other:?}"),
        }
        assert!(err.to_string().contains("key-version mismatch"));
    }

    #[test]
    fn rotate_wrong_old_key_is_fail_closed_then_succeeds() {
        let original = VerCfg {
            token: "rotate".into(),
        };
        let kit = Kit::new();
        kit.set_encrypted(&original, KEY).expect("initial write");
        let ready = kit.build().expect("build");

        let err = ready
            .rotate_master_key::<VerCfg>(KEY2, KEY2)
            .expect_err("wrong old key must fail closed");
        assert!(matches!(err, TraitKitError::BuildFailed { .. }));
        assert_eq!(
            ready.get_encrypted::<VerCfg>(KEY).unwrap(),
            original,
            "ciphertext stays untouched after a failed rotation"
        );
        assert_eq!(ready.encrypted_key_version::<VerCfg>().unwrap(), 1);

        let bumped = ready
            .rotate_master_key::<VerCfg>(KEY, KEY2)
            .expect("valid rotation");
        assert_eq!(bumped, 2, "envelope version bumps by one");
        assert_eq!(ready.get_encrypted::<VerCfg>(KEY2).unwrap(), original);
        assert_eq!(ready.encrypted_key_version::<VerCfg>().unwrap(), 2);
    }
}

// 场景 19：`register_if` 谓词为真但模块已注册 → `AlreadyRegistered`；
// 谓词为假直接跳过（返回 false，不触发注册）。
mod gap019_register_if_duplicate_conflict {
    use std::sync::Arc;
    use trait_kit::impl_module_meta;
    use trait_kit::kit::Kit;
    use trait_kit::prelude::*;

    struct Mod;
    impl_module_meta!(Mod, "gap019-mod");
    impl AutoBuilder for Mod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }

    #[test]
    fn register_if_true_after_register_returns_already_registered() {
        let mut kit = Kit::new();
        kit.register::<Mod>().expect("first register");
        let err = kit
            .register_if::<Mod>(|_| true)
            .expect_err("second registration must conflict");
        assert!(matches!(err, TraitKitError::AlreadyRegistered { .. }));
        assert!(
            !kit.register_if::<Mod>(|_| false).expect("false predicate"),
            "a false predicate skips registration without error"
        );
    }
}
