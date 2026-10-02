// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//
// 并发与竞态 E2E 缺口固化测试。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.21）：
// - CCY-06 require_ref 借用存活期间连续 reload/set_config 交错压力档
//   （真实行为修正版，见下方说明；单次语义版见 tests/e2e_config.rs
//   RLD-07 测试）
//
// 其余 CCY 域场景既有覆盖充分，此处仅声明引用、不重复固化：
// - 02 → src/kit/async_typemap.rs（cross_thread_access_does_not_panic、
//   arc_clone_shares_state）、src/kit/async_kit.rs（async_kit_concurrent_registration）
// - CCY-03（sync Kit !Sync 设计边界）→ tests/basic.rs 顶部的
//   `assert_not_impl_any!(Kit<Unbuilt>: Sync)` / `Kit<Ready>: Sync`
//   编译期静态断言固化（等效落点，比 trybuild 更快且稳定）
// - 05/07 → tests/e2e_advanced.rs（c06 100 模块 / c07 20 配置类型 /
//   c15 1MB 加密大值）
#![cfg(feature = "reload")]

use std::error::Error;
use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

struct StormMod;
impl_module_meta!(StormMod, "storm-mod");
impl AutoBuilder for StormMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(7))
    }
}

#[derive(Clone, Debug, PartialEq)]
struct StormCfg {
    v: u32,
}
impl Configurable for StormCfg {
    fn load() -> Result<Self, Box<dyn Error + Send>> {
        Ok(Self { v: 1 })
    }
}

/// CCY-06 真实行为固化（场景描述修正）：require_ref 借用的是
/// capabilities TypeMap，reload_config/set_config 写的是 configs
/// TypeMap（两个独立 RefCell）。借用存活期间连续交错重载/写配置
/// 压力档：无 panic、能力值稳定、配置终值一致。
///
/// 修正依据：TEST_SCENARIOS 编写时推测存在 borrow 冲突 panic；实现
/// 核实（kit.rs:108-109 字段布局）为两个独立 TypeMap。同 TypeMap 的
/// 冲突语义由 src/kit/typemap.rs::inner_ref_panics_if_mutably_borrowed
/// 在层内固化。本测试防止未来合并两个 TypeMap 时引入隐蔽 panic。
#[test]
fn e2e_ref_borrow_storm_interleaved_reload_set() {
    let mut kit = Kit::new();
    kit.register::<StormMod>().unwrap();
    kit.set_config(StormCfg { v: 0 });
    let ready = kit.build().unwrap();

    let guard = ready.require_ref::<StormMod>().unwrap();
    for i in 0..50u32 {
        // Ready 态写配置的唯一路径是 reload_config（set_config 为
        // Unbuilt 态方法）；借用存活期间连续重载。
        ready.reload_config::<StormCfg>().unwrap();
        // 能力值在整个风暴期间保持稳定（capabilities 侧不受写影响）。
        assert_eq!(**guard, 7, "第 {i} 轮：能力值应保持稳定");
        let cfg = ready.config::<StormCfg>().unwrap();
        assert_eq!(cfg.v, 1, "reload 后配置应为 C::load() 产出值");
    }
    drop(guard);
    ready.reload_config::<StormCfg>().unwrap();
    assert!(ready.contains::<StormMod>());
}

// ─── AsyncKit 并发 merge_config：记录序为锁到达序、覆盖可能互相丢失 ────

/// 文档化语义固化（src/kit/report.rs ConfigOverrideRecord、
/// API_REFERENCE.md 并发节）：读-改-写非原子——两线程并发对同一
/// config 类型 merge 不同字段时，config_overrides 每次调用各记一条
/// （记录序==加锁到达序，不承诺与调用发起序一致），最终值可能只含
/// 一方覆盖。需要独占写语义的调用方必须自行串行化。
#[cfg(all(feature = "async", feature = "confers", feature = "report"))]
mod async_merge_config_race_e2e {
    use std::sync::Arc;
    use trait_kit::prelude::*;

    #[derive(Clone, Debug, PartialEq)]
    struct RaceCfg {
        alpha: String,
        beta: String,
    }

    #[derive(Clone, Default)]
    struct RaceCfgOverride {
        alpha: Option<String>,
        beta: Option<String>,
    }

    impl ConfigInherit for RaceCfg {
        type Override = RaceCfgOverride;
        fn apply_override(&mut self, ovr: &Self::Override) {
            if let Some(v) = &ovr.alpha {
                self.alpha = v.clone();
            }
            if let Some(v) = &ovr.beta {
                self.beta = v.clone();
            }
        }
    }

    #[test]
    fn e2e_async_concurrent_merge_config_records_arrive_and_may_lose_writes() {
        let kit = Arc::new(AsyncKit::new());
        kit.set_config(RaceCfg {
            alpha: "base-alpha".into(),
            beta: "base-beta".into(),
        });

        let k1 = Arc::clone(&kit);
        let k2 = Arc::clone(&kit);
        let t1 = std::thread::spawn(move || {
            for i in 0..25 {
                k1.merge_config::<RaceCfg>(RaceCfgOverride {
                    alpha: Some(format!("a-{i}")),
                    beta: None,
                });
            }
        });
        let t2 = std::thread::spawn(move || {
            for i in 0..25 {
                k2.merge_config::<RaceCfg>(RaceCfgOverride {
                    alpha: None,
                    beta: Some(format!("b-{i}")),
                });
            }
        });
        t1.join().expect("线程一不应 panic");
        t2.join().expect("线程二不应 panic");

        // 文档化语义一：每次 merge_config 各记一条记录（共 50 条），
        // 记录序为锁到达序（此处不断言具体顺序，只断言两路各 25 条）。
        // take_config_overrides 为任意状态可调的排空 API。
        let drained_now = kit.take_config_overrides();
        assert_eq!(
            drained_now.len(),
            50,
            "每次 merge_config 调用应各记一条 override 记录"
        );
        assert!(
            drained_now.iter().all(|r| r.config.ends_with("RaceCfg")),
            "记录应指向被 merge 的配置类型"
        );

        // 文档化语义二：最终值是读-改-写竞争后的合法收敛——alpha 侧
        // 与 beta 侧各自最后落地的覆盖（不承诺双方都赢，但每字段必为
        // 某一方写入的值，绝不出现撕裂/中间态）。
        let cfg: RaceCfg = kit.config().expect("config 应可读");
        assert!(
            cfg.alpha == "base-alpha" || cfg.alpha.starts_with("a-"),
            "alpha 终值应为基值或某次 a-* 覆盖：got {:?}",
            cfg.alpha
        );
        assert!(
            cfg.beta == "base-beta" || cfg.beta.starts_with("b-"),
            "beta 终值应为基值或某次 b-* 覆盖：got {:?}",
            cfg.beta
        );

        // 记录不无限累积的使用契约：排空后再次排空为空。
        let drained_again = kit.take_config_overrides();
        assert!(drained_again.is_empty(), "排空后历史应归零");
    }
}

// ─── 事件总线并发 publish：回调锁外扇出，重入不死锁 ───────────────────

/// 多线程同时 publish + 订阅者回调内再 subscribe/publish（重入）：
/// 无死锁、无 panic（publish 在锁外扇出的契约，src/kit/events.rs）。
#[test]
fn e2e_event_bus_concurrent_publish_and_reentrant_subscribe() {
    use std::sync::Arc;
    use trait_kit::kit::events::{EventBus, KitEvent, MemoryEventBus};

    let bus = Arc::new(MemoryEventBus::new());
    let received = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // 订阅者：回调内重入（再 subscribe + 再 publish），仅首次触发
    // （reentrancy gate 防事件数发散）。
    let reentry_gate = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let inner_bus = Arc::clone(&bus);
    let inner_received = Arc::clone(&received);
    let gate_for_cb = Arc::clone(&reentry_gate);
    bus.subscribe(move |_event: &KitEvent| {
        inner_received.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if !gate_for_cb.swap(true, std::sync::atomic::Ordering::SeqCst) {
            inner_bus.subscribe(|_e: &KitEvent| {
                // 第二层订阅者：no-op（锁外扇出快照不含它，防发散）。
            });
            inner_bus.publish(KitEvent::ModuleBuilt {
                module: "reentrant",
                elapsed_us: 0,
            });
        }
    });

    // 4 线程 × 100 次并发 publish。
    let mut handles = Vec::new();
    for _ in 0..4 {
        let b = Arc::clone(&bus);
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                b.publish(KitEvent::ModuleBuilt {
                    module: "storm",
                    elapsed_us: 1,
                });
            }
        }));
    }
    for h in handles {
        h.join().expect("并发 publish 线程不应 panic");
    }

    // 主事件（4×100）+ 每个首次重入的 publish（并发下 ≥1，至多重入层）。
    let total = received.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        total >= 400,
        "主 publish 的 400 个事件应全部送达（实际 {total}，重入事件另计）"
    );
}
