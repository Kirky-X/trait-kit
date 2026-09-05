// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// 热重载 E2E 测试。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.6）：
// - RLD-06 订阅者 panic 语义固化：新值已存储、剩余订阅者被跳过
//   （panic 穿透 reload_config，文档化行为）

#![cfg(feature = "reload")]

use std::cell::Cell;
use std::error::Error;
use std::rc::Rc;
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
