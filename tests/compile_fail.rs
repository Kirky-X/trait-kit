// SPDX-License-Identifier: MIT
//! Compile-fail tests: verify that typestate misuse produces compile errors.

#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
