// Copyright (c) 2026 Kirky.X🌠
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
// - 03..10/12 → tests/basic.rs、src/kit/kit.rs、tests/e2e_advanced.rs
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

/// Ready 态 `require` 未注册模块 → `MissingCapability{key=NAME}`。
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

/// factory 闭包内 `M::build` 返回 Err → `BuildFailed{context=NAME}`
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

// ─── 宏声明模块 build 返回 Err 的传播路径（E2E 层，与内部宏测试互证） ───

use trait_kit::impl_auto_builder;

/// 宏模块专属错误类型：验证传播后 source 原样保真（不吞错、不二次包装）。
#[derive(Debug)]
struct MacroBoomErr;

impl std::fmt::Display for MacroBoomErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "macro boom")
    }
}
impl std::error::Error for MacroBoomErr {}

struct MacroFailingMod;
impl_module_meta!(MacroFailingMod, "macro-failing-mod");
impl_auto_builder!(MacroFailingMod, Arc<u32>, MacroBoomErr, |_kit| Err(
    MacroBoomErr
));

/// 宏声明的模块 build 返回 Err：kit.build() 失败且错误原样传播
/// （context==模块 NAME、source 下探到原始错误文本）。
#[test]
fn e2e_macro_module_build_err_propagates_from_kit_build() {
    let mut kit = Kit::new();
    kit.register::<MacroFailingMod>().unwrap();
    let err = kit
        .build()
        .expect_err("宏模块 build Err 应使 kit.build() 整体失败");
    match err {
        TraitKitError::BuildFailed { context, source } => {
            assert_eq!(context, "macro-failing-mod", "context 应为模块 NAME");
            assert!(
                source.to_string().contains("macro boom"),
                "source 应下探到原始错误文本：got '{source}'"
            );
        }
        other => panic!("expected BuildFailed, got: {other}"),
    }
}

// ─── soft_build 降级与 kit.build() 失败传播契约互不干扰 ─────────────
// soft_build 是 async fn：包裹点在异步构建体内（AsyncAutoBuilder），
// sync 面的"必需模块失败即整体失败"对照组用同步 AutoBuilder 固化。

#[cfg(feature = "async")]
mod soft_build_degradation_e2e {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::{Context, Poll, Waker};
    use trait_kit::impl_module_meta;
    use trait_kit::kit::soft_build::soft_build;
    use trait_kit::prelude::*;

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

    /// best-effort 外部资源：首次调用失败、之后成功。
    static BEST_EFFORT_FAILS: AtomicBool = AtomicBool::new(true);

    struct DegradingMod;
    impl_module_meta!(DegradingMod, "degrading-mod");
    impl AsyncAutoBuilder for DegradingMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
        {
            Box::pin(async {
                let cap = soft_build(
                    "external-flaky",
                    async {
                        if BEST_EFFORT_FAILS.swap(false, Ordering::SeqCst) {
                            Err("external dependency unavailable")
                        } else {
                            Ok(99u32)
                        }
                    },
                    0u32,
                )
                .await;
                Ok(Arc::new(cap))
            })
        }
    }

    struct RequiredFailingMod;
    impl_module_meta!(RequiredFailingMod, "required-failing-mod");
    impl AsyncAutoBuilder for RequiredFailingMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
        {
            Box::pin(async {
                Err(TraitKitError::MissingCapability {
                    key: "must-have".into(),
                })
            })
        }
    }

    /// soft_build 包裹 best-effort 外部资源：外部失败时模块仍 Ok（降级
    /// 值入库），kit.build() 不失败；对照无 soft_build 的必需模块失败
    /// 即整体失败。
    #[test]
    fn e2e_soft_build_degrades_without_failing_kit_build() {
        // 对照组：必需模块失败 → kit.build() 整体失败。
        let mut hard = AsyncKit::new();
        hard.register::<RequiredFailingMod>().unwrap();
        assert!(
            block_on(hard.build()).is_err(),
            "无 soft_build 的必需模块失败应使整体构建失败"
        );

        // 实验组：soft_build 包裹的 best-effort 失败 → 降级能力入库。
        let mut kit = AsyncKit::new();
        kit.register::<DegradingMod>().unwrap();
        let ready = block_on(kit.build()).expect("soft_build 降级路径不应使 kit.build() 失败");
        assert_eq!(
            *ready.require::<DegradingMod>().unwrap(),
            0,
            "外部失败时应取 fallback 降级值"
        );
    }
}

// ─── get_arc 错误分支（缺失 → MissingCapability；同指针别名互证） ─────

/// get_arc 的"能力存在但非 Arc<T> → CapabilityTypeMismatch"分支需要
/// crate-internal TypeMap 构造破坏不变量（公共 override/注册 API 均在
/// 类型系统内强制能力类型），由 src/kit/kit.rs 内联测试
/// get_arc_non_arc_capability_reports_type_mismatch 固化；此处固化
/// 集成面可构造的"完全缺失 → MissingCapability"与免克隆别名契约。
#[test]
fn e2e_get_arc_missing_module_returns_missing_capability() {
    let mut kit = Kit::new();
    kit.register::<PresentMod>().unwrap();
    let ready = kit.build().unwrap();

    let err = ready
        .get_arc::<UnregisteredMod, u32>()
        .expect_err("未构建模块 get_arc 应报缺失");
    match err {
        TraitKitError::MissingCapability { key } => assert_eq!(key, "unreg-mod"),
        other => panic!("expected MissingCapability, got: {other}"),
    }

    // 命中路径互证：Arc 能力 get_arc 与 require 克隆出的 Arc 同指针。
    let via_require = ready.require::<PresentMod>().unwrap();
    let via_get_arc = ready.get_arc::<PresentMod, u32>().unwrap();
    assert!(Arc::ptr_eq(&via_require, &via_get_arc));
}

// ─── lazy 首建失败（Err）后槽位放回、修复条件后可重试 ─────────────────

use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

static RETRY_LAZY_FAILS: AtomicBool = AtomicBool::new(true);

struct RetryLazyMod;
impl_module_meta!(RetryLazyMod, "retry-lazy-mod");
impl AutoBuilder for RetryLazyMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        if RETRY_LAZY_FAILS.swap(false, AtomicOrdering::SeqCst) {
            Err(TraitKitError::MissingCapability {
                key: "transient".into(),
            })
        } else {
            Ok(Arc::new(5))
        }
    }
}

/// lazy builder 首次返回 Err：require 得 BuildFailed{context=NAME}；
/// builder 放回槽位（src/kit/kit.rs 恢复契约），修复条件后再次 require
/// 构建成功，绝不退化为永久 MissingCapability。
#[test]
fn e2e_lazy_build_failure_is_retryable_after_slot_restore() {
    let mut kit = Kit::new();
    kit.register_lazy::<RetryLazyMod>().unwrap();
    let ready = kit.build().unwrap();

    let err = ready
        .require::<RetryLazyMod>()
        .expect_err("首次 require 应得到 BuildFailed");
    match err {
        TraitKitError::BuildFailed { context, source } => {
            assert_eq!(context, "retry-lazy-mod");
            assert!(source.to_string().contains("transient"));
        }
        other => panic!("expected BuildFailed, got: {other}"),
    }

    // 修复外部条件后重试：builder 已放回槽位，构建成功且值可读。
    let cap = ready
        .require::<RetryLazyMod>()
        .expect("槽位恢复后重试应构建成功（非永久 MissingCapability）");
    assert_eq!(*cap, 5);
}
