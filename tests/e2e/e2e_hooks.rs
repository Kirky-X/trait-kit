// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// 运行时钩子（生命周期 / 健康检查 / 构建观察者）E2E 缺口固化测试。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.9 / §2.10 / §2.11）：
// - LCY-05 某模块 `on_shutdown` 失败（panic）不阻断其余模块关闭
//   （kit.rs 文档契约 "A failed shutdown does not prevent other modules
//   from shutting down"——内部 catch_unwind 隔离，行为级断言）
// - LCY-07 多模块 on_ready 按拓扑序执行（依赖者的 ready 晚于被依赖者，
//   显式顺序断言）
// - HLT-07 checker 闭包找不到 capability 时返回
//   `Unhealthy("capability not found")`（防御分支：register_health_check
//   的类型系统不强制模块已注册）
// - OBS-05 多个 observer 注册时全部收到同一事件（遍历序 == 注册序）
//
// 其余 LCY/HLT/OBS 域场景既有覆盖充分，此处仅声明引用、不重复固化：
// - LCY-01..04 → src/kit/kit.rs、src/core/lifecycle.rs 内联测试、
//   tests/e2e_feature_combinations.rs（lifecycle_e2e 组）
// - LCY-06（AsyncLifecycle）→ src/kit/async_kit.rs 内联测试
// - HLT-01..06 → src/kit/kit.rs、src/core/health.rs、
//   tests/e2e_feature_combinations.rs（health_e2e 组）
// - OBS-01..04 → src/core/observer.rs、src/kit/kit.rs 内联测试、
//   tests/e2e_feature_combinations.rs::e2e_observer_notified_on_build
// - OBS-06（observer+decorator）→ e2e_feature_combinations.rs
// - OBS-07 / DEC-08（async 面）→ tests/e2e_async.rs

#![cfg(any(feature = "lifecycle", feature = "health", feature = "observer"))]

// ─── LCY-05 / LCY-07：生命周期 ─────────────────────────────────────────

#[cfg(feature = "lifecycle")]
mod lifecycle_hooks_e2e {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    /// LCY-07 共享顺序记录器：拓扑序断言用。
    static READY_ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    struct BaseMod;
    impl_module_meta!(BaseMod, "base-mod");
    impl AutoBuilder for BaseMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl trait_kit::core::Lifecycle for BaseMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            READY_ORDER.lock().unwrap().push("base-mod");
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<u32>) {}
    }

    struct MidMod;
    impl_module_meta!(MidMod, "mid-mod", deps = [BaseMod]);
    impl AutoBuilder for MidMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(2))
        }
    }
    impl trait_kit::core::Lifecycle for MidMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            READY_ORDER.lock().unwrap().push("mid-mod");
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<u32>) {}
    }

    /// LCY-07 依赖链末端的模块。
    struct TopMod;
    impl_module_meta!(TopMod, "top-mod", deps = [MidMod]);
    impl AutoBuilder for TopMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(3))
        }
    }
    impl trait_kit::core::Lifecycle for TopMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            READY_ORDER.lock().unwrap().push("top-mod");
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<u32>) {}
    }

    /// LCY-07：on_ready 拓扑序——base → mid → top（被依赖者先 ready）。
    #[test]
    fn e2e_on_ready_runs_in_topological_order() {
        READY_ORDER.lock().unwrap().clear();
        let mut kit = Kit::new();
        kit.register::<BaseMod>().unwrap();
        kit.register::<MidMod>().unwrap();
        kit.register::<TopMod>().unwrap();
        kit.register_lifecycle::<BaseMod>();
        kit.register_lifecycle::<MidMod>();
        kit.register_lifecycle::<TopMod>();
        let _ready = kit.build().unwrap();

        let order = READY_ORDER.lock().unwrap().clone();
        assert_eq!(
            order,
            vec!["base-mod", "mid-mod", "top-mod"],
            "on_ready 应按拓扑序执行：被依赖者先 ready"
        );
    }

    // LCY-05：中间模块 on_shutdown panic，首尾回调仍全部执行。

    static SHUTDOWN_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct FirstMod;
    impl_module_meta!(FirstMod, "first-mod");
    impl AutoBuilder for FirstMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(11))
        }
    }
    impl trait_kit::core::Lifecycle for FirstMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<u32>) {
            SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct PanickyMod;
    impl_module_meta!(PanickyMod, "panicky-mod");
    impl AutoBuilder for PanickyMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(22))
        }
    }
    impl trait_kit::core::Lifecycle for PanickyMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<u32>) {
            panic!("shutdown boom");
        }
    }

    struct LastMod;
    impl_module_meta!(LastMod, "last-mod");
    impl AutoBuilder for LastMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(33))
        }
    }
    impl trait_kit::core::Lifecycle for LastMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<u32>) {
            SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// LCY-05：失败关闭不阻断——panicky-mod 的 panic 被
    /// `Kit::shutdown` 内部 catch_unwind 隔离，其余模块回调全执行。
    #[test]
    fn e2e_failed_on_shutdown_does_not_block_others() {
        SHUTDOWN_COUNT.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<FirstMod>().unwrap();
        kit.register::<PanickyMod>().unwrap();
        kit.register::<LastMod>().unwrap();
        kit.register_lifecycle::<FirstMod>();
        kit.register_lifecycle::<PanickyMod>();
        kit.register_lifecycle::<LastMod>();
        let ready = kit.build().unwrap();

        // 静默 panic hook（本测试有意触发 panic，栈回溯输出无信息量）。
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        ready.shutdown();
        std::panic::set_hook(prev_hook);

        assert_eq!(
            SHUTDOWN_COUNT.load(Ordering::SeqCst),
            2,
            "中间模块 on_shutdown panic 后，其余两个模块回调仍应执行"
        );
    }
}

// ─── HLT-07：checker 找不到 capability 的防御分支 ──────────────────────

#[cfg(feature = "health")]
mod health_defensive_e2e {
    use std::sync::Arc;
    use trait_kit::core::health::HealthStatus;
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    struct GhostCap;

    /// 实现 HealthCheck（HealthCheck: AutoBuilder）但从不
    /// `register::<GhostMod>()` 的幽灵模块。
    struct GhostMod;
    impl_module_meta!(GhostMod, "ghost-mod");
    impl AutoBuilder for GhostMod {
        type Capability = Arc<GhostCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(GhostCap))
        }
    }
    impl trait_kit::core::HealthCheck for GhostMod {
        fn check(_cap: &Arc<GhostCap>) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    struct LiveCap;
    struct LiveMod;
    impl_module_meta!(LiveMod, "live-mod");
    impl AutoBuilder for LiveMod {
        type Capability = Arc<LiveCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(LiveCap))
        }
    }
    impl trait_kit::core::HealthCheck for LiveMod {
        fn check(_cap: &Arc<LiveCap>) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    /// HLT-07：register_health_check 的类型系统不强制模块已注册；
    /// 对未注册模块，checker 在 TypeMap 中找不到 capability →
    /// `Unhealthy { detail: "capability not found" }`（防御分支）。
    #[test]
    fn e2e_checker_missing_capability_reports_unhealthy() {
        let mut kit = Kit::new();
        kit.register::<LiveMod>().unwrap();
        // ghost-mod 从不注册模块，仅注册其 health checker。
        kit.register_health_check::<GhostMod>();
        kit.register_health_check::<LiveMod>();
        let ready = kit.build().unwrap();

        let report = ready.health_report();
        assert_eq!(report.len(), 2, "report 应覆盖全部已注册 checker");

        let ghost = report
            .iter()
            .find(|(name, _)| *name == "ghost-mod")
            .expect("ghost-mod checker 应出现在 report 中");
        match &ghost.1 {
            HealthStatus::Unhealthy { detail } => assert_eq!(
                detail, "capability not found",
                "未注册模块的 checker 应命中防御分支"
            ),
            other => panic!("expected Unhealthy, got: {other:?}"),
        }

        let live = report
            .iter()
            .find(|(name, _)| *name == "live-mod")
            .expect("live-mod checker 应出现在 report 中");
        assert!(live.1.is_healthy(), "已注册模块应如实上报 Healthy");
    }
}

// ─── OBS-05：多 observer 注册序 ────────────────────────────────────────

#[cfg(feature = "observer")]
mod observer_registration_order_e2e {
    use std::sync::Arc;
    use trait_kit::core::observer::BuildObserver;
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    struct ObsCap;
    struct ObservedMod;
    impl_module_meta!(ObservedMod, "observed-mod");
    impl AutoBuilder for ObservedMod {
        type Capability = Arc<ObsCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(ObsCap))
        }
    }

    /// 以 id 标记的记录型 observer。
    struct RecordingObserver {
        id: &'static str,
        log: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }
    impl BuildObserver for RecordingObserver {
        fn on_module_start(&self, _name: &'static str) {}
        fn on_module_built(&self, _name: &'static str, _elapsed: std::time::Duration) {
            self.log.lock().unwrap().push(self.id);
        }
        fn on_build_error(&self, _name: &'static str, _err: &TraitKitError) {}
    }

    /// OBS-05：多个 observer 全部收到同一事件，且遍历序 == 注册序。
    #[test]
    fn e2e_multiple_observers_all_notified_in_registration_order() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut kit = Kit::new();
        // 注册序：first → second → third。
        kit.with_observer(Arc::new(RecordingObserver {
            id: "first",
            log: Arc::clone(&log),
        }));
        kit.with_observer(Arc::new(RecordingObserver {
            id: "second",
            log: Arc::clone(&log),
        }));
        kit.with_observer(Arc::new(RecordingObserver {
            id: "third",
            log: Arc::clone(&log),
        }));
        kit.register::<ObservedMod>().unwrap();
        let _ready = kit.build().unwrap();

        let entries = log.lock().unwrap().clone();
        assert_eq!(
            entries,
            vec!["first", "second", "third"],
            "三个 observer 应按注册序全部收到 on_module_built"
        );
    }
}
