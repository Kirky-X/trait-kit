// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//
// 异步 Kit E2E 测试（缺口固化）。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.16 / §2.12）：
// - ASK-13 异步取消：build future 被 drop 后状态不半更新，重试 build 可成功
// - DEC-08 `AsyncKit::decorate`：async 构建路径装饰行为与 sync 一致
// - HLT-09 async 面健康检查报告口径与 sync 同构（见文件尾部）

#![cfg(feature = "async")]

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::task::{Context, Poll, Waker};
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

/// 最小单线程 Future 执行器（镜像 crate 内部 `test_helpers::block_on`，
/// 其为 `pub(crate)`，集成测试不可达）。
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

// ─── 异步构建取消安全 ──────────────────────────────────────────

static ASK13_STARTED: AtomicU32 = AtomicU32::new(0);
static ASK13_COMPLETED: AtomicU32 = AtomicU32::new(0);
/// 仅首次 poll 返回 Pending 的闸门：取消的构建停在挂起点，
/// 重试的构建（同一模块定义）下一轮 poll 即完成。
static ASK13_PEND_GATE: AtomicBool = AtomicBool::new(false);

struct PendOnceFut;
impl Future for PendOnceFut {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        if ASK13_PEND_GATE.swap(true, Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

struct CancelCap {
    v: u32,
}

struct CancelMod;
impl_module_meta!(CancelMod, "cancel-mod");
impl AsyncAutoBuilder for CancelMod {
    type Capability = std::sync::Arc<CancelCap>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        Box::pin(async {
            ASK13_STARTED.fetch_add(1, Ordering::SeqCst);
            PendOnceFut.await;
            ASK13_COMPLETED.fetch_add(1, Ordering::SeqCst);
            Ok(std::sync::Arc::new(CancelCap { v: 1 }))
        })
    }
}

#[test]
fn e2e_async_build_future_drop_is_cancel_safe() {
    // 第一次构建：手动 poll 一次进入模块异步体（STARTED=1，挂起中），
    // 随后 drop build future（连带消费掉的 AsyncKit 一起释放）。
    ASK13_STARTED.store(0, Ordering::SeqCst);
    ASK13_COMPLETED.store(0, Ordering::SeqCst);
    ASK13_PEND_GATE.store(false, Ordering::SeqCst);

    let mut kit = AsyncKit::new();
    kit.register::<CancelMod>().unwrap();
    {
        let fut = kit.build();
        let mut pfut = std::pin::pin!(fut);
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let polled = pfut.as_mut().poll(&mut cx);
        assert!(
            matches!(polled, Poll::Pending),
            "挂起模块应使 build future 返回 Pending"
        );
        assert_eq!(
            ASK13_STARTED.load(Ordering::SeqCst),
            1,
            "被取消前模块异步体已开始执行一次"
        );
    } // ← 此处 drop 构建 future（连带消费掉的 AsyncKit 一起释放）

    assert_eq!(
        ASK13_COMPLETED.load(Ordering::SeqCst),
        0,
        "future 被 drop 后异步体不得完成（无半更新状态外泄）"
    );

    // 重试构建：同一模块定义在全新 AsyncKit 上重试可成功（cancel-safe）。
    let mut retry = AsyncKit::new();
    retry.register::<CancelMod>().unwrap();
    let ready = block_on(retry.build()).expect("重试 build 应成功");
    let cap = ready.require::<CancelMod>().unwrap();
    assert_eq!(cap.v, 1);
    assert_eq!(ASK13_STARTED.load(Ordering::SeqCst), 2);
    assert_eq!(ASK13_COMPLETED.load(Ordering::SeqCst), 1);
}

// ─── AsyncKit::decorate 与 sync 行为一致 ───────────────────────

// 仅 decorator 特性下的用例消费该模块，定义随用例同门控避免 dead_code 告警
#[cfg(feature = "decorator")]
struct AsyncDecoMod;
#[cfg(feature = "decorator")]
impl_module_meta!(AsyncDecoMod, "async-deco-mod");
#[cfg(feature = "decorator")]
impl AsyncAutoBuilder for AsyncDecoMod {
    type Capability = std::sync::Arc<u32>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        Box::pin(async { Ok(std::sync::Arc::new(7u32)) })
    }
}

#[cfg(feature = "decorator")]
#[test]
fn e2e_async_kit_decorate_matches_sync_semantics() {
    let mut kit = AsyncKit::new();
    kit.register::<AsyncDecoMod>().unwrap();
    // 多装饰器按注册顺序洋葱式叠加（与 sync DEC-03 同口径）。
    kit.decorate::<AsyncDecoMod>(|cap: std::sync::Arc<u32>| std::sync::Arc::new(*cap + 1));
    kit.decorate::<AsyncDecoMod>(|cap: std::sync::Arc<u32>| std::sync::Arc::new(*cap * 100));
    let ready = block_on(kit.build()).expect("build 应成功");
    assert_eq!(
        *ready.require::<AsyncDecoMod>().unwrap(),
        800,
        "async 构建路径应按注册顺序装饰（7+1)*100"
    );
}

// ─── async 面健康检查与 sync 同构 ──────────────────────

#[cfg(feature = "health")]
mod async_health_isomorphism_e2e {
    use super::block_on;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use trait_kit::core::health::{AsyncHealthCheck, HealthStatus};
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    struct IsoAsyncMod;
    impl_module_meta!(IsoAsyncMod, "iso-async-mod");
    impl AsyncAutoBuilder for IsoAsyncMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
        {
            Box::pin(async { Ok(Arc::new(9u32)) })
        }
    }
    impl AsyncHealthCheck for IsoAsyncMod {
        fn check(cap: &Arc<u32>) -> HealthStatus {
            if **cap > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "zero value".into(),
                }
            }
        }
    }

    struct IsoSyncMod;
    impl_module_meta!(IsoSyncMod, "iso-sync-mod");
    impl AutoBuilder for IsoSyncMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(9u32))
        }
    }
    impl trait_kit::core::HealthCheck for IsoSyncMod {
        fn check(cap: &Arc<u32>) -> HealthStatus {
            if **cap > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "zero value".into(),
                }
            }
        }
    }

    struct GhostAsyncMod;
    impl_module_meta!(GhostAsyncMod, "ghost-async-mod");
    impl AsyncAutoBuilder for GhostAsyncMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
        {
            Box::pin(async { Ok(Arc::new(1u32)) })
        }
    }
    impl AsyncHealthCheck for GhostAsyncMod {
        fn check(_cap: &Arc<u32>) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    /// 同逻辑 checker 在 sync/async 两条产品线上产出同型报告；
    /// 未注册 checker 的错误口径同型（MissingConfig{key=NAME}）。
    #[test]
    fn e2e_async_health_report_isomorphic_to_sync() {
        // sync 侧（同逻辑 checker）。
        let mut sk = Kit::new();
        sk.register::<IsoSyncMod>().unwrap();
        sk.register_health_check::<IsoSyncMod>();
        let skr = sk.build().unwrap();
        let sync_report = skr.health_report();

        // async 侧。
        let mut ak = AsyncKit::new();
        ak.register::<IsoAsyncMod>().unwrap();
        ak.register_health_check::<IsoAsyncMod>();
        let akr = block_on(ak.build()).expect("async build 应成功");
        let async_report = akr.health_report();

        // 同构断言：报告形状与状态判定一致（模块名不同）。
        assert_eq!(sync_report.len(), 1);
        assert_eq!(async_report.len(), 1);
        assert_eq!(
            sync_report[0].1.is_healthy(),
            async_report[0].1.is_healthy(),
            "同逻辑 checker 在两条产品线上应产出同型判定"
        );
        assert!(async_report[0].1.is_healthy());

        // 未注册 checker 错误口径：与 sync 侧同型（MissingConfig）。
        let err = akr
            .health_check::<GhostAsyncMod>()
            .expect_err("未注册 checker 应返回错误");
        match err {
            TraitKitError::MissingConfig { key } => assert_eq!(key, "ghost-async-mod"),
            other => panic!("expected MissingConfig, got: {other}"),
        }
    }
}

// ─── LCY-ASYNC：shutdown_async 真正执行 async on_shutdown ──────────────

#[cfg(feature = "lifecycle")]
mod async_shutdown_e2e {
    use super::block_on;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use trait_kit::core::lifecycle::AsyncLifecycle;
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    static E2E_ASYNC_SHUTDOWN_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct ShutdownE2eMod;
    impl_module_meta!(ShutdownE2eMod, "shutdown-e2e-mod");
    impl AsyncAutoBuilder for ShutdownE2eMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
        {
            Box::pin(async { Ok(Arc::new(1u32)) })
        }
    }
    impl AsyncLifecycle for ShutdownE2eMod {
        fn on_shutdown<'a>(_cap: &'a Arc<u32>) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {
                E2E_ASYNC_SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);
            })
        }
    }

    /// 端到端契约：`shutdown_async()` 恰好执行一次 async `on_shutdown`
    /// （计数 +1），且为 one-shot——二次调用 no-op。`AsyncKit` 无 sync
    /// `shutdown()`，async 清理必须 await `shutdown_async()`。
    #[test]
    fn e2e_async_shutdown_async_runs_on_shutdown_hook() {
        let before = E2E_ASYNC_SHUTDOWN_COUNT.load(Ordering::SeqCst);
        let mut kit = AsyncKit::new();
        kit.register::<ShutdownE2eMod>().unwrap();
        kit.register_lifecycle::<ShutdownE2eMod>();
        let ready = block_on(kit.build()).expect("build 应成功");

        block_on(ready.shutdown_async());
        assert_eq!(
            E2E_ASYNC_SHUTDOWN_COUNT.load(Ordering::SeqCst),
            before + 1,
            "shutdown_async() 应恰好执行一次 async on_shutdown"
        );

        block_on(ready.shutdown_async());
        assert_eq!(
            E2E_ASYNC_SHUTDOWN_COUNT.load(Ordering::SeqCst),
            before + 1,
            "shutdown_async() 应为 one-shot（二次调用 no-op）"
        );
    }
}

// ─── SHUTDOWN-BRIDGE：Kit 组件清理桥接进协调器（停机顺序锁定） ──────────

#[cfg(all(feature = "lifecycle", feature = "shutdown"))]
mod shutdown_bridge_e2e {
    use super::block_on;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use trait_kit::core::lifecycle::AsyncLifecycle;
    use trait_kit::impl_module_meta;
    use trait_kit::kit::shutdown::{AsyncShutdownCoordinator, ShutdownPhase};
    use trait_kit::prelude::*;

    /// 停机事件流水：记录各停机机制（协调器 Send 钩子 / 桥接 local 钩子 /
    /// drop 兜底）的实际触发顺序。
    static EVENTS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    fn record(event: &'static str) {
        EVENTS.lock().unwrap().push(event);
    }

    /// 能力释放哨兵：drop 时记录事件，模拟"协调器清理之外的 drop 兜底"
    /// （daemon 场景中 CloseConnections 留空靠 drop 的既有双机制之一）。
    struct DropSentinel;
    impl Drop for DropSentinel {
        fn drop(&mut self) {
            record("capability-dropped");
        }
    }

    // 组件拓扑：BridgeUp 依赖 BridgeDown。桥接后预期 Up 先于 Down 关闭
    // （依赖者先于被依赖者，与 shutdown_async 的逆拓扑语义一致）。
    struct BridgeDown;
    impl_module_meta!(BridgeDown, "bridge-down");
    impl AsyncAutoBuilder for BridgeDown {
        type Capability = Arc<DropSentinel>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
        {
            Box::pin(async { Ok(Arc::new(DropSentinel)) })
        }
    }
    impl AsyncLifecycle for BridgeDown {
        fn on_shutdown<'a>(
            _cap: &'a Arc<DropSentinel>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {
                record("bridge-down");
            })
        }
    }

    struct BridgeUp;
    impl_module_meta!(BridgeUp, "bridge-up", deps = [BridgeDown]);
    impl AsyncAutoBuilder for BridgeUp {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
        {
            Box::pin(async { Ok(Arc::new(1u32)) })
        }
    }
    impl AsyncLifecycle for BridgeUp {
        fn on_shutdown<'a>(_cap: &'a Arc<u32>) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {
                record("bridge-up");
            })
        }
    }

    /// 停机顺序契约锁定：宿主协调器骨架（模拟 daemon 的
    /// AsyncShutdownCoordinator 三阶段 + CancellationToken/drop 双机制）
    /// 与桥接进来的 Kit 组件清理共存时——
    /// 1. 阶段序：StopRequests（令牌）→ CloseConnections（关闭资源）；
    /// 2. 同阶段内：宿主 Send 钩子 → 桥接 local 钩子；
    /// 3. 桥接钩子保持逆拓扑：依赖者（Up）先于被依赖者（Down）；
    /// 4. 桥接即所有权转移：shutdown_async 变 no-op，组件清理 exactly-once；
    /// 5. drop 兜底最后发生：桥接清理先于能力 drop。
    #[test]
    fn e2e_shutdown_bridge_order_transfer_and_one_shot() {
        EVENTS.lock().unwrap().clear();

        let mut kit = AsyncKit::new();
        kit.register::<BridgeDown>().unwrap();
        kit.register::<BridgeUp>().unwrap();
        kit.register_lifecycle::<BridgeDown>();
        kit.register_lifecycle::<BridgeUp>();
        let ready = block_on(kit.build()).expect("build 应成功");

        let coord = AsyncShutdownCoordinator::new();
        // 模拟宿主 CancellationToken：StopRequests 阶段翻转令牌。
        coord
            .register_hook(ShutdownPhase::StopRequests, || {
                Box::pin(async {
                    record("token-cancelled");
                })
            })
            .unwrap();
        // 宿主自有 CloseConnections Send 钩子。
        coord
            .register_hook(ShutdownPhase::CloseConnections, || {
                Box::pin(async {
                    record("coord-close");
                })
            })
            .unwrap();
        // 桥接：Kit 组件清理转移进 CloseConnections 阶段的 local 槽位。
        let bridged = ready
            .register_shutdown_into(&coord, ShutdownPhase::CloseConnections)
            .expect("桥接应成功");
        assert_eq!(bridged, 2, "两个生命周期模块的清理钩子被转移");

        let result = block_on(coord.shutdown_local()).expect("shutdown_local 应成功");
        assert!(result.is_ok());
        assert_eq!(
            result.phases.len(),
            3,
            "每阶段一条结果,与 shutdown() 形状一致"
        );
        assert_eq!(
            *EVENTS.lock().unwrap(),
            vec!["token-cancelled", "coord-close", "bridge-up", "bridge-down"],
            "停机顺序:令牌 → 宿主钩子 → 桥接组件清理(依赖者先)"
        );

        // 桥接即所有权转移:两条既有/新增路径二次调用均为 no-op,
        // 组件清理 exactly-once。
        block_on(ready.shutdown_async());
        let second = block_on(coord.shutdown_local()).expect("二次 shutdown_local 应成功");
        assert!(second.is_ok());
        assert_eq!(
            EVENTS.lock().unwrap().len(),
            4,
            "桥接后 shutdown_async() 与二次 shutdown_local() 均为 no-op"
        );

        // drop 兜底最后发生:桥接清理先于能力 drop(双机制顺序)。
        drop(ready);
        let events = EVENTS.lock().unwrap();
        assert_eq!(
            events.last(),
            Some(&"capability-dropped"),
            "drop 兜底在桥接清理之后"
        );
    }
}

// ─── 异步构建回调内 require 依赖 + 读 config（DI 注入路径 async 面） ────

/// 10 层异步依赖链：每层 build 内 require 前一层（被依赖者先构建）。
macro_rules! async_chain_link {
    ($ty:ident, $name:literal) => {
        struct $ty;
        impl trait_kit::core::ModuleMeta for $ty {
            const NAME: &'static str = $name;
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AsyncAutoBuilder for $ty {
            type Capability = std::sync::Arc<u32>;
            type Error = TraitKitError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
            {
                Box::pin(async { Ok(std::sync::Arc::new(1u32)) })
            }
        }
    };
    ($ty:ident, $name:literal, $dep:ty) => {
        struct $ty;
        impl trait_kit::core::ModuleMeta for $ty {
            const NAME: &'static str = $name;
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                static DEPS: &[(&str, std::any::TypeId)] = &[(
                    <$dep as trait_kit::core::ModuleMeta>::NAME,
                    std::any::TypeId::of::<$dep>(),
                )];
                DEPS
            }
        }
        impl AsyncAutoBuilder for $ty {
            type Capability = std::sync::Arc<u32>;
            type Error = TraitKitError;
            fn build<'a>(
                kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
            {
                Box::pin(async move {
                    // DI 注入路径：异步构建体内 require 依赖（拓扑序保证可取）。
                    let dep = kit.require::<$dep>()?;
                    Ok(std::sync::Arc::new(*dep + 1))
                })
            }
        }
    };
}

async_chain_link!(DiChain0, "di-chain-0");
async_chain_link!(DiChain1, "di-chain-1", DiChain0);
async_chain_link!(DiChain2, "di-chain-2", DiChain1);
async_chain_link!(DiChain3, "di-chain-3", DiChain2);
async_chain_link!(DiChain4, "di-chain-4", DiChain3);
async_chain_link!(DiChain5, "di-chain-5", DiChain4);
async_chain_link!(DiChain6, "di-chain-6", DiChain5);
async_chain_link!(DiChain7, "di-chain-7", DiChain6);
async_chain_link!(DiChain8, "di-chain-8", DiChain7);
async_chain_link!(DiChain9, "di-chain-9", DiChain8);

/// 依赖 + 配置同用的消费模块（异步面 DI 注入路径全量形态）。
struct AsyncDiCfgConsumer;
impl trait_kit::core::ModuleMeta for AsyncDiCfgConsumer {
    const NAME: &'static str = "async-di-cfg-consumer";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] =
            &[(DiChain2::NAME, std::any::TypeId::of::<DiChain2>())];
        DEPS
    }
}
impl AsyncAutoBuilder for AsyncDiCfgConsumer {
    type Capability = std::sync::Arc<(u32, u32)>;
    type Error = TraitKitError;
    fn build<'a>(
        kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        Box::pin(async move {
            let dep = kit.require::<DiChain2>()?;
            let cfg = kit.config::<u32>()?;
            Ok(std::sync::Arc::new((*dep, cfg)))
        })
    }
}

/// 异步构建体内 require 依赖 + 读 config：被依赖者先建、配置可读、
/// 10 层深链逐层 DI 成功（能力值沿链 +1）。
#[test]
fn e2e_async_build_requires_dependency_and_reads_config() {
    let mut kit = AsyncKit::new();
    kit.set_config(777u32);
    kit.register::<DiChain0>().expect("register");
    kit.register::<DiChain1>().expect("register");
    kit.register::<DiChain2>().expect("register");
    kit.register::<DiChain3>().expect("register");
    kit.register::<DiChain4>().expect("register");
    kit.register::<DiChain5>().expect("register");
    kit.register::<DiChain6>().expect("register");
    kit.register::<DiChain7>().expect("register");
    kit.register::<DiChain8>().expect("register");
    kit.register::<DiChain9>().expect("register");
    kit.register::<AsyncDiCfgConsumer>().expect("register");
    let ready = block_on(kit.build()).expect("async 依赖链 build 应成功");

    // 10 层链逐层 require：链尾能力 = 1 + 9 次自增。
    assert_eq!(
        *ready.require::<DiChain9>().unwrap(),
        10,
        "10 层异步链应按拓扑序逐层注入构建"
    );
    // 依赖 + config 同用：能力含两者数据。
    let (dep_val, cfg_val) = *ready.require::<AsyncDiCfgConsumer>().unwrap();
    assert_eq!(dep_val, 3, "DiChain2 能力 = 1 + 2 次自增");
    assert_eq!(cfg_val, 777, "异步构建体内应读到 Unbuilt 态 set_config 值");
}

// ─── AsyncKit::register_if 真/假双分支 + 已注册冲突 ─────────────────

struct RegisterIfOn;
impl_module_meta!(RegisterIfOn, "register-if-on");
impl AsyncAutoBuilder for RegisterIfOn {
    type Capability = std::sync::Arc<u32>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        Box::pin(async { Ok(std::sync::Arc::new(1u32)) })
    }
}

struct RegisterIfOff;
impl_module_meta!(RegisterIfOff, "register-if-off");
impl AsyncAutoBuilder for RegisterIfOff {
    type Capability = std::sync::Arc<u32>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        Box::pin(async { Ok(std::sync::Arc::new(2u32)) })
    }
}

/// register_if 谓词真 → 注册成功可取；假 → 跳过（require 报缺失）；
/// 已注册模块再注册 → AlreadyRegistered。
#[test]
fn e2e_async_register_if_true_false_and_conflict() {
    // 谓词真：注册成功。
    let mut kit = AsyncKit::new();
    let registered = kit
        .register_if::<RegisterIfOn>(|_k| true)
        .expect("register_if 不应返回错误");
    assert!(registered, "谓词真应返回 true（已注册）");
    let ready = block_on(kit.build()).expect("build 应成功");
    assert_eq!(*ready.require::<RegisterIfOn>().unwrap(), 1);

    // 谓词假：跳过注册，build 后 require 报缺失。
    let mut skipped = AsyncKit::new();
    let registered = skipped
        .register_if::<RegisterIfOff>(|_k| false)
        .expect("register_if 不应返回错误");
    assert!(!registered, "谓词假应返回 false（跳过）");
    let ready = block_on(skipped.build()).expect("build 应成功");
    let err = ready
        .require::<RegisterIfOff>()
        .expect_err("跳过注册的模块 require 应报缺失");
    assert!(matches!(err, TraitKitError::MissingCapability { .. }));

    // 已注册模块再次 register_if（谓词真）→ AlreadyRegistered。
    let mut dup = AsyncKit::new();
    dup.register::<RegisterIfOn>().unwrap();
    let err = dup
        .register_if::<RegisterIfOn>(|_k| true)
        .expect_err("重复注册应返回错误");
    assert!(matches!(err, TraitKitError::AlreadyRegistered { .. }));
}

// ─── AsyncKit::factory：非单例 + 失败传播 ──────────────────────────

struct FactoryFreshMod;
impl_module_meta!(FactoryFreshMod, "factory-fresh-mod");
impl AsyncAutoBuilder for FactoryFreshMod {
    type Capability = std::sync::Arc<u32>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        static CALLS: AtomicU32 = AtomicU32::new(0);
        Box::pin(async move { Ok(std::sync::Arc::new(CALLS.fetch_add(1, Ordering::SeqCst))) })
    }
}

struct FactoryFailingMod;
impl_module_meta!(FactoryFailingMod, "factory-failing-mod");
impl AsyncAutoBuilder for FactoryFailingMod {
    type Capability = std::sync::Arc<u32>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        Box::pin(async {
            Err(TraitKitError::MissingCapability {
                key: "factory boom".into(),
            })
        })
    }
}

/// factory 闭包每次调用产出全新实例（非单例），与 require 单例对比；
/// 闭包返回 Err → BuildFailed{context=NAME}，source 下探原错误。
#[test]
fn e2e_async_factory_fresh_instances_and_error_propagation() {
    // 注意：FactoryFailingMod 不能注册进 kit——模块构建失败即整体失败
    // 是 Kit 契约；factory::<M> 只借 Ready Kit 引用重跑 M::build，故
    // 失败模块仅以类型出现、不入注册表。
    let mut kit = AsyncKit::new();
    kit.register::<FactoryFreshMod>().unwrap();
    let ready = block_on(kit.build()).expect("build 应成功");

    // require 单例语义（对照）。
    let singleton_first = ready.require::<FactoryFreshMod>().unwrap();
    let singleton_second = ready.require::<FactoryFreshMod>().unwrap();
    assert!(std::sync::Arc::ptr_eq(&singleton_first, &singleton_second));

    // factory 非单例：两次调用产出不同实例，计数器各自递增。
    let produce = ready.factory::<FactoryFreshMod>();
    let a = block_on(produce()).expect("第一次 factory 调用应成功");
    let b = block_on(produce()).expect("第二次 factory 调用应成功");
    assert_ne!(*a, *b, "factory 每次调用应执行构建体（非缓存单例）");

    // 失败传播：context==NAME、source 下探原始错误文本。
    let failing = ready.factory::<FactoryFailingMod>();
    let err = block_on(failing()).expect_err("factory 构建失败应返回错误");
    match err {
        TraitKitError::BuildFailed { context, source } => {
            assert_eq!(context, "factory-failing-mod");
            assert!(
                source.to_string().contains("factory boom"),
                "source 应下探原错误文本：got '{source}'"
            );
        }
        other => panic!("expected BuildFailed, got: {other}"),
    }
}

// ─── 异步构建体 panic 直接穿透 build().await（无 catch_unwind 隔离） ───

struct AsyncPanicMod;
impl_module_meta!(AsyncPanicMod, "async-panic-mod");
impl AsyncAutoBuilder for AsyncPanicMod {
    type Capability = std::sync::Arc<u32>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        Box::pin(async {
            panic!("async build panic blast");
        })
    }
}

/// 与 sync lazy 面（catch_unwind 隔离为 BuildFailed）显式对照：async
/// 构建体 panic 沿 build().await 穿透到调用方，不转 BuildFailed。
#[test]
#[should_panic(expected = "async build panic blast")]
fn e2e_async_build_body_panic_propagates_to_caller() {
    let mut kit = AsyncKit::new();
    kit.register::<AsyncPanicMod>().unwrap();
    let _ = block_on(kit.build());
}

// ─── with_max_concurrency(0) 钳位为 1：限流下界峰值验证 ─────────────

static E2E_IN_FLIGHT: AtomicU32 = AtomicU32::new(0);
static E2E_PEAK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 每实例一次性挂起点：让并发执行器产生交错，从而暴露真实 in-flight 峰值。
struct E2eYieldOnce {
    yielded: bool,
}
impl Future for E2eYieldOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        let this = &mut *self;
        if this.yielded {
            Poll::Ready(())
        } else {
            this.yielded = true;
            Poll::Pending
        }
    }
}

macro_rules! in_flight_mod {
    ($ty:ident, $name:literal) => {
        struct $ty;
        impl trait_kit::core::ModuleMeta for $ty {
            const NAME: &'static str = $name;
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AsyncAutoBuilder for $ty {
            type Capability = std::sync::Arc<()>;
            type Error = TraitKitError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
            {
                Box::pin(async {
                    let cur = E2E_IN_FLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
                    E2E_PEAK.fetch_max(cur as usize, Ordering::SeqCst);
                    E2eYieldOnce { yielded: false }.await;
                    E2E_IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
                    Ok(std::sync::Arc::new(()))
                })
            }
        }
    };
}

in_flight_mod!(FluxModA, "flux-a");
in_flight_mod!(FluxModB, "flux-b");
in_flight_mod!(FluxModC, "flux-c");
in_flight_mod!(FluxModD, "flux-d");
in_flight_mod!(FluxModE, "flux-e");
in_flight_mod!(FluxModF, "flux-f");
in_flight_mod!(FluxModG, "flux-g");
in_flight_mod!(FluxModH, "flux-h");

fn reset_flux_counters() {
    E2E_IN_FLIGHT.store(0, Ordering::SeqCst);
    E2E_PEAK.store(0, Ordering::SeqCst);
}

/// with_max_concurrency(0) 钳位为 1（limit.max(1)）：8 个独立模块构建
/// 并发峰值 == 1；对照 limit=1 同样峰值 1；默认（无限制）峰值 >= 2。
#[test]
fn e2e_async_max_concurrency_zero_clamps_to_one() {
    // 档位一：limit=0 → 钳位为 1。
    let mut kit = AsyncKit::new();
    register_flux(&mut kit);
    kit.with_max_concurrency(0);
    reset_flux_counters();
    block_on(kit.build()).expect("钳位档 build 应成功");
    assert_eq!(
        E2E_PEAK.load(Ordering::SeqCst),
        1,
        "with_max_concurrency(0) 应钳位为 1（并发峰值==1）"
    );

    // 档位二：limit=1 → 峰值同样为 1。
    let mut kit = AsyncKit::new();
    register_flux(&mut kit);
    kit.with_max_concurrency(1);
    reset_flux_counters();
    block_on(kit.build()).expect("limit=1 档 build 应成功");
    assert_eq!(E2E_PEAK.load(Ordering::SeqCst), 1, "limit=1 时峰值应为 1");

    // 档位三：默认（无限制）→ 峰值超过 1（挂起点交错暴露并发）。
    let mut kit = AsyncKit::new();
    register_flux(&mut kit);
    reset_flux_counters();
    block_on(kit.build()).expect("默认档 build 应成功");
    assert!(
        E2E_PEAK.load(Ordering::SeqCst) > 1,
        "默认无限制时并发峰值应大于 1：got {}",
        E2E_PEAK.load(Ordering::SeqCst)
    );
}

// ─── AsyncKit::graph_dot / graph_mermaid 导出（async 面图导出） ──────

struct GraphExportDep;
impl_module_meta!(GraphExportDep, "graph-export-dep");
impl AsyncAutoBuilder for GraphExportDep {
    type Capability = std::sync::Arc<u32>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        Box::pin(async { Ok(std::sync::Arc::new(1u32)) })
    }
}

struct GraphExportConsumer;
impl_module_meta!(
    GraphExportConsumer,
    "graph-export-consumer",
    deps = [GraphExportDep]
);
impl AsyncAutoBuilder for GraphExportConsumer {
    type Capability = std::sync::Arc<u32>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>> {
        Box::pin(async { Ok(std::sync::Arc::new(2u32)) })
    }
}

/// 注册带依赖模块 build 后：DOT/Mermaid 含全部节点名与边关系；
/// 空 AsyncKit<Ready> 也能导出合法头。
#[test]
fn e2e_async_graph_export_dot_and_mermaid() {
    let mut kit = AsyncKit::new();
    kit.register::<GraphExportDep>().unwrap();
    kit.register::<GraphExportConsumer>().unwrap();
    let ready = block_on(kit.build()).expect("build 应成功");

    let dot = ready.graph_dot();
    assert!(dot.contains("digraph"), "DOT 应含 digraph 头：{dot}");
    assert!(dot.contains("graph-export-dep") && dot.contains("graph-export-consumer"));
    assert!(
        dot.contains("graph-export-dep") && dot.contains("->"),
        "DOT 应表达边关系：{dot}"
    );

    let mermaid = ready.graph_mermaid();
    assert!(
        mermaid.contains("graph TD"),
        "Mermaid 应含 graph TD 头：{mermaid}"
    );
    assert!(mermaid.contains("graph-export-dep") && mermaid.contains("graph-export-consumer"));
    assert!(mermaid.contains("-->"), "Mermaid 应表达边关系：{mermaid}");

    // 空 Ready Kit 也能导出（仅头，无节点）。
    let empty = block_on(AsyncKit::new().build()).expect("空 kit build 应成功");
    assert!(empty.graph_dot().contains("digraph"));
    assert!(empty.graph_mermaid().contains("graph TD"));
}

// ─── AsyncKit::emit_event 自定义事件直达总线 ────────────────────────

/// with_event_bus 后 emit_event(自定义 KitEvent) 订阅者收到；Unbuilt/Ready
/// 两态 emit 均可用；未注入时 NoOp 不 panic。
#[test]
fn e2e_async_emit_event_reaches_injected_bus_in_both_states() {
    use trait_kit::kit::events::{EventBus, KitEvent, MemoryEventBus};

    let bus = std::sync::Arc::new(MemoryEventBus::new());
    let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&received);
    bus.subscribe(move |event: &KitEvent| {
        sink.lock().unwrap().push(event.clone());
    });

    // Unbuilt 态 emit。
    let mut kit = AsyncKit::new();
    kit.with_event_bus(Some(
        std::sync::Arc::clone(&bus) as std::sync::Arc<dyn EventBus>
    ));
    kit.emit_event(KitEvent::ConfigChanged {
        key: "custom/unbuilt".into(),
        summary: "emitted while unbuilt".into(),
    });

    // Ready 态 emit。
    let ready = block_on(kit.build()).expect("build 应成功");
    ready.emit_event(KitEvent::ConfigChanged {
        key: "custom/ready".into(),
        summary: "emitted while ready".into(),
    });

    let events = received.lock().unwrap();
    assert_eq!(events.len(), 2, "两个事件都应到达订阅者");
    assert_eq!(events[0].kind(), "config_changed");
    assert_eq!(
        events[0],
        KitEvent::ConfigChanged {
            key: "custom/unbuilt".into(),
            summary: "emitted while unbuilt".into(),
        }
    );
    assert_eq!(
        events[1],
        KitEvent::ConfigChanged {
            key: "custom/ready".into(),
            summary: "emitted while ready".into(),
        }
    );

    // 未注入 bus：emit 为 NoOp，不 panic。
    let bare = AsyncKit::new();
    bare.emit_event(KitEvent::ConfigChanged {
        key: "noop".into(),
        summary: "no bus".into(),
    });
}

// ─── on_ready 失败 → 无任何 on_shutdown 执行（async 对位） ───────────

#[cfg(feature = "lifecycle")]
mod async_ready_discard_on_ready_failure_e2e {
    use super::block_on;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use trait_kit::core::lifecycle::AsyncLifecycle;
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    static ASYNC_SHUTDOWN_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct AsyncHealthyShutdownMod;
    impl_module_meta!(AsyncHealthyShutdownMod, "async-healthy-shutdown-mod");
    impl AsyncAutoBuilder for AsyncHealthyShutdownMod {
        type Capability = std::sync::Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
        {
            Box::pin(async { Ok(std::sync::Arc::new(1u32)) })
        }
    }
    impl AsyncLifecycle for AsyncHealthyShutdownMod {
        fn on_shutdown<'a>(
            _cap: &'a std::sync::Arc<u32>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {
                ASYNC_SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);
            })
        }
    }

    struct AsyncFailingReadyMod;
    impl_module_meta!(AsyncFailingReadyMod, "async-failing-ready-mod");
    impl AsyncAutoBuilder for AsyncFailingReadyMod {
        type Capability = std::sync::Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
        {
            Box::pin(async { Ok(std::sync::Arc::new(2u32)) })
        }
    }
    impl AsyncLifecycle for AsyncFailingReadyMod {
        fn on_ready<'a>(
            _kit: &'a AsyncKit<AsyncReady>,
        ) -> Pin<Box<dyn Future<Output = Result<(), TraitKitError>> + Send + 'a>> {
            Box::pin(async {
                Err(TraitKitError::MissingCapability {
                    key: "async on-ready boom".into(),
                })
            })
        }
        fn on_shutdown<'a>(
            _cap: &'a std::sync::Arc<u32>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {
                ASYNC_SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);
            })
        }
    }

    /// async 面同型：on_ready 失败 → build 失败（LifecycleFailed），
    /// 无任何 async on_shutdown 被执行。
    #[test]
    fn e2e_async_on_ready_failure_discards_ready_and_runs_no_shutdown() {
        let before = ASYNC_SHUTDOWN_COUNT.load(Ordering::SeqCst);
        let mut kit = AsyncKit::new();
        kit.register::<AsyncHealthyShutdownMod>().unwrap();
        kit.register::<AsyncFailingReadyMod>().unwrap();
        kit.register_lifecycle::<AsyncHealthyShutdownMod>();
        kit.register_lifecycle::<AsyncFailingReadyMod>();

        let err = block_on(kit.build()).expect_err("async on_ready 失败应使 build 失败");
        assert!(matches!(err, TraitKitError::LifecycleFailed { .. }));
        assert_eq!(
            ASYNC_SHUTDOWN_COUNT.load(Ordering::SeqCst),
            before,
            "async on_ready 失败后任何 on_shutdown 都不得执行"
        );
    }
}

fn register_flux(kit: &mut AsyncKit) {
    kit.register::<FluxModA>().expect("register");
    kit.register::<FluxModB>().expect("register");
    kit.register::<FluxModC>().expect("register");
    kit.register::<FluxModD>().expect("register");
    kit.register::<FluxModE>().expect("register");
    kit.register::<FluxModF>().expect("register");
    kit.register::<FluxModG>().expect("register");
    kit.register::<FluxModH>().expect("register");
}
