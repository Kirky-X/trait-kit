// Copyright © 2026 Kirky.X. All rights reserved.

//! Error model tests (TC-ERROR-001~006).

mod common;

use std::error::Error;

use trait_kit::prelude::*;

use common::*;

#[test]
fn tc_error_001_missing_capability_contains_key() {
    let kit = Kit::new();
    let result = kit.require::<MainLogger>();

    assert!(matches!(
        result,
        Err(KitError::MissingCapability { key: "main_logger" })
    ));
}

#[test]
fn tc_error_002_missing_config_contains_key() {
    let kit = Kit::new();
    let result = kit.config::<LoggerConfigKey>();

    assert!(matches!(
        result,
        Err(KitError::MissingConfig {
            key: "logger_config"
        })
    ));
}

#[test]
fn tc_error_003_build_failed_preserves_source() {
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
}

#[test]
fn tc_error_004_build_error_missing_config() {
    let error = BuildError::MissingConfig {
        module: "test_module",
    };
    assert!(error.to_string().contains("test_module"));
    assert!(error.to_string().contains("missing config"));
    assert!(error.source().is_none());
}

#[test]
fn tc_error_005_build_error_invalid_config() {
    let error = BuildError::InvalidConfig {
        module: "test_module",
        reason: "empty prefix",
    };
    assert!(error.to_string().contains("test_module"));
    assert!(error.to_string().contains("empty prefix"));
    assert!(error.source().is_none());
}

#[test]
fn tc_error_006_build_error_module_failed_preserves_source() {
    #[derive(Debug)]
    struct InnerError;

    impl std::fmt::Display for InnerError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "inner failure")
        }
    }

    impl std::error::Error for InnerError {}

    let error = BuildError::ModuleFailed {
        module: "logger_module",
        source: Box::new(InnerError),
    };

    assert!(error.to_string().contains("logger_module"));
    assert!(error.source().is_some());
    assert!(error
        .source()
        .unwrap()
        .to_string()
        .contains("inner failure"));
}
