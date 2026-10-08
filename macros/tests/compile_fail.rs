// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Compile-fail UI tests for `#[derive(Module)]`: invalid attribute
//! usage must produce clear, spanned compile errors.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
