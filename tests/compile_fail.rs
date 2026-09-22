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

#[cfg(feature = "async")]
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
