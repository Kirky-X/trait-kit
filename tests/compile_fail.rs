// Copyright © 2026 Kirky.X. All rights reserved.

//! Compile-fail tests using trybuild.

#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
