// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//
// 装饰器构建路径 E2E 测试。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.12）：
// - DEC-03 多装饰器按注册顺序洋葱式叠加（f1 先 f2 后 → f2 包 f1）
// - DEC-04 装饰器覆盖全部四条构建路径：eager / lazy / multi / interface
//   （eager 已有 tests/e2e_feature_combinations.rs::e2e_decorator_wraps_capability，
//     此处固化 lazy、multi、interface 三条路径）
// - DEC-07 装饰器闭包 panic / 内部 downcast 失败的文档化 panic 语义
// - decorate 未注册目标：build() 期文档化 panic（decorate 为发射后不管
//   式注册，与 try_decorate 的注册时立即报错互为对照）

#![cfg(feature = "decorator")]

use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

// ─── 多装饰器洋葱式叠加 ────────────────────────────────────────

struct OnionMod;
impl_module_meta!(OnionMod, "onion-mod");
impl AutoBuilder for OnionMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(10))
    }
}

#[test]
fn e2e_decorator_onion_order_registration_sequence() {
    let mut kit = Kit::new();
    kit.register::<OnionMod>().unwrap();
    // f1 先注册（+1），f2 后注册（×100）：应用序 f1 → f2，结果
    // (10 + 1) × 100 = 1100；若逆序则为 10 × 100 + 1 = 1001。
    kit.decorate::<OnionMod>(|cap: Arc<u32>| Arc::new(*cap + 1));
    kit.decorate::<OnionMod>(|cap: Arc<u32>| Arc::new(*cap * 100));
    let ready = kit.build().unwrap();
    assert_eq!(
        *ready.require::<OnionMod>().unwrap(),
        1100,
        "装饰器应按注册顺序洋葱式叠加：f1 先应用、f2 后包裹"
    );
}

// ─── lazy / multi / interface 构建路径装饰 ─────────────────────

struct LazyDeco;
impl_module_meta!(LazyDeco, "lazy-deco");
impl AutoBuilder for LazyDeco {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(5))
    }
}

struct MultiDecoA;
impl_module_meta!(MultiDecoA, "multi-deco-a");
impl AutoBuilder for MultiDecoA {
    type Capability = Arc<u64>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(1))
    }
}

struct MultiDecoB;
impl_module_meta!(MultiDecoB, "multi-deco-b");
impl AutoBuilder for MultiDecoB {
    type Capability = Arc<u64>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(2))
    }
}

// 以下 Gate 系列仅被 interface 路径测试使用，整体随 di feature 门控。
#[cfg(feature = "di")]
trait Gate: Send + Sync + 'static {
    fn open(&self) -> bool;
}

#[cfg(feature = "di")]
struct GateCap {
    open: bool,
}
#[cfg(feature = "di")]
impl Gate for GateCap {
    fn open(&self) -> bool {
        self.open
    }
}

#[cfg(feature = "di")]
struct GateModule;
#[cfg(feature = "di")]
impl_module_meta!(GateModule, "gate-mod");
#[cfg(feature = "di")]
impl AutoBuilder for GateModule {
    type Capability = Arc<GateCap>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(GateCap { open: false }))
    }
}
#[cfg(feature = "di")]
impl trait_kit::core::InterfaceBuilder for GateModule {
    type Interface = dyn Gate;
    type Capability = Arc<GateCap>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(GateCap { open: false }))
    }
    fn into_interface(cap: Self::Capability) -> Arc<Self::Interface> {
        cap as Arc<Self::Interface>
    }
}

/// lazy 路径：首次 require 触发构建时应用装饰器，缓存装饰后值。
#[test]
fn e2e_decorator_applies_on_lazy_first_require() {
    let mut kit = Kit::new();
    kit.register_lazy::<LazyDeco>().unwrap();
    kit.decorate::<LazyDeco>(|cap: Arc<u32>| Arc::new(*cap * 3));
    let ready = kit.build().unwrap();
    // 首次 require：惰性构建 + 装饰（5 × 3）。
    assert_eq!(*ready.require::<LazyDeco>().unwrap(), 15);
    // 二次 require：返回 OnceLock 缓存的装饰后值，不重复装饰。
    assert_eq!(*ready.require::<LazyDeco>().unwrap(), 15);
}

/// multi-binding 路径：装饰器按能力类型作用于每个聚合绑定。
#[test]
fn e2e_decorator_applies_on_multi_binding_path() {
    let mut kit = Kit::new();
    kit.register_multi::<MultiDecoA>().unwrap();
    kit.register_multi::<MultiDecoB>().unwrap();
    // 装饰器按 Arc<u64> 能力类型注册 → 聚合构建的每个绑定均被包装。
    kit.decorate::<MultiDecoA>(|cap: Arc<u64>| Arc::new(*cap + 100));
    let ready = kit.build().unwrap();
    let all = ready.require_all::<MultiDecoA>().unwrap();
    assert_eq!(*all[0], 101, "multi 绑定 A 应携带装饰");
    assert_eq!(*all[1], 102, "multi 绑定 B 应携带装饰");
}

/// interface 路径：register_as 构建产物在 into_interface 前被装饰。
#[cfg(feature = "di")]
#[test]
fn e2e_decorator_applies_on_interface_path() {
    let mut kit = Kit::new();
    kit.register_as::<GateModule>().unwrap();
    kit.decorate::<GateModule>(|mut cap: Arc<GateCap>| {
        Arc::get_mut(&mut cap).expect("刚构建的能力应唯一持有").open = true;
        cap
    });
    let ready = kit.build().unwrap();
    assert!(
        ready.resolve::<dyn Gate>().unwrap().open(),
        "register_as 路径产出的接口对象应携带装饰"
    );
}

// ─── 装饰器 panic 语义固化 ─────────────────────────────────────

/// 装饰器闭包自身 panic：panic 穿透 build() 传播给调用方。
#[test]
fn e2e_decorator_closure_panic_propagates_from_build() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut kit = Kit::new();
        kit.register::<OnionMod>().unwrap();
        kit.decorate::<OnionMod>(|_cap: Arc<u32>| -> Arc<u32> {
            panic!("decorator exploded");
        });
        kit.build().unwrap()
    }));
    let err = result.expect_err("装饰器闭包 panic 应穿透 build()");
    let msg = err
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| err.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        msg.contains("decorator exploded"),
        "panic 载荷应保留装饰器闭包的原始消息：got '{msg}'"
    );
}

/// 内部 downcast 失败：装饰器按能力类型注册，若该 TypeId 与某模块
/// 自身 TypeId 重合（模块类型被另一装饰注册当作能力类型），eager 路径
/// 的未映射回退查找会命中错误装饰 → 文档化 panic
/// `expect("decorator type mismatch")`。
#[derive(Clone)]
struct VictimMod;

impl_module_meta!(VictimMod, "victim-mod");
impl AutoBuilder for VictimMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(1))
    }
}

struct ImpostorMod;
impl_module_meta!(ImpostorMod, "impostor-mod");
impl AutoBuilder for ImpostorMod {
    // 能力类型 = VictimMod 模块类型本身（合法 Clone + 'static 类型），
    // 使装饰器注册键与 VictimMod 的模块 TypeId 重合。
    type Capability = VictimMod;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(VictimMod)
    }
}

#[test]
fn e2e_decorator_downcast_mismatch_panics_with_documented_message() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // 装饰器以 ImpostorCap（= VictimMod TypeId）为键注册。
        kit_with_impostor_decorator().build().unwrap()
    }));
    let err = result.expect_err("downcast 失败应触发文档化 panic");
    let msg = err
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| err.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        msg.contains("decorator type mismatch"),
        "应命中 expect(\"decorator type mismatch\") 文档化 panic：got '{msg}'"
    );
}

fn kit_with_impostor_decorator() -> Kit {
    let mut kit = Kit::new();
    kit.decorate::<ImpostorMod>(|cap: VictimMod| cap);
    // ImpostorMod 自身也须注册（未注册装饰目标在 build 期是另一条文档化
    // panic，见 e2e_decorate_unregistered_target_panics_on_build）；本场景
    // 的要点是 ImpostorMod 的能力类型与 VictimMod 的模块 TypeId 重合，
    // 与 ImpostorMod 是否注册无关。
    kit.register::<ImpostorMod>().unwrap();
    kit.register::<VictimMod>().unwrap();
    // VictimMod 未出现在装饰映射表 → eager 路径回退以模块自身 TypeId
    // 查找装饰器 → 命中 ImpostorMod 以 VictimMod 为键注册的装饰 →
    // Box<Arc<u32>> downcast 到 VictimMod 失败。
    kit
}

// ─── on_ready 失败 → Kit<Ready> 整体丢弃，任何 on_shutdown 都不执行 ────

#[cfg(feature = "lifecycle")]
mod ready_discard_on_ready_failure_e2e {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use trait_kit::core::Lifecycle;
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    static SYNC_SHUTDOWN_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct HealthyShutdownMod;
    impl_module_meta!(HealthyShutdownMod, "healthy-shutdown-mod");
    impl AutoBuilder for HealthyShutdownMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl Lifecycle for HealthyShutdownMod {
        fn on_shutdown(_cap: &Arc<u32>) {
            SYNC_SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct FailingReadyMod;
    impl_module_meta!(FailingReadyMod, "failing-ready-mod");
    impl AutoBuilder for FailingReadyMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(2))
        }
    }
    impl Lifecycle for FailingReadyMod {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
            Err(TraitKitError::MissingCapability {
                key: "on-ready boom".into(),
            })
        }
        fn on_shutdown(_cap: &Arc<u32>) {
            SYNC_SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// on_ready 返回 Err → build() 返回 LifecycleFailed，已成功构建的
    /// 其余模块（含 on_ready 失败模块自身）的 on_shutdown 一律不执行
    /// （Kit<Ready> 未产出，shutdown 回调表随 Kit 一起丢弃）。
    #[test]
    fn e2e_on_ready_failure_discards_ready_and_runs_no_shutdown() {
        let before = SYNC_SHUTDOWN_COUNT.load(Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<HealthyShutdownMod>().unwrap();
        kit.register::<FailingReadyMod>().unwrap();
        kit.register_lifecycle::<HealthyShutdownMod>();
        kit.register_lifecycle::<FailingReadyMod>();

        let err = kit.build().expect_err("on_ready 失败应使 build 整体失败");
        assert!(matches!(err, TraitKitError::LifecycleFailed { .. }));

        assert_eq!(
            SYNC_SHUTDOWN_COUNT.load(Ordering::SeqCst),
            before,
            "on_ready 失败后任何 on_shutdown 都不得执行"
        );
    }
}

// ─── decorate 未注册目标：实测行为固化 ─────────────────────────────

struct NeverRegisteredMod;
impl_module_meta!(NeverRegisteredMod, "never-registered-mod");
impl AutoBuilder for NeverRegisteredMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        unreachable!("该模块从不注册，不应被构建")
    }
}

/// decorate::<M> 于 M 未注册：`decorate()` 是"发射后不管"式注册（目标
/// 允许在装饰之后才注册），不做注册时校验；缺失目标延迟到 build() 期以
/// 文档化 panic 暴露（与 try_decorate 的注册时立即报错契约互为对照，
/// 后者由 kit_tests::try_decorate_rejects_unregistered_target_immediately
/// 固化）。
#[test]
fn e2e_decorate_unregistered_target_panics_on_build() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut kit = Kit::new();
        kit.register::<OnionMod>().unwrap();
        kit.decorate::<NeverRegisteredMod>(|_cap: Arc<u32>| -> Arc<u32> {
            unreachable!("未注册模块的装饰器闭包不应被应用")
        });
        kit.build().unwrap()
    }));
    let err = result.expect_err("未注册装饰目标应在 build() 期 panic");
    let msg = err
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| err.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        msg.contains("decorator target not registered"),
        "应命中文档化的 build 期 panic：got '{msg}'"
    );
    assert!(
        msg.contains("never-registered-mod"),
        "panic 消息应携带未注册目标的模块名：got '{msg}'"
    );
}
