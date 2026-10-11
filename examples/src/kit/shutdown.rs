// Copyright (c) 2026 Kirky.X🌠
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
    println!("=== ShutdownCoordinator demo ===\n");

    let coord = ShutdownCoordinator::new();

    // ─── 阶段 1：停止接收新请求 ─────────────────────────────
    coord.register_hook(ShutdownPhase::StopRequests, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!("  [phase 1] stop accepting new requests — close HTTP listener");
    });
    coord.register_hook(ShutdownPhase::StopRequests, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!("  [phase 1] mark as draining");
    });

    // ─── 阶段 2：排空队列 ──────────────────────────────────
    coord.register_hook(ShutdownPhase::DrainQueue, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!("  [phase 2] drain pending tasks in the message queue");
    });

    // ─── 阶段 3：关闭连接池 ─────────────────────────────────
    coord.register_hook(ShutdownPhase::CloseConnections, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!("  [phase 3] close database connection pool");
    });
    coord.register_hook(ShutdownPhase::CloseConnections, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!("  [phase 3] close Redis connection");
    });

    // ─── 设置超时 ──────────────────────────────────────────
    coord.set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_secs(5));
    coord.set_global_timeout(Duration::from_secs(30));

    // ─── 执行关闭流程 ──────────────────────────────────────
    println!("running shutdown...\n");
    let results = coord.shutdown();

    println!("\n--- shutdown results ---");
    for r in &results.phases {
        let status = if r.is_ok() {
            "✓ done"
        } else {
            "✗ timed out"
        };
        println!(
            "  {} ({:?}): {} (elapsed {:?})",
            r.phase.as_str(),
            r.phase,
            status,
            r.elapsed
        );
    }

    // ─── 验证 ──────────────────────────────────────────────
    let total_hooks = HOOK_COUNTER.load(Ordering::Relaxed);
    assert_eq!(total_hooks, 5, "all 5 hooks should have run");
    assert!(results.is_ok(), "all phases should complete normally");

    println!("\nshutdown: OK ({total_hooks} hooks executed)");
}
