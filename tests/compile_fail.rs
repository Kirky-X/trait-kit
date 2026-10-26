// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Compile-fail tests: verify that typestate misuse produces compile errors.
//!
//! 快照（tests/ui/*.stderr）仅在 `async` 启用时运行比对，原因：E0599 候选
//! 列表包含仅在该 feature 下存在并进入 prelude 的 `AsyncAutoBuilder`。
//!
//! 漂移模型（写 fixtures 时必须遵守，见 ui/*.rs 头部注释）：每个 fixture
//! 必须恰好产生一个错误——被测的 typestate E0599。若 fixture 里出现任何
//! 附带错误（如宏缺失级联出的 E0277），rustc 的 "other types implement
//! this trait" 建议会列出当前构建中真实存在的 impl，随 presets/compose
//! 等 feature 门控漂移，导致部分特性组合下快照必然失配（曾因此只在
//! `--all-features` 下通过）。
//!
//! probe 敏感组（tests/ui/probe_sensitive/）：fixture 的 E0599 help 段
//! 引用 `register_probe`（仅 probe feature 存在），故快照比对需要
//! async + probe 同时开启——与主组分开门控。
//!
//! feature 门控组（tests/ui/{observer,decorator,lifecycle}_ready/）：fixture
//! 体引用仅在该 feature 下存在的类型/方法（BuildObserver、decorate、
//! register_lifecycle），无对应 feature 时会产生附带错误，故按 feature
//! 分组门控。
//!
//! observer_ready 组额外要求全特性面：其 E0599 help 段（`with_subscriber`
//! 相似名建议）只在全部特性开启的方法面下产生；部分特性组合会退化为
//! "the method was found for Kit" note，实测单开任意一个缺失特性均不能
//! 稳定复现快照，故按全集门控（CI test/release 均以 --all-features 执行）。

#[cfg(feature = "async")]
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
    #[cfg(feature = "probe")]
    t.compile_fail("tests/ui/probe_sensitive/*.rs");
    #[cfg(all(
        feature = "async",
        feature = "confers",
        feature = "reload",
        feature = "encryption",
        feature = "di",
        feature = "interface",
        feature = "lifecycle",
        feature = "health",
        feature = "probe",
        feature = "request-scope",
        feature = "scope",
        feature = "toggle",
        feature = "observer",
        feature = "decorator",
        feature = "shutdown",
        feature = "i18n",
        feature = "report",
        feature = "presets",
        feature = "presets-remote",
        feature = "version-negotiation",
        feature = "negotiate",
        feature = "compose"
    ))]
    t.compile_fail("tests/ui/observer_ready/*.rs");
    #[cfg(feature = "decorator")]
    t.compile_fail("tests/ui/decorator_ready/*.rs");
    #[cfg(feature = "lifecycle")]
    t.compile_fail("tests/ui/lifecycle_ready/*.rs");
}
