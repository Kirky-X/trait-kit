// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// 热重载 E2E 测试。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.6）：
// - RLD-06 订阅者 panic 语义固化：新值已存储、剩余订阅者被跳过
//   （panic 穿透 reload_config，文档化行为）
// - RLD-07 require_ref 借用存活期间 reload_config/set_config 的真实行为
//   （见文件尾部：场景描述按实现修正后固化）

#![cfg(feature = "reload")]

use std::cell::Cell;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

#[derive(Clone, Debug, PartialEq)]
struct PanicCfg {
    v: u32,
}
impl Configurable for PanicCfg {
    fn load() -> Result<Self, Box<dyn Error + Send>> {
        Ok(Self { v: 2 })
    }
}

#[test]
fn e2e_reload_subscriber_panic_stores_new_value_skips_rest() {
    let kit = Kit::new();
    kit.set_config(PanicCfg { v: 1 });

    let second_notified = Rc::new(Cell::new(false));
    let flag = Rc::clone(&second_notified);
    // 订阅者一（先注册）：重载回调内 panic。
    kit.subscribe::<PanicCfg>(|| panic!("subscriber boom"));
    // 订阅者二（后注册）：若被通知则置位。
    kit.subscribe::<PanicCfg>(move || flag.set(true));

    // panic 穿透 reload_config 传播给调用方。
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        kit.reload_config::<PanicCfg>().unwrap()
    }));
    assert!(result.is_err(), "订阅者 panic 应穿透 reload_config");

    // 文档化语义一：新值已在回调前存储。
    assert_eq!(
        kit.config::<PanicCfg>().unwrap().v,
        2,
        "panic 前 C::load() 的新值应已写入配置表"
    );
    // 文档化语义二：剩余订阅者被跳过。
    assert!(!second_notified.get(), "panic 之后的订阅者不应收到本次通知");

    // panic 后 Kit 状态一致：另一配置类型的订阅与重载正常工作
    // （不重触仍处订阅表中的 panic 回调）。
    let after = Rc::new(Cell::new(false));
    let a = Rc::clone(&after);
    kit.subscribe::<FreshCfg>(move || a.set(true));
    kit.reload_config::<FreshCfg>().unwrap();
    assert!(after.get(), "panic 后其他配置类型重载应正常通知订阅者");
}

#[derive(Clone, Debug, PartialEq)]
struct FreshCfg {
    v: u32,
}
impl Configurable for FreshCfg {
    fn load() -> Result<Self, Box<dyn Error + Send>> {
        Ok(Self { v: 9 })
    }
}

// ─── RLD-07：require_ref 借用与配置写入的真实行为 ───────────────────────

struct RefCapMod;
impl_module_meta!(RefCapMod, "ref-cap-mod");
impl AutoBuilder for RefCapMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(42))
    }
}

/// RLD-07 真实行为固化（场景描述修正）：`require_ref` 借用的是
/// capabilities TypeMap（kit.rs），`reload_config`/`set_config` 写的是
/// configs TypeMap——两者为独立 RefCell，借用存活期间重载/写配置
/// 均正常工作，无 borrow 冲突。
///
/// 修正依据：TEST_SCENARIOS 编写时推测两者共享 inner_ref 会 panic；
/// 实现核实（kit.rs 字段布局）为两个独立 TypeMap。同 TypeMap 的
/// 借用冲突语义由 src/kit/typemap.rs::inner_ref_panics_if_mutably_borrowed
/// 在层内固化；本测试防止未来合并两个 TypeMap 时引入隐蔽 panic。
#[test]
fn e2e_require_ref_borrow_survives_reload_and_set_config() {
    let mut kit = Kit::new();
    kit.register::<RefCapMod>().unwrap();
    kit.set_config(FreshCfg { v: 1 });
    let ready = kit.build().unwrap();

    // 借用存活期间：重载（configs 写 + 订阅通知）正常执行。
    // （set_config 为 Unbuilt 态方法；Ready 态写配置的唯一路径是
    // reload_config——这正是本次固化的真实写路径。）
    let guard = ready.require_ref::<RefCapMod>().unwrap();
    ready.reload_config::<FreshCfg>().unwrap();
    assert_eq!(
        ready.config::<FreshCfg>().unwrap().v,
        9,
        "借用存活期间 reload_config 应正常更新配置"
    );

    // capabilities 侧借用不受 configs 写影响，读到原能力值。
    assert_eq!(**guard, 42, "require_ref 借用应读到完整能力值");
    drop(guard);

    // 释放借用后再次重载，Kit 状态一致。
    ready.reload_config::<FreshCfg>().unwrap();
    assert_eq!(ready.config::<FreshCfg>().unwrap().v, 9);
}
