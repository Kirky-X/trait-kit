// Copyright © 2026 Kirky.X. All rights reserved.

//! Automated verification that all examples compile and run successfully.
//! Covers TC-DOC-001 through TC-DOC-004.

use std::process::Command;

fn run_example(name: &str) {
    let output = Command::new("cargo")
        .args(["run", "--example", name])
        .output()
        .expect("failed to execute cargo run --example");

    assert!(
        output.status.success(),
        "Example `{name}` failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn tc_doc_001_basic_logger() {
    run_example("basic_logger");
}

#[test]
fn tc_doc_002_service_injection() {
    run_example("service_injection");
}

#[test]
fn tc_doc_003_config_center() {
    run_example("config_center");
}

#[test]
fn tc_doc_004_layered_app() {
    run_example("layered_app");
}
