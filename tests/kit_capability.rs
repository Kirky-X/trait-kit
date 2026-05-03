// Copyright © 2026 Kirky.X. All rights reserved.

//! Kit capability store tests (TC-KIT-001~007).

mod common;

use std::sync::Arc;

use trait_kit::prelude::*;

use common::*;

#[test]
fn tc_kit_001_provide_first_registration() {
    let kit = Kit::new();
    let logger: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[test] ".to_string(),
    });

    let result = kit.provide::<MainLogger>(logger.clone());
    assert!(result.is_ok());
    assert!(kit.contains::<MainLogger>());
    assert!(Arc::ptr_eq(&kit.require::<MainLogger>().unwrap(), &logger));
}

#[test]
fn tc_kit_002_provide_duplicate_fails() {
    let kit = Kit::new();
    let logger_1: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[v1] ".to_string(),
    });
    let logger_2: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[v2] ".to_string(),
    });

    kit.provide::<MainLogger>(logger_1.clone()).unwrap();
    let result = kit.provide::<MainLogger>(logger_2);

    assert!(matches!(
        result,
        Err(KitError::DuplicateCapability { key: "main_logger" })
    ));
    assert!(Arc::ptr_eq(
        &kit.require::<MainLogger>().unwrap(),
        &logger_1
    ));
}

#[test]
fn tc_kit_003_replace_overwrites_existing() {
    let kit = Kit::new();
    let logger_1: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[v1] ".to_string(),
    });
    let logger_2: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[v2] ".to_string(),
    });

    kit.provide::<MainLogger>(logger_1).unwrap();
    kit.replace::<MainLogger>(logger_2.clone());

    assert!(Arc::ptr_eq(
        &kit.require::<MainLogger>().unwrap(),
        &logger_2
    ));
}

#[test]
fn tc_kit_004_replace_works_without_existing() {
    let kit = Kit::new();
    let logger: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[test] ".to_string(),
    });

    kit.replace::<MainLogger>(logger.clone());

    assert!(kit.contains::<MainLogger>());
    assert!(Arc::ptr_eq(&kit.require::<MainLogger>().unwrap(), &logger));
}

#[test]
fn tc_kit_005_require_returns_registered() {
    let kit = Kit::new();
    let logger: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[test] ".to_string(),
    });

    kit.provide::<MainLogger>(logger.clone()).unwrap();
    let result = kit.require::<MainLogger>();

    assert!(result.is_ok());
    assert!(Arc::ptr_eq(&result.unwrap(), &logger));
}

#[test]
fn tc_kit_006_require_missing_fails() {
    let kit = Kit::new();
    let result = kit.require::<MainLogger>();

    assert!(matches!(
        result,
        Err(KitError::MissingCapability { key: "main_logger" })
    ));
}

#[test]
fn tc_kit_007_contains_missing_returns_false() {
    let kit = Kit::new();
    assert!(!kit.contains::<MainLogger>());

    assert!(matches!(
        kit.require::<MainLogger>(),
        Err(KitError::MissingCapability { key: "main_logger" })
    ));
}
