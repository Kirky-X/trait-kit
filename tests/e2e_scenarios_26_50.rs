// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! E2E 场景批次 26-50（异常路径）：真实 API 驱动，逐场景断言错误面。
//!
//! 场景点名的既有测试（e17/c14/e07/e27/e2e_hooks/e2e_features）原样运行，
//! 本文件补齐无既有锚点的场景，全部走公开 API。

#![allow(clippy::needless_pass_by_value, clippy::type_complexity)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

// ─── 场景 26 · RLD-10 AsyncKit 订阅者 panic 隔离（async, reload） ───

#[cfg(all(feature = "async", feature = "reload"))]
mod rld10_subscriber_panic {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[derive(Clone, Debug, PartialEq)]
    struct RldCfg {
        v: u32,
    }
    impl Configurable for RldCfg {
        fn load() -> Result<Self, Box<dyn std::error::Error + Send>> {
            Ok(Self { v: 7 })
        }
    }

    /// async 侧：panic 订阅者被隔离（不穿透），其后的订阅者仍被通知，
    /// 新值已存储，调用方拿到 Ok。sync 侧对照：订阅者 panic 穿透且
    /// 其后订阅者被跳过（Kit::reload_config 文档化语义）。
    #[test]
    fn rld10_async_subscriber_panic_isolated_sync_penetrates() {
        // async 侧
        let kit = AsyncKit::new();
        kit.set_config(RldCfg { v: 1 });
        let before_flag = Arc::new(AtomicBool::new(false));
        let after_flag = Arc::new(AtomicBool::new(false));
        kit.subscribe::<RldCfg>({
            let f = Arc::clone(&before_flag);
            move || {
                f.store(true, Ordering::SeqCst);
                panic!("async subscriber boom");
            }
        });
        kit.subscribe::<RldCfg>({
            let f = Arc::clone(&after_flag);
            move || f.store(true, Ordering::SeqCst)
        });
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = kit.reload_config::<RldCfg>();
        std::panic::set_hook(quiet);
        assert!(
            result.is_ok(),
            "async reload_config should return Ok despite a panicking subscriber: {result:?}"
        );
        assert!(
            before_flag.load(Ordering::SeqCst),
            "the panicking subscriber itself must have been dispatched"
        );
        assert!(
            after_flag.load(Ordering::SeqCst),
            "subscribers registered after the panicking one must still be notified"
        );
        assert_eq!(
            kit.config::<RldCfg>().unwrap().v,
            7,
            "the new value must already be stored when the panic is isolated"
        );

        // sync 对照：panic 穿透 reload_config，其余订阅者被跳过
        let kit = Kit::new();
        kit.set_config(RldCfg { v: 1 });
        let after_sync = Rc::new(Cell::new(false));
        kit.subscribe::<RldCfg>(|| panic!("sync subscriber boom"));
        kit.subscribe::<RldCfg>({
            let f = Rc::clone(&after_sync);
            move || f.set(true)
        });
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            kit.reload_config::<RldCfg>()
        }));
        std::panic::set_hook(quiet);
        assert!(
            caught.is_err(),
            "sync reload_config must propagate the subscriber panic (documented semantics)"
        );
        assert!(
            !after_sync.get(),
            "sync side skips the remaining subscribers after the panic"
        );
        let ready = kit.build().unwrap();
        assert_eq!(
            ready.config::<RldCfg>().unwrap().v,
            7,
            "sync side stores the new value before invoking callbacks"
        );
    }
}

// ─── 场景 28 · ENC-03 未 set 直接 get → MissingConfig ───
// ─── 场景 29 · ENC-06 不可序列化值 → BuildFailed ───
// ─── 场景 30 · ENC-07 短密钥 InvalidInput 消息含实际长度 ───
// ─── 场景 32 · ENC-09 弱密钥两侧拒收（sync/async） ───

#[cfg(feature = "encryption")]
mod enc_error_surface {
    use super::*;
    use trait_kit::kit::ModuleConfig;

    const KEY32: [u8; 32] = *b"0123456789abcdef0123456789abcdef";

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct EncCfg {
        v: String,
    }
    impl ModuleConfig for EncCfg {
        const PATH: &'static str = "config/enc-error.toml";
        fn default_value() -> Self {
            Self { v: String::new() }
        }
    }

    /// ENC-03：未 set 直接 get → MissingConfig，绝不返回明文。
    #[test]
    fn enc03_get_without_set_returns_missing_config() {
        let kit = Kit::new();
        let built = kit.build().unwrap();
        match built.get_encrypted::<EncCfg>(&KEY32) {
            Err(TraitKitError::MissingConfig { key }) => {
                assert_eq!(key, "EncCfg", "MissingConfig key should name the type");
            }
            Ok(v) => panic!("must never return plaintext, got: {v:?}"),
            other => panic!("expected MissingConfig, got: {other:?}"),
        }
    }

    /// ENC-06：JSON 无法序列化的值（非字符串键 map）→ BuildFailed。
    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct NotSerializable {
        m: std::collections::HashMap<Vec<u8>, u32>,
    }
    impl ModuleConfig for NotSerializable {
        const PATH: &'static str = "config/enc-unserializable.toml";
        fn default_value() -> Self {
            Self {
                m: std::collections::HashMap::new(),
            }
        }
    }

    #[test]
    fn enc06_unserializable_value_propagates_build_failed() {
        let kit = Kit::new();
        let cfg = NotSerializable {
            m: std::collections::HashMap::from([(vec![1u8, 2], 3u32)]),
        };
        match kit.set_encrypted(&cfg, &KEY32) {
            Err(TraitKitError::BuildFailed { context, source }) => {
                assert_eq!(&*context, "set_encrypted");
                let msg = source.to_string();
                assert!(
                    msg.contains("key must be a string"),
                    "serialization error should surface: {msg}"
                );
            }
            Ok(()) => panic!("unserializable value must not be accepted"),
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
        // 失败的 set 不得留下密文槽位（fail-closed）。
        let built = kit.build().unwrap();
        assert!(
            matches!(
                built.get_encrypted::<NotSerializable>(&KEY32),
                Err(TraitKitError::MissingConfig { .. })
            ),
            "a failed set_encrypted must not leave a decryptable slot"
        );
    }

    /// ENC-07：20 字节短密钥 → BuildFailed/InvalidInput，消息含实际
    /// 长度 20 与下限 32（en-US 默认 locale）。
    #[test]
    fn enc07_short_key_invalid_input_message_contains_lengths() {
        let kit = Kit::new();
        let err = kit
            .set_encrypted(&EncCfg { v: "x".into() }, &[7u8; 20])
            .expect_err("20-byte key must be rejected");
        match err {
            TraitKitError::BuildFailed { context, source } => {
                assert_eq!(&*context, "set_encrypted");
                let io = source
                    .downcast_ref::<std::io::Error>()
                    .expect("source should be an io::Error");
                assert_eq!(io.kind(), std::io::ErrorKind::InvalidInput);
                let msg = io.to_string();
                assert!(
                    msg.contains("32") && msg.contains("20"),
                    "message should contain min 32 and actual 20: {msg}"
                );
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
        // get 侧同样立即拒绝（先于 MissingConfig 查找）。
        let kit = Kit::new();
        kit.set_encrypted(&EncCfg { v: "x".into() }, &KEY32)
            .unwrap();
        let built = kit.build().unwrap();
        let err = built
            .get_encrypted::<EncCfg>(&[7u8; 20])
            .expect_err("short key must be rejected on get too");
        assert!(
            matches!(err, TraitKitError::BuildFailed { .. }),
            "get-side short key should be BuildFailed, got: {err:?}"
        );
    }

    /// ENC-09：全零 / 常量重复字节密钥在 set 与 get 两侧拒收，
    /// sync 与 async 口径一致。
    #[test]
    fn enc09_weak_key_rejected_on_both_sides_sync_and_async() {
        let weak_zero = [0u8; 32];
        let weak_repeat = [0xABu8; 32];

        // sync set 侧
        let kit = Kit::new();
        for (label, weak) in [("all-zero", &weak_zero), ("repeated", &weak_repeat)] {
            let err = kit
                .set_encrypted(&EncCfg { v: "x".into() }, weak)
                .expect_err(label);
            match err {
                TraitKitError::BuildFailed { context, source } => {
                    assert_eq!(&*context, "set_encrypted");
                    let msg = source.to_string();
                    assert!(
                        msg.contains("constant-byte"),
                        "{label} key should hit the weak-material message: {msg}"
                    );
                }
                other => panic!("expected BuildFailed for {label}, got: {other:?}"),
            }
        }

        // sync get 侧：先以合法密钥落库，再以弱密钥取 → 拒绝
        kit.set_encrypted(&EncCfg { v: "secret".into() }, &KEY32)
            .unwrap();
        let built = kit.build().unwrap();
        for weak in [&weak_zero, &weak_repeat] {
            let err = built
                .get_encrypted::<EncCfg>(weak)
                .expect_err("weak key must be rejected on get");
            let msg = err.to_string();
            assert!(
                msg.contains("constant-byte"),
                "get-side weak key should hit the weak-material message: {msg}"
            );
        }

        // async set 侧
        let akit = AsyncKit::new();
        for weak in [&weak_zero, &weak_repeat] {
            let err = akit
                .set_encrypted(&EncCfg { v: "x".into() }, weak)
                .expect_err("async set with weak key");
            let msg = err.to_string();
            assert!(
                msg.contains("constant-byte"),
                "async weak key should match sync wording: {msg}"
            );
        }

        // async get 侧
        akit.set_encrypted(&EncCfg { v: "secret".into() }, &KEY32)
            .unwrap();
        for weak in [&weak_zero, &weak_repeat] {
            let err = akit
                .get_encrypted::<EncCfg>(weak)
                .expect_err("async get with weak key");
            let msg = err.to_string();
            assert!(
                msg.contains("constant-byte"),
                "async get weak key should match sync wording: {msg}"
            );
        }
    }
}

// ─── 场景 34 · ITF-03 build 前 resolve → MissingCapability ───
// ─── 场景 36 · ITF-07 同模块两次 register_as → AlreadyRegistered ───
// ─── 场景 37 · ITF-08 InterfaceBuilder build 失败 → BuildFailed(context=<interface>) ───

#[cfg(feature = "di")]
mod itf_error_surface {
    use super::*;
    use trait_kit::core::InterfaceBuilder;
    use trait_kit::core::ModuleMeta;

    trait Greeter: Send + Sync + 'static {
        fn greet(&self) -> &'static str;
    }

    struct GreeterCap;
    impl Greeter for GreeterCap {
        fn greet(&self) -> &'static str {
            "hello"
        }
    }

    struct GreeterModule;
    impl_module_meta!(GreeterModule, "itf-greeter");
    impl InterfaceBuilder for GreeterModule {
        type Interface = dyn Greeter;
        type Capability = Arc<GreeterCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<GreeterCap>, TraitKitError> {
            Ok(Arc::new(GreeterCap))
        }
        fn into_interface(cap: Arc<GreeterCap>) -> Arc<dyn Greeter> {
            cap
        }
    }

    /// ITF-03：register_as 之后、build 之前 resolve → MissingCapability。
    #[test]
    fn itf03_resolve_before_build_returns_missing_capability() {
        let mut kit = Kit::new();
        kit.register_as::<GreeterModule>().expect("register_as");
        let res = kit.resolve::<dyn Greeter>();
        match res {
            Err(TraitKitError::MissingCapability { key }) => {
                assert_eq!(key, "interface");
            }
            Err(e) => panic!("expected MissingCapability, got: {e:?}"),
            Ok(_) => panic!("capability must not exist before build"),
        }
        // build 后同一 resolve 成功——失败只在 build 前发生。
        let built = kit.build().unwrap();
        assert_eq!(built.resolve::<dyn Greeter>().unwrap().greet(), "hello");
    }

    /// ITF-07：同一模块两次 register_as → AlreadyRegistered（报告的
    /// 是占用者名字 = 该模块自己的 NAME）。
    #[test]
    fn itf07_same_module_registered_as_twice_returns_already_registered() {
        let mut kit = Kit::new();
        kit.register_as::<GreeterModule>()
            .expect("first register_as");
        match kit.register_as::<GreeterModule>() {
            Err(TraitKitError::AlreadyRegistered { module }) => {
                assert_eq!(module, "itf-greeter");
            }
            Ok(_) => panic!("second register_as must be rejected"),
            other => panic!("expected AlreadyRegistered, got: {other:?}"),
        }
    }

    struct BrokenCap;
    struct BrokenModule;
    impl ModuleMeta for BrokenModule {
        const NAME: &'static str = "itf-broken";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl InterfaceBuilder for BrokenModule {
        type Interface = dyn Greeter;
        type Capability = Arc<BrokenCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<BrokenCap>, TraitKitError> {
            Err(TraitKitError::MissingCapability { key: "boom".into() })
        }
        fn into_interface(_cap: Arc<BrokenCap>) -> Arc<dyn Greeter> {
            unreachable!("build always fails")
        }
    }

    /// ITF-08：InterfaceBuilder::build 返回 Err → BuildFailed，
    /// context 为 `<interface>`（en-US locale）。
    #[test]
    fn itf08_interface_build_failure_is_build_failed_with_interface_context() {
        let mut kit = Kit::new();
        kit.register_as::<BrokenModule>().expect("register_as");
        match kit.build() {
            Err(TraitKitError::BuildFailed { context, .. }) => {
                assert_eq!(&*context, "<interface>");
            }
            Ok(_) => panic!("build must fail"),
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
    }
}

// ─── 场景 40 · HLT-04 未注册 checker → MissingConfig(key=NAME) ───
// ─── 场景 41 · HLT-05 Unhealthy 分支如实上报 ───

#[cfg(feature = "health")]
mod hlt_error_surface {
    use super::*;
    use trait_kit::core::health::{HealthCheck, HealthStatus};

    struct ZeroCap {
        val: u32,
    }

    struct ZeroMod;
    impl_module_meta!(ZeroMod, "zero-mod");
    impl AutoBuilder for ZeroMod {
        type Capability = Arc<ZeroCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<ZeroCap>, TraitKitError> {
            Ok(Arc::new(ZeroCap { val: 0 }))
        }
    }
    impl HealthCheck for ZeroMod {
        fn check(cap: &Arc<ZeroCap>) -> HealthStatus {
            if cap.val > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "zero".into(),
                }
            }
        }
    }

    /// HLT-04：未 register_health_check 直接 health_check::<M> →
    /// MissingConfig，key 为模块 NAME。
    #[test]
    fn hlt04_unregistered_checker_returns_missing_config_with_module_name() {
        let mut kit = Kit::new();
        kit.register::<ZeroMod>().unwrap();
        let ready = kit.build().unwrap();
        match ready.health_check::<ZeroMod>() {
            Err(TraitKitError::MissingConfig { key }) => {
                assert_eq!(key, "zero-mod", "key must be the module NAME");
            }
            Ok(status) => panic!("no checker registered, got: {status:?}"),
            other => panic!("expected MissingConfig, got: {other:?}"),
        }
    }

    /// HLT-05：零值模块的 checker 如实返回 Unhealthy{detail:"zero"}，
    /// health_report 与 health_check 两个入口一致，不美化成 Healthy。
    #[test]
    fn hlt05_unhealthy_branch_reported_truthfully() {
        let mut kit = Kit::new();
        kit.register::<ZeroMod>().unwrap();
        kit.register_health_check::<ZeroMod>();
        let ready = kit.build().unwrap();

        let report = ready.health_report();
        let entry = report
            .iter()
            .find(|(name, _)| *name == "zero-mod")
            .expect("zero-mod should appear in the report");
        match &entry.1 {
            HealthStatus::Unhealthy { detail } => assert_eq!(detail, "zero"),
            other => panic!("expected Unhealthy, got: {other:?}"),
        }

        match ready.health_check::<ZeroMod>().unwrap() {
            HealthStatus::Unhealthy { detail } => assert_eq!(detail, "zero"),
            other => panic!("expected Unhealthy, got: {other:?}"),
        }
        assert!(!ready.health_check::<ZeroMod>().unwrap().is_healthy());
    }
}

// ─── 场景 43 · OBS-03 构建失败回调 on_build_error 后继续传播 ───

#[cfg(feature = "observer")]
mod obs03_on_build_error {
    use super::*;
    use trait_kit::core::observer::BuildObserver;

    struct ObservedFailMod;
    impl_module_meta!(ObservedFailMod, "observed-fail-mod");
    impl AutoBuilder for ObservedFailMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
            Err(TraitKitError::MissingCapability {
                key: "obs-dep".into(),
            })
        }
    }

    struct ErrorRecorder {
        calls: AtomicUsize,
        last_name: std::sync::Mutex<Option<&'static str>>,
        last_cause: std::sync::Mutex<Option<String>>,
    }
    impl BuildObserver for ErrorRecorder {
        fn on_build_error(&self, module_name: &'static str, error: &TraitKitError) {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last_name.lock().unwrap() = Some(module_name);
            // 构建循环把原始错误包成 BuildFailed(context=模块名) 再回调，
            // 这里解包验证原始原因仍然可见。
            if let TraitKitError::BuildFailed { source, .. } = error {
                *self.last_cause.lock().unwrap() = Some(format!("{source:?}"));
            }
        }
    }

    /// 模块构建失败：observer.on_build_error 先收到（模块名 + 原始错误），
    /// 随后 build 仍把错误原样传播给调用方。
    #[test]
    fn obs03_on_build_error_called_then_error_propagates() {
        let recorder = Arc::new(ErrorRecorder {
            calls: AtomicUsize::new(0),
            last_name: std::sync::Mutex::new(None),
            last_cause: std::sync::Mutex::new(None),
        });
        let mut kit = Kit::new();
        kit.with_observer(Arc::clone(&recorder) as Arc<dyn BuildObserver>);
        kit.register::<ObservedFailMod>().unwrap();

        let err = kit
            .build()
            .expect_err("build must keep failing after the observer callback");
        assert_eq!(recorder.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            *recorder.last_name.lock().unwrap(),
            Some("observed-fail-mod")
        );
        let seen = recorder
            .last_cause
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_default();
        assert!(
            seen.contains("obs-dep"),
            "the observer must see the original cause (obs-dep) inside the wrapper: {seen}"
        );
        match err {
            TraitKitError::BuildFailed { context, source } => {
                assert_eq!(&*context, "observed-fail-mod");
                assert!(
                    format!("{source:?}").contains("obs-dep"),
                    "the original cause must survive propagation: {source:?}"
                );
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
    }
}

// ─── 场景 45 · SCP-03 Scope 重复注册报错 ───
// ─── 场景 46 · SCP-04 Scope require 未注册 → MissingCapability ───

#[cfg(feature = "request-scope")]
mod scp_error_surface {
    use super::*;

    struct ScopeMod;
    impl_module_meta!(ScopeMod, "scope-mod");
    impl AutoBuilder for ScopeMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
            Ok(Arc::new(9))
        }
    }

    struct GhostMod;
    impl_module_meta!(GhostMod, "scope-ghost-mod");
    impl AutoBuilder for GhostMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
            unreachable!("never registered")
        }
    }

    /// SCP-03：同一模块在 Scope 内二次 register → AlreadyRegistered。
    #[test]
    fn scp03_duplicate_scope_register_returns_already_registered() {
        let mut scope = trait_kit::kit::Scope::new();
        scope.register::<ScopeMod>().expect("first register");
        match scope.register::<ScopeMod>() {
            Err(TraitKitError::AlreadyRegistered { module }) => {
                assert_eq!(module, "scope-mod");
            }
            Ok(_) => panic!("duplicate scope register must be rejected"),
            other => panic!("expected AlreadyRegistered, got: {other:?}"),
        }
    }

    /// SCP-04：require 未注册模块 → MissingCapability（key=NAME）。
    #[test]
    fn scp04_require_unregistered_returns_missing_capability() {
        let scope = trait_kit::kit::Scope::new();
        match scope.require::<GhostMod>() {
            Err(TraitKitError::MissingCapability { key }) => {
                assert_eq!(key, "scope-ghost-mod");
            }
            Ok(_) => panic!("unregistered module must not resolve"),
            other => panic!("expected MissingCapability, got: {other:?}"),
        }
        assert!(!scope.contains::<GhostMod>());
    }
}

// ─── 场景 47 · SCP-09 AsyncScope 异常面 ───

#[cfg(all(feature = "request-scope", feature = "async"))]
mod scp09_async_scope {
    use super::*;
    use trait_kit::kit::AsyncScope;

    struct AsyncScopeMod;
    impl_module_meta!(AsyncScopeMod, "async-scope-mod");
    impl trait_kit::core::AsyncAutoBuilder for AsyncScopeMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(
            _kit: &trait_kit::kit::AsyncKit,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<Arc<u32>, TraitKitError>> + Send>>
        {
            Box::pin(async { Ok(Arc::new(1)) })
        }
    }

    struct AsyncGhostMod;
    impl_module_meta!(AsyncGhostMod, "async-ghost-mod");
    impl trait_kit::core::AsyncAutoBuilder for AsyncGhostMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(
            _kit: &trait_kit::kit::AsyncKit,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<Arc<u32>, TraitKitError>> + Send>>
        {
            Box::pin(async {
                let _never: Result<Arc<u32>, TraitKitError>;
                unreachable!("never inserted")
            })
        }
    }

    /// 重复 insert 覆盖旧值（last-write-wins）；未 insert 的 require →
    /// MissingCapability（key=NAME）。
    #[test]
    fn scp09_async_scope_insert_overwrites_and_missing_require_errors() {
        let scope = AsyncScope::new();
        scope.insert::<AsyncScopeMod>(Arc::new(1));
        scope.insert::<AsyncScopeMod>(Arc::new(2));
        assert_eq!(*scope.require::<AsyncScopeMod>().unwrap(), 2);

        match scope.require::<AsyncGhostMod>() {
            Err(TraitKitError::MissingCapability { key }) => {
                assert_eq!(key, "async-ghost-mod");
            }
            Ok(_) => panic!("uninserted module must not resolve"),
            other => panic!("expected MissingCapability, got: {other:?}"),
        }
        assert!(!scope.contains::<AsyncGhostMod>());
    }
}

// ─── 场景 48 · TGL-04 开启但已注册 → AlreadyRegistered ───
// ─── 场景 49 · TGL-07 开关关闭对应能力不可获取，开启后恢复 ───

#[cfg(feature = "toggle")]
mod tgl_error_surface {
    use super::*;

    struct ToggledMod;
    impl_module_meta!(ToggledMod, "toggled-mod");
    impl AutoBuilder for ToggledMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
            Ok(Arc::new(5))
        }
    }

    /// TGL-04：开关开启 + 模块已注册 → AlreadyRegistered。
    #[test]
    fn tgl04_enabled_toggle_with_registered_module_returns_already_registered() {
        let mut kit = Kit::new();
        kit.register::<ToggledMod>().unwrap();
        kit.enable_toggle("tgl-04", true);
        match kit.register_if_toggle::<ToggledMod>("tgl-04") {
            Err(TraitKitError::AlreadyRegistered { module }) => {
                assert_eq!(module, "toggled-mod");
            }
            Ok(registered) => panic!("expected AlreadyRegistered, got Ok({registered})"),
            other => panic!("expected AlreadyRegistered, got: {other:?}"),
        }
    }

    /// TGL-07：开关关闭 → register_if_toggle 返回 Ok(false)，模块不进
    /// 图（require 缺失 / contains false）；同一 kit 上重新开启后再注册
    /// 即恢复可用。
    #[test]
    fn tgl07_off_toggle_blocks_capability_on_restore() {
        let mut kit = Kit::new();
        kit.enable_toggle("tgl-07", false);
        let registered = kit
            .register_if_toggle::<ToggledMod>("tgl-07")
            .expect("off-branch returns Ok");
        assert!(!registered, "off toggle must skip registration");

        // 同一 Unbuilt kit：重新开启后再注册，能力恢复。
        kit.enable_toggle("tgl-07", true);
        let registered = kit
            .register_if_toggle::<ToggledMod>("tgl-07")
            .expect("on-branch returns Ok");
        assert!(registered, "re-enabled toggle must register");

        let ready = kit.build().unwrap();
        assert!(ready.contains::<ToggledMod>());
        assert_eq!(*ready.require::<ToggledMod>().unwrap(), 5);

        // 对照：纯关闭分支下 build 后能力不可获取。
        let mut kit_off = Kit::new();
        kit_off.enable_toggle("tgl-07-off", false);
        kit_off
            .register_if_toggle::<ToggledMod>("tgl-07-off")
            .expect("off-branch Ok");
        let ready_off = kit_off.build().unwrap();
        assert!(!ready_off.contains::<ToggledMod>());
        match ready_off.require::<ToggledMod>() {
            Err(TraitKitError::MissingCapability { key }) => {
                assert_eq!(key, "toggled-mod");
            }
            Ok(_) => panic!("capability must be absent while toggle is off"),
            other => panic!("expected MissingCapability, got: {other:?}"),
        }
    }
}

// ─── 场景 50 · SHD-03 阶段超时跳过剩余 hook，后续阶段继续 ───

#[cfg(feature = "shutdown")]
mod shd03_phase_timeout {
    use super::*;
    use std::time::Duration;
    use trait_kit::kit::shutdown::{ShutdownCoordinator, ShutdownPhase};

    /// StopRequests 阶段：慢钩子独占超过阶段预算 → 同阶段剩余钩子被跳过
    /// 且该阶段标记 timed_out；后续 DrainQueue 阶段照常执行。
    #[test]
    fn shd03_phase_timeout_skips_remaining_hooks_and_continues() {
        let coord = ShutdownCoordinator::new();
        coord.set_phase_timeout(ShutdownPhase::StopRequests, Duration::from_millis(40));

        coord.register_hook(ShutdownPhase::StopRequests, || {
            std::thread::sleep(Duration::from_millis(120));
        });
        let skipped = Arc::new(AtomicBool::new(false));
        let s = Arc::clone(&skipped);
        coord.register_hook(ShutdownPhase::StopRequests, move || {
            s.store(true, Ordering::SeqCst);
        });

        let drained = Arc::new(AtomicBool::new(false));
        let d = Arc::clone(&drained);
        coord.register_hook(ShutdownPhase::DrainQueue, move || {
            d.store(true, Ordering::SeqCst);
        });
        let closed = Arc::new(AtomicBool::new(false));
        let c = Arc::clone(&closed);
        coord.register_hook(ShutdownPhase::CloseConnections, move || {
            c.store(true, Ordering::SeqCst);
        });

        let result = coord.shutdown();

        let stop = result
            .phases
            .iter()
            .find(|p| p.phase == ShutdownPhase::StopRequests)
            .expect("StopRequests phase result");
        assert!(stop.timed_out, "StopRequests must be marked timed_out");
        assert!(
            !skipped.load(Ordering::SeqCst),
            "the hook after the slow one must be skipped"
        );
        assert!(
            drained.load(Ordering::SeqCst),
            "the later DrainQueue phase must still run"
        );
        assert!(
            closed.load(Ordering::SeqCst),
            "the later CloseConnections phase must still run"
        );
        assert!(
            !result.is_ok(),
            "overall result must reflect the timed-out phase"
        );
        assert_eq!(
            result.timed_out_phases(),
            vec![ShutdownPhase::StopRequests],
            "only StopRequests should be timed out"
        );
    }
}
