// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// 优雅关闭运行时 E2E 测试。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.15 / §2.18）：
// - SHD-06 `into_result()` 超时态 → `Err(ShutdownTimedOut{phases})`
// - ERR-07 `ShutdownTimedOut` Display 含全部超时阶段名
//   （阶段名以 `ShutdownPhase::as_str()` 原文嵌入，与 locale 无关）

#![cfg(feature = "shutdown")]

use std::time::Duration;
use trait_kit::kit::{ShutdownCoordinator, ShutdownPhase, ShutdownResult};
use trait_kit::prelude::*;

/// 真实超时路径：阶段内慢钩子耗尽超时预算 → 剩余钩子被跳过 →
/// 本阶段标记 timed_out → into_result 映射为 ShutdownTimedOut。
#[test]
fn e2e_shutdown_timed_out_into_result_and_display() {
    let coord = ShutdownCoordinator::new();
    coord.set_phase_timeout(ShutdownPhase::StopRequests, Duration::from_millis(1));
    coord.register_hook(ShutdownPhase::StopRequests, || {
        std::thread::sleep(Duration::from_millis(30));
    });
    coord.register_hook(ShutdownPhase::StopRequests, || {
        panic!("超时后剩余钩子不应执行");
    });

    let results = coord.shutdown();
    assert!(!results[0].is_ok(), "StopRequests 应标记 timed_out");
    assert!(results[1].is_ok() && results[2].is_ok());

    let err = ShutdownResult { phases: results }
        .into_result()
        .expect_err("存在超时阶段时应返回 Err");
    match &err {
        TraitKitError::ShutdownTimedOut { phases } => {
            assert_eq!(phases, &[ShutdownPhase::StopRequests]);
        }
        other => panic!("应变体为 ShutdownTimedOut：got {other:?}"),
    }
    // ERR-07：Display 含超时阶段可读名（as_str 原文，locale 无关）。
    let msg = err.to_string();
    assert!(
        msg.contains("stop_requests"),
        "Display 应含超时阶段名 stop_requests：got '{msg}'"
    );
}

/// 全局超时路径：全局预算为零 → 三阶段全部标记 timed_out →
/// Display 同时含全部三个阶段名。
#[test]
fn e2e_shutdown_global_timeout_display_contains_all_phases() {
    let coord = ShutdownCoordinator::new();
    coord.set_global_timeout(Duration::ZERO);
    coord.register_hook(ShutdownPhase::CloseConnections, || {
        panic!("全局超时后钩子不应执行");
    });

    let results = coord.shutdown();
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| !r.is_ok()), "三阶段应全部超时");

    let err = ShutdownResult { phases: results }
        .into_result()
        .expect_err("全部超时应返回 Err");
    let msg = err.to_string();
    for name in [
        ShutdownPhase::StopRequests,
        ShutdownPhase::DrainQueue,
        ShutdownPhase::CloseConnections,
    ] {
        assert!(
            msg.contains(name.as_str()),
            "Display 应含全部超时阶段名 {}：got '{msg}'",
            name.as_str()
        );
    }
}
