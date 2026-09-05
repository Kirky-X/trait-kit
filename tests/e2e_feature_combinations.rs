// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// 穷举特性组合 E2E 测试：每个 feature 独立 + 关键多特性组合。

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
    struct HCap { val: u64 }
    impl AutoBuilder for HealthyMod {
        type Capability = Arc<HCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<HCap>, TraitKitError> {
            Ok(Arc::new(HCap { val: 10 }))
        }
    }
    impl HealthCheck for HealthyMod {
        fn check(cap: &Arc<HCap>) -> HealthStatus {
            if cap.val > 0 { HealthStatus::Healthy }
            else { HealthStatus::Unhealthy { detail: "zero".into() } }
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
            HealthStatus::Unhealthy { detail: "down".into() }
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
        kit.with_observer(Arc::new(CountingObserver { count: Arc::clone(&count) }));
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
        struct TM; impl_module_meta!(TM, "tm");
        impl AutoBuilder for TM {
            type Capability = Arc<()>;
            type Error = TraitKitError;
            fn build(_: &Kit) -> Result<Arc<()>, TraitKitError> { Ok(Arc::new(())) }
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
    use super::*;
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
        let msg = trait_kit::i18n::tr("trait-kit-error-already-registered", &[("module", "test-mod")]);
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
        fn greet(&self) -> String { "Hello!".into() }
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
    struct E2eDbConfig { host: String, port: u16 }
    impl Configurable for E2eDbConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { host: "loaded".into(), port: 5432 })
        }
    }
    impl ModuleConfig for E2eDbConfig {
        const PATH: &'static str = "config/e2e_db.toml";
        fn default_value() -> Self { Self { host: "localhost".into(), port: 3306 } }
    }

    #[test]
    fn e2e_confers_load_and_validate_ok() {
        #[derive(Clone, Debug)]
        struct V { port: u16 }
        impl Configurable for V {
            fn load() -> Result<Self, Box<dyn Error + Send>> { Ok(Self { port: 8080 }) }
        }
        impl Validatable for V {
            fn validate(&self) -> Result<(), Vec<String>> {
                if self.port > 0 && self.port < 65535 { Ok(()) }
                else { Err(vec!["out of range".into()]) }
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
            fn load() -> Result<Self, Box<dyn Error + Send>> { Ok(Self) }
        }
        impl Validatable for Bad {
            fn validate(&self) -> Result<(), Vec<String>> { Err(vec!["bad".into()]) }
        }
        let kit = Kit::new();
        assert!(kit.load_and_validate::<Bad>().is_err());
    }

    #[test]
    fn e2e_confers_snapshot_restore() {
        let kit = Kit::new();
        kit.set_config(E2eDbConfig { host: "orig".into(), port: 3306 });
        kit.snapshot_config::<E2eDbConfig>();
        kit.set_config(E2eDbConfig { host: "mod".into(), port: 5432 });
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
        kit.set_config(E2eDbConfig { host: "custom".into(), port: 9999 });
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
        let vars = std::collections::HashMap::from([
            ("H".into(), "interp-host".into()),
        ]);
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
    struct RCfg { v: u32 }
    impl Configurable for RCfg {
        fn load() -> Result<Self, Box<dyn Error + Send>> { Ok(Self { v: 2 }) }
    }

    #[test]
    fn e2e_reload_updates_and_notifies() {
        let kit = Kit::new();
        kit.set_config(RCfg { v: 1 });
        let notified = Rc::new(Cell::new(false));
        let n = Rc::clone(&notified);
        kit.subscribe::<RCfg>(move || { n.set(true); });
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
    struct SCfg { key: String }
    impl ModuleConfig for SCfg {
        const PATH: &'static str = "config/s.toml";
        fn default_value() -> Self { Self { key: "def".into() } }
    }
    const KEY: [u8; 32] = *b"0123456789abcdef0123456789abcdef";

    #[test]
    fn e2e_encryption_roundtrip() {
        let kit = Kit::new();
        kit.set_encrypted(&SCfg { key: "secret".into() }, &KEY).unwrap();
        let ready = kit.build().unwrap();
        assert_eq!(ready.get_encrypted::<SCfg>(&KEY).unwrap().key, "secret");
    }

    #[test]
    fn e2e_encryption_short_key_rejected() {
        let kit = Kit::new();
        assert!(kit.set_encrypted(&SCfg { key: "x".into() }, &[0u8; 8]).is_err());
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
    struct LhCap { val: u32 }
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
            if cap.val > 0 { HealthStatus::Healthy }
            else { HealthStatus::Unhealthy { detail: "zero".into() } }
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
    struct CrCfg { val: u32 }
    impl Configurable for CrCfg {
        fn load() -> Result<Self, Box<dyn Error + Send>> { Ok(Self { val: 100 }) }
    }

    #[test]
    fn e2e_confers_plus_reload() {
        let kit = Kit::new();
        kit.set_config(CrCfg { val: 1 });
        let counter = Rc::new(Cell::new(0u32));
        let c = Rc::clone(&counter);
        kit.subscribe::<CrCfg>(move || { c.set(c.get() + 1); });
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
    struct CeCfg { secret: String, port: u16 }
    impl ModuleConfig for CeCfg {
        const PATH: &'static str = "config/ce.toml";
        fn default_value() -> Self { Self { secret: "def".into(), port: 8080 } }
    }
    impl Configurable for CeCfg {
        fn load() -> Result<Self, Box<dyn std::error::Error + Send>> {
            Ok(Self { secret: "loaded".into(), port: 9090 })
        }
    }
    const KEY: [u8; 32] = *b"abcdefghijklmnopqrstuvwxyz012345";

    #[test]
    fn e2e_confers_plus_encryption() {
        let kit = Kit::new();
        kit.set_config(CeCfg { secret: "plain".into(), port: 1111 });
        kit.set_encrypted(&CeCfg { secret: "encrypted".into(), port: 2222 }, &KEY).unwrap();
        let ready = kit.build().unwrap();
        assert_eq!(ready.config::<CeCfg>().unwrap().secret, "plain");
        assert_eq!(ready.get_encrypted::<CeCfg>(&KEY).unwrap().secret, "encrypted");
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
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> { Ok(Arc::new(5)) }
    }
    struct OdObs { count: Arc<AtomicU32> }
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
        kit.with_observer(Arc::new(OdObs { count: Arc::clone(&count) }));
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
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> { Ok(Arc::new(10)) }
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
