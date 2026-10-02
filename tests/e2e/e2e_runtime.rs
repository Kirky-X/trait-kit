// Copyright (c) 2026 Kirky.X🌠
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
use trait_kit::kit::{ShutdownCoordinator, ShutdownPhase};
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

    let result = coord.shutdown();
    assert!(!result.phases[0].is_ok(), "StopRequests 应标记 timed_out");
    assert!(result.phases[1].is_ok() && result.phases[2].is_ok());

    let err = result.into_result().expect_err("存在超时阶段时应返回 Err");
    match &err {
        TraitKitError::ShutdownTimedOut { phases } => {
            assert_eq!(phases, &[ShutdownPhase::StopRequests]);
        }
        other => panic!("应变体为 ShutdownTimedOut：got {other:?}"),
    }
    // Display 含超时阶段可读名（as_str 原文，locale 无关）。
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

    let result = coord.shutdown();
    assert_eq!(result.len(), 3);
    assert!(!result.is_ok(), "三阶段应全部超时");

    let err = result.into_result().expect_err("全部超时应返回 Err");
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

// ─── 健康历史环形容量边界（health 落点，HLX-03 同文件） ────────────────

/// capacity=0 禁用采样（record 为 no-op、history 恒空）；先采 n 条再
/// 收缩容量至 k<n → 既有历史立即截断到最近 k 条。
#[cfg(feature = "health")]
mod health_history_capacity_edge_e2e {
    use std::sync::Arc;
    use trait_kit::core::HealthCheck;
    use trait_kit::core::health::HealthStatus;
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    struct HistMod;
    impl_module_meta!(HistMod, "hist-mod");
    impl AutoBuilder for HistMod {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }
    impl HealthCheck for HistMod {
        fn check(_cap: &Arc<u32>) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    fn kit_with_checker() -> trait_kit::kit::Kit<Ready> {
        let mut kit = Kit::new();
        kit.register::<HistMod>().unwrap();
        kit.register_health_check::<HistMod>();
        kit.build().unwrap()
    }

    #[test]
    fn e2e_health_history_zero_capacity_disables_sampling() {
        let ready = kit_with_checker();
        ready.set_health_history_capacity(0);
        for _ in 0..5 {
            ready.record_health_history();
        }
        assert!(
            ready.health_history().is_empty(),
            "capacity=0 时 record 应为 no-op（history 恒空）"
        );
    }

    #[test]
    fn e2e_health_history_shrink_truncates_existing_samples() {
        let ready = kit_with_checker();
        ready.set_health_history_capacity(4);
        for _ in 0..4 {
            ready.record_health_history();
        }
        assert_eq!(ready.health_history().len(), 4, "先采满 4 条");

        // 收缩容量到 2：既有历史立即截断到最近 2 条。
        ready.set_health_history_capacity(2);
        let history = ready.health_history();
        assert_eq!(history.len(), 2, "收缩容量应立即截断既有历史到最近 k 条");
        // 时间戳与状态字段齐全。
        assert!(matches!(history[1].status, HealthStatus::Healthy));
    }
}
