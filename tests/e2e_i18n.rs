// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// i18n 组合 E2E 测试（独立测试进程：全局 I18nManager 为进程级 OnceLock
// 单例，zh locale 断言必须锚定在专属测试二进制内，防止与默认 locale
// 初始化竞争）。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md）：
// - CMP-14 i18n+shutdown：`ShutdownTimedOut` 错误消息走 `tr()` 翻译链，
//   zh locale 下错误文本本地化（本文件）
// - ERR-09 en（默认回退）侧断言落点见 tests/e2e_i18n_en.rs

#![cfg(feature = "i18n")]

use trait_kit::i18n::I18nManager;

/// 将本测试进程的全局 I18nManager 锚定为 zh-CN。
///
/// 所有测试首行调用：OnceLock 竞争双方都写入同一 locale，胜者恒为
/// zh-CN，因此无需 serial 门控。
fn ensure_zh() {
    let _ = I18nManager::init_with_locale("zh-CN");
}

#[cfg(feature = "shutdown")]
mod i18n_shutdown_e2e {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use trait_kit::i18n::I18nManager;
    use trait_kit::kit::{ShutdownCoordinator, ShutdownPhase, ShutdownResult};

    use super::ensure_zh;

    /// CMP-14：`ShutdownTimedOut` 的 Display 经 `tr()` 查询 zh 目录，
    /// 超时阶段名以 `as_str()` 原文嵌入本地化模板。
    #[test]
    fn e2e_i18n_shutdown_timed_out_display_localized_zh() {
        ensure_zh();
        assert_eq!(I18nManager::global().unwrap().locale_tag(), "zh-CN");

        // 真实路径：阶段超时 → ShutdownPhaseResult(timed_out) →
        // ShutdownResult::into_result → Err(ShutdownTimedOut)。
        // 超时语义：每个钩子执行前检查耗时，超时后跳过本阶段剩余钩子，
        // 因此用「慢钩子 + 应被跳过的钩子」触发 timed_out。
        static SKIPPED: AtomicBool = AtomicBool::new(false);
        SKIPPED.store(false, Ordering::SeqCst);
        let coord = ShutdownCoordinator::new();
        coord.set_phase_timeout(ShutdownPhase::StopRequests, Duration::from_millis(1));
        coord.register_hook(ShutdownPhase::StopRequests, || {
            std::thread::sleep(Duration::from_millis(30));
        });
        coord.register_hook(ShutdownPhase::StopRequests, || {
            SKIPPED.store(true, Ordering::SeqCst);
        });
        coord.register_hook(ShutdownPhase::DrainQueue, || {});
        let results = coord.shutdown();
        assert!(
            !results[0].is_ok(),
            "StopRequests 累计耗时超过 1ms 阶段超时，应标记 timed_out"
        );
        assert!(
            !SKIPPED.load(Ordering::SeqCst),
            "超时后本阶段剩余钩子应被跳过"
        );
        assert!(results[1].is_ok(), "DrainQueue 无钩子应正常完成");

        let err = ShutdownResult { phases: results }
            .into_result()
            .expect_err("存在超时阶段时应返回 ShutdownTimedOut");
        let msg = err.to_string();
        assert!(
            msg.contains("优雅关闭"),
            "zh locale 下 Display 应走 zh 目录：got '{msg}'"
        );
        assert!(
            msg.contains("stop_requests"),
            "超时阶段名 stop_requests 应嵌入消息：got '{msg}'"
        );
    }

    /// CMP-14 补充：直接构造变体的 Display 与协调器产出路径一致
    /// （同一 tr() 消息 id：trait-kit-error-shutdown-timed-out）。
    #[test]
    fn e2e_i18n_shutdown_timed_out_variant_display_zh() {
        ensure_zh();
        let err = trait_kit::TraitKitError::ShutdownTimedOut {
            phases: vec![ShutdownPhase::StopRequests, ShutdownPhase::CloseConnections],
        };
        let msg = err.to_string();
        assert!(msg.contains("优雅关闭"), "got '{msg}'");
        assert!(msg.contains("stop_requests"), "got '{msg}'");
        assert!(msg.contains("close_connections"), "got '{msg}'");
    }
}
