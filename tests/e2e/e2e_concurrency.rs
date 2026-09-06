// Copyright (c) 2026 Kirky.X
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
// - CCY-01/02 → src/kit/async_typemap.rs（cross_thread_access_does_not_panic、
//   arc_clone_shares_state）、src/kit/async_kit.rs（async_kit_concurrent_registration）
// - CCY-03（sync Kit !Sync 设计边界）→ tests/basic.rs 顶部的
//   `assert_not_impl_any!(Kit<Unbuilt>: Sync)` / `Kit<Ready>: Sync`
//   编译期静态断言固化（等效落点，比 trybuild 更快且稳定）
// - CCY-04/05/07 → tests/e2e_advanced.rs（c06 100 模块 / c07 20 配置类型 /
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
