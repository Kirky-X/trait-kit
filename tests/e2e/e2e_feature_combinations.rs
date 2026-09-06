// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// 穷举特性组合 E2E 测试：每个 feature 独立 + 关键多特性组合。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.20）：
// - CMP-10 shutdown+decorator 关闭次序（shutdown_decorator_e2e）
// - CMP-11 toggle+scope 开关门控作用域（toggle_scope_e2e）
// - CMP-12 interface+decorator interface 构建路径装饰（interface_decorator_e2e）
// - CMP-13 encryption+reload 双链共存（encryption_reload_e2e）
// - CMP-15 全 feature 行为级烟囱（all_features_smoke_e2e）
// （CMP-14 i18n+shutdown 落点为 tests/e2e_i18n.rs，因 zh locale 全局单例
//   需独立测试进程锚定，避免与默认 locale 测试互相竞争 OnceLock 初始化。）

use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

// ─── 无特性：基础 Kit 操作 ─────────────────────────────────────────────

struct NoFeatureModule;
impl_module_meta!(NoFeatureModule, "no-feature");
impl AutoBuilder for NoFeatureModule {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
        Ok(Arc::new(42))
    }
}

#[test]
fn e2e_no_feature_basic_build() {
    let mut kit = Kit::new();
    kit.register::<NoFeatureModule>().unwrap();
    let ready = kit.build().unwrap();
    let cap = ready.require::<NoFeatureModule>().unwrap();
    assert_eq!(*cap, 42);
}

#[test]
fn e2e_no_feature_graph_export() {
    let mut kit = Kit::new();
    kit.register::<NoFeatureModule>().unwrap();
    let ready = kit.build().unwrap();
    assert!(ready.graph_dot().contains("no-feature"));
    assert!(ready.graph_mermaid().contains("no-feature"));
}

// ─── lifecycle：on_ready + on_shutdown ──────────────────────────────────

#[cfg(feature = "lifecycle")]
mod lifecycle_e2e {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use trait_kit::core::lifecycle::Lifecycle;

    static READY_CALLED: AtomicBool = AtomicBool::new(false);
    static SHUTDOWN_VAL: AtomicU32 = AtomicU32::new(0);

    struct LcMod;
    impl_module_meta!(LcMod, "lc-mod");
    impl AutoBuilder for LcMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
            Ok(Arc::new(7))
        }
    }
    impl Lifecycle for LcMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            READY_CALLED.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn on_shutdown(cap: &Arc<u32>) {
            SHUTDOWN_VAL.store(**cap, Ordering::SeqCst);
        }
    }

    #[test]
    fn e2e_lifecycle_on_ready_after_build() {
        READY_CALLED.store(false, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<LcMod>().unwrap();
        kit.register_lifecycle::<LcMod>();
        let _ready = kit.build().unwrap();
        assert!(READY_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn e2e_lifecycle_on_shutdown_on_explicit_shutdown() {
        SHUTDOWN_VAL.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<LcMod>().unwrap();
        kit.register_lifecycle::<LcMod>();
        let ready = kit.build().unwrap();
        ready.shutdown();
        assert_eq!(SHUTDOWN_VAL.load(Ordering::SeqCst), 7);
    }
}

// ─── health：健康检查 ───────────────────────────────────────────────────

#[cfg(feature = "health")]
mod health_e2e {
    use super::*;
    use trait_kit::core::health::{HealthCheck, HealthStatus};

    struct HealthyMod;
    impl_module_meta!(HealthyMod, "healthy");
    struct HCap {
        val: u64,
    }
    impl AutoBuilder for HealthyMod {
        type Capability = Arc<HCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<HCap>, TraitKitError> {
            Ok(Arc::new(HCap { val: 10 }))
        }
    }
    impl HealthCheck for HealthyMod {
        fn check(cap: &Arc<HCap>) -> HealthStatus {
            if cap.val > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "zero".into(),
                }
            }
        }
    }

    struct UnhealthyMod;
    impl_module_meta!(UnhealthyMod, "unhealthy");
    struct UCap;
    impl AutoBuilder for UnhealthyMod {
        type Capability = Arc<UCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<UCap>, TraitKitError> {
            Ok(Arc::new(UCap))
        }
    }
    impl HealthCheck for UnhealthyMod {
        fn check(_cap: &Arc<UCap>) -> HealthStatus {
            HealthStatus::Unhealthy {
                detail: "down".into(),
            }
        }
    }

    #[test]
    fn e2e_health_all_healthy() {
        let mut kit = Kit::new();
        kit.register::<HealthyMod>().unwrap();
        kit.register_health_check::<HealthyMod>();
        let ready = kit.build().unwrap();
        let report = ready.health_report();
        assert_eq!(report.len(), 1);
        assert!(report[0].1.is_healthy());
    }

    #[test]
    fn e2e_health_mixed() {
        let mut kit = Kit::new();
        kit.register::<HealthyMod>().unwrap();
        kit.register::<UnhealthyMod>().unwrap();
        kit.register_health_check::<HealthyMod>();
        kit.register_health_check::<UnhealthyMod>();
        let ready = kit.build().unwrap();
        let report = ready.health_report();
        assert_eq!(report.len(), 2);
        let healthy_count = report.iter().filter(|(_, s)| s.is_healthy()).count();
        assert_eq!(healthy_count, 1);
    }

    #[test]
    fn e2e_health_single_module_check() {
        let mut kit = Kit::new();
        kit.register::<HealthyMod>().unwrap();
        kit.register_health_check::<HealthyMod>();
        let ready = kit.build().unwrap();
        let status = ready.health_check::<HealthyMod>().unwrap();
        assert!(status.is_healthy());
    }

    #[test]
    fn e2e_health_status_constructors() {
        assert!(!HealthStatus::degraded("slow").is_healthy());
        assert!(!HealthStatus::unhealthy("down").is_healthy());
        assert!(HealthStatus::Healthy.is_healthy());
    }
}

// ─── observer：构建观察者 ───────────────────────────────────────────────

#[cfg(feature = "observer")]
mod observer_e2e {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use trait_kit::core::observer::BuildObserver;

    struct ObsMod;
    impl_module_meta!(ObsMod, "obs-mod");
    impl AutoBuilder for ObsMod {
        type Capability = Arc<String>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<String>, TraitKitError> {
            Ok(Arc::new("built".into()))
        }
    }

    struct CountingObserver {
        count: Arc<AtomicU32>,
    }
    impl BuildObserver for CountingObserver {
        fn on_module_built(&self, _name: &'static str, _elapsed: Duration) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn e2e_observer_notified_on_build() {
        let count = Arc::new(AtomicU32::new(0));
        let mut kit = Kit::new();
        kit.register::<ObsMod>().unwrap();
        kit.with_observer(Arc::new(CountingObserver {
            count: Arc::clone(&count),
        }));
        let _ready = kit.build().unwrap();
        assert!(count.load(Ordering::SeqCst) > 0);
    }
}

// ─── decorator：模块装饰器 ──────────────────────────────────────────────

#[cfg(feature = "decorator")]
mod decorator_e2e {
    use super::*;

    struct DecModule;
    impl_module_meta!(DecModule, "dec-mod");
    impl AutoBuilder for DecModule {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
            Ok(Arc::new(10))
        }
    }

    #[test]
    fn e2e_decorator_wraps_capability() {
        let mut kit = Kit::new();
        kit.register::<DecModule>().unwrap();
        kit.decorate::<DecModule>(|cap: Arc<u32>| Arc::new(*cap * 2));
        let ready = kit.build().unwrap();
        assert_eq!(*ready.require::<DecModule>().unwrap(), 20);
    }

    #[test]
    fn e2e_decorator_no_op_when_absent() {
        let mut kit = Kit::new();
        kit.register::<DecModule>().unwrap();
        let ready = kit.build().unwrap();
        assert_eq!(*ready.require::<DecModule>().unwrap(), 10);
    }
}

// ─── scope：作用域依赖 ─────────────────────────────────────────────────

#[cfg(feature = "scope")]
mod scope_e2e {
    use super::*;

    struct ScopeMod;
    impl_module_meta!(ScopeMod, "scope-mod");
    impl AutoBuilder for ScopeMod {
        type Capability = Arc<String>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<String>, TraitKitError> {
            Ok(Arc::new("global".into()))
        }
    }

    #[test]
    fn e2e_scope_create_empty() {
        let mut kit = Kit::new();
        kit.register::<ScopeMod>().unwrap();
        let ready = kit.build().unwrap();
        let scope = ready.create_scope();
        assert!(!scope.contains::<ScopeMod>());
    }

    #[test]
    fn e2e_scope_isolation() {
        let mut kit = Kit::new();
        kit.register::<ScopeMod>().unwrap();
        let ready = kit.build().unwrap();
        let _s1 = ready.create_scope();
        let _s2 = ready.create_scope();
        // Each scope is independent and empty
    }
}

// ─── toggle：特性开关 ──────────────────────────────────────────────────

#[cfg(feature = "toggle")]
mod toggle_e2e {
    use super::*;

    #[test]
    fn e2e_toggle_enable_query() {
        let kit = Kit::new();
        kit.enable_toggle("exp", true);
        assert!(kit.is_toggle_enabled("exp"));
        kit.enable_toggle("exp", false);
        assert!(!kit.is_toggle_enabled("exp"));
    }

    #[test]
    fn e2e_toggle_default_disabled() {
        let kit = Kit::new();
        assert!(!kit.is_toggle_enabled("nonexistent"));
    }

    #[test]
    fn e2e_toggle_multiple() {
        let kit = Kit::new();
        kit.enable_toggle("a", true);
        kit.enable_toggle("b", true);
        assert!(kit.is_toggle_enabled("a"));
        assert!(kit.is_toggle_enabled("b"));
        kit.enable_toggle("a", false);
        assert!(!kit.is_toggle_enabled("a"));
        assert!(kit.is_toggle_enabled("b"));
    }

    #[test]
    fn e2e_toggle_persists_after_build() {
        let mut kit = Kit::new();
        struct TM;
        impl_module_meta!(TM, "tm");
        impl AutoBuilder for TM {
            type Capability = Arc<()>;
            type Error = TraitKitError;
            fn build(_: &Kit) -> Result<Arc<()>, TraitKitError> {
                Ok(Arc::new(()))
            }
        }
        kit.register::<TM>().unwrap();
        kit.enable_toggle("feat-x", true);
        let ready = kit.build().unwrap();
        assert!(ready.is_toggle_enabled("feat-x"));
    }
}

// ─── shutdown：优雅关闭 ────────────────────────────────────────────────

#[cfg(feature = "shutdown")]
mod shutdown_e2e {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use trait_kit::kit::ShutdownCoordinator;
    use trait_kit::kit::ShutdownPhase;

    #[test]
    fn e2e_shutdown_all_phases_execute() {
        static CALLED: AtomicUsize = AtomicUsize::new(0);
        CALLED.store(0, Ordering::SeqCst);

        let coord = ShutdownCoordinator::new();
        coord.register_hook(ShutdownPhase::StopRequests, || {
            CALLED.fetch_add(1, Ordering::SeqCst);
        });
        coord.register_hook(ShutdownPhase::DrainQueue, || {
            CALLED.fetch_add(1, Ordering::SeqCst);
        });
        coord.register_hook(ShutdownPhase::CloseConnections, || {
            CALLED.fetch_add(1, Ordering::SeqCst);
        });

        let results = coord.shutdown();
        assert_eq!(CALLED.load(Ordering::SeqCst), 3);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| !r.timed_out));
    }

    #[test]
    fn e2e_shutdown_set_timeouts() {
        use std::time::Duration;
        let coord = ShutdownCoordinator::new();
        coord.set_global_timeout(Duration::from_secs(5));
        coord.set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_secs(1));
    }

    #[test]
    fn e2e_shutdown_empty_phases_succeed() {
        let coord = ShutdownCoordinator::new();
        let results = coord.shutdown();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| !r.timed_out));
    }
}

// ─── i18n：国际化 ──────────────────────────────────────────────────────

#[cfg(feature = "i18n")]
mod i18n_e2e {
    use trait_kit::i18n::I18nManager;

    #[test]
    fn e2e_i18n_init_and_locale() {
        let mgr = I18nManager::init();
        assert!(!mgr.locale_tag().is_empty());
    }

    #[test]
    fn e2e_i18n_translate_fallback() {
        let mgr = I18nManager::init();
        // Unknown key returns itself as fallback
        let result = mgr.translate("unknown-key-xyz", &[]);
        assert!(!result.is_empty());
    }

    #[test]
    fn e2e_i18n_tr_global_function() {
        let msg = trait_kit::i18n::tr(
            "trait-kit-error-already-registered",
            &[("module", "test-mod")],
        );
        assert!(msg.contains("test-mod"));
    }
}

// ─── interface：接口/实现分离 ───────────────────────────────────────────

#[cfg(feature = "interface")]
mod interface_e2e {
    use std::sync::Arc;
    use trait_kit::core::InterfaceBuilder;
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    trait Greeter: Send + Sync + 'static {
        fn greet(&self) -> String;
    }

    struct EnglishGreeter;
    impl Greeter for EnglishGreeter {
        fn greet(&self) -> String {
            "Hello!".into()
        }
    }

    struct GreeterModule;
    impl_module_meta!(GreeterModule, "greeter");
    impl InterfaceBuilder for GreeterModule {
        type Interface = dyn Greeter;
        type Capability = Arc<EnglishGreeter>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<EnglishGreeter>, TraitKitError> {
            Ok(Arc::new(EnglishGreeter))
        }
        fn into_interface(cap: Arc<EnglishGreeter>) -> Arc<dyn Greeter> {
            cap as Arc<dyn Greeter>
        }
    }

    #[test]
    fn e2e_interface_register_and_resolve() {
        let mut kit = Kit::new();
        kit.register_as::<GreeterModule>().unwrap();
        let ready = kit.build().unwrap();
        let greeter = ready.resolve::<dyn Greeter>().unwrap();
        assert_eq!(greeter.greet(), "Hello!");
    }
}

// ─── confers：配置系统 ─────────────────────────────────────────────────

#[cfg(feature = "confers")]
mod confers_e2e {
    use super::*;
    use std::error::Error;
    use trait_kit::kit::ModuleConfig;

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct E2eDbConfig {
        host: String,
        port: u16,
    }
    impl Configurable for E2eDbConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self {
                host: "loaded".into(),
                port: 5432,
            })
        }
    }
    impl ModuleConfig for E2eDbConfig {
        const PATH: &'static str = "config/e2e_db.toml";
        fn default_value() -> Self {
            Self {
                host: "localhost".into(),
                port: 3306,
            }
        }
    }

    #[test]
    fn e2e_confers_load_and_validate_ok() {
        #[derive(Clone, Debug)]
        struct V {
            port: u16,
        }
        impl Configurable for V {
            fn load() -> Result<Self, Box<dyn Error + Send>> {
                Ok(Self { port: 8080 })
            }
        }
        impl Validatable for V {
            fn validate(&self) -> Result<(), Vec<String>> {
                if self.port > 0 && self.port < 65535 {
                    Ok(())
                } else {
                    Err(vec!["out of range".into()])
                }
            }
        }
        let kit = Kit::new();
        kit.load_and_validate::<V>().unwrap();
        let ready = kit.build().unwrap();
        assert_eq!(ready.config::<V>().unwrap().port, 8080);
    }

    #[test]
    fn e2e_confers_load_and_validate_reject() {
        #[derive(Clone, Debug)]
        struct Bad;
        impl Configurable for Bad {
            fn load() -> Result<Self, Box<dyn Error + Send>> {
                Ok(Self)
            }
        }
        impl Validatable for Bad {
            fn validate(&self) -> Result<(), Vec<String>> {
                Err(vec!["bad".into()])
            }
        }
        let kit = Kit::new();
        assert!(kit.load_and_validate::<Bad>().is_err());
    }

    #[test]
    fn e2e_confers_snapshot_restore() {
        let kit = Kit::new();
        kit.set_config(E2eDbConfig {
            host: "orig".into(),
            port: 3306,
        });
        kit.snapshot_config::<E2eDbConfig>();
        kit.set_config(E2eDbConfig {
            host: "mod".into(),
            port: 5432,
        });
        kit.restore_config::<E2eDbConfig>().unwrap();
        let ready = kit.build().unwrap();
        let cfg: E2eDbConfig = ready.config().unwrap();
        assert_eq!(cfg.host, "orig");
    }

    #[test]
    fn e2e_confers_populate_defaults() {
        let kit = Kit::new();
        assert!(kit.populate_defaults::<E2eDbConfig>());
        let ready = kit.build().unwrap();
        assert_eq!(ready.config::<E2eDbConfig>().unwrap().port, 3306);
    }

    #[test]
    fn e2e_confers_populate_defaults_noop() {
        let kit = Kit::new();
        kit.set_config(E2eDbConfig {
            host: "custom".into(),
            port: 9999,
        });
        assert!(!kit.populate_defaults::<E2eDbConfig>());
        let ready = kit.build().unwrap();
        assert_eq!(ready.config::<E2eDbConfig>().unwrap().host, "custom");
    }

    #[test]
    fn e2e_confers_merge_json_deep() {
        use serde_json::json;
        use trait_kit::kit::merge_json_deep;
        let mut base = json!({"a": {"b": 1, "c": 2}});
        let overlay = json!({"a": {"c": 99, "d": 3}});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"a": {"b": 1, "c": 99, "d": 3}}));
    }

    #[test]
    fn e2e_confers_load_config_with_interpolation() {
        let kit = Kit::new();
        let vars = std::collections::HashMap::from([("H".into(), "interp-host".into())]);
        kit.load_config_with::<E2eDbConfig, _>(&vars).unwrap();
        let ready = kit.build().unwrap();
        let cfg: E2eDbConfig = ready.config().unwrap();
        assert!(!cfg.host.is_empty());
    }
}

// ─── reload：热重载 ────────────────────────────────────────────────────

#[cfg(feature = "reload")]
mod reload_e2e {
    use super::*;
    use std::cell::Cell;
    use std::error::Error;
    use std::rc::Rc;

    #[derive(Clone, Debug, PartialEq)]
    struct RCfg {
        v: u32,
    }
    impl Configurable for RCfg {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { v: 2 })
        }
    }

    #[test]
    fn e2e_reload_updates_and_notifies() {
        let kit = Kit::new();
        kit.set_config(RCfg { v: 1 });
        let notified = Rc::new(Cell::new(false));
        let n = Rc::clone(&notified);
        kit.subscribe::<RCfg>(move || {
            n.set(true);
        });
        kit.reload_config::<RCfg>().unwrap();
        assert!(notified.get());
        let ready = kit.build().unwrap();
        assert_eq!(ready.config::<RCfg>().unwrap().v, 2);
    }
}

// ─── encryption：加密配置 ──────────────────────────────────────────────

#[cfg(feature = "encryption")]
mod encryption_e2e {
    use super::*;
    use trait_kit::kit::ModuleConfig;

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct SCfg {
        key: String,
    }
    impl ModuleConfig for SCfg {
        const PATH: &'static str = "config/s.toml";
        fn default_value() -> Self {
            Self { key: "def".into() }
        }
    }
    const KEY: [u8; 32] = *b"0123456789abcdef0123456789abcdef";

    #[test]
    fn e2e_encryption_roundtrip() {
        let kit = Kit::new();
        kit.set_encrypted(
            &SCfg {
                key: "secret".into(),
            },
            &KEY,
        )
        .unwrap();
        let ready = kit.build().unwrap();
        assert_eq!(ready.get_encrypted::<SCfg>(&KEY).unwrap().key, "secret");
    }

    #[test]
    fn e2e_encryption_short_key_rejected() {
        let kit = Kit::new();
        assert!(
            kit.set_encrypted(&SCfg { key: "x".into() }, &[0u8; 8])
                .is_err()
        );
    }

    #[test]
    fn e2e_encryption_get_short_key_rejected() {
        let kit = Kit::new();
        kit.set_encrypted(&SCfg { key: "x".into() }, &KEY).unwrap();
        let ready = kit.build().unwrap();
        assert!(ready.get_encrypted::<SCfg>(&[0u8; 8]).is_err());
    }
}

// ─── async：异步 Kit ──────────────────────────────────────────────────

#[cfg(feature = "async")]
mod async_e2e {
    use trait_kit::prelude::*;
    #[test]
    fn e2e_async_kit_config_ops() {
        let kit = AsyncKit::new();
        kit.set_config(42u32);
        assert_eq!(kit.config::<u32>().unwrap(), 42);
    }
}

// ─── 多特性组合 ────────────────────────────────────────────────────────

#[cfg(all(feature = "lifecycle", feature = "health"))]
mod lifecycle_health_e2e {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use trait_kit::core::health::{HealthCheck, HealthStatus};
    use trait_kit::core::lifecycle::Lifecycle;

    static LH_READY: AtomicBool = AtomicBool::new(false);

    struct LhMod;
    impl_module_meta!(LhMod, "lh-mod");
    struct LhCap {
        val: u32,
    }
    impl AutoBuilder for LhMod {
        type Capability = Arc<LhCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<LhCap>, TraitKitError> {
            Ok(Arc::new(LhCap { val: 5 }))
        }
    }
    impl Lifecycle for LhMod {
        fn on_ready(ready_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            let report = ready_kit.health_report();
            LH_READY.store(!report.is_empty(), Ordering::SeqCst);
            Ok(())
        }
    }
    impl HealthCheck for LhMod {
        fn check(cap: &Arc<LhCap>) -> HealthStatus {
            if cap.val > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "zero".into(),
                }
            }
        }
    }

    #[test]
    fn e2e_lifecycle_plus_health() {
        LH_READY.store(false, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<LhMod>().unwrap();
        kit.register_lifecycle::<LhMod>();
        kit.register_health_check::<LhMod>();
        let _ready = kit.build().unwrap();
        assert!(LH_READY.load(Ordering::SeqCst));
    }
}

#[cfg(all(feature = "confers", feature = "reload"))]
mod confers_reload_e2e {
    use super::*;
    use std::cell::Cell;
    use std::error::Error;
    use std::rc::Rc;

    #[derive(Clone, Debug, PartialEq)]
    struct CrCfg {
        val: u32,
    }
    impl Configurable for CrCfg {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { val: 100 })
        }
    }

    #[test]
    fn e2e_confers_plus_reload() {
        let kit = Kit::new();
        kit.set_config(CrCfg { val: 1 });
        let counter = Rc::new(Cell::new(0u32));
        let c = Rc::clone(&counter);
        kit.subscribe::<CrCfg>(move || {
            c.set(c.get() + 1);
        });
        kit.reload_config::<CrCfg>().unwrap();
        assert_eq!(counter.get(), 1);
        let ready = kit.build().unwrap();
        assert_eq!(ready.config::<CrCfg>().unwrap().val, 100);
    }
}

#[cfg(all(feature = "confers", feature = "encryption"))]
mod confers_encryption_e2e {
    use super::*;
    use trait_kit::kit::ModuleConfig;

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct CeCfg {
        secret: String,
        port: u16,
    }
    impl ModuleConfig for CeCfg {
        const PATH: &'static str = "config/ce.toml";
        fn default_value() -> Self {
            Self {
                secret: "def".into(),
                port: 8080,
            }
        }
    }
    impl Configurable for CeCfg {
        fn load() -> Result<Self, Box<dyn std::error::Error + Send>> {
            Ok(Self {
                secret: "loaded".into(),
                port: 9090,
            })
        }
    }
    const KEY: [u8; 32] = *b"abcdefghijklmnopqrstuvwxyz012345";

    #[test]
    fn e2e_confers_plus_encryption() {
        let kit = Kit::new();
        kit.set_config(CeCfg {
            secret: "plain".into(),
            port: 1111,
        });
        kit.set_encrypted(
            &CeCfg {
                secret: "encrypted".into(),
                port: 2222,
            },
            &KEY,
        )
        .unwrap();
        let ready = kit.build().unwrap();
        assert_eq!(ready.config::<CeCfg>().unwrap().secret, "plain");
        assert_eq!(
            ready.get_encrypted::<CeCfg>(&KEY).unwrap().secret,
            "encrypted"
        );
    }
}

#[cfg(all(feature = "observer", feature = "decorator"))]
mod observer_decorator_e2e {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use trait_kit::core::observer::BuildObserver;

    struct OdMod;
    impl_module_meta!(OdMod, "od-mod");
    impl AutoBuilder for OdMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
            Ok(Arc::new(5))
        }
    }
    struct OdObs {
        count: Arc<AtomicU32>,
    }
    impl BuildObserver for OdObs {
        fn on_module_built(&self, _: &'static str, _: Duration) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn e2e_observer_plus_decorator() {
        let count = Arc::new(AtomicU32::new(0));
        let mut kit = Kit::new();
        kit.register::<OdMod>().unwrap();
        kit.with_observer(Arc::new(OdObs {
            count: Arc::clone(&count),
        }));
        kit.decorate::<OdMod>(|cap: Arc<u32>| Arc::new(*cap + 100));
        let ready = kit.build().unwrap();
        assert_eq!(*ready.require::<OdMod>().unwrap(), 105);
        assert!(count.load(Ordering::SeqCst) > 0);
    }
}

#[cfg(all(feature = "scope", feature = "lifecycle"))]
mod scope_lifecycle_e2e {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use trait_kit::core::lifecycle::Lifecycle;

    static SCOPE_IN_READY: AtomicBool = AtomicBool::new(false);

    struct SlMod;
    impl_module_meta!(SlMod, "sl-mod");
    impl AutoBuilder for SlMod {
        type Capability = Arc<String>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<String>, TraitKitError> {
            Ok(Arc::new("base".into()))
        }
    }
    impl Lifecycle for SlMod {
        fn on_ready(ready_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            let scope = ready_kit.create_scope();
            SCOPE_IN_READY.store(!scope.contains::<SlMod>(), Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn e2e_scope_plus_lifecycle() {
        SCOPE_IN_READY.store(false, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<SlMod>().unwrap();
        kit.register_lifecycle::<SlMod>();
        let _ready = kit.build().unwrap();
        assert!(SCOPE_IN_READY.load(Ordering::SeqCst));
    }
}

#[cfg(all(feature = "toggle", feature = "decorator"))]
mod toggle_decorator_e2e {
    use super::*;

    struct TdMod;
    impl_module_meta!(TdMod, "td-mod");
    impl AutoBuilder for TdMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
            Ok(Arc::new(10))
        }
    }

    #[test]
    fn e2e_toggle_plus_decorator() {
        let mut kit = Kit::new();
        kit.register::<TdMod>().unwrap();
        kit.enable_toggle("enhanced", true);
        if kit.is_toggle_enabled("enhanced") {
            kit.decorate::<TdMod>(|cap: Arc<u32>| Arc::new(*cap * 3));
        }
        let ready = kit.build().unwrap();
        assert_eq!(*ready.require::<TdMod>().unwrap(), 30);
    }
}

// ─── CMP-10：shutdown + decorator 关闭次序 ─────────────────────────────
//
// 语义固化：被装饰模块关闭时，装饰层先于核心层释放（Drop 外层先于内层），
// 且 `Lifecycle::on_shutdown` 接收到的是装饰后的最外层能力。

#[cfg(all(feature = "shutdown", feature = "decorator", feature = "lifecycle"))]
mod shutdown_decorator_e2e {
    use super::*;
    use std::sync::Mutex;
    use trait_kit::core::lifecycle::Lifecycle;
    use trait_kit::kit::{ShutdownCoordinator, ShutdownPhase};

    type Log = Arc<Mutex<Vec<String>>>;

    /// 核心资源（最内层）。Drop 时记录释放次序。
    struct CoreRes {
        val: u32,
        log: Log,
    }
    impl Drop for CoreRes {
        fn drop(&mut self) {
            self.log
                .lock()
                .unwrap()
                .push(format!("core-drop val={}", self.val));
        }
    }

    /// 装饰层（外层），持有内层引用形成链。Drop 时先记录自身释放，
    /// 随后字段 `inner` 的 drop glue 才释放内层（装饰层先于核心层）。
    struct DecoratedRes {
        val: u32,
        log: Log,
        #[allow(dead_code, reason = "inner/core 仅承载 Drop 次序，从不读取")]
        inner: Option<Arc<DecoratedRes>>,
        #[allow(dead_code, reason = "inner/core 仅承载 Drop 次序，从不读取")]
        core: Option<Arc<CoreRes>>,
    }
    impl Drop for DecoratedRes {
        fn drop(&mut self) {
            self.log
                .lock()
                .unwrap()
                .push(format!("decorator-drop val={}", self.val));
        }
    }

    struct SdMod;
    impl_module_meta!(SdMod, "sd-mod");
    impl AutoBuilder for SdMod {
        type Capability = Arc<DecoratedRes>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            // 样例日志句柄（测试数据，非真实凭据）。
            let log: Log = Arc::new(Mutex::new(Vec::new()));
            Ok(Arc::new(DecoratedRes {
                val: 10,
                log: Arc::clone(&log),
                inner: None,
                core: Some(Arc::new(CoreRes { val: 10, log })),
            }))
        }
    }
    impl Lifecycle for SdMod {
        fn on_shutdown(cap: &Arc<DecoratedRes>) {
            cap.log
                .lock()
                .unwrap()
                .push(format!("on-shutdown val={}", cap.val));
        }
    }

    #[test]
    fn e2e_shutdown_decorator_release_order() {
        let coord = ShutdownCoordinator::new();
        coord.register_hook(ShutdownPhase::StopRequests, || {});
        coord.register_hook(ShutdownPhase::DrainQueue, || {});
        coord.register_hook(ShutdownPhase::CloseConnections, || {});

        let mut kit = Kit::new();
        kit.register::<SdMod>().unwrap();
        kit.register_lifecycle::<SdMod>();
        // 两个装饰器按注册顺序叠加：f1 先（10→20），f2 后（20→40）。
        kit.decorate::<SdMod>(|cap: Arc<DecoratedRes>| {
            let log = Arc::clone(&cap.log);
            Arc::new(DecoratedRes {
                val: cap.val * 2,
                log,
                inner: Some(cap),
                core: None,
            })
        });
        kit.decorate::<SdMod>(|cap: Arc<DecoratedRes>| {
            let log = Arc::clone(&cap.log);
            Arc::new(DecoratedRes {
                val: cap.val * 2,
                log,
                inner: Some(cap),
                core: None,
            })
        });

        let log: Log = {
            let ready = kit.build().unwrap();
            // require 的克隆在作用域内结束，不干扰后续 drop 次序。
            let cap = ready.require::<SdMod>().unwrap();
            let observed = Arc::clone(&cap);
            let log = Arc::clone(&observed.log);
            assert_eq!(observed.val, 40, "decorator 应已包裹基础能力 10*2*2");
            drop(observed);

            // 协调器三阶段先行，随后 Kit 级 on_shutdown 观察到最外层 val=40。
            let results = coord.shutdown();
            assert_eq!(results.len(), 3);
            assert!(results.iter().all(|r| r.is_ok()));
            ready.shutdown();
            log
        };
        // drop(ready) 后能力表释放：装饰层(40) → 内层装饰(20) → 基础层(10)
        // → 核心资源。装饰层一律先于核心层释放。
        let events = log.lock().unwrap().clone();
        assert_eq!(
            events,
            vec![
                "on-shutdown val=40",
                "decorator-drop val=40",
                "decorator-drop val=20",
                "decorator-drop val=10",
                "core-drop val=10",
            ],
            "on_shutdown 应观察最外层，且装饰层先于核心层释放"
        );
    }
}

// ─── CMP-11：toggle + scope 开关门控作用域 ─────────────────────────────

#[cfg(all(feature = "toggle", feature = "scope"))]
mod toggle_scope_e2e {
    use super::*;

    struct TsMod;
    impl_module_meta!(TsMod, "ts-mod");
    impl AutoBuilder for TsMod {
        type Capability = Arc<String>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<String>, TraitKitError> {
            Ok(Arc::new("scoped".into()))
        }
    }

    #[test]
    fn e2e_toggle_gates_scope_registration() {
        let mut kit = Kit::new();
        kit.register::<TsMod>().unwrap();
        let ready = kit.build().unwrap();

        // 分支一：开关开启 → 作用域内注册并可用。
        ready.enable_toggle("per-request", true);
        let mut scope_on = ready.create_scope();
        if ready.is_toggle_enabled("per-request") {
            scope_on.register::<TsMod>().unwrap();
        }
        assert!(scope_on.contains::<TsMod>());
        assert_eq!(
            *scope_on.require::<TsMod>().unwrap(),
            "scoped",
            "开关开启时作用域内能力可获取"
        );

        // 分支二：开关关闭 → 跳过作用域注册，能力不可获取。
        ready.enable_toggle("per-request", false);
        let mut scope_off = ready.create_scope();
        if ready.is_toggle_enabled("per-request") {
            scope_off.register::<TsMod>().unwrap();
        }
        assert!(!scope_off.contains::<TsMod>());
        assert!(
            scope_off.require::<TsMod>().is_err(),
            "开关关闭时作用域内不应注册模块"
        );
    }
}

// ─── CMP-12：interface + decorator interface 构建路径装饰 ───────────────
//
// DEC-04 契约：装饰器须覆盖全部四条构建路径（eager/lazy/multi/interface）。
// register_as 路径的装饰按能力类型（`M::Capability`）在 `into_interface`
// 转换之前应用，`resolve` 取回的是装饰后的接口对象。

#[cfg(all(feature = "interface", feature = "decorator"))]
mod interface_decorator_e2e {
    use super::*;
    use trait_kit::core::InterfaceBuilder;

    trait Codec: Send + Sync + 'static {
        fn quality(&self) -> u32;
    }

    /// 具体能力：装饰器按值包装（同一能力类型，提升 quality）。
    struct CodecCap {
        quality: u32,
    }
    impl Codec for CodecCap {
        fn quality(&self) -> u32 {
            self.quality
        }
    }

    struct CodecModule;
    impl_module_meta!(CodecModule, "codec-mod");
    impl AutoBuilder for CodecModule {
        type Capability = Arc<CodecCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(CodecCap { quality: 10 }))
        }
    }
    impl InterfaceBuilder for CodecModule {
        type Interface = dyn Codec;
        type Capability = Arc<CodecCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(CodecCap { quality: 10 }))
        }
        fn into_interface(cap: Self::Capability) -> Arc<Self::Interface> {
            cap as Arc<Self::Interface>
        }
    }

    #[test]
    fn e2e_interface_path_decorator_applies() {
        let mut kit = Kit::new();
        kit.register_as::<CodecModule>().unwrap();
        // 装饰器与 eager/lazy/multi 路径同口径：按模块能力类型注册。
        kit.decorate::<CodecModule>(|cap: Arc<CodecCap>| {
            Arc::new(CodecCap {
                quality: cap.quality * 10,
            })
        });
        let ready = kit.build().unwrap();
        let codec = ready.resolve::<dyn Codec>().unwrap();
        assert_eq!(
            codec.quality(),
            100,
            "register_as 构建路径产出的接口对象应携带装饰（10 * 10）"
        );
    }
}

// ─── CMP-13：encryption + reload 双链共存 ──────────────────────────────

#[cfg(all(feature = "encryption", feature = "reload"))]
mod encryption_reload_e2e {
    use super::*;
    use std::cell::Cell;
    use std::error::Error;
    use std::rc::Rc;
    use trait_kit::kit::ModuleConfig;

    #[derive(Clone, Debug, PartialEq)]
    struct ErRuntimeCfg {
        v: u32,
    }
    impl Configurable for ErRuntimeCfg {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { v: 2 })
        }
    }

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct ErSecret {
        token: String,
    }
    impl ModuleConfig for ErSecret {
        const PATH: &'static str = "config/er-secret.toml";
        fn default_value() -> Self {
            Self {
                token: std::env::var("TRAIT_KIT_TEST_API_KEY").unwrap_or_else(|_| "sample".into()),
            }
        }
    }
    // 32 字节样例主密钥（测试夹具，非真实凭据）。
    // pragma: allowlist secret
    const MASTER_KEY: [u8; 32] = *b"0123456789abcdef0123456789abcdef";

    #[test]
    fn e2e_encryption_plus_reload_dual_chain() {
        let kit = Kit::new();

        // reload 链（confers/watch）：明文配置订阅 + 重载。
        kit.set_config(ErRuntimeCfg { v: 1 });
        let hits = Rc::new(Cell::new(0u32));
        let h = Rc::clone(&hits);
        kit.subscribe::<ErRuntimeCfg>(move || {
            h.set(h.get() + 1);
        });

        // encryption 链（confers/encryption）：密文存储与明文配置并存。
        let secret = ErSecret {
            token: std::env::var("TRAIT_KIT_TEST_API_KEY")
                .unwrap_or_else(|_| "demo-er-6241".into()),
        };
        kit.set_encrypted(&secret, &MASTER_KEY).unwrap();

        let ready = kit.build().unwrap();
        assert_eq!(
            ready.config::<ErRuntimeCfg>().unwrap().v,
            1,
            "明文配置不受密文存储影响"
        );
        assert_eq!(
            ready.get_encrypted::<ErSecret>(&MASTER_KEY).unwrap(),
            secret,
            "密文 roundtrip 与 reload 链共存"
        );

        // Ready 态重载：watch 引擎与加密引擎互不干扰。
        ready.reload_config::<ErRuntimeCfg>().unwrap();
        assert_eq!(hits.get(), 1);
        assert_eq!(ready.config::<ErRuntimeCfg>().unwrap().v, 2);
        assert_eq!(
            ready.get_encrypted::<ErSecret>(&MASTER_KEY).unwrap(),
            secret,
            "重载后密文原样保留"
        );
    }
}

// ─── CMP-15：全 feature 行为级烟囱 ─────────────────────────────────────

#[cfg(all(
    feature = "async",
    feature = "confers",
    feature = "reload",
    feature = "encryption",
    feature = "interface",
    feature = "lifecycle",
    feature = "health",
    feature = "scope",
    feature = "toggle",
    feature = "observer",
    feature = "decorator",
    feature = "shutdown",
    feature = "i18n"
))]
mod all_features_smoke_e2e {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use trait_kit::core::InterfaceBuilder;
    use trait_kit::core::health::{HealthCheck, HealthStatus};
    use trait_kit::core::lifecycle::Lifecycle;
    use trait_kit::core::observer::BuildObserver;
    use trait_kit::i18n::I18nManager;
    use trait_kit::kit::{ShutdownCoordinator, ShutdownPhase};

    // ── 各子系统共用一组最小模块（样例数据，非真实凭据）──

    struct SmokeRes {
        val: u32,
    }

    struct SmokeMod;
    impl_module_meta!(SmokeMod, "smoke-core");
    impl AutoBuilder for SmokeMod {
        type Capability = Arc<SmokeRes>;
        type Error = TraitKitError;
        fn build(kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            let factor = kit.config::<u32>().unwrap_or(1);
            Ok(Arc::new(SmokeRes { val: 7 * factor }))
        }
    }
    impl Lifecycle for SmokeMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<SmokeRes>) {}
    }
    impl HealthCheck for SmokeMod {
        fn check(cap: &Arc<SmokeRes>) -> HealthStatus {
            if cap.val > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::unhealthy("zero")
            }
        }
    }

    struct LazySmoke;
    impl_module_meta!(LazySmoke, "smoke-lazy");
    impl AutoBuilder for LazySmoke {
        type Capability = Arc<SmokeRes>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(SmokeRes { val: 100 }))
        }
    }

    struct MultiSmokeA;
    impl_module_meta!(MultiSmokeA, "smoke-multi-a");
    impl AutoBuilder for MultiSmokeA {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    struct MultiSmokeB;
    impl_module_meta!(MultiSmokeB, "smoke-multi-b");
    impl AutoBuilder for MultiSmokeB {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(2))
        }
    }

    trait Smoker: Send + Sync + 'static {
        fn smoke(&self) -> bool;
    }
    struct SmokerImpl;
    impl Smoker for SmokerImpl {
        fn smoke(&self) -> bool {
            true
        }
    }
    struct SmokerModule;
    impl_module_meta!(SmokerModule, "smoke-iface");
    impl AutoBuilder for SmokerModule {
        type Capability = Arc<SmokerImpl>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(SmokerImpl))
        }
    }
    impl InterfaceBuilder for SmokerModule {
        type Interface = dyn Smoker;
        type Capability = Arc<SmokerImpl>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(SmokerImpl))
        }
        fn into_interface(cap: Self::Capability) -> Arc<Self::Interface> {
            cap as Arc<Self::Interface>
        }
    }

    struct CountingObs {
        built: Arc<AtomicU32>,
    }
    impl BuildObserver for CountingObs {
        fn on_module_built(&self, _: &'static str, _: Duration) {
            self.built.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn e2e_all_features_single_kit_smoke() {
        // ── 构建：toggle 门控注册 + config + observer + decorator +
        //    lifecycle + health，一次 build 打通 ──
        let mut kit = Kit::new();
        kit.set_config(2u32);
        kit.enable_toggle("smoke", true);
        kit.register_if_toggle::<SmokeMod>("smoke").unwrap();
        kit.register_lazy::<LazySmoke>().unwrap();
        kit.register_multi::<MultiSmokeA>().unwrap();
        kit.register_multi::<MultiSmokeB>().unwrap();
        kit.register_as::<SmokerModule>().unwrap();
        kit.register_lifecycle::<SmokeMod>();
        kit.register_health_check::<SmokeMod>();

        let built_count = Arc::new(AtomicU32::new(0));
        kit.with_observer(Arc::new(CountingObs {
            built: Arc::clone(&built_count),
        }));
        // 装饰器按能力类型（Arc<SmokeRes>）注册：eager 与 lazy 路径同键生效。
        kit.decorate::<SmokeMod>(|cap: Arc<SmokeRes>| Arc::new(SmokeRes { val: cap.val * 10 }));

        // ── 配置链（reload）+ 加密链（encryption）先于 build 注入 ──
        kit.set_encrypted(
            &EncSmoke {
                token: "enc-smoke".into(),
            },
            &SMOKE_KEY,
        )
        .unwrap();

        let ready = kit.build().unwrap();

        // eager + decorator：7 * 2 * 10 = 140。
        assert_eq!(ready.require::<SmokeMod>().unwrap().val, 140);
        // lazy：首次 require 触发构建；装饰器按能力类型同样命中 lazy 路径。
        assert_eq!(ready.require::<LazySmoke>().unwrap().val, 1000);
        // multi：注册顺序聚合。
        let multi = ready.require_all::<MultiSmokeA>().unwrap();
        assert_eq!(*multi[0], 1);
        assert_eq!(*multi[1], 2);
        // interface：resolve 动态分派。
        assert!(ready.resolve::<dyn Smoker>().unwrap().smoke());
        // health：报告含已注册 checker 且健康。
        assert!(ready.health_check::<SmokeMod>().unwrap().is_healthy());
        // toggle：跨 build 保持。
        assert!(ready.is_toggle_enabled("smoke"));
        // observer：eager 构建路径回调已触发。
        assert!(built_count.load(Ordering::SeqCst) >= 1);
        // 加密读取。
        assert_eq!(
            ready.get_encrypted::<EncSmoke>(&SMOKE_KEY).unwrap().token,
            "enc-smoke"
        );
        // scope：就绪 Kit 派生空作用域并独立构建（scope 路径无装饰器）。
        let mut scope = ready.create_scope();
        scope.register::<LazySmoke>().unwrap();
        assert_eq!(scope.require::<LazySmoke>().unwrap().val, 100);

        // shutdown 特性：协调器三阶段 + Kit 级关闭。
        let coord = ShutdownCoordinator::new();
        coord.register_hook(ShutdownPhase::CloseConnections, || {});
        assert!(coord.shutdown().iter().all(|r| r.is_ok()));
        ready.shutdown();

        // ── async 面：AsyncKit 同烟囱最小打通 ──
        let mut akit = AsyncKit::new();
        akit.register::<AsyncSmoke>().unwrap();
        let aready = block_on(akit.build()).unwrap();
        assert_eq!(aready.require::<AsyncSmoke>().unwrap().val, 21);

        // ── i18n：全局翻译便捷函数（默认 locale，不锁定语言）──
        let mgr = I18nManager::init();
        let msg = mgr.translate("trait-kit-error-already-registered", &[("module", "smoke")]);
        assert!(msg.contains("smoke"));
    }

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct EncSmoke {
        token: String,
    }
    impl ModuleConfig for EncSmoke {
        const PATH: &'static str = "config/smoke-enc.toml";
        fn default_value() -> Self {
            Self {
                token: std::env::var("TRAIT_KIT_TEST_API_KEY").unwrap_or_else(|_| "sample".into()),
            }
        }
    }

    struct AsyncSmoke;
    impl_module_meta!(AsyncSmoke, "smoke-async");
    impl AsyncAutoBuilder for AsyncSmoke {
        type Capability = Arc<SmokeRes>;
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
            Box::pin(async { Ok(Arc::new(SmokeRes { val: 7 * 3 })) })
        }
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Waker};
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

    // 32 字节样例主密钥（测试夹具，非真实凭据）。
    // pragma: allowlist secret
    const SMOKE_KEY: [u8; 32] = *b"0123456789abcdef0123456789abcdef";
}
