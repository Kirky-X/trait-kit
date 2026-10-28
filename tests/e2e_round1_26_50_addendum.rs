// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! E2E 场景批次 26-50 补遗（第 1 轮缺口）：
//!
//! - 场景 31 · ENC-08 空 master_key（0 字节）走 <32 拒绝分支（set/get 两侧）
//! - 场景 33 · ITF-05 两模块注册同一接口 → 第二次 AlreadyRegistered（占用者名）
//! - 场景 38 · LC-03 on_ready Err → LifecycleFailed{context=NAME} 精确断言
//! - 场景 39 · LC-05 某模块 on_shutdown panic 不阻断其余模块关闭（行为级缺口）
//! - 场景 42 · HLT-06 sync 侧 checker 找不到 capability → Unhealthy("capability not found")
//! - 场景 50 · SHD-04 全局超时中止后续阶段 + timed_out_phases 列出被中止阶段

#![allow(clippy::needless_pass_by_value, clippy::type_complexity)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

// ─── 场景 31 · ENC-08 空 master_key（0 字节）走 <32 拒绝分支 ───

#[cfg(feature = "encryption")]
mod enc08_empty_key {
    use super::*;
    use trait_kit::kit::ModuleConfig;

    const KEY32: [u8; 32] = *b"0123456789abcdef0123456789abcdef";

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct EmptyKeyCfg {
        v: String,
    }
    impl ModuleConfig for EmptyKeyCfg {
        const PATH: &'static str = "config/enc-empty-key.toml";
        fn default_value() -> Self {
            Self { v: String::new() }
        }
    }

    /// 空 master_key（0 字节）在 set 侧立即拒绝：BuildFailed{context=
    /// "set_encrypted"}，InvalidInput，消息同时含下限 32 与实际长度 0。
    #[test]
    fn enc08_empty_key_rejected_on_set_side() {
        let kit = Kit::new();
        let err = kit
            .set_encrypted(&EmptyKeyCfg { v: "x".into() }, &[])
            .expect_err("0-byte key must be rejected on set");
        match err {
            TraitKitError::BuildFailed { context, source } => {
                assert_eq!(&*context, "set_encrypted");
                let io = source
                    .downcast_ref::<std::io::Error>()
                    .expect("source should be an io::Error");
                assert_eq!(io.kind(), std::io::ErrorKind::InvalidInput);
                let msg = io.to_string();
                assert!(
                    msg.contains("32") && msg.contains("0"),
                    "message should contain min 32 and actual length 0: {msg}"
                );
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
    }

    /// 空 master_key 在 get 侧同样立即拒绝，且先于 MissingConfig 查找：
    /// 完全没存过加密值的 kit 也返回 BuildFailed（非 MissingConfig）；
    /// 已有合法密文的 kit 以空钥 get 同样 BuildFailed，绝不返回明文。
    #[test]
    fn enc08_empty_key_rejected_on_get_side_before_missing_config() {
        let built = Kit::new().build().unwrap();
        match built.get_encrypted::<EmptyKeyCfg>(&[]) {
            Err(TraitKitError::BuildFailed { .. }) => {}
            Err(TraitKitError::MissingConfig { .. }) => {
                panic!("key validation must run before the MissingConfig lookup")
            }
            Ok(v) => panic!("must never return plaintext, got: {v:?}"),
            other => panic!("expected BuildFailed, got: {other:?}"),
        }

        let kit = Kit::new();
        kit.set_encrypted(&EmptyKeyCfg { v: "secret".into() }, &KEY32)
            .unwrap();
        let built = kit.build().unwrap();
        let err = built
            .get_encrypted::<EmptyKeyCfg>(&[])
            .expect_err("0-byte key must be rejected on get too");
        assert!(
            matches!(err, TraitKitError::BuildFailed { .. }),
            "get-side empty key should be BuildFailed, got: {err:?}"
        );
    }
}

// ─── 场景 33 · ITF-05 两模块注册同一接口 → 第二次 AlreadyRegistered ───

#[cfg(feature = "di")]
mod itf05_two_modules_same_interface {
    use super::*;
    use trait_kit::core::{InterfaceBuilder, ModuleMeta};

    trait Codec: Send + Sync + 'static {
        fn encode(&self) -> &'static str;
    }

    struct CodecCapA;
    impl Codec for CodecCapA {
        fn encode(&self) -> &'static str {
            "a"
        }
    }
    struct CodecCapB;
    impl Codec for CodecCapB {
        fn encode(&self) -> &'static str {
            "b"
        }
    }

    struct CodecModuleA;
    impl ModuleMeta for CodecModuleA {
        const NAME: &'static str = "codec-a";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl InterfaceBuilder for CodecModuleA {
        type Interface = dyn Codec;
        type Capability = Arc<CodecCapA>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<CodecCapA>, TraitKitError> {
            Ok(Arc::new(CodecCapA))
        }
        fn into_interface(cap: Arc<CodecCapA>) -> Arc<dyn Codec> {
            cap
        }
    }

    struct CodecModuleB;
    impl ModuleMeta for CodecModuleB {
        const NAME: &'static str = "codec-b";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl InterfaceBuilder for CodecModuleB {
        type Interface = dyn Codec;
        type Capability = Arc<CodecCapB>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<CodecCapB>, TraitKitError> {
            Ok(Arc::new(CodecCapB))
        }
        fn into_interface(cap: Arc<CodecCapB>) -> Arc<dyn Codec> {
            cap
        }
    }

    /// 两个不同模块声明同一 `dyn Codec` 接口：第一次注册成功，
    /// 第二次被接口占用检查拒绝（src/kit/kit.rs:680-687），AlreadyRegistered
    /// 报告占用者（codec-a，先注册者），而不是被拒的新来者（codec-b）。
    #[test]
    fn itf05_second_module_for_same_interface_returns_already_registered() {
        let mut kit = Kit::new();
        kit.register_as::<CodecModuleA>()
            .expect("first register_as wins the interface");
        match kit.register_as::<CodecModuleB>() {
            Err(TraitKitError::AlreadyRegistered { module }) => {
                assert_eq!(
                    module, "codec-a",
                    "the error must name the occupying module, not the newcomer"
                );
            }
            Ok(()) => panic!("second register_as for the same interface must be rejected"),
            other => panic!("expected AlreadyRegistered, got: {other:?}"),
        }

        // 接口归属不变：构建后 resolve 拿到的仍是先注册者的实现。
        let built = kit.build().unwrap();
        assert_eq!(built.resolve::<dyn Codec>().unwrap().encode(), "a");
    }
}

// ─── 场景 38 · LC-03 on_ready Err → LifecycleFailed{context=NAME} 精确断言 ───

#[cfg(feature = "lifecycle")]
mod lc03_on_ready_context {
    use super::*;

    struct ReadyBoomMod;
    impl_module_meta!(ReadyBoomMod, "ready-boom-mod");
    impl AutoBuilder for ReadyBoomMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<u32>, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl trait_kit::core::Lifecycle for ReadyBoomMod {
        fn on_ready(_kit: &trait_kit::kit::Kit<trait_kit::kit::Ready>) -> Result<(), Self::Error> {
            Err(TraitKitError::MissingCapability {
                key: "ready-boom".into(),
            })
        }
    }

    /// on_ready 返回 Err → build() 整体失败，LifecycleFailed 的 context
    /// 精确等于失败模块的 NAME（"ready-boom-mod"），原始错误仍可从
    /// source 中恢复（包装点 src/kit/kit.rs:1555-1558）。
    #[test]
    fn lc03_on_ready_error_is_lifecycle_failed_with_module_name_context() {
        let mut kit = Kit::new();
        kit.register::<ReadyBoomMod>().unwrap();
        kit.register_lifecycle::<ReadyBoomMod>();
        match kit.build() {
            Err(TraitKitError::LifecycleFailed { context, source }) => {
                assert_eq!(
                    &*context, "ready-boom-mod",
                    "context must be the failing module's NAME"
                );
                assert!(
                    source.to_string().contains("ready-boom"),
                    "the module's original error must survive as source: {source}"
                );
            }
            Ok(_) => panic!("build must fail when on_ready errors"),
            other => panic!("expected LifecycleFailed, got: {other:?}"),
        }
    }
}

// ─── 场景 39 · LC-05 某模块 on_shutdown panic 不阻断其余模块关闭 ───

#[cfg(feature = "lifecycle")]
mod lc05_shutdown_panic_isolation {
    use super::*;

    struct PanicCap {
        ran: Arc<AtomicBool>,
    }
    struct NormalCapB {
        ran: Arc<AtomicBool>,
    }
    struct NormalCapC {
        ran: Arc<AtomicBool>,
    }

    struct PanicMod;
    impl_module_meta!(PanicMod, "shutdown-panic-mod");
    impl AutoBuilder for PanicMod {
        type Capability = Arc<PanicCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(PanicCap {
                ran: Arc::new(AtomicBool::new(false)),
            }))
        }
    }
    impl trait_kit::core::Lifecycle for PanicMod {
        fn on_shutdown(cap: &Self::Capability) {
            cap.ran.store(true, Ordering::SeqCst);
            panic!("on_shutdown boom");
        }
    }

    struct NormalModB;
    impl_module_meta!(NormalModB, "shutdown-normal-b");
    impl AutoBuilder for NormalModB {
        type Capability = Arc<NormalCapB>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(NormalCapB {
                ran: Arc::new(AtomicBool::new(false)),
            }))
        }
    }
    impl trait_kit::core::Lifecycle for NormalModB {
        fn on_shutdown(cap: &Self::Capability) {
            cap.ran.store(true, Ordering::SeqCst);
        }
    }

    struct NormalModC;
    impl_module_meta!(NormalModC, "shutdown-normal-c");
    impl AutoBuilder for NormalModC {
        type Capability = Arc<NormalCapC>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(NormalCapC {
                ran: Arc::new(AtomicBool::new(false)),
            }))
        }
    }
    impl trait_kit::core::Lifecycle for NormalModC {
        fn on_shutdown(cap: &Self::Capability) {
            cap.ran.store(true, Ordering::SeqCst);
        }
    }

    /// panic 的 on_shutdown 被 `Kit::shutdown` 的 catch_unwind 隔离
    /// （src/kit/kit.rs:2870）：panic 不穿透 shutdown()，注册在 panic
    /// 模块之后的两个正常模块的 on_shutdown 仍全部执行。同步侧
    /// `Lifecycle::on_shutdown` 签名返回 ()（src/core/lifecycle.rs:42），
    /// 无 Err 变体，panic 是唯一失败形态。
    #[test]
    fn lc05_panicking_on_shutdown_does_not_block_other_modules() {
        let mut kit = Kit::new();
        kit.register::<PanicMod>().unwrap();
        kit.register::<NormalModB>().unwrap();
        kit.register::<NormalModC>().unwrap();
        kit.register_lifecycle::<PanicMod>();
        kit.register_lifecycle::<NormalModB>();
        kit.register_lifecycle::<NormalModC>();

        let ready = kit.build().unwrap();
        let panic_ran = Arc::clone(&ready.require::<PanicMod>().unwrap().ran);
        let b_ran = Arc::clone(&ready.require::<NormalModB>().unwrap().ran);
        let c_ran = Arc::clone(&ready.require::<NormalModC>().unwrap().ran);

        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        // 不用 catch_unwind 包裹：shutdown() 自身必须把 panic 隔离在内部，
        // 若穿透则测试线程直接崩。
        ready.shutdown();
        std::panic::set_hook(quiet);

        assert!(
            panic_ran.load(Ordering::SeqCst),
            "the panicking hook itself must have been invoked"
        );
        assert!(
            b_ran.load(Ordering::SeqCst),
            "normal module B must still be shut down after the panicking module"
        );
        assert!(
            c_ran.load(Ordering::SeqCst),
            "normal module C must still be shut down after the panicking module"
        );

        // shutdown 是一次性 drain：二次调用不再重复执行任何回调。
        ready.shutdown();
        assert!(b_ran.load(Ordering::SeqCst) && c_ran.load(Ordering::SeqCst));
    }
}

// ─── 场景 42 · HLT-06 sync 侧 checker 找不到 capability → Unhealthy ───

#[cfg(feature = "health")]
mod hlt06_sync_capability_not_found {
    use super::*;
    use trait_kit::core::health::{HealthCheck, HealthStatus};

    struct OrphanMod;
    impl_module_meta!(OrphanMod, "orphan-mod");
    impl AutoBuilder for OrphanMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl HealthCheck for OrphanMod {
        fn check(_cap: &Arc<u32>) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    /// 注册了 checker 但模块从未 register/build：能力缺失时防御分支
    /// （src/kit/kit.rs:1584）返回 Unhealthy{detail: "capability not
    /// found"}（en-US 默认 locale，locales/en/logs.ftl:23），不 panic、
    /// 不静默跳过、不美化成 Healthy。health_check 与 health_report
    /// 两个入口口径一致。
    #[test]
    fn hlt06_sync_checker_without_capability_is_unhealthy_not_found() {
        let mut kit = Kit::new();
        kit.register_health_check::<OrphanMod>();
        let built = kit.build().expect("build with no modules");

        match built.health_check::<OrphanMod>().unwrap() {
            HealthStatus::Unhealthy { detail } => {
                assert_eq!(
                    detail, "capability not found",
                    "the defensive branch must surface the documented detail"
                );
            }
            other => panic!("expected Unhealthy, got: {other:?}"),
        }

        let report = built.health_report();
        assert_eq!(report.len(), 1, "the checker is still sampled once");
        assert_eq!(report[0].0, "orphan-mod");
        match &report[0].1 {
            HealthStatus::Unhealthy { detail } => assert_eq!(detail, "capability not found"),
            other => panic!("expected Unhealthy in report, got: {other:?}"),
        }
    }
}

// ─── 场景 50 · SHD-04 全局超时中止后续阶段 + timed_out_phases ───

#[cfg(feature = "shutdown")]
mod shd04_global_timeout {
    use super::*;
    use std::time::Duration;
    use trait_kit::kit::shutdown::{ShutdownCoordinator, ShutdownPhase};

    /// 全局预算 10ms，StopRequests 的慢钩子独占 80ms：该钩子本身执行
    /// 完毕后，后续 DrainQueue / CloseConnections 阶段在钩子间粒度被
    /// 全局超时立即中止（钩子不执行、阶段标记 timed_out），
    /// timed_out_phases() 恰好列出两个被中止阶段。
    #[test]
    fn shd04_global_timeout_aborts_later_phases_and_reports_them() {
        let coord = ShutdownCoordinator::new();
        coord.set_global_timeout(Duration::from_millis(10));

        coord.register_hook(ShutdownPhase::StopRequests, || {
            std::thread::sleep(Duration::from_millis(80));
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

        assert!(
            !drained.load(Ordering::SeqCst),
            "DrainQueue hooks must not run after the global budget is exhausted"
        );
        assert!(
            !closed.load(Ordering::SeqCst),
            "CloseConnections hooks must not run after the global budget is exhausted"
        );
        assert!(
            !result.is_ok(),
            "overall result must reflect the aborted phases"
        );
        assert_eq!(
            result.timed_out_phases(),
            vec![ShutdownPhase::DrainQueue, ShutdownPhase::CloseConnections],
            "exactly the two aborted phases should be marked timed out"
        );
        // StopRequests 自身：钩子已启动、阶段预算（默认 30s）未超 → 不算超时。
        let stop = result.phases.first().expect("StopRequests phase result");
        assert!(
            !stop.timed_out,
            "the already-running phase is not itself timed out"
        );
    }
}
