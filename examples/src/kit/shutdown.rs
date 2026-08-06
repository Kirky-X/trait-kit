// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Shutdown feature — 优雅关闭协调器
//!
//! 演示：
//! - `ShutdownCoordinator` 分阶段关闭（StopRequests → DrainQueue → CloseConnections）
//! - `register_hook()` 注册各阶段关闭钩子
//! - `set_phase_timeout()` / `set_global_timeout()` 超时控制
//! - `shutdown()` 执行并检查结果
//!
//! Run: `cargo run -p trait-kit-example --example shutdown --features shutdown`

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use trait_kit::kit::shutdown::{ShutdownCoordinator, ShutdownPhase};

static HOOK_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn main() {
    println!("=== ShutdownCoordinator 示例 ===\n");

    let coord = ShutdownCoordinator::new();

    // ─── 阶段 1：停止接收新请求 ─────────────────────────────
    coord.register_hook(ShutdownPhase::StopRequests, || {
        HOOK_COUNTER.fetch_add(1, Ordering::SeqCst);
        println!("  [phase 1] 停止接收新请求 — 关闭 HTTP listener");
    });
    coord.register_hook(ShutdownPhase::StopRequests, || {
        HOOK_COUNTER.fetch_add(1, Ordering::SeqCst);
        println!("  [phase 1] 标记为 draining 状态");
    });

    // ─── 阶段 2：排空队列 ──────────────────────────────────
    coord.register_hook(ShutdownPhase::DrainQueue, || {
        HOOK_COUNTER.fetch_add(1, Ordering::SeqCst);
        println!("  [phase 2] 排空消息队列中的待处理任务");
    });

    // ─── 阶段 3：关闭连接池 ─────────────────────────────────
    coord.register_hook(ShutdownPhase::CloseConnections, || {
        HOOK_COUNTER.fetch_add(1, Ordering::SeqCst);
        println!("  [phase 3] 关闭数据库连接池");
    });
    coord.register_hook(ShutdownPhase::CloseConnections, || {
        HOOK_COUNTER.fetch_add(1, Ordering::SeqCst);
        println!("  [phase 3] 关闭 Redis 连接");
    });

    // ─── 设置超时 ──────────────────────────────────────────
    coord.set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_secs(5));
    coord.set_global_timeout(Duration::from_secs(30));

    // ─── 执行关闭流程 ──────────────────────────────────────
    println!("执行关闭流程...\n");
    let results = coord.shutdown();

    println!("\n--- 关闭结果 ---");
    for r in &results {
        let status = if r.is_ok() { "✓ 完成" } else { "✗ 超时" };
        println!(
            "  {} ({:?}): {} (耗时 {:?})",
            r.phase.as_str(),
            r.phase,
            status,
            r.elapsed
        );
    }

    // ─── 验证 ──────────────────────────────────────────────
    let total_hooks = HOOK_COUNTER.load(Ordering::SeqCst);
    assert_eq!(total_hooks, 5, "所有 5 个钩子应被执行");
    assert!(
        results.iter().all(|r| r.is_ok()),
        "所有阶段应正常完成"
    );

    println!("\nshutdown: OK (共执行 {total_hooks} 个钩子)");
}
