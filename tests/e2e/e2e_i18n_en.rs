// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// ERR-09 en 侧断言（独立测试进程锚定 en 目录）。
//
// 全局 I18nManager 为进程级 OnceLock 单例，且 `tr()` 内部的
// `I18nManager::init()` 会检测系统 locale（检测失败才回退 en-US），
// 因此 en 断言必须显式锚定：本测试二进制内所有测试首行调用
// `ensure_en()`，与 tests/e2e_i18n.rs（zh-CN 锚定进程）互为平行进程。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.18）：
// - ERR-09 错误 Display 双语目录绑定：en 目录覆盖全部错误消息 id，
//   8 个枚举变体逐一以 en 文本断言（zh 侧见 tests/e2e_i18n.rs）
//
// 既有覆盖引用：ERR-01..05/08 的语义细节（source 链/Debug 形态）由
// src/error.rs 内联测试覆盖；本文件固化「en 目录可达性」这一翻译面。

#![cfg(feature = "i18n")]

use trait_kit::TraitKitError;
use trait_kit::i18n::I18nManager;

/// 将本测试进程的全局 I18nManager 锚定为 en。
///
/// OnceLock 竞争双方都写入同一 locale，胜者恒为 en，无需 serial 门控。
fn ensure_en() {
    let _ = I18nManager::init_with_locale("en");
}

fn sample_source() -> Box<dyn std::error::Error + Send + 'static> {
    Box::new(std::io::Error::other("inner boom"))
}

/// ERR-09：8 个枚举变体在 en 目录下逐一可读（消息 id 全覆盖）。
#[test]
fn e2e_error_display_all_variants_english() {
    ensure_en();
    assert_eq!(I18nManager::global().unwrap().locale_tag(), "en");

    // ERR-01：CycleDetected 含全部环上模块名。
    let msg = TraitKitError::CycleDetected {
        cycle: vec!["mod-a", "mod-b"],
    }
    .to_string();
    assert!(
        msg.contains("dependency cycle detected"),
        "en 目录 CycleDetected：got '{msg}'"
    );
    assert!(
        msg.contains("mod-a") && msg.contains("mod-b"),
        "got '{msg}'"
    );

    // ERR-02：DependencyMissing 同时含发起模块与缺失依赖。
    let msg = TraitKitError::DependencyMissing {
        module: "db",
        missing: "logger",
    }
    .to_string();
    assert!(msg.contains("depends on"), "got '{msg}'");
    assert!(msg.contains("db") && msg.contains("logger"), "got '{msg}'");

    // ERR-03：AlreadyRegistered 含模块名。
    let msg = TraitKitError::AlreadyRegistered { module: "dup" }.to_string();
    assert!(msg.contains("is already registered"), "got '{msg}'");
    assert!(msg.contains("dup"), "got '{msg}'");

    // ERR-04：BuildFailed 含 context 与底层 source 文本。
    let msg = TraitKitError::BuildFailed {
        context: "svc".into(),
        source: sample_source(),
    }
    .to_string();
    assert!(msg.contains("failed to build"), "got '{msg}'");
    assert!(
        msg.contains("svc") && msg.contains("inner boom"),
        "got '{msg}'"
    );

    // ERR-05：MissingCapability / MissingConfig 含 key。
    let msg = TraitKitError::MissingCapability {
        key: "cap-k".into(),
    }
    .to_string();
    assert!(
        msg.contains("missing capability") && msg.contains("cap-k"),
        "got '{msg}'"
    );
    let msg = TraitKitError::MissingConfig {
        key: "cfg-k".into(),
    }
    .to_string();
    assert!(
        msg.contains("missing config") && msg.contains("cfg-k"),
        "got '{msg}'"
    );

    // ERR-06：LifecycleFailed 含 context 与 source。
    let msg = TraitKitError::LifecycleFailed {
        context: "hook".into(),
        source: sample_source(),
    }
    .to_string();
    assert!(msg.contains("lifecycle hook failed"), "got '{msg}'");
    assert!(
        msg.contains("hook") && msg.contains("inner boom"),
        "got '{msg}'"
    );
}

/// ERR-09 补充：ShutdownTimedOut 的 en 文本（shutdown 门控变体）。
#[cfg(feature = "shutdown")]
#[test]
fn e2e_error_display_shutdown_timed_out_english() {
    ensure_en();
    let msg = TraitKitError::ShutdownTimedOut {
        phases: vec![
            trait_kit::kit::ShutdownPhase::StopRequests,
            trait_kit::kit::ShutdownPhase::CloseConnections,
        ],
    }
    .to_string();
    assert!(
        msg.contains("graceful shutdown timed out"),
        "en 目录 ShutdownTimedOut：got '{msg}'"
    );
    assert!(
        msg.contains("stop_requests") && msg.contains("close_connections"),
        "超时阶段名应嵌入消息：got '{msg}'"
    );
}
