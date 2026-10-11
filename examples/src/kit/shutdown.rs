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
use trait_kit::i18n::tr;
use trait_kit::kit::shutdown::{ShutdownCoordinator, ShutdownPhase};

static HOOK_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn main() {
    println!("{}\n", tr("trait-kit-example-shutdown-title", &[]));

    let coord = ShutdownCoordinator::new();

    // ─── 阶段 1：停止接收新请求 ─────────────────────────────
    coord.register_hook(ShutdownPhase::StopRequests, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!(
            "  {}",
            tr("trait-kit-example-shutdown-stop-requests-listener", &[])
        );
    });
    coord.register_hook(ShutdownPhase::StopRequests, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!(
            "  {}",
            tr("trait-kit-example-shutdown-stop-requests-draining", &[])
        );
    });

    // ─── 阶段 2：排空队列 ──────────────────────────────────
    coord.register_hook(ShutdownPhase::DrainQueue, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!("  {}", tr("trait-kit-example-shutdown-drain-queue", &[]));
    });

    // ─── 阶段 3：关闭连接池 ─────────────────────────────────
    coord.register_hook(ShutdownPhase::CloseConnections, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!("  {}", tr("trait-kit-example-shutdown-close-db-pool", &[]));
    });
    coord.register_hook(ShutdownPhase::CloseConnections, || {
        HOOK_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!("  {}", tr("trait-kit-example-shutdown-close-redis", &[]));
    });

    // ─── 设置超时 ──────────────────────────────────────────
    coord.set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_secs(5));
    coord.set_global_timeout(Duration::from_secs(30));

    // ─── 执行关闭流程 ──────────────────────────────────────
    println!("{}\n", tr("trait-kit-example-shutdown-running", &[]));
    let results = coord.shutdown();

    println!("\n{}", tr("trait-kit-example-shutdown-results-header", &[]));
    for r in &results.phases {
        let status = if r.is_ok() {
            tr("trait-kit-example-shutdown-status-done", &[])
        } else {
            tr("trait-kit-example-shutdown-status-timed-out", &[])
        };
        let phase_debug = format!("{:?}", r.phase);
        let elapsed = format!("{:?}", r.elapsed);
        println!(
            "  {}",
            tr(
                "trait-kit-example-shutdown-result-line",
                &[
                    ("phase", r.phase.as_str()),
                    ("phase_debug", &phase_debug),
                    ("status", &status),
                    ("elapsed", &elapsed),
                ]
            )
        );
    }

    // ─── 验证 ──────────────────────────────────────────────
    let total_hooks = HOOK_COUNTER.load(Ordering::Relaxed);
    assert_eq!(total_hooks, 5, "all 5 hooks should have run");
    assert!(results.is_ok(), "all phases should complete normally");

    println!(
        "\n{}",
        tr(
            "trait-kit-example-shutdown-ok",
            &[("total_hooks", &total_hooks.to_string())]
        )
    );
}
