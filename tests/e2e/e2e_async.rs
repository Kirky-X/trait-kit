// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// 异步 Kit E2E 测试（缺口固化）。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.16 / §2.12）：
// - ASK-13 异步取消：build future 被 drop 后状态不半更新，重试 build 可成功
// - DEC-08 `AsyncKit::decorate`：async 构建路径装饰行为与 sync 一致

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

// ─── ASK-13：异步构建取消安全 ──────────────────────────────────────────

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

// ─── DEC-08：AsyncKit::decorate 与 sync 行为一致 ───────────────────────

struct AsyncDecoMod;
impl_module_meta!(AsyncDecoMod, "async-deco-mod");
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
