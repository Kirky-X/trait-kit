// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Compile-fail tests: verify that typestate misuse produces compile errors.
//!
//! 快照（tests/ui/*.stderr）在 `async` feature 启用的语义下录制：E0599 候选
//! 列表包含仅在该 feature 下进入 prelude 的 `AsyncAutoBuilder`。feature 集
//! 不同会让 rustc 诊断文本漂移，因此仅在 `async` 启用时运行快照比对——
//! 实测 default 与 `--features async` / `--all-features` 在此门控下结果一致。

#[cfg(feature = "async")]
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
