// Copyright © 2026 Kirky.X. All rights reserved.

//! KitModuleBuilder tests (TC-BUILDER-001~003).

mod common;

use std::sync::Arc;

use trait_kit::prelude::*;

use common::*;

#[test]
fn tc_builder_001_kit_provide_succeeds() {
    let kit = Kit::new();
    let logger = LoggerBuilder::new()
        .config(LoggerConfig {
            prefix: "[test] ".to_string(),
        })
        .kit(&kit)
        .provide::<MainLogger>()
        .unwrap();

    assert!(kit.contains::<MainLogger>());
    assert!(Arc::ptr_eq(&kit.require::<MainLogger>().unwrap(), &logger));
}

#[test]
fn tc_builder_002_build_failed_wraps_in_kit_error() {
    let kit = Kit::new();
    let result = LoggerBuilder::new()
        .config(LoggerConfig {
            prefix: "".to_string(),
        })
        .kit(&kit)
        .provide::<MainLogger>();

    match result {
        Err(KitError::BuildFailed { module, source }) => {
            assert_eq!(module, LoggerModule::NAME);
            assert!(source.downcast_ref::<LoggerBuildError>().is_some());
        }
        other => panic!("expected BuildFailed, got: {:?}", other.map(|_| ())),
    }

    assert!(!kit.contains::<MainLogger>());
}

#[test]
fn tc_builder_003_duplicate_capability_key() {
    let kit = Kit::new();

    LoggerBuilder::new()
        .config(LoggerConfig {
            prefix: "[v1] ".to_string(),
        })
        .kit(&kit)
        .provide::<MainLogger>()
        .unwrap();

    let result = LoggerBuilder::new()
        .config(LoggerConfig {
            prefix: "[v2] ".to_string(),
        })
        .kit(&kit)
        .provide::<MainLogger>();

    assert!(matches!(
        result,
        Err(KitError::DuplicateCapability { key: "main_logger" })
    ));
}
