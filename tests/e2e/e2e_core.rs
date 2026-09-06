// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// 核心面（无 feature）E2E 缺口固化测试。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md）：
// - CAP-02 `require` 未注册/未构建模块 → `MissingCapability{key=NAME}`
//   （sync 侧断言；async 同型见 src/kit/async_kit.rs 内联测试）
// - CAP-11 factory 构建失败 → `BuildFailed{context=NAME}` 传播（闭包内
//   Err 路径，含 source 下钻）
//
// 其余 core 域场景既有覆盖充分，此处仅声明引用、不重复固化：
// - MET-01..06/09 → src/core/meta.rs、src/core/macros.rs 内联测试；
//   tests/e2e_advanced.rs::a25_impl_module_meta_macro_three_forms
// - REG-01..18 → tests/basic.rs（b 系列）、tests/e2e_advanced.rs
//   （a/e/c 系列）、src/kit/kit.rs 内联测试
// - REG-19（typestate 编译期违规）→ tests/compile_fail.rs 驱动 tests/ui/ 三例；
//   sync Kit `!Sync` 设计边界（CCY-03）由 tests/basic.rs 顶部的
//   `assert_not_impl_any!` 编译期断言固化（等效落点）
// - CAP-01/03..10/12 → tests/basic.rs、src/kit/kit.rs、tests/e2e_advanced.rs
// - DEP-01..10 → src/kit/graph.rs 内联测试、tests/basic.rs
// - ERR-01..05/08 → src/error.rs 内联测试；ERR-06 → tests/e2e_hooks.rs；
//   ERR-07 → tests/e2e_runtime.rs；ERR-09 → tests/e2e_i18n_en.rs（en 回退）
//   与 tests/e2e_i18n.rs（zh locale）
// - PRE-01 → src/prelude.rs 内联测试；PRE-02 → tests/basic.rs；
//   PRE-03 → tests/e2e_presets.rs（结构核对）

use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

/// 已定义但从不注册的模块（CAP-02 未注册分支）。
struct UnregisteredMod;
impl_module_meta!(UnregisteredMod, "unreg-mod");
impl AutoBuilder for UnregisteredMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(1))
    }
}

/// 注册后 build 成功、但另一类型从未注册的对照模块。
struct PresentMod;
impl_module_meta!(PresentMod, "present-mod");
impl AutoBuilder for PresentMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(2))
    }
}

/// CAP-02：Ready 态 `require` 未注册模块 → `MissingCapability{key=NAME}`。
#[test]
fn e2e_require_unregistered_returns_missing_capability() {
    let mut kit = Kit::new();
    kit.register::<PresentMod>().unwrap();
    let ready = kit.build().unwrap();

    let err = ready
        .require::<UnregisteredMod>()
        .expect_err("未注册模块的 require 应返回错误");
    match err {
        TraitKitError::MissingCapability { key } => {
            assert_eq!(key, "unreg-mod", "错误 key 应为模块 NAME");
        }
        other => panic!("expected MissingCapability, got: {other}"),
    }
}

/// CAP-02 补充：`contains` 对未注册模块返回 false（与 require 错误口径互证）。
#[test]
fn e2e_contains_false_for_unregistered() {
    let mut kit = Kit::new();
    kit.register::<PresentMod>().unwrap();
    let ready = kit.build().unwrap();
    assert!(ready.contains::<PresentMod>());
    assert!(!ready.contains::<UnregisteredMod>());
}

/// CAP-11：factory 闭包内 `M::build` 返回 Err → `BuildFailed{context=NAME}`
/// 传播，source 保留底层错误语义（不吞错、不替换型别）。
struct FailingMod;
impl_module_meta!(FailingMod, "failing-mod");
impl AutoBuilder for FailingMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Err(TraitKitError::MissingCapability { key: "boom".into() })
    }
}

#[test]
fn e2e_factory_build_failure_returns_build_failed() {
    let mut kit = Kit::new();
    kit.register::<PresentMod>().unwrap();
    let ready = kit.build().unwrap();

    let produce = ready.factory::<FailingMod>();
    let err = produce().expect_err("factory 内 build 失败应返回错误");
    match err {
        TraitKitError::BuildFailed { context, source } => {
            assert_eq!(context, "failing-mod", "context 应为模块 NAME");
            assert!(
                source.to_string().contains("boom"),
                "source 应保留底层错误语义：got '{source}'"
            );
        }
        other => panic!("expected BuildFailed, got: {other}"),
    }
}

/// CAP-11 对照：factory 成功路径每次调用产出全新实例（单例语义互证）。
#[test]
fn e2e_factory_success_path_produces_fresh_instances() {
    let mut kit = Kit::new();
    kit.register::<PresentMod>().unwrap();
    let ready = kit.build().unwrap();

    let produce = ready.factory::<PresentMod>();
    let a = produce().unwrap();
    let b = produce().unwrap();
    assert_eq!(*a, 2);
    assert_eq!(*b, 2);
    assert!(!Arc::ptr_eq(&a, &b), "factory 每次应产出新实例而非共享单例");
}
