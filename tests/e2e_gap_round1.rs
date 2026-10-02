// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//
// E2E 缺口补盲（验收清单第 1 轮）：
// 覆盖 docs/TEST_SCENARIOS.md 之外、验收清单 342/343/344/346/349/350/
// 353/354/355/356/360 号场景——既有测试资产未覆盖的 Kit 边界契约。
// 每个模块按 feature 独立门控，任意 feature 组合（含无 feature）可编译。

// 场景 342：`Kit<Ready>::shutdown` 二次调用 no-op（drain 语义）。
#[cfg(feature = "lifecycle")]
mod gap_342_ready_shutdown_twice_noop {
    use std::sync::{Arc, Mutex};
    use trait_kit::core::{AutoBuilder, Lifecycle};
    use trait_kit::impl_module_meta;
    use trait_kit::kit::Kit;
    use trait_kit::prelude::*;

    static SHUT_ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    struct GapLeaf;
    impl_module_meta!(GapLeaf, "gap342-leaf");
    impl AutoBuilder for GapLeaf {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl Lifecycle for GapLeaf {
        fn on_shutdown(_cap: &Arc<u32>) {
            SHUT_ORDER.lock().unwrap().push("gap342-leaf");
        }
    }

    struct GapTop;
    impl_module_meta!(GapTop, "gap342-top");
    impl AutoBuilder for GapTop {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(2))
        }
    }
    impl Lifecycle for GapTop {
        fn on_shutdown(_cap: &Arc<u32>) {
            SHUT_ORDER.lock().unwrap().push("gap342-top");
        }
    }

    #[test]
    fn ready_shutdown_second_call_is_noop() {
        SHUT_ORDER.lock().unwrap().clear();
        let mut kit = Kit::new();
        kit.register::<GapLeaf>().unwrap();
        kit.register::<GapTop>().unwrap();
        kit.register_lifecycle::<GapLeaf>();
        kit.register_lifecycle::<GapTop>();
        let ready = kit.build().unwrap();

        ready.shutdown();
        assert_eq!(
            *SHUT_ORDER.lock().unwrap(),
            vec!["gap342-top", "gap342-leaf"],
            "首次 shutdown 逆构建序执行"
        );

        ready.shutdown();
        assert_eq!(
            *SHUT_ORDER.lock().unwrap(),
            vec!["gap342-top", "gap342-leaf"],
            "第二次 shutdown 对已 drain 的回调表必须为 no-op"
        );
    }
}

// 场景 343：`is_toggle_enabled` 对非 Bool 值返回 false。
#[cfg(feature = "toggle")]
mod gap_343_toggle_non_bool_false {
    use trait_kit::kit::{Kit, ToggleValue};

    #[test]
    fn is_toggle_enabled_false_for_int_float_str_values() {
        let kit = Kit::new();
        kit.set_toggle("gap343.int", ToggleValue::Int(1));
        kit.set_toggle("gap343.float", ToggleValue::Float(1.0));
        kit.set_toggle("gap343.str", ToggleValue::Str("true".into()));
        kit.set_toggle("gap343.bool", ToggleValue::Bool(true));

        assert!(
            !kit.is_toggle_enabled("gap343.int"),
            "Int(1) 非 Bool，必须 false（非真值强制）"
        );
        assert!(!kit.is_toggle_enabled("gap343.float"));
        assert!(
            !kit.is_toggle_enabled("gap343.str"),
            "Str(\"true\") 非 Bool，必须 false"
        );
        assert!(kit.is_toggle_enabled("gap343.bool"), "Bool(true) 对照组");
        assert!(!kit.is_toggle_enabled("gap343.unknown"), "未知 key 对照组");
    }
}

// 场景 344：`register_health_check` 同模块重复注册覆盖旧 checker
// （HashMap::insert 语义：不报错、不累积、以最新注册为准）。
#[cfg(feature = "health")]
mod gap_344_health_check_overwrite {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use trait_kit::core::health::HealthStatus;
    use trait_kit::core::{AutoBuilder, HealthCheck};
    use trait_kit::impl_module_meta;
    use trait_kit::kit::Kit;
    use trait_kit::prelude::*;

    static SECOND_IMPL: AtomicBool = AtomicBool::new(false);

    struct GapHealthMod;
    impl_module_meta!(GapHealthMod, "gap344-health");
    impl AutoBuilder for GapHealthMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl HealthCheck for GapHealthMod {
        fn check(_cap: &Arc<u32>) -> HealthStatus {
            if SECOND_IMPL.load(Ordering::SeqCst) {
                HealthStatus::degraded("second checker")
            } else {
                HealthStatus::Healthy
            }
        }
    }

    #[test]
    fn duplicate_register_health_check_overwrites_without_error() {
        // 第一轮：首次注册的自报为 Healthy。
        let mut kit1 = Kit::new();
        kit1.register::<GapHealthMod>().unwrap();
        kit1.register_health_check::<GapHealthMod>();
        let ready1 = kit1.build().unwrap();
        assert!(matches!(
            ready1.health_check::<GapHealthMod>().unwrap(),
            HealthStatus::Healthy
        ));

        // 翻转自报状态后第二轮：同模块二次（三次）注册不报错，
        // insert 覆盖旧 checker，报告不累积重复条目。
        SECOND_IMPL.store(true, Ordering::SeqCst);
        let mut kit2 = Kit::new();
        kit2.register::<GapHealthMod>().unwrap();
        kit2.register_health_check::<GapHealthMod>();
        kit2.register_health_check::<GapHealthMod>();
        let ready2 = kit2.build().unwrap();

        let status = ready2.health_check::<GapHealthMod>().unwrap();
        assert!(
            matches!(&status, HealthStatus::Degraded { detail } if detail == "second checker"),
            "应以最新注册的 checker 自报为准，got {status:?}"
        );
        let report = ready2.health_report();
        assert_eq!(
            report.iter().filter(|(n, _)| *n == "gap344-health").count(),
            1,
            "重复注册不得在报告中累积多条"
        );
    }
}

// 场景 346/349：版本族入口对未加密类型的 fail-closed（MissingConfig）。
#[cfg(feature = "encryption")]
mod gap_346_349_version_family_on_unencrypted {
    use trait_kit::kit::{Kit, ModuleConfig};
    use trait_kit::prelude::*;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct NeverEncrypted;
    impl ModuleConfig for NeverEncrypted {
        const PATH: &'static str = "gap346/never-encrypted";
        fn default_value() -> Self {
            Self
        }
    }

    #[test]
    fn version_family_entries_missing_config_on_unencrypted_type() {
        let ready = Kit::new().build().expect("empty kit builds");

        let err = ready
            .encrypted_key_version::<NeverEncrypted>()
            .expect_err("未加密类型查询 key 版本必须报错");
        assert!(
            matches!(&err, TraitKitError::MissingConfig { key } if key.contains("NeverEncrypted")),
            "encrypted_key_version → MissingConfig{{key=类型名}}，got {err:?}"
        );

        let err = ready
            .get_encrypted_with_version::<NeverEncrypted>(&[1u8; 32], 1)
            .expect_err("未加密类型按版本读取必须报错");
        assert!(
            matches!(&err, TraitKitError::MissingConfig { .. }),
            "get_encrypted_with_version → MissingConfig，got {err:?}"
        );

        let err = ready
            .rotate_master_key::<NeverEncrypted>(&[1u8; 32], &[2u8; 32])
            .expect_err("未加密类型轮换必须报错");
        assert!(
            matches!(&err, TraitKitError::MissingConfig { .. }),
            "rotate_master_key → MissingConfig（fail-closed，不凭空建密文），got {err:?}"
        );
    }
}

// 场景 350：密钥正确但反序列化失败 → BuildFailed，明文中间缓冲零化
// 不外泄（kit.rs get_encrypted serde 分支）。
#[cfg(feature = "encryption")]
mod gap_350_deserialize_failure_build_failed {
    use serde::{Deserialize, Serialize};
    use trait_kit::kit::{Kit, ModuleConfig};
    use trait_kit::prelude::*;

    /// 序列化输出与自身反序列化不对称的类型：serialize 产出裸字符串，
    /// deserialize 要求 `{"token": …}` 结构——set 成功、get 在 serde
    /// 分支失败（解密成功、解析失败）。
    #[derive(Debug, Clone)]
    struct AsymmetricCfg {
        token: String,
    }
    impl ModuleConfig for AsymmetricCfg {
        const PATH: &'static str = "gap350/asymmetric";
        fn default_value() -> Self {
            Self {
                token: String::new(),
            }
        }
    }
    impl Serialize for AsymmetricCfg {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(&format!("raw:{}", self.token))
        }
    }
    impl<'de> Deserialize<'de> for AsymmetricCfg {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            // 反序列化强制要求对象结构，而序列化只产出字符串——
            // set→get 必然在 serde 分支失败（解密成功、解析失败）。
            #[derive(Deserialize)]
            struct Shape {
                token: String,
            }
            let shape = Shape::deserialize(deserializer)?;
            Ok(Self { token: shape.token })
        }
    }

    #[test]
    fn get_encrypted_serde_branch_fails_with_build_failed() {
        let kit = Kit::new();
        kit.set_encrypted(
            &AsymmetricCfg {
                token: "s3cret".into(),
            },
            b"0123456789abcdef",
        )
        .expect("set 成功（serialize 侧无约束）");
        let ready = kit.build().unwrap();

        let err = ready
            .get_encrypted::<AsymmetricCfg>(b"0123456789abcdef")
            .expect_err("密钥正确但反序列化失败必须走 serde 失败分支");
        match &err {
            TraitKitError::BuildFailed { context, source } => {
                assert_eq!(context, "get_encrypted");
                assert!(
                    source.to_string().contains("invalid type"),
                    "source 应为 serde 反序列化错误（解密已成功）：{source}"
                );
            }
            other => panic!("expected BuildFailed, got {other:?}"),
        }
    }
}

// 场景 353：ConfigChanged 事件摘要语义全序列
// （set new / set replaced / set_arc new / restore snapshot）。
#[cfg(feature = "confers")]
mod gap_353_config_changed_summary_sequence {
    use std::sync::{Arc, Mutex};
    use trait_kit::kit::Kit;
    use trait_kit::kit::events::{EventBus, KitEvent, MemoryEventBus};

    #[derive(Debug, Clone, PartialEq)]
    struct GapCfg(u32);

    #[test]
    fn summaries_follow_set_set_arc_restore_sequence() {
        let bus = Arc::new(MemoryEventBus::new());
        let log: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        {
            let sink = Arc::clone(&log);
            bus.subscribe(move |event| {
                if let KitEvent::ConfigChanged { key, summary } = event {
                    sink.lock().unwrap().push((key.clone(), summary.clone()));
                }
            });
        }

        let mut kit = Kit::new();
        kit.with_event_bus(Some(Arc::clone(&bus) as Arc<dyn EventBus>));

        kit.set_config(GapCfg(1));
        kit.set_config(GapCfg(2));
        kit.set_config_arc(GapCfg(3));
        assert!(kit.snapshot_config::<GapCfg>(), "快照取当前值");
        kit.set_config_arc(GapCfg(4));
        kit.restore_config::<GapCfg>().expect("restore 回滚");

        let summaries: Vec<String> = log.lock().unwrap().iter().map(|(_, s)| s.clone()).collect();
        assert_eq!(
            summaries,
            vec![
                "set (new)",
                "set (replaced)",
                "set_arc (new)",
                "set_arc (replaced)",
                "set (replaced)",
                "restore snapshot",
            ],
            "摘要序列须精确反映存储语义（restore 在 set 事件之上追加独立审计）"
        );
        assert_eq!(kit.config::<GapCfg>().unwrap(), GapCfg(2), "回滚到快照值");
    }
}

// 场景 354：Scope 模块自包含契约——构建回调拿到临时空 Kit，
// 依赖/配置不可达（不得回落父 Kit）。
#[cfg(all(feature = "request-scope", feature = "confers"))]
mod gap_354_scope_module_self_contained {
    use std::sync::Arc;
    use trait_kit::core::AutoBuilder;
    use trait_kit::impl_module_meta;
    use trait_kit::kit::Kit;
    use trait_kit::prelude::*;

    #[derive(Debug, Clone, PartialEq)]
    struct ParentCfg(u32);

    struct ScopedCfgReader;
    impl_module_meta!(ScopedCfgReader, "gap354-scoped-reader");
    impl AutoBuilder for ScopedCfgReader {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            // 临时空 Kit：读不到父 Kit 已 set 的同名配置 → MissingConfig。
            let value = kit.config::<ParentCfg>()?;
            Ok(Arc::new(value.0))
        }
    }

    #[test]
    fn scoped_module_cannot_reach_parent_config() {
        let mut kit = Kit::new();
        kit.register::<ScopedCfgReader>().unwrap();
        kit.set_config(ParentCfg(42));
        let ready = kit.build().unwrap();
        assert!(ready.contains::<ScopedCfgReader>());

        let scope = ready.create_scope();
        let mut scope = scope;
        scope.register::<ScopedCfgReader>().unwrap();
        let err = scope
            .require::<ScopedCfgReader>()
            .expect_err("作用域模块读配置必须失败而非回落父 Kit");
        match err {
            TraitKitError::BuildFailed { context, source } => {
                assert_eq!(context, "gap354-scoped-reader");
                assert!(
                    matches!(
                        source.downcast_ref::<TraitKitError>(),
                        Some(TraitKitError::MissingConfig { .. })
                    ),
                    "source 应为 MissingConfig（ParentCfg 不可达），got {source:?}"
                );
            }
            other => panic!("expected BuildFailed, got {other:?}"),
        }
    }
}

// 场景 355：Scope !Send + !Sync（单线程契约）、AsyncScope Send + Sync
// 的编译期负向/正向断言。
#[cfg(feature = "request-scope")]
mod gap_355_scope_threading_contract {
    use static_assertions::{assert_impl_all, assert_not_impl_any};
    use trait_kit::kit::{AsyncScope, Scope};

    #[test]
    fn scope_single_thread_async_scope_send_sync() {
        assert_not_impl_any!(Scope: Send, Sync);
        assert_impl_all!(AsyncScope: Send, Sync);
    }
}

// 场景 356：ConfersConfigHandle 完整访问面
// （get_bool 类型不匹配 None、get_raw/keys/contains）。
#[cfg(feature = "presets")]
mod gap_356_confers_config_handle_surface {
    use std::sync::Arc;
    use trait_kit::kit::presets::{ConfersConfigModule, register_confers_config};
    use trait_kit::prelude::*;

    /// confers ConfigValue 的内存 provider mock
    /// （镜像 tests/e2e/e2e_presets.rs::PresetMapProvider 形态）。
    struct GapProvider {
        pairs: std::collections::HashMap<String, confers::AnnotatedValue>,
    }
    impl GapProvider {
        fn from_pairs<const N: usize>(pairs: [(&str, confers::ConfigValue); N]) -> Arc<Self> {
            Arc::new(Self {
                pairs: pairs
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k.to_string(),
                            confers::AnnotatedValue::new(v, confers::SourceId::default(), k),
                        )
                    })
                    .collect(),
            })
        }
    }
    impl confers::ConfigProvider for GapProvider {
        fn get_raw(&self, key: &str) -> Option<&confers::AnnotatedValue> {
            self.pairs.get(key)
        }
        fn keys(&self) -> Vec<String> {
            self.pairs.keys().cloned().collect()
        }
    }

    #[test]
    fn handle_full_accessor_surface() {
        let mut kit = Kit::new();
        register_confers_config(
            &mut kit,
            GapProvider::from_pairs([
                ("flag.on", confers::ConfigValue::Bool(true)),
                ("str.key", confers::ConfigValue::String("text".into())),
                ("int.key", confers::ConfigValue::I64(7)),
            ]),
        )
        .expect("register preset");
        let ready = kit.build().expect("build ok");
        let handle = ready.require::<ConfersConfigModule>().expect("require");

        assert_eq!(handle.get_bool("flag.on"), Some(true), "Bool 命中");
        assert_eq!(
            handle.get_bool("str.key"),
            None,
            "get_bool 于 string 值必须 None（类型不匹配静默）"
        );
        assert_eq!(handle.get_bool("absent"), None, "缺键 None");
        assert!(handle.contains("int.key"));
        assert!(!handle.contains("absent"));
        let mut keys = handle.keys();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "flag.on".to_string(),
                "int.key".to_string(),
                "str.key".to_string()
            ],
            "keys() 列全键"
        );
        assert!(
            handle.get_raw("int.key").is_some(),
            "get_raw 返回原始注解值"
        );
        assert!(handle.get_raw("absent").is_none());
        assert_eq!(handle.get_int("int.key"), Some(7));
    }
}

// 场景 360：子 Kit 内部缺依赖折叠为父 BuildFailed{context=子名}
// （非 DependencyMissing 直穿）。
#[cfg(feature = "compose")]
mod gap_360_sub_kit_missing_dep_folds_to_build_failed {
    use std::any::TypeId;
    use std::sync::Arc;
    use trait_kit::core::{AutoBuilder, ModuleMeta};
    use trait_kit::kit::{Kit, SubKitModule, SubKitSpec};
    use trait_kit::prelude::*;

    struct ChildDependent;
    impl ModuleMeta for ChildDependent {
        const NAME: &'static str = "gap360-child-dependent";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[(
                <ChildAbsent as ModuleMeta>::NAME,
                TypeId::of::<ChildAbsent>(),
            )];
            DEPS
        }
    }
    impl AutoBuilder for ChildDependent {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }

    struct ChildAbsent;
    impl ModuleMeta for ChildAbsent {
        const NAME: &'static str = "gap360-child-absent";
    }

    struct BrokenChildSpec;
    impl SubKitSpec for BrokenChildSpec {
        const NAME: &'static str = "gap360-broken-child";
        fn compose(kit: &mut Kit) {
            kit.register::<ChildDependent>().unwrap();
        }
    }

    #[test]
    fn child_missing_dependency_reports_build_failed_with_child_name() {
        let mut kit = Kit::new();
        kit.register::<SubKitModule<BrokenChildSpec>>().unwrap();
        let err = kit.build().expect_err("子内缺依赖必须使父 build 失败");
        match &err {
            TraitKitError::BuildFailed { context, source } => {
                assert_eq!(
                    context, "gap360-broken-child",
                    "context 折叠为子 spec NAME（与跨 Kit 声明依赖报 DependencyMissing 形成口径对照）"
                );
                let text = source.to_string();
                assert!(
                    text.contains("gap360-child-absent"),
                    "source 应保留子内 DependencyMissing 详情：{text}"
                );
                assert!(
                    !matches!(
                        source.downcast_ref::<TraitKitError>(),
                        Some(TraitKitError::DependencyMissing { module, .. })
                            if *module == "gap360-broken-child"
                    ),
                    "缺失发生在子图内部，外层不得伪装成跨 Kit 声明依赖口径"
                );
            }
            other => panic!("expected BuildFailed, got {other:?}"),
        }
    }
}

// 场景 341：register_lifecycle 于未注册模块——sync 静默丢弃 on_ready
// （只遍历拓扑序 sorted），async 仍按注册序在图内模块之后执行。
// sync/async 行为分歧固化为文档契约。
#[cfg(all(feature = "lifecycle", feature = "async"))]
mod gap_341_lifecycle_on_unregistered_module_divergence {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::task::{Context, Poll, Waker};
    use trait_kit::core::{AsyncAutoBuilder, AsyncLifecycle, AutoBuilder};
    use trait_kit::impl_module_meta;
    use trait_kit::kit::AsyncKit;
    use trait_kit::kit::Kit;
    use trait_kit::prelude::*;

    static SYNC_GHOST_READY: AtomicU32 = AtomicU32::new(0);
    static ASYNC_GHOST_READY: AtomicU32 = AtomicU32::new(0);
    static GRAPH_READY: AtomicU32 = AtomicU32::new(0);

    struct GhostMod;
    impl_module_meta!(GhostMod, "gap341-ghost");
    impl AutoBuilder for GhostMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl AsyncAutoBuilder for GhostMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Self::Capability, TraitKitError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(Arc::new(1)) })
        }
    }
    impl Lifecycle for GhostMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), Self::Error> {
            SYNC_GHOST_READY.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    impl AsyncLifecycle for GhostMod {
        fn on_ready<'a>(
            _kit: &'a AsyncKit<trait_kit::kit::AsyncReady>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Self::Error>> + Send + 'a>>
        {
            Box::pin(async {
                ASYNC_GHOST_READY.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    struct GraphMod;
    impl_module_meta!(GraphMod, "gap341-graph");
    impl AutoBuilder for GraphMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(2))
        }
    }
    impl AsyncAutoBuilder for GraphMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Self::Capability, TraitKitError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(Arc::new(2)) })
        }
    }
    impl Lifecycle for GraphMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), Self::Error> {
            GRAPH_READY.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn sync_lifecycle_on_unregistered_module_is_silently_dropped() {
        SYNC_GHOST_READY.store(0, Ordering::SeqCst);
        GRAPH_READY.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<GraphMod>().unwrap();
        kit.register_lifecycle::<GraphMod>();
        // 只 register_lifecycle、不 register 模块本体。
        kit.register_lifecycle::<GhostMod>();
        let ready = kit.build().expect("未注册模块的 lifecycle 不应阻断构建");
        let _ = ready.require::<GraphMod>().unwrap();
        assert_eq!(
            SYNC_GHOST_READY.load(Ordering::SeqCst),
            0,
            "sync 侧：幽灵模块 on_ready 从未执行（只遍历拓扑序 sorted）"
        );
        assert_eq!(
            GRAPH_READY.load(Ordering::SeqCst),
            1,
            "图内模块 on_ready 正常"
        );
    }

    #[test]
    fn async_lifecycle_on_unregistered_module_still_runs_last() {
        ASYNC_GHOST_READY.store(0, Ordering::SeqCst);
        let mut kit = AsyncKit::new();
        kit.register::<GraphMod>().unwrap();
        kit.register_lifecycle::<GhostMod>();
        let ready = block_on(kit.build()).expect("async 侧同样构建成功");
        let _ = ready.require::<GraphMod>().unwrap();
        assert_eq!(
            ASYNC_GHOST_READY.load(Ordering::SeqCst),
            1,
            "async 侧：幽灵模块 on_ready 在图内模块之后仍执行（unwrap_or(usize::MAX) 排序）"
        );
    }

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
}
